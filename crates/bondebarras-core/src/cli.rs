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
}
