//! herdr-rss: a Herdr plugin that reads RSS, Atom, and JSON feeds in a pane.
//!
//! With no arguments the binary is the TUI, launched by Herdr as a plugin pane.
//! With a subcommand it is a CLI over the same store, for agents and scripts.
//! Two probes back the launcher script: `--launch-decision` and
//! `--open-direction`.

mod cli;
mod config;
mod feeds;
mod fetch;
mod herdr;
mod html;
mod launch;
mod parse;
mod readability;
mod store;
mod tui;

use std::io::Read;
use std::process::ExitCode;

use config::Config;
use herdr::PluginEnv;

const USAGE: &str = "usage: herdr-rss [refresh [--feed URL] [--detach] | list [--unread] [--feed URL] [--limit N] \
| show ID | mark ID... [--unread] | star ID... | add URL [--name N] [--group G] | import FILE | export] [--json]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        None => tui::run(),
        Some("--launch-decision") => launch_decision(),
        Some("--open-direction") => open_direction(),
        Some("refresh") => cli::refresh(&args[1..]),
        Some("list") => cli::list(&args[1..]),
        Some("show") => cli::show(&args[1..]),
        Some("mark") => cli::mark(&args[1..]),
        Some("star") => cli::star(&args[1..]),
        Some("add") => cli::add(&args[1..]),
        Some("import") => cli::import(&args[1..]),
        Some("export") => cli::export(&args[1..]),
        Some("--help" | "-h" | "help") => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown subcommand: {other}\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("herdr-rss: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Reads `herdr pane list` JSON on stdin and prints `OPEN`, `FOCUS <id>`, or
/// `CLOSE <id>` for the launcher script.
fn launch_decision() -> Result<(), String> {
    let mut json = String::new();
    std::io::stdin()
        .read_to_string(&mut json)
        .map_err(|e| format!("read stdin: {e}"))?;
    let root = PluginEnv::plugin_root()?;
    println!("{}", launch::decide(&json, &root)?);
    Ok(())
}

/// Prints the configured split direction for the launcher script.
fn open_direction() -> Result<(), String> {
    let env = PluginEnv::from_env()?;
    let config = Config::load(&env)?;
    println!("{}", config.open_direction);
    Ok(())
}
