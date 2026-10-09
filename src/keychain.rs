//! KSeF tokens live in the macOS Keychain (service `ksef-cli`, account = environment). The app never
//! writes them to disk; `security` prompts for the value itself, so it never passes through this process.
//! `KSEF_TOKEN` overrides the Keychain, e.g. for one-off runs on another machine.

use crate::ksef::Env;
use std::process::{Command, Stdio};

const SERVICE: &str = "ksef-cli";

pub fn token(env: Env) -> Result<String, String> {
    if let Some(token) = std::env::var("KSEF_TOKEN").ok().filter(|t| !t.trim().is_empty()) {
        return Ok(token);
    }
    let output = Command::new("security")
        .args(["find-generic-password", "-s", SERVICE, "-a", env.name(), "-w"])
        .output()
        .map_err(|e| format!("cannot run `security` ({e}); set KSEF_TOKEN instead"))?;
    if !output.status.success() {
        return Err(format!(
            "no KSeF {env} token in the Keychain: run `ksef token --env {env}` (see tutorials/)"
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Interactive: `security` asks for the token twice (hidden) and stores or replaces it.
pub fn store(env: Env) -> Result<(), String> {
    println!("Paste the KSeF {env} token when asked (input is hidden, asked twice).");
    let status = Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-s",
            SERVICE,
            "-a",
            env.name(),
            "-l",
            &format!("KSeF token ({env})"),
            "-w",
        ])
        .stdin(Stdio::inherit())
        .status()
        .map_err(|e| format!("cannot run `security`: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("the Keychain did not store the token (passwords did not match?)".into())
    }
}

pub fn has_token(env: Env) -> bool {
    Command::new("security")
        .args(["find-generic-password", "-s", SERVICE, "-a", env.name()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
