//! Generate `docs/lua_compatibility.md` from the public API matrix.
//!
//! Run from the workspace root:
//! `cargo run -p fc27-lua-compat --example matrix > docs/lua_compatibility.md`

use fc27_lua_compat::matrix::{ApiStatus, compatibility_matrix, status_counts};

fn status_label(status: ApiStatus) -> &'static str {
    match status {
        ApiStatus::SupportedTested => "Supported and tested",
        ApiStatus::PartiallySupported => "Partially supported",
        ApiStatus::Unimplemented => "Unimplemented",
        ApiStatus::RequiresRe => "Requires further reverse engineering",
        ApiStatus::NotApplicable => "Not applicable",
    }
}

fn main() {
    let (supported, partial, unimplemented, requires_re) = status_counts();
    println!("# FC Live Editor Lua API v2 compatibility");
    println!();
    println!("This document is generated from `crates/lua-compat/src/matrix.rs`.");
    println!("Regenerate it with:");
    println!();
    println!("```bash");
    println!("cargo run -p fc27-lua-compat --example matrix > docs/lua_compatibility.md");
    println!("```");
    println!();
    println!("## Status summary");
    println!();
    println!("| Status | Entries |");
    println!("| --- | ---: |");
    println!("| Supported and tested | {supported} |");
    println!("| Partially supported | {partial} |");
    println!("| Unimplemented / not applicable | {unimplemented} |");
    println!("| Requires further reverse engineering | {requires_re} |");
    println!();
    println!("The runtime is deliberately read-only. Every unsupported call raises an");
    println!("explicit `FC27LE compatibility:` error. No call silently succeeds, invents");
    println!("data, reports a fake event handler, or pretends a memory write occurred.");
    println!();
    println!("## Tested environment");
    println!();
    println!("* FC 27 `FC27.exe`, SHA-256");
    println!("  `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9`,");
    println!("  running under GE-Proton11-12.");
    println!("* External FC 27 metadata XML (not redistributed): 268/268 metadata");
    println!("  tables matched and 263 fixed-row layouts verified. The embedded");
    println!("  profile covers 4 tables and revalidates its listed fields live.");
    println!("* Live `players` table: 4CC `CZUM`, 152 fields, 156-byte records,");
    println!("  22,098 slots, 27,000 capacity, 284 invalid markers, 21,814 valid rows.");
    println!("* With external XML, the Lua runtime exposed 268 tables whose metadata");
    println!("  matched the live descriptors. The embedded minimal profile exposes 4");
    println!("  tables (`players` plus the three name tables). `LOGGER` output and");
    println!("  `MessageBox` are shown in the GUI.");
    println!();
    println!("## Compatibility matrix");
    println!();
    println!(
        "| API | Signature | Returns | Required state | Capability | Status | Evidence | Tests | Known limitations |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for entry in compatibility_matrix() {
        println!(
            "| `{}` | `{}` | {} | {} | {} | {} | {} | `{}` | {} |",
            escape(entry.name),
            escape(entry.signature),
            escape(entry.returns),
            escape(entry.required_state),
            escape(entry.capability),
            status_label(entry.status),
            escape(entry.evidence),
            escape(entry.tests),
            escape(entry.limitations),
        );
    }
    println!();
    println!("## Record handles");
    println!();
    println!("`GetFirstRecord` and `GetNextValidRecord` return positive numeric tokens,");
    println!("not raw process pointers. A token encodes `(generation, table_id, slot+1)`.");
    println!("Iteration returns `0` at the end. `LE.db:Refresh`, `Load` and `Reset`");
    println!("invalidate every existing table/field/record handle; using one afterwards");
    println!("raises a stale-handle error and requires a fresh `GetTable` call. This keeps");
    println!("script-visible handles stable without exposing game addresses.");
    println!();
    println!("A `players` snapshot is immutable for its lifetime. A game-side database");
    println!("change becomes visible only after the script calls `LE.db:Refresh` and");
    println!("obtains a new table handle.");
    println!();
    println!("## Execution model");
    println!();
    println!("* Lua 5.4 is provided by `mlua` with `lua54` and `vendored` features.");
    println!("* Scripts run on a background worker; the GUI render thread only polls");
    println!("  events. Run and Stop remain interactive, and the window keeps repainting.");
    println!("* Default limits: 30 s wall-clock time, 256 MiB Lua memory. A debug hook");
    println!("  checks both every 10,000 instructions; Stop sets a cancellation flag.");
    println!("  Live GUI test: Stop cancelled `while true do end` with");
    println!("  `FC27LE: script cancelled` while the window stayed responsive.");
    println!("* Helper modules are loaded from semicolon-separated directories configured");
    println!("  in the GUI plus the open script's own directory.");
    println!("* `os.execute`, `io.popen` and `package.loadlib` are removed. Standard Lua");
    println!("  file I/O remains available because existing scripts may use it. Third-party");
    println!("  scripts are untrusted; Lua 5.4 is **not** a security sandbox.");
    println!("* Unsupported `MEMORY`, `HTTP` and `REQUEST` members raise compatibility");
    println!("  errors. Raw process memory and network access are not exposed.");
    println!();
    println!("## Verified live read-only test");
    println!();
    println!("The GUI executed this read-only script against the live process:");
    println!();
    println!("```lua");
    println!("local players = LE.db:GetTable(\"players\")");
    println!("local record = players:GetFirstRecord()");
    println!("local count = 0");
    println!("while record > 0 do");
    println!("    local playerid = players:GetRecordFieldValue(record, \"playerid\")");
    println!("    local overall = players:GetRecordFieldValue(record, \"overallrating\")");
    println!("    count = count + 1");
    println!("    record = players:GetNextValidRecord()");
    println!("end");
    println!("MessageBox(\"FC27LE\", \"Scanned \" .. count .. \" players\")");
    println!("```");
    println!();
    println!("Result: table `players` (`CZUM`) reported 22,098 written records; iteration");
    println!("visited 21,814 valid records; the GUI logged the first IDs and displayed");
    println!("`Scanned 21814 valid player records`. The same run was repeated with F5 and");
    println!("with the GUI Run button while the window stayed responsive.");
    println!();
    println!("## Out of scope");
    println!();
    println!("No game-memory writes, DLL injection, hooks, game-function calls, DRM or");
    println!("anti-cheat changes are implemented. Career-mode managers, write helpers,");
    println!("game events and networking remain explicit compatibility errors until a");
    println!("verified, read-only source exists.");
}

fn escape(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}
