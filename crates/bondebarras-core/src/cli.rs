//! Command-line surface. With no subcommand, bondebarras opens the TUI.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "bondebarras",
    version,
    about = "Audit et nettoyage des orgs GitHub"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Affiche l'état des organisations sans rien supprimer.
    Scan {
        /// Limite le scan à une organisation.
        #[arg(long)]
        org: Option<String>,
        /// Sortie JSON sur stdout, pour un pipeline machine.
        #[arg(long)]
        json: bool,
    },
    /// Supprime des ressources sans interface. Sans `--yes`, affiche le plan
    /// sans rien toucher.
    Clean {
        /// Organisation ciblée.
        #[arg(long)]
        org: String,
        /// Dépôt ciblé.
        #[arg(long)]
        repo: String,
        /// Inclut les caches Actions.
        #[arg(long)]
        caches: bool,
        /// Inclut les artifacts.
        #[arg(long)]
        artifacts: bool,
        /// Inclut les workflow runs.
        #[arg(long)]
        runs: bool,
        /// Restreint aux ressources rattachées à une PR fermée.
        #[arg(long = "stale-pr")]
        stale_pr: bool,
        /// Restreint aux ressources d'au moins N jours.
        #[arg(long = "older-than")]
        older_than: Option<i64>,
        /// Confirme sans interaction. Sans lui, rien n'est supprimé.
        #[arg(long)]
        yes: bool,
    },
}
