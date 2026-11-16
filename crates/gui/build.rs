use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=FC27LE_GIT_COMMIT");
    if let Ok(commit) = std::env::var("FC27LE_GIT_COMMIT") {
        if !commit.trim().is_empty() {
            println!("cargo:rustc-env=FC27LE_GIT_COMMIT={}", commit.trim());
            return;
        }
    }
    let output = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !commit.is_empty() {
                println!("cargo:rustc-env=FC27LE_GIT_COMMIT={commit}");
            }
        }
    }
}
