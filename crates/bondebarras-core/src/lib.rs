//! bondebarras — audit and cleanup of GitHub organization resources.

pub mod api;
pub mod auth;
pub mod billing;
pub mod clean;
pub mod cli;
pub mod commands;
pub mod model;
pub mod packages;
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
            Ok(code) => code,
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

/// Every organization the token can see.
///
/// Only the callers that actually need the full listing pay for this call:
/// `clean` and `scan --org X` are already scoped by their own flags and must
/// not fail because a narrowly-scoped cron token cannot enumerate orgs.
async fn visible_orgs(client: &api::Client) -> anyhow::Result<Vec<String>> {
    Ok(client
        .get_json("/user/orgs?per_page=100")
        .await?
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|o| o["login"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

async fn run_async() -> anyhow::Result<ExitCode> {
    let cli = cli::Cli::parse();
    let token = auth::resolve_token()?;
    // Shared: the TUI hands clones to the spawned deletion tasks.
    let client = Arc::new(api::Client::new(&token)?);

    match cli.command {
        Some(cli::Command::Scan { org, json }) => {
            let targets: Vec<String> = match org {
                Some(o) => vec![o],
                None => visible_orgs(&client).await?,
            };
            commands::scan::run(&client, &targets, json).await?;
            Ok(ExitCode::SUCCESS)
        }
        Some(cli::Command::Clean {
            org,
            repo,
            caches,
            artifacts,
            runs,
            stale_pr,
            older_than,
            yes,
        }) => {
            let filter = commands::clean::CleanFilter {
                caches,
                artifacts,
                runs,
                // Task 6 adds the `--packages` clap flag and passes it
                // through here. Until it exists, no invocation can name the
                // family, so no package version is ever selected — the
                // correct behaviour, not a stopgap.
                packages: false,
                stale_pr,
                older_than,
            };
            commands::clean::run(&client, &org, &repo, &filter, yes).await
        }
        None => {
            let orgs = visible_orgs(&client).await?;
            let summaries = scan::overview(&client, &orgs).await;
            tui::run_tui(client, summaries).await?;
            Ok(ExitCode::SUCCESS)
        }
    }
}
