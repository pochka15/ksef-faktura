//! The terminal side: while the browser page does invoices, sending and KSeF lookups, the terminal keeps
//! what needs a terminal: editing config.json (vim) and pasting a token into the Keychain (hidden input).

use crate::app::App;
use crate::config::Config;
use crate::editor;
use crate::keychain;
use crate::ksef::Env;
use std::io::Write;
use std::path::Path;

const MENU: &str = "  o  open the page in the browser
  c  edit config (seller, buyers, defaults)
  t  store a KSeF token in the Keychain
  q  quit";

/// Runs until `q` or end of input; the server keeps serving in the background meanwhile.
/// `envs`: the KSeF environments in use (only prod unless `--test-envs`).
pub fn menu(home: &Path, url: &str, envs: &[Env]) -> Result<(), String> {
    println!("\nksef is running at {url}\n(the page stops working when you quit here)");
    loop {
        println!("\n{MENU}");
        let Some(choice) = ask("ksef> ")? else {
            return Ok(());
        };
        let result = match choice.as_str() {
            "o" | "open" => open_browser(url),
            "c" | "config" => App::load(home).and_then(|mut app| edit_config(&mut app)),
            "t" | "token" => App::load(home).and_then(|mut app| token(&mut app, None, envs)),
            "q" | "quit" | "exit" => return Ok(()),
            "" => Ok(()),
            other => Err(format!("unknown choice '{other}'")),
        };
        if let Err(e) = result {
            println!("error: {e}");
        }
    }
}

pub fn open_browser(url: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(url)
        .status()
        .map(|_| ())
        .map_err(|e| format!("open: {e}"))
}

/// One trimmed line from stdin; `None` at end of input (^D).
fn ask(prompt: &str) -> Result<Option<String>, String> {
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let n = std::io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
    Ok((n > 0).then(|| line.trim().to_string()))
}

/// Opens config.json until it parses; reloads it into `app`.
pub fn edit_config(app: &mut App) -> Result<(), String> {
    let path = app.store.config_path();
    loop {
        editor::edit(&path, app.config.editor.as_deref())?;
        match Config::load(&path) {
            Ok(config) => {
                app.config = config;
                warn_config(&app.config);
                println!("config saved: {}", path.display());
                return Ok(());
            }
            Err(e) => {
                println!("error: {e}");
                let again = ask("open it again to fix? [Y/n] ")?.unwrap_or_default();
                if again.eq_ignore_ascii_case("n") {
                    return Err("config.json is still broken; the previous version stays in use".into());
                }
            }
        }
    }
}

pub fn warn_config(config: &Config) {
    for p in config.problems() {
        println!("config warning: {p}");
    }
}

/// Asks which environment only when more than one is in use.
pub fn token(app: &mut App, env: Option<Env>, envs: &[Env]) -> Result<(), String> {
    let env = match (env, envs) {
        (Some(env), _) => env,
        (None, [only]) => *only,
        (None, _) => {
            let answer = ask("token for which environment (test/demo/prod): ")?.ok_or("cancelled")?;
            if answer.is_empty() {
                return Err("no environment given, nothing stored".into());
            }
            Env::parse(&answer)?
        }
    };
    if env == Env::Test {
        let nip = app.ensure_test_nip()?;
        println!("Your NIP for the test environment: {nip} (test_nip in config.json)");
        println!("In the web app choose \"Zaloguj uwierzytelnieniem testowym\" and use this NIP everywhere.");
    }
    println!(
        "Generate a token in {} (Tokeny > Generuj token, permissions: wystawianie i przeglądanie faktur).",
        env.web_app_url()
    );
    if keychain::has_token(env) {
        println!("(a {env} token is already stored; this replaces it)");
    }
    keychain::store(env)?;
    println!("stored in the Keychain as service 'ksef-cli', account '{env}'");
    Ok(())
}
