//! bondebarras — audit and cleanup of GitHub organization resources.

pub mod api;
pub mod auth;
pub mod billing;
pub mod clean;
pub mod cli;
pub mod commands;
pub mod model;
pub mod packages;
pub mod refs;
pub mod repos;
pub mod scan;
pub mod stale;
pub mod tui;
pub mod update;

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

    match cli.command {
        // The repository `update` queries is public, and a version check
        // must never require GitHub authentication (see the design doc,
        // §2) — so this is the one arm that never calls
        // `authenticated_client()`, unlike every other one below.
        Some(cli::Command::Update { check }) => {
            commands::update::run(check)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(cli::Command::Scan { org, json }) => {
            let client = authenticated_client().await?;
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
            packages,
            branches,
            tags,
            assets,
            stale_pr,
            older_than,
            yes,
        }) => {
            let client = authenticated_client().await?;
            let filter = commands::clean::CleanFilter {
                caches,
                artifacts,
                runs,
                packages,
                branches,
                tags,
                assets,
                stale_pr,
                older_than,
            };
            commands::clean::run(&client, &org, &repo, &filter, yes).await
        }
        None => {
            let client = authenticated_client().await?;
            let orgs = visible_orgs(&client).await?;
            let summaries = scan::overview(&client, &orgs).await;
            tui::run_tui(client, summaries).await?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Resolve a token and build the shared GitHub client. Called lazily by
/// every command except `update`, which must work without either.
///
/// Shared: the TUI hands clones to the spawned deletion tasks.
async fn authenticated_client() -> anyhow::Result<Arc<api::Client>> {
    let token = auth::resolve_token()?;
    Ok(Arc::new(api::Client::new(&token)?))
}
