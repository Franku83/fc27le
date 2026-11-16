use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use fc27_core::{Result, units::format_bytes};
use fc27_game::anchors::{ANCHORS, AnchorVerification, ScanOptions, verify_anchor};
use fc27_game::build_identity::{BuildIdentity, identify_file};
use fc27_game::profile::{PROTECTION_MODULE, THIRD_PARTY_MODULE, EAAC_MODULE, EDITOR_DLL, GAME_EXE};
use fc27_process::{ModuleInfo, ModuleKind, Pid, ProcessHandle, modules::find_module};
use serde::Serialize;

use crate::cli::ProbeArgs;
use crate::commands::open_process;
use crate::output::print_json;

const INTERESTING_MODULES: &[&str] = &[
    GAME_EXE,
    EDITOR_DLL,
    PROTECTION_MODULE,
    THIRD_PARTY_MODULE,
    EAAC_MODULE,
];

#[derive(Debug, Serialize)]
struct ModulePresence {
    name: &'static str,
    present: bool,
    base: Option<u64>,
    size: Option<u64>,
    path: Option<String>,
    kind: Option<ModuleKind>,
    pe_timestamp: Option<u32>,
    note: String,
}

#[derive(Debug, Serialize)]
struct Observation {
    id: &'static str,
    text: String,
    confidence: &'static str,
}

#[derive(Debug, Serialize)]
struct ReadProbe {
    memory_read_permitted: bool,
    address_checked: Option<u64>,
    detail: String,
}

#[derive(Debug, Serialize)]
struct ProbeOutput {
    pid: Pid,
    generated_at_unix: u64,
    process: fc27_process::ProcessInfo,
    policy: fc27_platform_linux::InspectionPolicy,
    read_probe: ReadProbe,
    modules_of_interest: Vec<ModulePresence>,
    game_build: Option<BuildIdentity>,
    build_identity_error: Option<String>,
    anchors: Vec<AnchorVerification>,
    observations: Vec<Observation>,
    limitations: Vec<&'static str>,
}

pub fn run(args: &ProbeArgs, json: bool) -> Result<u8> {
    let process = open_process(args.pid)?;
    let info = process.info().clone();
    let policy = fc27_platform_linux::inspect_policy();
    let modules = process.modules()?;

    let mut observations = Vec::new();
    let mut limitations = vec![
        "Read-only: no memory writes, ptrace attach, injection, hooks or binary patching.",
        "A SHA-256 match identifies the file, not the correctness of any memory structure.",
        "The event-dispatcher pattern is the only structure anchor with a documented expectation.",
        "Player data is evidence-gated; run `players --explain` for the missing items.",
        "Wine maps PE images as writable+executable; Linux mapping permissions are not Windows \
         section protections.",
        "Read access depends on the target being traceable (a descendant, or a PR_SET_PTRACER \
         exception such as the one Wine appears to install); the tool never changes ptrace policy.",
    ];

    let modules_of_interest = INTERESTING_MODULES
        .iter()
        .map(|name| describe_interest(&modules, name, &mut observations))
        .collect::<Vec<_>>();

    let game_module = find_module(&modules, GAME_EXE).cloned();
    let read_probe = match &game_module {
        Some(game) => match process.read(game.base, 2) {
            Ok(bytes) if bytes == b"MZ" => {
                observations.push(Observation {
                    id: "memory-read-permitted",
                    text: "A same-user read of the game PE header succeeded under the current \
                           policy. This indicates that the target is traceable (descendant or a \
                           PR_SET_PTRACER exception); no privilege was used."
                        .to_string(),
                    confidence: "confirmed",
                });
                ReadProbe {
                    memory_read_permitted: true,
                    address_checked: Some(game.base),
                    detail: "read 2 bytes at the PE header; MZ signature present".to_string(),
                }
            }
            Ok(bytes) => ReadProbe {
                memory_read_permitted: false,
                address_checked: Some(game.base),
                detail: format!(
                    "read {} bytes at the PE header, but the MZ signature was absent",
                    bytes.len()
                ),
            },
            Err(error) => {
                observations.push(Observation {
                    id: "memory-read-denied",
                    text: format!(
                        "A same-user read of the game PE header failed: {error}. The tool does not \
                         escalate privileges or change ptrace policy; run it as the same user, \
                         ensure the target is traceable, or use the controlled selftest."
                    ),
                    confidence: "confirmed",
                });
                ReadProbe {
                    memory_read_permitted: false,
                    address_checked: Some(game.base),
                    detail: error.to_string(),
                }
            }
        },
        None => ReadProbe {
            memory_read_permitted: false,
            address_checked: None,
            detail: "no game module to probe".to_string(),
        },
    };

    if let Some(game) = &game_module {
        if let Some(mapping) = process.mapping_containing(game.base) {
            observations.push(Observation {
                id: "wine-image-permissions",
                text: format!(
                    "{GAME_EXE} base mapping is {} at {:#x}; Wine does not mirror PE section \
                     protections in /proc/<pid>/maps.",
                    mapping.permissions, mapping.start
                ),
                confidence: "confirmed",
            });
        }
        if let Some(pe) = &game.pe {
            if pe.image_base == game.base {
                observations.push(Observation {
                    id: "preferred-base",
                    text: format!(
                        "{GAME_EXE} is mapped at its preferred image base {:#x}, so RVAs map \
                         directly to VAs (base + RVA).",
                        game.base
                    ),
                    confidence: "confirmed",
                });
            } else {
                observations.push(Observation {
                    id: "relocated-image",
                    text: format!(
                        "{GAME_EXE} is relocated: preferred base {:#x}, mapped base {:#x}. Always \
                         resolve VAs through the mapped base.",
                        pe.image_base, game.base
                    ),
                    confidence: "confirmed",
                });
            }
            if let Some(entry) = game.address_of_rva(u64::from(pe.entry_point_rva)) {
                observations.push(Observation {
                    id: "entry-point",
                    text: format!(
                        "PE entry point RVA {:#x} resolves to {entry:#x}.",
                        pe.entry_point_rva
                    ),
                    confidence: "confirmed",
                });
            }
        }
    } else {
        observations.push(Observation {
            id: "game-module-missing",
            text: format!(
                "{GAME_EXE} is not mapped in this process; memory anchors cannot be verified."
            ),
            confidence: "confirmed",
        });
    }

    let (game_build, build_identity_error) = match &game_module {
        Some(game) if !args.no_hash => match identify_file(Path::new(&game.path), true) {
            Ok(identity) => (Some(identity), None),
            Err(error) => (None, Some(error.to_string())),
        },
        Some(game) => match identify_file(Path::new(&game.path), false) {
            Ok(identity) => (Some(identity), None),
            Err(error) => (None, Some(error.to_string())),
        },
        None => (None, None),
    };

    let mut anchors = Vec::new();
    if !args.skip_anchors {
        let options = ScanOptions {
            max_bytes: args.anchor_max_mib.saturating_mul(1024 * 1024),
            max_matches: 16,
            executable_sections_only: true,
        };
        for anchor in ANCHORS {
            let verification = verify_anchor(&process, anchor, options);
            observations.push(Observation {
                id: anchor.id,
                text: verification.message.clone(),
                confidence: anchor.confidence,
            });
            anchors.push(verification);
        }
    } else {
        observations.push(Observation {
            id: "anchors-skipped",
            text: "Live anchor verification was skipped (--skip-anchors).".to_string(),
            confidence: "n/a",
        });
        limitations.push("No structure anchor was verified in this run.");
    }

    if !args.no_hash {
        observations.push(Observation {
            id: "hash-mode",
            text: "SHA-256 was computed for the game executable and compared with the known \
                   build table."
                .to_string(),
            confidence: "confirmed",
        });
    }

    let output = ProbeOutput {
        pid: args.pid,
        generated_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default(),
        process: info,
        policy,
        read_probe,
        modules_of_interest,
        game_build,
        build_identity_error,
        anchors,
        observations,
        limitations,
    };

    tracing::info!(
        pid = args.pid.0,
        game_module = game_module.is_some(),
        anchors = output.anchors.len(),
        "probe complete"
    );

    if json {
        print_json(&output)?;
        return Ok(0);
    }

    print_human(&output);
    Ok(0)
}

fn describe_interest(
    modules: &[ModuleInfo],
    name: &'static str,
    observations: &mut Vec<Observation>,
) -> ModulePresence {
    let found = find_module(modules, name);
    let (present, base, size, path, kind, pe_timestamp) = match found {
        Some(module) => (
            true,
            Some(module.base),
            Some(module.size),
            Some(module.path.clone()),
            Some(module.kind),
            module.pe.as_ref().map(|pe| pe.timestamp),
        ),
        None => (false, None, None, None, None, None),
    };

    let note = match (name, present) {
        (GAME_EXE, true) => "Game image resolved from /proc mappings.".to_string(),
        (GAME_EXE, false) => "Game image is not mapped; this is not the game process.".to_string(),
        (EDITOR_DLL, true) => {
            observations.push(Observation {
                id: "editor-loaded",
                text: "proprietary editor DLL is injected. Its log stages can be compared with the \
                       previous research, but this project does not interact with it."
                    .to_string(),
                confidence: "confirmed",
            });
            "Live Editor DLL is loaded in this process.".to_string()
        }
        (EDITOR_DLL, false) => {
            observations.push(Observation {
                id: "editor-not-loaded",
                text: "proprietary editor DLL is not injected. The game was started without the Live \
                       Editor launcher, so editor-internal structures are absent."
                    .to_string(),
                confidence: "confirmed",
            });
            "Live Editor DLL is not loaded.".to_string()
        }
        (PROTECTION_MODULE, true) => {
            observations.push(Observation {
                id: "activation-module-loaded",
                text: "activation module is loaded. This differs from the modified-build \
                       hypothesis in reports/root_cause.md and should be recorded."
                    .to_string(),
                confidence: "confirmed",
            });
            "EA activation module is loaded.".to_string()
        }
        (PROTECTION_MODULE, false) => {
            observations.push(Observation {
                id: "activation-module-absent",
                text: "activation module is not loaded, consistent with reports/root_cause.md \
                       (the Live Editor's unbounded wait for this module)."
                    .to_string(),
                confidence: "corroborated",
            });
            "EA activation module is not loaded.".to_string()
        }
        (THIRD_PARTY_MODULE, true) => {
            observations.push(Observation {
                id: "thirdparty-loaded",
                text: "third-party-module.dll is loaded. This is the third-party build distribution \
                       described in reports/runtime_evidence.md, not a other distribution install."
                    .to_string(),
                confidence: "confirmed",
            });
            "third-party third-party module is loaded.".to_string()
        }
        (THIRD_PARTY_MODULE, false) => "third-party third-party module is not loaded.".to_string(),
        (EAAC_MODULE, true) => {
            "EA Anti-Cheat service launcher module is loaded; recorded only, never interacted with."
                .to_string()
        }
        (EAAC_MODULE, false) => "EA Anti-Cheat service launcher module is not loaded.".to_string(),
        _ => "Recorded for completeness.".to_string(),
    };

    ModulePresence {
        name,
        present,
        base,
        size,
        path,
        kind,
        pe_timestamp,
        note,
    }
}

fn print_human(output: &ProbeOutput) {
    println!("FC 27 reconnaissance report");
    println!("generated:  unix {}", output.generated_at_unix);
    println!(
        "process:    pid {} ({}) [{}]",
        output.process.pid,
        output.process.name,
        output.process.state.as_deref().unwrap_or("?")
    );
    println!(
        "exe:        {}",
        output.process.exe_path.as_deref().unwrap_or("-")
    );
    println!(
        "cwd:        {}",
        output.process.cwd.as_deref().unwrap_or("-")
    );
    println!("command:    {}", output.process.command_line());

    if let Some(wine) = &output.process.wine {
        println!("\nWine/Proton");
        println!("  prefix:      {}", wine.prefix.as_deref().unwrap_or("-"));
        println!(
            "  proton root: {}",
            wine.proton_root.as_deref().unwrap_or("-")
        );
        println!(
            "  loader:      {}",
            wine.wine_loader.as_deref().unwrap_or("-")
        );
        println!(
            "  environment: {}",
            if wine.environment_readable {
                "readable"
            } else {
                "not readable"
            }
        );
    }

    println!("\nRead access");
    println!(
        "  permitted: {}",
        if output.read_probe.memory_read_permitted {
            "yes"
        } else {
            "no"
        }
    );
    if let Some(address) = output.read_probe.address_checked {
        println!("  address:   {address:#x}");
    }
    println!("  detail:    {}", output.read_probe.detail);

    println!("\nModules of interest");
    for module in &output.modules_of_interest {
        if module.present {
            println!(
                "  [present] {:<40} base={:#x} size={} {}",
                module.name,
                module.base.unwrap_or(0),
                format_bytes(module.size.unwrap_or(0)),
                module.path.as_deref().unwrap_or("-")
            );
        } else {
            println!("  [absent ] {:<40} {}", module.name, module.note);
            continue;
        }
        println!("            {}", module.note);
    }

    println!("\nBuild identity");
    if let Some(build) = &output.game_build {
        println!("  path:      {}", build.path);
        println!("  size:      {} bytes", build.size_bytes);
        println!(
            "  sha256:    {}",
            build.sha256.as_deref().unwrap_or("(not computed)")
        );
        if let Some(pe) = &build.pe {
            println!(
                "  PE:        machine={:#x} image_base={:#x} size_of_image={:#x} entry={:#x}",
                pe.machine, pe.image_base, pe.size_of_image, pe.entry_point_rva
            );
            println!(
                "             timestamp={} subsystem={} sections={}",
                pe.timestamp, pe.subsystem, pe.section_count
            );
        }
        for note in &build.notes {
            println!("  note:      {note}");
        }
    } else {
        println!(
            "  unavailable: {}",
            output
                .build_identity_error
                .as_deref()
                .unwrap_or("no game module")
        );
    }

    println!("\nVerified anchors");
    if output.anchors.is_empty() {
        println!("  (none)");
    }
    for anchor in &output.anchors {
        println!(
            "  [{}] {}",
            format!("{:?}", anchor.status).to_lowercase(),
            anchor.id
        );
        println!("    {}", anchor.message);
        println!("    source: {}", anchor.source);
        if anchor.match_count > 0 {
            println!(
                "    matches: {} at {}",
                anchor.match_count,
                anchor
                    .match_vas
                    .iter()
                    .map(|va| format!("{va:#x}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    println!("\nObservations");
    for observation in &output.observations {
        println!("  - [{}] {}", observation.confidence, observation.text);
    }

    println!("\nInspection policy");
    println!(
        "  yama ptrace_scope={} perf_event_paranoid={} uid={}",
        output
            .policy
            .yama_ptrace_scope
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/a".into()),
        output
            .policy
            .perf_event_paranoid
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/a".into()),
        output.policy.uid
    );

    println!("\nLimitations");
    for limitation in &output.limitations {
        println!("  - {limitation}");
    }
}
