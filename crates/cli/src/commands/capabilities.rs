use fc27_core::Result;
use fc27_game::capabilities::CAPABILITIES;
use serde::Serialize;

use crate::output::print_json;

#[derive(Debug, Serialize)]
struct CapabilitiesOutput {
    capabilities: &'static [fc27_game::capabilities::Capability],
    policy: &'static str,
}

pub fn run(json: bool) -> Result<u8> {
    let output = CapabilitiesOutput {
        capabilities: CAPABILITIES,
        policy: "Read-only. No memory writes, injection, hooks, DRM/anti-cheat changes or \
                 binary patching are implemented.",
    };

    if json {
        print_json(&output)?;
        return Ok(0);
    }

    println!("{:<28} {:<26} {:<18} note", "id", "capability", "status");
    for capability in CAPABILITIES {
        println!(
            "{:<28} {:<26} {:<18} {}",
            capability.id,
            capability.name,
            format!("{:?}", capability.status),
            capability.note
        );
    }
    println!("\n{}", output.policy);

    Ok(0)
}
