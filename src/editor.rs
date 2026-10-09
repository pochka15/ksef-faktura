//! Opens a file for editing and waits until the user is done: the configured command (e.g. `vim`) runs in
//! this terminal; without one, TextEdit opens and we wait for Enter.

use std::io::Write;
use std::path::Path;
use std::process::Command;

pub fn edit(path: &Path, editor: Option<&str>) -> Result<(), String> {
    match editor.map(str::trim).filter(|e| !e.is_empty()) {
        Some(command) => {
            // Through `sh` so commands with flags work: "code --wait", "subl -w".
            let status = Command::new("sh")
                .arg("-c")
                .arg(format!("{command} \"$1\""))
                .arg("sh")
                .arg(path)
                .status()
                .map_err(|e| format!("running {command}: {e}"))?;
            if !status.success() {
                return Err(format!("{command} exited with {status}"));
            }
        }
        None => {
            Command::new("open")
                .args(["-e"])
                .arg(path)
                .status()
                .map_err(|e| format!("opening TextEdit: {e}"))?;
            print!("Opened in TextEdit. Save (⌘S), then press Enter here to continue... ");
            let _ = std::io::stdout().flush();
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
