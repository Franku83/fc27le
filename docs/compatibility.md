# Compatibility

This document states exactly what has been tested, what is expected to work, and
what is deliberately unsupported. Nothing here is a promise about an untested
build.

## 1. Host compatibility

| Requirement | Status |
| --- | --- |
| Linux with `/proc/<pid>/{maps,mem,status,environ,cmdline}` | required; tested on CachyOS |
| Kernel | tested `7.3.4-1-cachyos`; any reasonably modern Linux should work |
| Architecture | x86_64 tested; the PE parser handles PE32/PE32+ headers, the Linux backend is architecture-neutral |
| Rust | stable toolchain; tested with `rustc 1.100.0` |
| Root | **not required**; no privilege is requested |
| `hidepid` | supported; hidden processes simply do not appear |
| Yama `ptrace_scope` | observed `1`; reads worked because the target was traceable |
| Containers / pressure-vessel | tested inside the Steam Linux Runtime wrapper; no special handling |

## 2. Tested game session

| Component | Tested value |
| --- | --- |
| Game | EA SPORTS FC 27 |
| Executable | `FC27.exe`, 235,283,352 bytes |
| SHA-256 | `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9` |
| Distribution | third-party build (`.thirdparty` section, `third-party-module.dll` loaded) |
| Runtime | GE-Proton11-12 (`/path/to/protons/GE-Proton11-12-x86_64`) |
| Prefix | `/path/to/.local/share/third-party launcher/prefixes/FC27/pfx/` |
| Loader | `files/lib/wine/i386-unix/wine64-preloader` |
| Image base | `0x140000000` (preferred, not relocated) |
| Image size | `0xFA79000` |
| Session launch | third-party launcher, not the Live Editor launcher |

## 3. Game-build compatibility

Builds are identified by SHA-256, never assumed:

| Build hash | Label | Status |
| --- | --- | --- |
| `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9` | third-party build, 2026-11-12 | **tested**: discovery, modules, anchor verified |
| other distribution release | — | **not tested**; the previous reports also lacked the other distribution file set |
| Any other hash | — | unknown: expect anchor mismatch; re-research before use |

The tool reports `known_build: null` for an unknown hash. A hash match only
identifies the file; it does not validate offsets or structures.

### Anchor expectations

| Anchor | Expected RVA | Tested build |
| --- | ---: | --- |
| `fc27.event-dispatcher` | `0x1CD3B5C` | verified |

A different build may move the anchor. `probe` reports `mismatch`,
`pattern_missing`, `scan_truncated` or `module_missing`; it never assumes the
expected location is correct. If the anchor fails, treat all dependent
conclusions as invalid for that build.

## 4. Wine/Proton behaviour

Observed and relied upon:

* a Wine process's `/proc/<pid>/exe` is the preloader, not the PE image;
* PE images are mapped writable+executable (`rwxp`) in `/proc/<pid>/maps`;
* only some image sub-ranges are file-backed; the rest is anonymous;
* `SizeOfImage` is required to get a correct module size;
* Wine reserves the preferred image base (`WINEPRELOADRESERVE=140000000-14fa79000`),
  so RVAs and VAs coincide for this build;
* Wine's process tree appears traceable from the same user even under Yama
  scope 1, most plausibly through a `PR_SET_PTRACER` exception (reproduced with a
  controlled helper, not read from kernel internals).

Not guaranteed:

* the traceability exception on other Wine/Proton versions;
* the preferred image base on other builds (the tool always uses the mapped base);
* the exact set of `memfd` and anonymous mappings;
* module enumeration order or count;
* the presence of `proprietary editor DLL` or `activation module`.

If Wine changes any of these, the tool should fail with a clear error or an
anchor mismatch rather than silently misread.

## 5. Permission compatibility

| Situation | Expected result |
| --- | --- |
| Same user, target is a descendant (selftest) | works |
| Same user, target has `PR_SET_PTRACER` exception (observed Wine tree) | works |
| Same user, unrelated native target, Yama scope 1 | `permission_denied` (exit 5) |
| Different user, unprivileged | `permission_denied` |
| `hidepid=2` | process may be invisible; `process_not_found` |
| Yama scope 2 or 3 | `permission_denied` unless the target is traceable |
| Root | technically bypasses DAC/Yama, but **unsupported and unnecessary**; the project never requests it |

The tool does not change `ptrace_scope`, capabilities, groups or sysctls. If the
game is not traceable on a future setup, the correct outcome is a clean
permission error, not escalation.

## 6. Feature compatibility

| Feature | Compatibility |
| --- | --- |
| Process discovery | works with Wine/Proton; heuristic reasons are reported |
| Process metadata | Linux `/proc` fields only |
| Mappings | exact `/proc/<pid>/maps` snapshot |
| Modules | PE and ELF; data files excluded |
| PE headers/sections | PE32/PE32+; headers limited to 1 MiB |
| Bounded reads | 1 MiB per read; holes rejected |
| Pattern scans | PE sections only; 64 MiB default cap; configurable |
| Build SHA-256 | opt-out with `--no-hash` |
| Player listing | **not compatible / not implemented** (evidence gate) |
| Memory writes, hooks, injection | **not implemented by design** |
| Overlay UI | **not implemented by design** |
| Windows backend | **stub**, not compiled/tested on Windows |
| Lua API | **not implemented** |

## 7. Failure modes

| Symptom | Likely cause | Response |
| --- | --- | --- |
| `process_not_found` (6) | game exited / wrong PID / hidepid | run `discover` again |
| `permission_denied` (5) | target not traceable under current policy | use same user; use selftest; do not escalate |
| `unmapped_range` | address not mapped or read-protected | re-check `maps`; use module+offset |
| `read_limit` (2) | request > 1 MiB | split the read |
| `module ... is not loaded` (4) | wrong PID or module name | run `modules` |
| `pattern missing` | wrong build or wrong pattern | re-verify against the file; update the anchor |
| `mismatch` | pattern exists at another RVA | treat the build as unsupported until re-researched |
| `scan truncated` | scan cap reached before expected RVA | raise `--max-mib` deliberately |
| `insufficient_evidence` (3) | player resolver gated | read `players --explain` |

## 8. Anti-cheat / DRM / online boundary

* The project does not bypass, patch, emulate or interact with EA Anti-Cheat,
  `activation module`, Denuvo or any licensing component.
* It does not inject code, install hooks, or change game memory.
* It records whether anti-cheat/activation modules are loaded, and nothing more.
* Use offline and single-player only. Do not run it against online modes or
  services.
* The previous Live Editor compatibility patch is **out of scope** and is not
  reproduced here.

## 9. Versioning and support policy

A build is "supported" for read-only metadata when:

1. `discover` identifies the game with explicit reasons;
2. SHA-256 is recorded (known or unknown);
3. `probe` resolves the PE image and sections;
4. every anchor used by a feature verifies at its expected RVA.

Player listing will require all of the above **plus** the evidence gates in
`docs/research_roadmap.md`. Until then it is intentionally unavailable on every
build.
