use ksef::app::App;
use ksef::config::{self, Config};
use ksef::ksef::Env;
use ksef::server;
use ksef::store::Store;
use ksef::{editor, job, shell};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const USAGE: &str = "\
usage: ksef [command]
  (none) [--no-open]             start the page in the browser (invoices, sending, KSeF) + a small menu here
         [--test-envs]           also offer the KSeF test and demo environments (default: production only)
  config                         edit config.json (seller, buyers, defaults)
  token [--env E]                store a KSeF token in the macOS Keychain (default prod; E: test|demo|prod)
  test-nip                       show/create the fake NIP for the test environment
  run <job.json | ->             one JSON action in, JSON result out (see tutorials/05-json-mode.md)
  paths                          where config, invoices and PDFs live";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json_mode = args.first().is_some_and(|a| a == "run");
    if let Err(message) = run(args) {
        if json_mode {
            println!("{}", serde_json::json!({"ok": false, "error": message}));
        } else {
            eprintln!("error: {message}");
        }
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is not set")?;
    // `ksef --no-open` is the default command with a flag.
    let (command, rest) = match args.split_first() {
        Some((c, r)) if !c.starts_with("--") => (c.as_str(), r),
        _ => ("", &args[..]),
    };
    let opts = Opts::parse(rest)?;
    let interactive = matches!(command, "" | "config");
    match command {
        "help" | "-h" | "--help" => {
            println!("{USAGE}");
            return Ok(());
        }
        "paths" => return paths(&home),
        _ => {}
    }
    ensure_config(&home, interactive)?;
    let mut app = App::load(&home)?;
    match command {
        "" => serve(&home, &app, !opts.no_open, &opts.envs()),
        "config" => shell::edit_config(&mut app),
        "token" => shell::token(&mut app, opts.env, &opts.envs()),
        "test-nip" => {
            let nip = app.ensure_test_nip()?;
            println!("{nip}");
            Ok(())
        }
        "run" => {
            let source = opts.arg(0).ok_or("usage: ksef run <job.json | ->")?;
            let text = if source == "-" {
                let mut text = String::new();
                std::io::stdin()
                    .read_to_string(&mut text)
                    .map_err(|e| e.to_string())?;
                text
            } else {
                ksef::store::read(Path::new(source))?
            };
            let result = job::run(&app, job::parse(&text)?);
            println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
            if result["ok"] == true {
                Ok(())
            } else {
                std::process::exit(1)
            }
        }
        other => Err(format!("unknown command '{other}'\n{USAGE}")),
    }
}

fn serve(home: &Path, app: &App, open: bool, envs: &[Env]) -> Result<(), String> {
    shell::warn_config(&app.config);
    let home_for_server = home.to_path_buf();
    let url = server::start(server::Site {
        load: Arc::new(move || App::load(&home_for_server)),
        envs: envs.to_vec(),
    })
    .map_err(|e| format!("cannot start the local server: {e}"))?;
    if open {
        shell::open_browser(&url)?;
    }
    shell::menu(home, &url, envs)
}

/// First run: write the example config and open it, so the next steps have real data.
fn ensure_config(home: &Path, interactive: bool) -> Result<(), String> {
    let store = Store::locate(home);
    let path = store.config_path();
    if path.exists() {
        return Ok(());
    }
    if !interactive {
        return Err(format!(
            "no config yet at {}: run `ksef config` first",
            path.display()
        ));
    }
    ksef::store::write(&path, config::EXAMPLE.as_bytes())?;
    println!(
        "Created {} with example data. Replace it with your details (seller = you, buyers = your clients).",
        path.display()
    );
    editor::edit(&path, None)?;
    Config::load(&path).map(|_| ())
}

fn paths(home: &Path) -> Result<(), String> {
    let store = Store::locate(home);
    println!("data:    {}  (set KSEF_HOME to move it)", store.root().display());
    println!("config:  {}", store.config_path().display());
    println!("invoices:{}", store.root().join("invoices").display());
    if let Ok(config) = Config::load(&store.config_path()) {
        println!("PDFs:    {}", config.output_dir(home).display());
    }
    Ok(())
}

#[derive(Default)]
struct Opts {
    positional: Vec<String>,
    env: Option<Env>,
    no_open: bool,
    test_envs: bool,
}

impl Opts {
    fn parse(args: &[String]) -> Result<Opts, String> {
        let mut opts = Opts::default();
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            let mut value = || it.next().ok_or_else(|| format!("{arg} needs a value"));
            match arg.as_str() {
                "--env" | "-e" => opts.env = Some(Env::parse(value()?)?),
                "--no-open" => opts.no_open = true,
                "--test-envs" => opts.test_envs = true,
                flag if flag.starts_with("--") => return Err(format!("unknown option {flag}\n{USAGE}")),
                _ => opts.positional.push(arg.clone()),
            }
        }
        Ok(opts)
    }

    /// Production only, unless `--test-envs`: test and demo matter only while trying things out.
    fn envs(&self) -> Vec<Env> {
        if self.test_envs {
            Env::ALL.to_vec()
        } else {
            vec![Env::Prod]
        }
    }

    fn arg(&self, i: usize) -> Option<&str> {
        self.positional.get(i).map(String::as_str)
    }
}
