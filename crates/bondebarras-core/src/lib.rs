//! bondebarras — audit and cleanup of GitHub organization resources.

pub mod api;
pub mod auth;
pub mod billing;
pub mod clean;
pub mod cli;
pub mod commands;
pub mod model;
pub mod scan;
pub mod stale;
pub mod tui;

use clap::Parser;
use std::process::ExitCode;
use std::sync::Arc;

/// Entry point shared by the binary. Returns the process exit code.
pub fn run() -> ExitCode {
    match tokio::runtime::Runtime::new() {
        Ok(rt) => match rt.block_on(run_async()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Erreur : {e}");
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Erreur : impossible de démarrer l'exécuteur asynchrone — {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run_async() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    let token = auth::resolve_token()?;
    // Shared: the TUI hands clones to the spawned deletion tasks.
    let client = Arc::new(api::Client::new(&token)?);

    let orgs: Vec<String> = client
        .get_json("/user/orgs?per_page=100")
        .await?
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|o| o["login"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    match cli.command {
        Some(cli::Command::Scan { org, json }) => {
            let targets: Vec<String> = match org {
                Some(o) => vec![o],
                None => orgs,
            };
            commands::scan::run(&client, &targets, json).await?;
        }
        None => {
            let summaries = scan::overview(&client, &orgs).await;
            tui::run_tui(client, summaries).await?;
        }
    }
    Ok(())
}
