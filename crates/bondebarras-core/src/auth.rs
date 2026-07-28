//! Token resolution and OAuth scope inspection.

use anyhow::{Context, Result, bail};
use std::process::Command;

/// OAuth scopes granted to the active token, read from `X-OAuth-Scopes`.
#[derive(Debug, Clone, Default)]
pub struct Scopes(Vec<String>);

impl Scopes {
    /// Parse the comma-separated header value GitHub returns on every response.
    pub fn parse(header: &str) -> Self {
        Scopes(
            header
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        )
    }

    pub fn has(&self, scope: &str) -> bool {
        self.0.iter().any(|s| s == scope)
    }
}

/// Repository deletion is the one v0.1-adjacent operation that needs a scope
/// beyond `repo`. The TUI greys the action out rather than failing at delete
/// time.
pub fn can_delete_repo(scopes: &Scopes) -> bool {
    scopes.has("delete_repo")
}

/// Resolve a token: `gh auth token` first, then `$GITHUB_TOKEN`.
///
/// Reusing the `gh` session means zero configuration for users who already
/// have the CLI logged in, which is the common case.
pub fn resolve_token() -> Result<String> {
    if let Ok(output) = Command::new("gh").args(["auth", "token"]).output()
        && output.status.success()
    {
        let token = String::from_utf8(output.stdout)
            .context("`gh auth token` a renvoyé une sortie non-UTF-8")?
            .trim()
            .to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            return Ok(token.to_string());
        }
    }

    bail!(
        "Erreur : aucun jeton GitHub trouvé.\n\
         Connectez-vous avec `gh auth login`, ou définissez la variable \
         d'environnement GITHUB_TOKEN."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_comma_separated_scope_header() {
        let s = Scopes::parse("repo, workflow, delete:packages");
        assert!(s.has("repo"));
        assert!(s.has("workflow"));
        assert!(s.has("delete:packages"));
        assert!(!s.has("delete_repo"));
    }

    #[test]
    fn an_empty_header_yields_no_scopes() {
        let s = Scopes::parse("");
        assert!(!s.has("repo"));
    }

    #[test]
    fn repo_deletion_needs_its_own_scope() {
        assert!(!can_delete_repo(&Scopes::parse("repo, workflow")));
        assert!(can_delete_repo(&Scopes::parse("repo, delete_repo")));
    }
}
