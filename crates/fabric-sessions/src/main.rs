//! `fabric-sessions`: the Fabric Sessions service's binary (SB-397). In W1 it only names itself;
//! W2 gives it the owner, the socket API and its MCP server. Plan:
//! `docs/plans/2026-11-16-session-service/plan.md`.
use std::process::ExitCode;

const USAGE: &str =
    "Fabric Sessions: one always-on service that owns coding sessions apart from every app.

Usage: fabric-sessions [--version | --help]

Nothing runs yet: the service arrives in a later release.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["--version"] | ["-V"] => {
            println!("fabric-sessions {}", fabric_sessions_core::VERSION);
            ExitCode::SUCCESS
        }
        [] | ["--help"] | ["-h"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
