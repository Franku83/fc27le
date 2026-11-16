//! End-to-end tests for the controlled selftest and JSON CLI contract.

use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_fc27le")
}

#[test]
fn selftest_passes_and_is_json_serializable() {
    let output = Command::new(binary())
        .args(["selftest", "--json"])
        .output()
        .expect("run fc27le selftest");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "selftest failed (status {:?})\nstdout:\n{stdout}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("selftest stdout must be valid JSON");
    assert_eq!(value["passed"], serde_json::Value::Bool(true));

    let checks = value["checks"]
        .as_array()
        .expect("checks array must be present");
    assert!(checks.len() >= 8, "expected at least 8 checks");
    for required in [
        "magic_bytes_match",
        "pattern_bytes_match",
        "oversized_read_refused",
        "unmapped_read_refused",
        "module_enumeration",
        "process_discovery",
    ] {
        let check = checks
            .iter()
            .find(|check| check["name"] == required)
            .unwrap_or_else(|| panic!("missing check {required}"));
        assert_eq!(
            check["passed"],
            serde_json::Value::Bool(true),
            "check {required} did not pass: {check}"
        );
    }
}

#[test]
fn discover_emits_valid_json() {
    let output = Command::new(binary())
        .args(["discover", "--json"])
        .output()
        .expect("run fc27le discover");

    assert!(
        output.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("valid JSON");
    assert!(value["process_count"].as_u64().unwrap_or(0) > 0);
    assert!(value["candidates"].is_array());
}

#[test]
fn capabilities_lists_verified_player_reader() {
    let output = Command::new(binary())
        .args(["capabilities", "--json"])
        .output()
        .expect("run fc27le capabilities");
    assert!(output.status.success());

    let value: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("valid JSON");
    let capabilities = value["capabilities"].as_array().expect("array");
    let player = capabilities
        .iter()
        .find(|item| item["id"] == "player-listing")
        .expect("player-listing capability");
    assert_eq!(player["status"], "implemented_read_only");
    assert!(
        player["note"]
            .as_str()
            .unwrap_or_default()
            .contains("editedplayernames")
    );

    let names = capabilities
        .iter()
        .find(|item| item["id"] == "player-name-resolution")
        .expect("player-name-resolution capability");
    assert_eq!(names["status"], "implemented_read_only");
    assert!(
        names["note"]
            .as_str()
            .unwrap_or_default()
            .contains("No name is guessed")
    );
}

#[test]
fn oversized_read_is_rejected_before_touching_memory() {
    let output = Command::new(binary())
        .args([
            "read",
            "1",
            "--address",
            "0x0",
            "--length",
            "1048577",
            "--json",
        ])
        .output()
        .expect("run fc27le read");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("safety limit") || stderr.contains("read_limit"),
        "unexpected stderr: {stderr}"
    );
}
