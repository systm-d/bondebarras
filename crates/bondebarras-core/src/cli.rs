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
        /// Inclut les versions de packages (conteneurs). GitHub n'en expose
        /// pas la taille : aucun octet n'est promis pour cette famille.
        #[arg(long)]
        packages: bool,
        /// Inclut les branches mergées. Une branche vivante — par défaut,
        /// protégée, ou sans PR mergée derrière elle — n'est jamais prise en
        /// masse, drapeau ou pas.
        #[arg(long)]
        branches: bool,
        /// Inclut les tags. Un tag n'est jamais présélectionnable en masse :
        /// c'est ce sur quoi pointent les releases, `go get`, `Cargo.toml`.
        #[arg(long)]
        tags: bool,
        /// Inclut les assets de releases. La release elle-même n'est jamais
        /// supprimée — seuls ses binaires le sont.
        #[arg(long)]
        assets: bool,
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
    /// Vérifie s'il existe une version plus récente, et propose ou applique
    /// la mise à jour selon la manière dont bondebarras a été installé. Ne
    /// requiert aucun jeton GitHub : le dépôt est public.
    Update {
        /// N'affiche que la disponibilité d'une nouvelle version ; n'installe
        /// et ne propose rien.
        #[arg(long)]
        check: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean_packages_flag(args: &[&str]) -> bool {
        let mut full = vec!["bondebarras", "clean", "--org", "o", "--repo", "r"];
        full.extend_from_slice(args);
        match Cli::try_parse_from(full).unwrap().command {
            Some(Command::Clean { packages, .. }) => packages,
            other => panic!("expected Command::Clean, got {other:?}"),
        }
    }

    #[test]
    fn packages_joins_the_family_flags() {
        assert!(clean_packages_flag(&["--packages"]));
    }

    #[test]
    fn packages_absent_defaults_to_false() {
        // Like every other family flag, naming no family must select
        // nothing — `--packages` is not an exception that defaults to true.
        assert!(!clean_packages_flag(&[]));
    }

    fn clean_v04_flags(args: &[&str]) -> (bool, bool, bool) {
        let mut full = vec!["bondebarras", "clean", "--org", "o", "--repo", "r"];
        full.extend_from_slice(args);
        match Cli::try_parse_from(full).unwrap().command {
            Some(Command::Clean {
                branches,
                tags,
                assets,
                ..
            }) => (branches, tags, assets),
            other => panic!("expected Command::Clean, got {other:?}"),
        }
    }

    #[test]
    fn branches_tags_and_assets_join_the_family_flags() {
        assert_eq!(clean_v04_flags(&["--branches"]), (true, false, false));
        assert_eq!(clean_v04_flags(&["--tags"]), (false, true, false));
        assert_eq!(clean_v04_flags(&["--assets"]), (false, false, true));
    }

    #[test]
    fn branches_tags_and_assets_absent_default_to_false() {
        // Like every other family flag, naming none of the three must select
        // nothing — not "everything v0.4 added".
        assert_eq!(clean_v04_flags(&[]), (false, false, false));
    }

    #[test]
    fn branches_tags_and_assets_are_cumulative_with_each_other() {
        assert_eq!(
            clean_v04_flags(&["--branches", "--assets"]),
            (true, false, true)
        );
    }

    fn update_check_flag(args: &[&str]) -> bool {
        let mut full = vec!["bondebarras", "update"];
        full.extend_from_slice(args);
        match Cli::try_parse_from(full).unwrap().command {
            Some(Command::Update { check }) => check,
            other => panic!("expected Command::Update, got {other:?}"),
        }
    }

    #[test]
    fn update_check_defaults_to_false() {
        assert!(!update_check_flag(&[]));
    }

    #[test]
    fn update_check_flag_is_parsed() {
        assert!(update_check_flag(&["--check"]));
    }
}
