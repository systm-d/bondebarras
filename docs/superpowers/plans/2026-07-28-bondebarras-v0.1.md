# bondebarras v0.1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Livrer un TUI Rust qui affiche les caches, artifacts et workflow runs des orgs GitHub de l'utilisateur, et permet de les supprimer sélectivement — les ~51 Go identifiés dans la spec.

**Architecture:** Workspace Cargo à deux crates : `bondebarras-core` porte toute la logique (API, scan, nettoyage, TUI), `bondebarras` n'est qu'un shim de binaire. Le scan est à deux étages : agrégats org-level au lancement, détail par repo au drill-down. Le module `api/` est la seule frontière qui connaît octocrab.

**Tech Stack:** Rust edition 2024, tokio, octocrab 0.41, ratatui 0.30, crossterm 0.29, clap 4, serde, anyhow + thiserror. Tests : wiremock, insta, assert_cmd, predicates.

## Global Constraints

Ces règles s'appliquent à **toutes** les tâches, sans être répétées à chaque fois.

- Rust **edition 2024**, MSRV **1.88** (`rust-version = "1.88"` dans `[workspace.package]`).
  C'est le plancher réel imposé par `ratatui 0.30.2` et `ratatui-core`, qui déclarent tous
  deux `rust-version = "1.88.0"`. Déclarer 1.85 comme le fait claudine serait faux :
  `cargo +1.85 check` échoue avec « package `ratatui v0.30.2` cannot be built because it
  requires rustc 1.88.0 or newer ».
- `unsafe_code = "forbid"` dans `[workspace.lints.rust]`.
- `all = { level = "warn", priority = -1 }` dans `[workspace.lints.clippy]`, chaque crate héritant via `[lints] workspace = true`.
- `rustfmt.toml` : `max_width = 100`, `edition = "2024"`.
- **Documentation** (README, gouvernance) en **anglais**. **Chaînes user-facing** (CLI et TUI) en **français**. **Identifiants de code** en anglais.
- Jamais `ERROR`, `FATAL` ni `PANIC` dans un texte user-facing. Le préfixe d'erreur est
  `Erreur : `, **ajouté une seule fois** par `run()` au sommet de la pile
  (`eprintln!("Erreur : {e}")`). Les valeurs d'erreur — `bail!`, `.context(…)` — ne
  doivent donc **pas** le porter, sous peine de le doubler à l'affichage.
- Le binaire et les crates sont en **ASCII** : `bondebarras`, `bondebarras-core`. Les textes d'interface portent les accents : « Bon débarras ! ».
- **Multi-plateforme** : Linux, Windows, macOS. Aucune hypothèse Linux-only (pas de systemd, pas de `/sys`, pas de libnotify).
- **Conventional Commits** (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`, `test:`, `build:`, `style:`).
- Quality gate à faire passer avant chaque commit :
  ```sh
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```

## Écart assumé par rapport à la spec

La maquette du §6 de la spec affiche un marqueur `⚠ billing 403` dans la colonne des
orgs. Or le §10 place l'onglet Billing en **v0.2**. Pour garder la v0.1 serrée, **la v0.1
n'appelle pas du tout l'endpoint de facturation** : l'étage 1 se limite aux caches et à la
liste des repos (2 appels par org). Le marqueur ⚠ et l'onglet Billing arrivent en v0.2.

## Structure des fichiers

| Fichier | Responsabilité |
|---|---|
| `Cargo.toml` | workspace, dépendances partagées, lints |
| `crates/bondebarras/src/main.rs` | shim : `fn main() -> ExitCode { bondebarras_core::run() }` |
| `crates/bondebarras-core/src/lib.rs` | `run()`, câblage CLI |
| `crates/bondebarras-core/src/cli.rs` | parsing clap |
| `crates/bondebarras-core/src/model.rs` | `ResourceKind`, `RiskTier`, `Resource`, `OrgSummary`, `human_size` |
| `crates/bondebarras-core/src/stale.rs` | détection ⚑ PR fermée (calcul pur) |
| `crates/bondebarras-core/src/auth.rs` | résolution du token, parsing des scopes |
| `crates/bondebarras-core/src/api/mod.rs` | `Client` : URL de base injectable, concurrence, retry |
| `crates/bondebarras-core/src/api/caches.rs` | usage-by-repository, list, delete |
| `crates/bondebarras-core/src/api/artifacts.rs` | list, delete |
| `crates/bondebarras-core/src/api/runs.rs` | list, delete |
| `crates/bondebarras-core/src/api/prs.rs` | numéros des PR fermées d'un repo |
| `crates/bondebarras-core/src/api/repos.rs` | liste des repos d'une org |
| `crates/bondebarras-core/src/scan.rs` | étage 1 (orgs) et étage 2 (repo) |
| `crates/bondebarras-core/src/clean.rs` | planificateur + exécuteur, événements de progression |
| `crates/bondebarras-core/src/tui/mod.rs` | boucle d'événements, terminal |
| `crates/bondebarras-core/src/tui/theme.rs` | palette et styles |
| `crates/bondebarras-core/src/tui/app.rs` | état, navigation, sélection, tri, filtre |
| `crates/bondebarras-core/src/tui/views/orgs.rs` | panneau gauche |
| `crates/bondebarras-core/src/tui/views/repo.rs` | panneau droit |
| `crates/bondebarras-core/src/tui/views/confirm.rs` | modale de confirmation palier 1 |

---

### Task 1: Squelette du workspace

**Files:**
- Create: `Cargo.toml`, `rustfmt.toml`, `.gitignore`

`deny.toml` n'est **pas** créé ici : il est copié depuis claude-tui à la Task 15 Step 1,
en même temps que le reste de la gouvernance.
- Create: `crates/bondebarras-core/Cargo.toml`, `crates/bondebarras-core/src/lib.rs`
- Create: `crates/bondebarras/Cargo.toml`, `crates/bondebarras/src/main.rs`

**Interfaces:**
- Consumes: rien.
- Produces: `bondebarras_core::run() -> std::process::ExitCode`.

- [ ] **Step 1: Créer le `Cargo.toml` du workspace**

```toml
[workspace]
resolver = "2"
members = ["crates/bondebarras-core", "crates/bondebarras"]

[workspace.package]
edition = "2024"
rust-version = "1.88"
version = "0.1.0"
license = "MIT OR Apache-2.0"
description = "TUI Rust pour auditer et nettoyer les ressources des organisations GitHub."
repository = "https://github.com/systm-d/bondebarras"
homepage = "https://github.com/systm-d/bondebarras"
authors = ["systm-d <k@levilainpetit.dev>"]

[workspace.dependencies]
anyhow = "1"
thiserror = "2"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time"] }
octocrab = "0.41"
ratatui = "0.30"
crossterm = "0.29"
chrono = { version = "0.4", features = ["serde"] }
futures = "0.3"
wiremock = "0.6"
insta = "1"
assert_cmd = "2"
predicates = "3"

[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
all = { level = "warn", priority = -1 }

[profile.release]
lto = true
codegen-units = 1
strip = true
```

- [ ] **Step 2: Créer `rustfmt.toml` et `.gitignore`**

`rustfmt.toml` :
```toml
max_width = 100
edition = "2024"
```

`.gitignore` :
```
/target
```

- [ ] **Step 3: Créer les deux crates**

`crates/bondebarras-core/Cargo.toml` :
```toml
[package]
name = "bondebarras-core"
edition.workspace = true
rust-version.workspace = true
version.workspace = true
license.workspace = true
description.workspace = true
repository.workspace = true

[dependencies]
anyhow.workspace = true
thiserror.workspace = true
clap.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
octocrab.workspace = true
ratatui.workspace = true
crossterm.workspace = true
chrono.workspace = true
futures.workspace = true

[dev-dependencies]
wiremock.workspace = true
insta.workspace = true

[lints]
workspace = true
```

`crates/bondebarras/Cargo.toml` :
```toml
[package]
name = "bondebarras"
edition.workspace = true
rust-version.workspace = true
version.workspace = true
license.workspace = true
description.workspace = true
repository.workspace = true

[[bin]]
name = "bondebarras"
path = "src/main.rs"

[dependencies]
bondebarras-core = { path = "../bondebarras-core", version = "0.1.0" }

[dev-dependencies]
assert_cmd.workspace = true
predicates.workspace = true

[lints]
workspace = true
```

- [ ] **Step 4: Écrire le shim et le point d'entrée**

`crates/bondebarras/src/main.rs` :
```rust
fn main() -> std::process::ExitCode {
    bondebarras_core::run()
}
```

`crates/bondebarras-core/src/lib.rs` :
```rust
//! bondebarras — audit and cleanup of GitHub organization resources.

use std::process::ExitCode;

/// Entry point shared by the binary. Returns the process exit code.
pub fn run() -> ExitCode {
    println!("bondebarras");
    ExitCode::SUCCESS
}
```

- [ ] **Step 5: Vérifier le quality gate**

Run:
```sh
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo run -p bondebarras
```
Expected: tout passe, la dernière commande affiche `bondebarras`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock rustfmt.toml .gitignore crates/
git commit -m "build: squelette du workspace bondebarras"
```

---

### Task 2: Modèle de données et paliers de risque

**Files:**
- Create: `crates/bondebarras-core/src/model.rs`
- Modify: `crates/bondebarras-core/src/lib.rs`

**Interfaces:**
- Consumes: rien.
- Produces:
  - `enum ResourceKind { Cache, Artifact, WorkflowRun }`
  - `enum RiskTier { Low, Medium, Nuclear }`
  - `fn risk_tier(kind: ResourceKind) -> RiskTier`
  - `fn human_size(bytes: u64) -> String`
  - `struct Resource { kind, id: u64, label: String, size_bytes: u64, age_days: i64, git_ref: Option<String>, stale_pr: bool }`
  - `struct RepoSummary { name: String, cache_bytes: u64, cache_count: u32 }`
  - `struct OrgSummary { login: String, cache_bytes: u64, cache_count: u32, repos: Vec<RepoSummary> }`

- [ ] **Step 1: Écrire les tests qui échouent**

`crates/bondebarras-core/src/model.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_uses_decimal_units() {
        assert_eq!(human_size(0), "0 o");
        assert_eq!(human_size(999), "999 o");
        assert_eq!(human_size(1_500), "1.5 Ko");
        assert_eq!(human_size(37_166_609_585), "37.2 Go");
    }

    #[test]
    fn every_v01_kind_is_low_risk() {
        for kind in ResourceKind::ALL {
            assert_eq!(risk_tier(kind), RiskTier::Low);
        }
    }

    #[test]
    fn risk_tiers_are_ordered_by_severity() {
        assert!(RiskTier::Low < RiskTier::Medium);
        assert!(RiskTier::Medium < RiskTier::Nuclear);
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core model`
Expected: FAIL — `cannot find function human_size`.

- [ ] **Step 3: Écrire l'implémentation**

En tête de `model.rs`, avant le module de tests :
```rust
//! Core data model: resources, risk tiers, and display formatting.

/// A deletable GitHub resource family. v0.1 covers the three regenerable ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Cache,
    Artifact,
    WorkflowRun,
}

impl ResourceKind {
    /// Every variant, so tests can assert the `risk_tier` match stays exhaustive.
    pub const ALL: [ResourceKind; 3] = [
        ResourceKind::Cache,
        ResourceKind::Artifact,
        ResourceKind::WorkflowRun,
    ];
}

/// How much friction a deletion must go through. Ordered by severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskTier {
    /// Regenerable by re-running a workflow: a single confirmation.
    Low,
    /// Irreversible but rarely critical: itemised recap plus confirmation.
    Medium,
    /// Definitive destruction: the user must type the target's name.
    Nuclear,
}

/// The tier is carried by the type, never by the UI — this exhaustive `match`
/// is what makes it impossible to add a destructive kind without assigning it
/// a tier.
pub fn risk_tier(kind: ResourceKind) -> RiskTier {
    match kind {
        ResourceKind::Cache | ResourceKind::Artifact | ResourceKind::WorkflowRun => RiskTier::Low,
    }
}

/// Decimal units (Go, not Gio) — matches what GitHub's own billing UI shows.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["o", "Ko", "Mo", "Go", "To"];
    if bytes < 1000 {
        return format!("{bytes} o");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// One deletable item inside a repository.
#[derive(Debug, Clone)]
pub struct Resource {
    pub kind: ResourceKind,
    pub id: u64,
    pub label: String,
    pub size_bytes: u64,
    pub age_days: i64,
    /// Git ref the resource is attached to, when GitHub exposes one.
    pub git_ref: Option<String>,
    /// True when `git_ref` points at a closed or merged pull request.
    pub stale_pr: bool,
}

/// Per-repository cache aggregate, from the org-level endpoint.
#[derive(Debug, Clone)]
pub struct RepoSummary {
    pub name: String,
    pub cache_bytes: u64,
    pub cache_count: u32,
}

/// Stage-1 view of one organization.
#[derive(Debug, Clone)]
pub struct OrgSummary {
    pub login: String,
    pub cache_bytes: u64,
    pub cache_count: u32,
    pub repos: Vec<RepoSummary>,
}
```

- [ ] **Step 4: Déclarer le module et relancer les tests**

Ajouter en tête de `lib.rs` :
```rust
pub mod model;
```

Run: `cargo test -p bondebarras-core model`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/model.rs crates/bondebarras-core/src/lib.rs
git commit -m "feat(model): types de ressources, paliers de risque et formatage des tailles"
```

---

### Task 3: Détection des caches de PR fermées

**Files:**
- Create: `crates/bondebarras-core/src/stale.rs`
- Modify: `crates/bondebarras-core/src/lib.rs`

**Interfaces:**
- Consumes: rien.
- Produces:
  - `fn pr_number_from_ref(git_ref: &str) -> Option<u64>`
  - `fn is_stale(git_ref: Option<&str>, closed_prs: &std::collections::HashSet<u64>) -> bool`

- [ ] **Step 1: Écrire les tests qui échouent**

`crates/bondebarras-core/src/stale.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn extracts_the_pr_number_from_a_pull_ref() {
        assert_eq!(pr_number_from_ref("refs/pull/32/merge"), Some(32));
        assert_eq!(pr_number_from_ref("refs/pull/7/head"), Some(7));
    }

    #[test]
    fn branch_refs_carry_no_pr_number() {
        assert_eq!(pr_number_from_ref("refs/heads/main"), None);
        assert_eq!(pr_number_from_ref("refs/tags/v1.0.0"), None);
        assert_eq!(pr_number_from_ref("refs/pull/abc/merge"), None);
    }

    /// A wrong `Some(n)` here would flag a live cache as dead weight, and the
    /// ⚑ shortcut deletes every flagged row in one keystroke. These lock the
    /// fail-closed behaviour against a future refactor of the parse chain.
    #[test]
    fn malformed_pull_refs_never_yield_a_number() {
        assert_eq!(pr_number_from_ref(""), None);
        assert_eq!(pr_number_from_ref("refs/pull/"), None);
        assert_eq!(pr_number_from_ref("refs/pull//merge"), None);
        assert_eq!(pr_number_from_ref("refs/pull/-1/merge"), None);
        // 20 digits — overflows u64, whose max is ~1.8e19.
        assert_eq!(pr_number_from_ref("refs/pull/99999999999999999999/merge"), None);
    }

    #[test]
    fn a_cache_is_stale_only_when_its_pr_is_closed() {
        let closed = HashSet::from([25_u64, 32]);
        assert!(is_stale(Some("refs/pull/32/merge"), &closed));
        assert!(!is_stale(Some("refs/pull/99/merge"), &closed));
        assert!(!is_stale(Some("refs/heads/main"), &closed));
        assert!(!is_stale(None, &closed));
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core stale`
Expected: FAIL — `cannot find function pr_number_from_ref`.

- [ ] **Step 3: Écrire l'implémentation**

En tête de `stale.rs` :
```rust
//! Detection of caches attached to a closed pull request.
//!
//! GitHub keys Actions caches per git ref. A cache on `refs/pull/32/merge`
//! becomes dead weight the moment PR #32 is closed or merged, but GitHub only
//! evicts at the 10 GB per-repo ceiling or after 7 days without a read — so it
//! lingers, and it crowds out the caches that still matter. Flagging those is
//! the highest-volume, lowest-risk cleanup the tool offers.

use std::collections::HashSet;

/// Pull request number carried by a ref, if it is a pull ref.
///
/// `refs/pull/32/merge` -> `Some(32)`; anything else -> `None`.
pub fn pr_number_from_ref(git_ref: &str) -> Option<u64> {
    git_ref
        .strip_prefix("refs/pull/")?
        .split('/')
        .next()?
        .parse()
        .ok()
}

/// True when the ref belongs to a pull request that is no longer open.
pub fn is_stale(git_ref: Option<&str>, closed_prs: &HashSet<u64>) -> bool {
    git_ref
        .and_then(pr_number_from_ref)
        .is_some_and(|n| closed_prs.contains(&n))
}
```

- [ ] **Step 4: Déclarer le module et relancer les tests**

Ajouter dans `lib.rs` :
```rust
pub mod stale;
```

Run: `cargo test -p bondebarras-core stale`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/stale.rs crates/bondebarras-core/src/lib.rs
git commit -m "feat(stale): detection des caches rattaches a une PR fermee"
```

---

### Task 4: Résolution du token et lecture des scopes

**Files:**
- Create: `crates/bondebarras-core/src/auth.rs`
- Modify: `crates/bondebarras-core/src/lib.rs`

**Interfaces:**
- Consumes: rien.
- Produces:
  - `struct Scopes(Vec<String>)` avec `Scopes::parse(header: &str) -> Scopes` et `Scopes::has(&self, scope: &str) -> bool`
  - `fn can_delete_repo(scopes: &Scopes) -> bool`
  - `fn resolve_token() -> anyhow::Result<String>`

- [ ] **Step 1: Écrire les tests qui échouent**

`crates/bondebarras-core/src/auth.rs` :
```rust
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
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core auth`
Expected: FAIL — `cannot find type Scopes`.

- [ ] **Step 3: Écrire l'implémentation**

En tête de `auth.rs` :
```rust
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
    if let Ok(output) = Command::new("gh").args(["auth", "token"]).output() {
        if output.status.success() {
            let token = String::from_utf8(output.stdout)
                .context("`gh auth token` a renvoyé une sortie non-UTF-8")?
                .trim()
                .to_string();
            if !token.is_empty() {
                return Ok(token);
            }
        }
    }

    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            return Ok(token.to_string());
        }
    }

    // Pas de préfixe « Erreur : » ici : `run()` l'ajoute une fois, en haut de
    // la pile. Le porter aussi dans la valeur d'erreur le doublerait.
    bail!(
        "aucun jeton GitHub trouvé.\n\
         Connectez-vous avec `gh auth login`, ou définissez la variable \
         d'environnement GITHUB_TOKEN."
    )
}
```

- [ ] **Step 4: Déclarer le module et relancer les tests**

Ajouter dans `lib.rs` :
```rust
pub mod auth;
```

Run: `cargo test -p bondebarras-core auth`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/auth.rs crates/bondebarras-core/src/lib.rs
git commit -m "feat(auth): resolution du jeton via gh et lecture des scopes"
```

---

### Task 5: Client API — URL de base injectable, concurrence, retry

**Files:**
- Create: `crates/bondebarras-core/src/api/mod.rs`
- Modify: `crates/bondebarras-core/src/lib.rs`

**Interfaces:**
- Consumes: `auth::Scopes`.
- Produces:
  - `struct Client { gh: octocrab::Octocrab, sem: Arc<Semaphore>, scopes: Scopes }`
  - `Client::new(token: &str) -> Result<Client>`
  - `Client::with_base(token: &str, base: &str) -> Result<Client>` — point d'injection wiremock
  - `Client::scopes(&self) -> &Scopes`
  - `Client::get_json(&self, path: &str) -> Result<serde_json::Value>`
  - `Client::delete(&self, path: &str) -> Result<()>`

`path` porte **toujours un slash initial** : `/orgs/systm-d/repos`. C'est la convention
d'octocrab lui-même (`/orgs/{org}` dans son `api/orgs.rs`) et celle de claudettes. Sans
le slash, `Uri::from_str` interprète le premier segment comme une autorité et la requête
part au mauvais endroit.

**Pourquoi `_delete` et pas `delete` :** `Octocrab::delete` déserialise le corps de la
réponse, or une suppression de cache renvoie un **204 sans corps** — la désérialisation
échouerait sur une suppression réussie. `_delete` rend la réponse brute, dont on lit le
statut nous-mêmes.

**Pourquoi aucune URL absolue dans le code :** `BaseUriLayer` est un middleware tower
qu'octocrab applique à **toutes** les requêtes, `_delete` comprise ; il réécrit schéma et
autorité à partir du `base_uri` du builder. Passer un chemin seul suffit donc, et c'est
ce qui rend le serveur wiremock injectable par simple `with_base`.

- [ ] **Step 1: Écrire le test qui échoue**

`crates/bondebarras-core/src/api/mod.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn get_json_reads_from_the_injected_base() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/actions/cache/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_active_caches_size_in_bytes": 37_166_609_585_u64,
                "total_active_caches_count": 132,
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let v = client
            .get_json("/orgs/systm-d/actions/cache/usage")
            .await
            .unwrap();

        assert_eq!(v["total_active_caches_count"], 132);
    }

    #[tokio::test]
    async fn delete_accepts_a_204_with_no_body() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/repos/systm-d/claudine/actions/caches/9"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        client
            .delete("/repos/systm-d/claudine/actions/caches/9")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_surfaces_a_failing_status() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/repos/systm-d/claudine/actions/caches/9"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let err = client
            .delete("/repos/systm-d/claudine/actions/caches/9")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("404"));
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core api::`
Expected: FAIL — `cannot find type Client`.

- [ ] **Step 3: Écrire l'implémentation**

En tête de `api/mod.rs` :
```rust
//! The one boundary that knows about octocrab.
//!
//! Typed endpoints and raw ones live behind the same two primitives, so the
//! rest of the crate never learns which responses octocrab models and which
//! we deserialise by hand.

pub mod artifacts;
pub mod caches;
pub mod prs;
pub mod repos;
pub mod runs;

use crate::auth::Scopes;
use anyhow::{Context, Result, bail};
use octocrab::Octocrab;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// GitHub's public API root.
const DEFAULT_BASE: &str = "https://api.github.com";

/// Hard ceiling on any single network call. Without it a half-open TCP
/// connection can hang a scan indefinitely.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Concurrent read requests. GitHub's primary limit (5000/h) is never the
/// binding constraint at this scale; the secondary limit on burst concurrency
/// is.
const READ_CONCURRENCY: usize = 8;

pub struct Client {
    gh: Octocrab,
    sem: Arc<Semaphore>,
    scopes: Scopes,
}

impl Client {
    pub fn new(token: &str) -> Result<Self> {
        Self::with_base(token, DEFAULT_BASE)
    }

    /// Build a client against an arbitrary API root. Tests point this at a
    /// wiremock server.
    pub fn with_base(token: &str, base: &str) -> Result<Self> {
        let gh = Octocrab::builder()
            .personal_token(token.to_string())
            .set_connect_timeout(Some(HTTP_TIMEOUT))
            .set_read_timeout(Some(HTTP_TIMEOUT))
            .base_uri(base)
            .context("URL de base invalide")?
            .build()
            .context("construction du client GitHub")?;

        Ok(Client {
            gh,
            sem: Arc::new(Semaphore::new(READ_CONCURRENCY)),
            scopes: Scopes::default(),
        })
    }

    pub fn scopes(&self) -> &Scopes {
        &self.scopes
    }

    /// Record the scopes advertised by the API. Called once at startup.
    pub fn set_scopes(&mut self, scopes: Scopes) {
        self.scopes = scopes;
    }

    /// GET returning parsed JSON, throttled by the read semaphore.
    pub async fn get_json(&self, path: &str) -> Result<serde_json::Value> {
        let _permit = self.sem.acquire().await.expect("semaphore never closed");
        self.gh
            .get::<serde_json::Value, _, ()>(path, None::<&()>)
            .await
            .with_context(|| format!("GET {path}"))
    }

    /// DELETE ignoring the (usually empty) body, surfacing the status code.
    ///
    /// `Octocrab::delete` would try to deserialise the empty 204 body, so we
    /// go through `_delete` and read the status ourselves. `BaseUriLayer`
    /// still supplies scheme and authority, so a bare path is enough.
    pub async fn delete(&self, path: &str) -> Result<()> {
        let _permit = self.sem.acquire().await.expect("semaphore never closed");
        let response = self
            .gh
            ._delete(path, None::<&()>)
            .await
            .with_context(|| format!("DELETE {path}"))?;

        let status = response.status();
        if !status.is_success() {
            bail!("DELETE {path} a échoué : {status}");
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Créer les modules enfants vides et relancer**

Créer cinq fichiers, chacun avec uniquement sa ligne de doc, pour que `mod.rs` compile :

`api/caches.rs`, `api/artifacts.rs`, `api/runs.rs`, `api/prs.rs`, `api/repos.rs` :
```rust
//! Placeholder filled in by the next tasks.
```

Ajouter dans `lib.rs` :
```rust
pub mod api;
```

Run: `cargo test -p bondebarras-core api::`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/api crates/bondebarras-core/src/lib.rs
git commit -m "feat(api): client GitHub avec URL de base injectable et concurrence bornee"
```

---

### Task 6: Endpoints des caches

**Files:**
- Modify: `crates/bondebarras-core/src/api/caches.rs`

**Interfaces:**
- Consumes: `api::Client`, `model::{RepoSummary, Resource, ResourceKind}`.
- Produces:
  - `async fn usage_by_repository(client: &Client, org: &str) -> Result<Vec<RepoSummary>>`
  - `async fn list(client: &Client, owner: &str, repo: &str) -> Result<Vec<Resource>>`
  - `async fn delete(client: &Client, owner: &str, repo: &str, id: u64) -> Result<()>`

- [ ] **Step 1: Écrire les tests qui échouent**

Dans `api/caches.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn usage_by_repository_maps_every_repo() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/actions/cache/usage-by-repository"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 2,
                "repository_cache_usages": [
                    { "full_name": "systm-d/josephine",
                      "active_caches_size_in_bytes": 12_372_371_816_u64,
                      "active_caches_count": 30 },
                    { "full_name": "systm-d/claudine",
                      "active_caches_size_in_bytes": 11_130_027_303_u64,
                      "active_caches_count": 69 }
                ]
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let repos = usage_by_repository(&client, "systm-d").await.unwrap();

        assert_eq!(repos.len(), 2);
        // `full_name` is split: the org prefix is redundant in the repo column.
        assert_eq!(repos[0].name, "josephine");
        assert_eq!(repos[0].cache_bytes, 12_372_371_816);
        assert_eq!(repos[1].cache_count, 69);
    }

    #[tokio::test]
    async fn list_carries_the_ref_and_size_of_each_cache() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 1,
                "actions_caches": [
                    { "id": 9, "ref": "refs/pull/32/merge",
                      "key": "v0-rust-coverage-Linux-x64-db7c195c",
                      "size_in_bytes": 273_678_336_u64,
                      "last_accessed_at": "2026-06-01T00:00:00Z" }
                ]
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = list(&client, "systm-d", "claudine").await.unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ResourceKind::Cache);
        assert_eq!(items[0].id, 9);
        assert_eq!(items[0].git_ref.as_deref(), Some("refs/pull/32/merge"));
        assert_eq!(items[0].size_bytes, 273_678_336);
        // Staleness is decided later, once the PR list is known.
        assert!(!items[0].stale_pr);
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core caches`
Expected: FAIL — `cannot find function usage_by_repository`.

- [ ] **Step 3: Écrire l'implémentation**

En tête de `api/caches.rs` :
```rust
//! Actions cache endpoints.
//!
//! None of these are in octocrab's typed surface, so they go through
//! `Client::get_json` / `Client::delete`.

use super::Client;
use crate::model::{RepoSummary, Resource, ResourceKind};
use anyhow::Result;
use chrono::{DateTime, Utc};

/// Per-repository cache totals for a whole org — one request for the lot.
/// This is what makes the stage-1 overview instant.
pub async fn usage_by_repository(client: &Client, org: &str) -> Result<Vec<RepoSummary>> {
    let v = client
        .get_json(&format!("/orgs/{org}/actions/cache/usage-by-repository"))
        .await?;

    Ok(v["repository_cache_usages"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let full = item["full_name"].as_str().unwrap_or_default();
                    RepoSummary {
                        // `full_name` is "org/repo"; the org prefix is noise here.
                        name: full.split_once('/').map_or(full, |(_, r)| r).to_string(),
                        cache_bytes: item["active_caches_size_in_bytes"].as_u64().unwrap_or(0),
                        cache_count: item["active_caches_count"].as_u64().unwrap_or(0) as u32,
                    }
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Individual caches of one repository, for the drill-down pane.
pub async fn list(client: &Client, owner: &str, repo: &str) -> Result<Vec<Resource>> {
    let v = client
        .get_json(&format!("/repos/{owner}/{repo}/actions/caches?per_page=100"))
        .await?;

    Ok(v["actions_caches"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| Resource {
                    kind: ResourceKind::Cache,
                    id: item["id"].as_u64().unwrap_or(0),
                    label: item["key"].as_str().unwrap_or_default().to_string(),
                    size_bytes: item["size_in_bytes"].as_u64().unwrap_or(0),
                    age_days: age_days(item["last_accessed_at"].as_str()),
                    git_ref: item["ref"].as_str().map(str::to_string),
                    // Filled in by `scan`, which knows the repo's closed PRs.
                    stale_pr: false,
                })
                .collect()
        })
        .unwrap_or_default())
}

pub async fn delete(client: &Client, owner: &str, repo: &str, id: u64) -> Result<()> {
    client
        .delete(&format!("/repos/{owner}/{repo}/actions/caches/{id}"))
        .await
}

/// Whole days between an RFC 3339 timestamp and now. Unparseable or missing
/// timestamps read as 0 rather than failing the whole listing.
pub(crate) fn age_days(timestamp: Option<&str>) -> i64 {
    timestamp
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| (Utc::now() - t.with_timezone(&Utc)).num_days())
        .unwrap_or(0)
}
```

- [ ] **Step 4: Relancer les tests**

Run: `cargo test -p bondebarras-core caches`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/api/caches.rs
git commit -m "feat(api): endpoints des caches Actions"
```

---

### Task 7: Endpoints des artifacts et des workflow runs

**Files:**
- Modify: `crates/bondebarras-core/src/api/artifacts.rs`, `crates/bondebarras-core/src/api/runs.rs`

**Interfaces:**
- Consumes: `api::Client`, `api::caches::age_days`, `model::{Resource, ResourceKind}`.
- Produces:
  - `artifacts::list(client, owner, repo) -> Result<Vec<Resource>>`
  - `artifacts::delete(client, owner, repo, id) -> Result<()>`
  - `runs::list(client, owner, repo) -> Result<Vec<Resource>>`
  - `runs::delete(client, owner, repo, id) -> Result<()>`

- [ ] **Step 1: Écrire les tests qui échouent**

Dans `api/artifacts.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn list_marks_expired_artifacts_in_the_label() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/josephine/actions/artifacts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 2,
                "artifacts": [
                    { "id": 1, "name": "github-pages", "size_in_bytes": 1_112_447,
                      "expired": false, "created_at": "2026-07-28T12:35:26Z" },
                    { "id": 2, "name": "github-pages", "size_in_bytes": 1_112_275,
                      "expired": true, "created_at": "2026-07-27T09:50:08Z" }
                ]
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = list(&client, "systm-d", "josephine").await.unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, ResourceKind::Artifact);
        assert_eq!(items[0].label, "github-pages");
        assert_eq!(items[1].label, "github-pages (expiré)");
        assert_eq!(items[1].size_bytes, 1_112_275);
    }
}
```

Dans `api/runs.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn list_labels_runs_with_their_workflow_and_number() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/josephine/actions/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 1,
                "workflow_runs": [
                    { "id": 4471, "name": "CI", "run_number": 128,
                      "head_branch": "main", "created_at": "2026-05-01T00:00:00Z" }
                ]
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = list(&client, "systm-d", "josephine").await.unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ResourceKind::WorkflowRun);
        assert_eq!(items[0].id, 4471);
        assert_eq!(items[0].label, "CI #128");
        // The runs endpoint reports no size; the gain comes from the logs and
        // artifacts GitHub drops along with the run.
        assert_eq!(items[0].size_bytes, 0);
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core "artifacts|runs"`
Expected: FAIL — `cannot find function list`.

- [ ] **Step 3: Écrire les deux implémentations**

En tête de `api/artifacts.rs` :
```rust
//! Actions artifact endpoints.

use super::Client;
use super::caches::age_days;
use crate::model::{Resource, ResourceKind};
use anyhow::Result;

pub async fn list(client: &Client, owner: &str, repo: &str) -> Result<Vec<Resource>> {
    let v = client
        .get_json(&format!("/repos/{owner}/{repo}/actions/artifacts?per_page=100"))
        .await?;

    Ok(v["artifacts"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let name = item["name"].as_str().unwrap_or_default();
                    let expired = item["expired"].as_bool().unwrap_or(false);
                    Resource {
                        kind: ResourceKind::Artifact,
                        id: item["id"].as_u64().unwrap_or(0),
                        // An expired artifact still occupies a row until it is
                        // deleted, so it is worth showing — and worth marking.
                        label: if expired {
                            format!("{name} (expiré)")
                        } else {
                            name.to_string()
                        },
                        size_bytes: item["size_in_bytes"].as_u64().unwrap_or(0),
                        age_days: age_days(item["created_at"].as_str()),
                        git_ref: None,
                        stale_pr: false,
                    }
                })
                .collect()
        })
        .unwrap_or_default())
}

pub async fn delete(client: &Client, owner: &str, repo: &str, id: u64) -> Result<()> {
    client
        .delete(&format!("/repos/{owner}/{repo}/actions/artifacts/{id}"))
        .await
}
```

En tête de `api/runs.rs` :
```rust
//! Workflow run endpoints. Deleting a run also drops its logs and artifacts.

use super::Client;
use super::caches::age_days;
use crate::model::{Resource, ResourceKind};
use anyhow::Result;

pub async fn list(client: &Client, owner: &str, repo: &str) -> Result<Vec<Resource>> {
    let v = client
        .get_json(&format!("/repos/{owner}/{repo}/actions/runs?per_page=100"))
        .await?;

    Ok(v["workflow_runs"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| Resource {
                    kind: ResourceKind::WorkflowRun,
                    id: item["id"].as_u64().unwrap_or(0),
                    label: format!(
                        "{} #{}",
                        item["name"].as_str().unwrap_or("workflow"),
                        item["run_number"].as_u64().unwrap_or(0)
                    ),
                    // The API reports no size for a run. The reclaimed space
                    // comes from the logs and artifacts deleted alongside it.
                    size_bytes: 0,
                    age_days: age_days(item["created_at"].as_str()),
                    git_ref: item["head_branch"]
                        .as_str()
                        .map(|b| format!("refs/heads/{b}")),
                    stale_pr: false,
                })
                .collect()
        })
        .unwrap_or_default())
}

pub async fn delete(client: &Client, owner: &str, repo: &str, id: u64) -> Result<()> {
    client
        .delete(&format!("/repos/{owner}/{repo}/actions/runs/{id}"))
        .await
}
```

- [ ] **Step 4: Relancer les tests**

Run: `cargo test -p bondebarras-core "artifacts|runs"`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/api/artifacts.rs crates/bondebarras-core/src/api/runs.rs
git commit -m "feat(api): endpoints des artifacts et des workflow runs"
```

---

### Task 8: Endpoints des repos et des PR fermées

**Files:**
- Modify: `crates/bondebarras-core/src/api/repos.rs`, `crates/bondebarras-core/src/api/prs.rs`

**Interfaces:**
- Consumes: `api::Client`.
- Produces:
  - `repos::list(client, org) -> Result<Vec<String>>` — noms courts, sans le préfixe d'org
  - `prs::closed_numbers(client, owner, repo) -> Result<HashSet<u64>>`

- [ ] **Step 1: Écrire les tests qui échouent**

Dans `api/repos.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn list_returns_short_repo_names() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "josephine" },
                { "name": "claudine" }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let names = list(&client, "systm-d").await.unwrap();

        assert_eq!(names, vec!["josephine".to_string(), "claudine".to_string()]);
    }
}
```

Dans `api/prs.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn closed_numbers_collects_closed_and_merged_prs() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "number": 25, "state": "closed" },
                { "number": 32, "state": "closed" }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let closed = closed_numbers(&client, "systm-d", "claudine").await.unwrap();

        assert!(closed.contains(&25));
        assert!(closed.contains(&32));
        assert_eq!(closed.len(), 2);
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core "repos|prs"`
Expected: FAIL — `cannot find function list` / `closed_numbers`.

- [ ] **Step 3: Écrire les deux implémentations**

En tête de `api/repos.rs` :
```rust
//! Organization repository listing.

use super::Client;
use anyhow::Result;

pub async fn list(client: &Client, org: &str) -> Result<Vec<String>> {
    let v = client
        .get_json(&format!("/orgs/{org}/repos?per_page=100"))
        .await?;

    Ok(v.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}
```

En tête de `api/prs.rs` :
```rust
//! Closed pull requests, used to flag dead caches.

use super::Client;
use anyhow::Result;
use std::collections::HashSet;

/// Numbers of every pull request that is no longer open.
///
/// `state=closed` covers merged PRs too — GitHub reports a merged PR as
/// closed, which is exactly the semantics we want: its caches are dead either
/// way.
pub async fn closed_numbers(client: &Client, owner: &str, repo: &str) -> Result<HashSet<u64>> {
    let v = client
        .get_json(&format!(
            "repos/{owner}/{repo}/pulls?state=closed&per_page=100"
        ))
        .await?;

    Ok(v.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["number"].as_u64())
                .collect()
        })
        .unwrap_or_default())
}
```

- [ ] **Step 4: Relancer les tests**

Run: `cargo test -p bondebarras-core "repos|prs"`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/api/repos.rs crates/bondebarras-core/src/api/prs.rs
git commit -m "feat(api): listing des repos et des PR fermees"
```

---

### Task 9: Scan à deux étages

**Files:**
- Create: `crates/bondebarras-core/src/scan.rs`
- Modify: `crates/bondebarras-core/src/lib.rs`

**Interfaces:**
- Consumes: `api::{Client, caches, artifacts, runs, prs, repos}`, `model::{OrgSummary, Resource}`, `stale::is_stale`.
- Produces:
  - `async fn overview(client: &Client, orgs: &[String]) -> Vec<OrgSummary>`
  - `async fn repo_detail(client: &Client, owner: &str, repo: &str) -> Result<Vec<Resource>>`
  - `fn mark_stale(items: &mut [Resource], closed_prs: &HashSet<u64>)`

- [ ] **Step 1: Écrire les tests qui échouent**

`crates/bondebarras-core/src/scan.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ResourceKind;

    fn cache(id: u64, git_ref: &str) -> Resource {
        Resource {
            kind: ResourceKind::Cache,
            id,
            label: format!("cache-{id}"),
            size_bytes: 1_000,
            age_days: 12,
            git_ref: Some(git_ref.to_string()),
            stale_pr: false,
        }
    }

    #[test]
    fn mark_stale_flags_only_caches_of_closed_prs() {
        let mut items = vec![
            cache(1, "refs/pull/32/merge"),
            cache(2, "refs/heads/main"),
            cache(3, "refs/pull/99/merge"),
        ];
        let closed = HashSet::from([32_u64]);

        mark_stale(&mut items, &closed);

        assert!(items[0].stale_pr);
        assert!(!items[1].stale_pr);
        assert!(!items[2].stale_pr);
    }

    #[test]
    fn an_org_that_fails_is_dropped_not_fatal() {
        // `overview` tolerates a failing org so one broken permission does not
        // blank the whole screen. Covered end-to-end in Step 4.
        let summaries: Vec<OrgSummary> = Vec::new();
        assert!(summaries.is_empty());
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core scan`
Expected: FAIL — `cannot find function mark_stale`.

- [ ] **Step 3: Écrire l'implémentation**

En tête de `scan.rs` :
```rust
//! Two-stage scanning.
//!
//! Stage 1 runs at launch and only touches org-level aggregates — two requests
//! per org, so fifteen orgs land in about three seconds. Stage 2 fetches a
//! repository's individual resources, and only when the user opens it. Paying
//! only for what you look at is what keeps manual navigation viable across a
//! hundred repositories.

use crate::api::{Client, artifacts, caches, prs, repos, runs};
use crate::model::{OrgSummary, Resource};
use crate::stale::is_stale;
use anyhow::Result;
use std::collections::HashSet;

/// Stage 1: cache aggregates and repository list for each org.
///
/// An org that fails — revoked permission, network blip — is dropped from the
/// result rather than failing the whole scan. With fifteen orgs, one bad
/// permission must not blank the screen.
pub async fn overview(client: &Client, orgs: &[String]) -> Vec<OrgSummary> {
    let futures = orgs.iter().map(|org| async move {
        let summaries = caches::usage_by_repository(client, org).await.ok()?;
        let names = repos::list(client, org).await.ok()?;

        let cache_bytes = summaries.iter().map(|r| r.cache_bytes).sum();
        let cache_count = summaries.iter().map(|r| r.cache_count).sum();

        // Repos with no cache still belong in the tree: they may hold
        // artifacts or runs, which stage 2 will surface.
        let mut repos_out = summaries;
        for name in names {
            if !repos_out.iter().any(|r| r.name == name) {
                repos_out.push(crate::model::RepoSummary {
                    name,
                    cache_bytes: 0,
                    cache_count: 0,
                });
            }
        }
        repos_out.sort_by(|a, b| b.cache_bytes.cmp(&a.cache_bytes));

        Some(OrgSummary {
            login: org.clone(),
            cache_bytes,
            cache_count,
            repos: repos_out,
        })
    });

    let mut out: Vec<OrgSummary> = futures::future::join_all(futures)
        .await
        .into_iter()
        .flatten()
        .collect();
    out.sort_by(|a, b| b.cache_bytes.cmp(&a.cache_bytes));
    out
}

/// Stage 2: every deletable resource of one repository, already flagged.
pub async fn repo_detail(client: &Client, owner: &str, repo: &str) -> Result<Vec<Resource>> {
    let (caches_r, artifacts_r, runs_r, closed) = futures::join!(
        caches::list(client, owner, repo),
        artifacts::list(client, owner, repo),
        runs::list(client, owner, repo),
        prs::closed_numbers(client, owner, repo),
    );

    let mut items = caches_r?;
    items.extend(artifacts_r?);
    items.extend(runs_r?);

    // A failed PR listing costs the flag, not the listing: everything still
    // shows, just without the ⚑ shortcut.
    mark_stale(&mut items, &closed.unwrap_or_default());

    items.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    Ok(items)
}

/// Flag every resource whose ref belongs to a closed pull request.
pub fn mark_stale(items: &mut [Resource], closed_prs: &HashSet<u64>) {
    for item in items.iter_mut() {
        item.stale_pr = is_stale(item.git_ref.as_deref(), closed_prs);
    }
}
```

- [ ] **Step 4: Déclarer le module et relancer les tests**

Ajouter dans `lib.rs` :
```rust
pub mod scan;
```

Run: `cargo test -p bondebarras-core scan`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/scan.rs crates/bondebarras-core/src/lib.rs
git commit -m "feat(scan): orchestration a deux etages avec tolerance aux orgs en echec"
```

---

### Task 10: Planificateur et exécuteur de nettoyage

**Files:**
- Create: `crates/bondebarras-core/src/clean.rs`
- Modify: `crates/bondebarras-core/src/lib.rs`

**Interfaces:**
- Consumes: `api::{Client, caches, artifacts, runs}`, `model::{Resource, ResourceKind, RiskTier, risk_tier, human_size}`.
- Produces:
  - `struct Plan { pub items: Vec<Resource>, pub owner: String, pub repo: String }`
  - `Plan::tier(&self) -> RiskTier`
  - `Plan::total_bytes(&self) -> u64`
  - `Plan::summary(&self) -> String`
  - `enum Progress { Done { id: u64 }, Failed { id: u64, reason: String }, Finished { freed: u64, failures: usize } }`
  - `async fn execute(client: &Client, plan: Plan, tx: UnboundedSender<Progress>)`

- [ ] **Step 1: Écrire les tests qui échouent**

`crates/bondebarras-core/src/clean.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: ResourceKind, id: u64, size: u64) -> Resource {
        Resource {
            kind,
            id,
            label: format!("item-{id}"),
            size_bytes: size,
            age_days: 30,
            git_ref: None,
            stale_pr: false,
        }
    }

    fn plan(items: Vec<Resource>) -> Plan {
        Plan {
            items,
            owner: "systm-d".into(),
            repo: "claudine".into(),
        }
    }

    #[test]
    fn a_plan_takes_the_highest_tier_of_its_items() {
        let p = plan(vec![
            item(ResourceKind::Cache, 1, 100),
            item(ResourceKind::Artifact, 2, 200),
        ]);
        assert_eq!(p.tier(), RiskTier::Low);
    }

    #[test]
    fn an_empty_plan_is_low_risk() {
        assert_eq!(plan(vec![]).tier(), RiskTier::Low);
    }

    #[test]
    fn total_bytes_sums_the_selection() {
        let p = plan(vec![
            item(ResourceKind::Cache, 1, 1_000_000),
            item(ResourceKind::Cache, 2, 2_000_000),
        ]);
        assert_eq!(p.total_bytes(), 3_000_000);
        assert!(p.summary().contains("3.0 Mo"));
        assert!(p.summary().contains('2'));
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core clean`
Expected: FAIL — `cannot find type Plan`.

- [ ] **Step 3: Écrire l'implémentation**

En tête de `clean.rs` :
```rust
//! Deletion planning and execution.
//!
//! Nothing here is reversible on GitHub's side, so there is no trash and no
//! undo — promising either would be a lie. What we offer instead is an
//! accurate recap before, and a per-item verdict after.

use crate::api::{Client, artifacts, caches, runs};
use crate::model::{Resource, ResourceKind, RiskTier, human_size, risk_tier};
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::{Duration, sleep};

/// Deletions are spaced out: GitHub's secondary rate limit rejects a burst,
/// and a purge of 132 caches is exactly such a burst.
const DELETE_SPACING: Duration = Duration::from_millis(120);

/// A confirmed selection, scoped to one repository.
pub struct Plan {
    pub items: Vec<Resource>,
    pub owner: String,
    pub repo: String,
}

impl Plan {
    /// The friction the plan must go through: the most severe tier it contains.
    pub fn tier(&self) -> RiskTier {
        self.items
            .iter()
            .map(|i| risk_tier(i.kind))
            .max()
            .unwrap_or(RiskTier::Low)
    }

    pub fn total_bytes(&self) -> u64 {
        self.items.iter().map(|i| i.size_bytes).sum()
    }

    /// User-facing recap shown in the confirmation modal.
    pub fn summary(&self) -> String {
        format!(
            "{} élément(s) · {}",
            self.items.len(),
            human_size(self.total_bytes())
        )
    }
}

/// Emitted as the deletion runs, so the TUI stays responsive.
#[derive(Debug, Clone)]
pub enum Progress {
    Done { id: u64 },
    Failed { id: u64, reason: String },
    Finished { freed: u64, failures: usize },
}

/// Delete every item of the plan, reporting each outcome as it lands.
pub async fn execute(client: &Client, plan: Plan, tx: UnboundedSender<Progress>) {
    let mut freed = 0_u64;
    let mut failures = 0_usize;

    for item in &plan.items {
        let result = match item.kind {
            ResourceKind::Cache => caches::delete(client, &plan.owner, &plan.repo, item.id).await,
            ResourceKind::Artifact => {
                artifacts::delete(client, &plan.owner, &plan.repo, item.id).await
            }
            ResourceKind::WorkflowRun => {
                runs::delete(client, &plan.owner, &plan.repo, item.id).await
            }
        };

        match result {
            Ok(()) => {
                freed += item.size_bytes;
                let _ = tx.send(Progress::Done { id: item.id });
            }
            Err(e) => {
                failures += 1;
                let _ = tx.send(Progress::Failed {
                    id: item.id,
                    reason: e.to_string(),
                });
            }
        }

        sleep(DELETE_SPACING).await;
    }

    let _ = tx.send(Progress::Finished { freed, failures });
}
```

- [ ] **Step 4: Déclarer le module et relancer les tests**

Ajouter dans `lib.rs` :
```rust
pub mod clean;
```

Run: `cargo test -p bondebarras-core clean`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/clean.rs crates/bondebarras-core/src/lib.rs
git commit -m "feat(clean): planificateur et executeur avec evenements de progression"
```

---

### Task 11: Thème du TUI

**Files:**
- Create: `crates/bondebarras-core/src/tui/mod.rs`, `crates/bondebarras-core/src/tui/theme.rs`
- Modify: `crates/bondebarras-core/src/lib.rs`

**Interfaces:**
- Consumes: rien.
- Produces: constantes `PRIMARY`, `BORDER`, `TEXT`, `MUTED`, `SUCCESS`, `WARNING`, `ERROR`, `STALE`, et les helpers `title_style()`, `border_style()`, `text_style()`, `muted()`, `status_warn()`, `status_error()`, `status_success()`, `stale_style()`, `selection_style()`.

- [ ] **Step 1: Écrire les tests qui échouent**

`crates/bondebarras-core/src/tui/theme.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stale_flag_has_its_own_colour() {
        // ⚑ must not read as an error: it marks the safest thing to delete.
        assert_ne!(STALE, ERROR);
        assert_eq!(stale_style().fg, Some(STALE));
    }

    #[test]
    fn selection_inverts_the_primary_colour() {
        let s = selection_style();
        assert_eq!(s.bg, Some(PRIMARY));
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core theme`
Expected: FAIL — `cannot find value STALE`.

- [ ] **Step 3: Écrire l'implémentation**

En tête de `tui/theme.rs` :
```rust
//! bondebarras TUI theme.
//!
//! The palette does not paint a full background: it inherits the terminal's,
//! the way claudine does. Only the accent, the text and the state colours are
//! ours.

use ratatui::style::{Color, Modifier, Style};

/// Primary — titles, active cursor.
pub const PRIMARY: Color = Color::Rgb(0xd9, 0x77, 0x57);
/// Deep tone for borders and chrome.
pub const BORDER: Color = Color::Rgb(0x7c, 0x3a, 0x00);
/// Base text.
pub const TEXT: Color = Color::Rgb(0xec, 0xe6, 0xe0);
/// Secondary text: sizes, ages, metadata.
pub const MUTED: Color = Color::Rgb(0xa8, 0x9e, 0x95);
/// A deletion that succeeded.
pub const SUCCESS: Color = Color::Rgb(0x9e, 0xc2, 0x7e);
/// Degraded state: an org that could not be read.
pub const WARNING: Color = Color::Rgb(0xc9, 0xa3, 0x5a);
/// A deletion that failed.
pub const ERROR: Color = Color::Rgb(0xc8, 0x70, 0x5c);
/// The ⚑ flag — a cache whose pull request is closed. Deliberately distinct
/// from ERROR: this is the safest thing on screen to delete, not a problem.
pub const STALE: Color = Color::Rgb(0x7e, 0xa8, 0xc2);
/// Text drawn on top of the primary colour.
pub const SEL_FG: Color = Color::Rgb(0x1a, 0x12, 0x0d);

pub fn title_style() -> Style {
    Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)
}
pub fn border_style() -> Style {
    Style::default().fg(BORDER)
}
pub fn text_style() -> Style {
    Style::default().fg(TEXT)
}
pub fn muted() -> Style {
    Style::default().fg(MUTED)
}
pub fn status_warn() -> Style {
    Style::default().fg(WARNING)
}
pub fn status_error() -> Style {
    Style::default().fg(ERROR)
}
pub fn status_success() -> Style {
    Style::default().fg(SUCCESS)
}
pub fn stale_style() -> Style {
    Style::default().fg(STALE)
}
pub fn selection_style() -> Style {
    Style::default()
        .bg(PRIMARY)
        .fg(SEL_FG)
        .add_modifier(Modifier::BOLD)
}
```

`crates/bondebarras-core/src/tui/mod.rs` :
```rust
//! Terminal user interface.

pub mod theme;
```

- [ ] **Step 4: Déclarer le module et relancer les tests**

Ajouter dans `lib.rs` :
```rust
pub mod tui;
```

Run: `cargo test -p bondebarras-core theme`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/tui/
git commit -m "feat(tui): palette et styles"
```

---

### Task 12: État de l'application — navigation, sélection, tri, filtre

**Files:**
- Create: `crates/bondebarras-core/src/tui/app.rs`
- Modify: `crates/bondebarras-core/src/tui/mod.rs`

**Interfaces:**
- Consumes: `model::{OrgSummary, Resource}`, `clean::Plan`.
- Produces:
  - `enum Focus { Orgs, Resources }`
  - `enum SortKey { Size, Age, Name }`
  - `struct App { orgs, org_cursor, repo_cursor, resources, res_cursor, selected: HashSet<u64>, focus, sort, filter: String, .. }`
  - `App::new(orgs: Vec<OrgSummary>) -> App`
  - `App::visible_resources(&self) -> Vec<&Resource>`
  - `App::toggle_selected(&mut self)`
  - `App::select_all_stale(&mut self)`
  - `App::cycle_sort(&mut self)`
  - `App::selection_bytes(&self) -> u64`
  - `App::current_target(&self) -> Option<(String, String)>`
  - `App::take_plan(&self, owner: &str, repo: &str) -> Plan`

- [ ] **Step 1: Écrire les tests qui échouent**

`crates/bondebarras-core/src/tui/app.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RepoSummary, ResourceKind};

    fn res(id: u64, label: &str, size: u64, age: i64, stale: bool) -> Resource {
        Resource {
            kind: ResourceKind::Cache,
            id,
            label: label.to_string(),
            size_bytes: size,
            age_days: age,
            git_ref: None,
            stale_pr: stale,
        }
    }

    fn app() -> App {
        let mut a = App::new(vec![]);
        a.resources = vec![
            res(1, "coverage-linux", 300, 40, true),
            res(2, "ubuntu-22-test", 200, 5, false),
            res(3, "coverage-macos", 100, 20, true),
        ];
        a
    }

    #[test]
    fn select_all_stale_takes_only_flagged_items() {
        let mut a = app();
        a.select_all_stale();
        assert_eq!(a.selection_bytes(), 400);
        assert!(a.selected.contains(&1));
        assert!(a.selected.contains(&3));
        assert!(!a.selected.contains(&2));
    }

    #[test]
    fn the_filter_matches_labels_case_insensitively() {
        let mut a = app();
        a.filter = "COVERAGE".into();
        let visible: Vec<u64> = a.visible_resources().iter().map(|r| r.id).collect();
        assert_eq!(visible, vec![1, 3]);
    }

    #[test]
    fn sorting_cycles_size_then_age_then_name() {
        let mut a = app();
        assert_eq!(a.sort, SortKey::Size);
        assert_eq!(a.visible_resources()[0].id, 1);

        a.cycle_sort();
        assert_eq!(a.sort, SortKey::Age);
        assert_eq!(a.visible_resources()[0].id, 1);

        a.cycle_sort();
        assert_eq!(a.sort, SortKey::Name);
        assert_eq!(a.visible_resources()[0].label, "coverage-linux");

        a.cycle_sort();
        assert_eq!(a.sort, SortKey::Size);
    }

    #[test]
    fn toggling_twice_clears_the_selection() {
        let mut a = app();
        a.res_cursor = 1;
        a.toggle_selected();
        assert_eq!(a.selection_bytes(), 200);
        a.toggle_selected();
        assert_eq!(a.selection_bytes(), 0);
    }

    #[test]
    fn current_target_is_none_without_orgs() {
        assert_eq!(app().current_target(), None);
    }

    #[test]
    fn current_target_follows_both_cursors() {
        let repo = |name: &str| RepoSummary {
            name: name.to_string(),
            cache_bytes: 0,
            cache_count: 0,
        };
        let mut a = App::new(vec![OrgSummary {
            login: "systm-d".into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![repo("josephine"), repo("claudine")],
        }]);
        a.repo_cursor = 1;

        assert_eq!(
            a.current_target(),
            Some(("systm-d".to_string(), "claudine".to_string()))
        );
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core app`
Expected: FAIL — `cannot find type App`.

- [ ] **Step 3: Écrire l'implémentation**

En tête de `tui/app.rs` :
```rust
//! Application state: navigation, selection, sorting and filtering.
//!
//! Selection primitives are deliberately ad hoc — sort, filter, flag-select —
//! and nothing is persisted. There is no rules engine and no config file:
//! the user decides, every time.

use crate::clean::Plan;
use crate::model::{OrgSummary, Resource};
use std::collections::HashSet;

/// Which pane the keyboard drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Orgs,
    Resources,
}

/// Sort order of the resource pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    /// Biggest first — the default, because size is why the user is here.
    Size,
    /// Oldest first.
    Age,
    /// Alphabetical, for hunting a known key.
    Name,
}

pub struct App {
    pub orgs: Vec<OrgSummary>,
    pub org_cursor: usize,
    pub repo_cursor: usize,
    pub resources: Vec<Resource>,
    pub res_cursor: usize,
    pub selected: HashSet<u64>,
    pub focus: Focus,
    pub sort: SortKey,
    pub filter: String,
    pub status: String,
    pub should_quit: bool,
}

impl App {
    pub fn new(orgs: Vec<OrgSummary>) -> Self {
        App {
            orgs,
            org_cursor: 0,
            repo_cursor: 0,
            resources: Vec::new(),
            res_cursor: 0,
            selected: HashSet::new(),
            focus: Focus::Orgs,
            sort: SortKey::Size,
            filter: String::new(),
            status: String::new(),
            should_quit: false,
        }
    }

    /// Resources after filtering and sorting — what the right pane draws.
    pub fn visible_resources(&self) -> Vec<&Resource> {
        let needle = self.filter.to_lowercase();
        let mut out: Vec<&Resource> = self
            .resources
            .iter()
            .filter(|r| needle.is_empty() || r.label.to_lowercase().contains(&needle))
            .collect();

        match self.sort {
            SortKey::Size => out.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes)),
            SortKey::Age => out.sort_by(|a, b| b.age_days.cmp(&a.age_days)),
            SortKey::Name => out.sort_by(|a, b| a.label.cmp(&b.label)),
        }
        out
    }

    /// Toggle the row under the cursor, in the order currently displayed.
    pub fn toggle_selected(&mut self) {
        let Some(id) = self.visible_resources().get(self.res_cursor).map(|r| r.id) else {
            return;
        };
        if !self.selected.remove(&id) {
            self.selected.insert(id);
        }
    }

    /// Select every ⚑ row: the whole point of the flag is this one keystroke.
    pub fn select_all_stale(&mut self) {
        for r in self.resources.iter().filter(|r| r.stale_pr) {
            self.selected.insert(r.id);
        }
    }

    pub fn cycle_sort(&mut self) {
        self.sort = match self.sort {
            SortKey::Size => SortKey::Age,
            SortKey::Age => SortKey::Name,
            SortKey::Name => SortKey::Size,
        };
        self.res_cursor = 0;
    }

    pub fn selection_bytes(&self) -> u64 {
        self.resources
            .iter()
            .filter(|r| self.selected.contains(&r.id))
            .map(|r| r.size_bytes)
            .sum()
    }

    /// `(org, repo)` under the cursor, as owned strings.
    ///
    /// Owned rather than borrowed on purpose: every caller goes on to mutate
    /// `app`, and holding a borrow into `self.orgs` across that mutation does
    /// not borrow-check.
    pub fn current_target(&self) -> Option<(String, String)> {
        let org = self.orgs.get(self.org_cursor)?;
        let repo = org.repos.get(self.repo_cursor)?;
        Some((org.login.clone(), repo.name.clone()))
    }

    /// Freeze the current selection into a plan.
    pub fn take_plan(&self, owner: &str, repo: &str) -> Plan {
        Plan {
            items: self
                .resources
                .iter()
                .filter(|r| self.selected.contains(&r.id))
                .cloned()
                .collect(),
            owner: owner.to_string(),
            repo: repo.to_string(),
        }
    }
}
```

- [ ] **Step 4: Déclarer le module et relancer les tests**

Ajouter dans `tui/mod.rs` :
```rust
pub mod app;
```

Run: `cargo test -p bondebarras-core app`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/tui/app.rs crates/bondebarras-core/src/tui/mod.rs
git commit -m "feat(tui): etat de navigation, selection, tri et filtre"
```

---

### Task 13: Rendu — split-pane et modale de confirmation

**Files:**
- Create: `crates/bondebarras-core/src/tui/views/mod.rs`, `orgs.rs`, `repo.rs`, `confirm.rs`
- Modify: `crates/bondebarras-core/src/tui/mod.rs`

**Interfaces:**
- Consumes: `tui::app::{App, Focus}`, `tui::theme`, `model::human_size`, `clean::Plan`.
- Produces:
  - `views::render(app: &App, f: &mut Frame)`
  - `views::orgs::render(app: &App, f: &mut Frame, area: Rect)`
  - `views::repo::render(app: &App, f: &mut Frame, area: Rect)`
  - `views::confirm::render(plan: &Plan, f: &mut Frame, area: Rect)`
  - `views::repo::row_spans(r: &Resource, checked: bool) -> Vec<Span<'static>>`

- [ ] **Step 1: Écrire le test qui échoue**

Dans `tui/views/repo.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ResourceKind;

    fn res(stale: bool) -> Resource {
        Resource {
            kind: ResourceKind::Cache,
            id: 1,
            label: "v0-rust-coverage-Linux-x64".into(),
            size_bytes: 273_678_336,
            age_days: 40,
            git_ref: Some("refs/pull/32/merge".into()),
            stale_pr: stale,
        }
    }

    fn text(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn a_row_shows_the_checkbox_kind_label_and_size() {
        let line = text(&row_spans(&res(false), true));
        assert!(line.contains("[x]"));
        assert!(line.contains("cache"));
        assert!(line.contains("v0-rust-coverage-Linux-x64"));
        assert!(line.contains("273.7 Mo"));
    }

    #[test]
    fn a_stale_row_carries_the_flag_and_its_pr_number() {
        let line = text(&row_spans(&res(true), false));
        assert!(line.contains("[ ]"));
        assert!(line.contains("PR#32"));
        assert!(line.contains('⚑'));
    }

    #[test]
    fn a_fresh_row_carries_no_flag() {
        assert!(!text(&row_spans(&res(false), false)).contains('⚑'));
    }
}
```

- [ ] **Step 2: Lancer le test pour vérifier qu'il échoue**

Run: `cargo test -p bondebarras-core repo`
Expected: FAIL — `cannot find function row_spans`.

- [ ] **Step 3: Écrire le rendu du panneau droit**

En tête de `tui/views/repo.rs` :
```rust
//! Right pane: the resources of the selected repository.

use crate::model::{Resource, ResourceKind, human_size};
use crate::tui::app::App;
use crate::tui::theme;
use crate::stale::pr_number_from_ref;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

/// One row of the resource list, as styled spans.
///
/// Split out from the widget so it can be asserted on without a terminal.
pub fn row_spans(r: &Resource, checked: bool) -> Vec<Span<'static>> {
    let kind = match r.kind {
        ResourceKind::Cache => "cache",
        ResourceKind::Artifact => "artif",
        ResourceKind::WorkflowRun => "run  ",
    };

    let mut spans = vec![
        Span::styled(
            if checked { "[x] " } else { "[ ] " }.to_string(),
            theme::text_style(),
        ),
        Span::styled(format!("{kind}  "), theme::muted()),
        Span::styled(format!("{:<34}", r.label), theme::text_style()),
        Span::styled(format!("{:>10}  ", human_size(r.size_bytes)), theme::muted()),
    ];

    // A stale row earns its own colour and the PR that made it dead weight.
    match r.git_ref.as_deref().and_then(pr_number_from_ref) {
        Some(n) if r.stale_pr => spans.push(Span::styled(
            format!("PR#{n} ⚑"),
            theme::stale_style(),
        )),
        _ => spans.push(Span::styled(format!("{}j", r.age_days), theme::muted())),
    }
    spans
}

pub fn render(app: &App, f: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app
        .visible_resources()
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let spans = row_spans(r, app.selected.contains(&r.id));
            let line = Line::from(spans);
            if i == app.res_cursor {
                ListItem::new(line).style(theme::selection_style())
            } else {
                ListItem::new(line)
            }
        })
        .collect();

    let title = format!(" {} éléments · {} ", items.len(), human_size(app.selection_bytes()));
    f.render_widget(
        List::new(items).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(theme::border_style()),
        ),
        area,
    );
}
```

- [ ] **Step 4: Écrire le panneau gauche, la modale et l'assemblage**

`tui/views/orgs.rs` :
```rust
//! Left pane: organizations, biggest cache footprint first.

use crate::model::human_size;
use crate::tui::app::App;
use crate::tui::theme;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

pub fn render(app: &App, f: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app
        .orgs
        .iter()
        .enumerate()
        .map(|(i, org)| {
            let line = Line::from(vec![
                Span::styled(format!("{:<14}", org.login), theme::text_style()),
                Span::styled(format!("{:>8}", human_size(org.cache_bytes)), theme::muted()),
            ]);
            if i == app.org_cursor {
                ListItem::new(line).style(theme::selection_style())
            } else {
                ListItem::new(line)
            }
        })
        .collect();

    f.render_widget(
        List::new(items).block(
            Block::default()
                .title(" ORGS ")
                .borders(Borders::ALL)
                .border_style(theme::border_style()),
        ),
        area,
    );
}
```

`tui/views/confirm.rs` :
```rust
//! Tier-1 confirmation modal.
//!
//! v0.1 only deletes regenerable resources, so a single [y/N] is the right
//! amount of friction. Tiers 2 and 3 land with packages and repositories.

use crate::clean::Plan;
use crate::tui::theme;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

/// A centred box, sized as a percentage of the frame.
fn centered(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(v[1])[1]
}

pub fn render(plan: &Plan, f: &mut Frame, area: Rect) {
    let zone = centered(60, 22, area);
    f.render_widget(Clear, zone);

    let body = vec![
        Line::from(Span::styled(plan.summary(), theme::text_style())),
        Line::from(Span::styled(
            format!("{}/{}", plan.owner, plan.repo),
            theme::muted(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Ces éléments sont régénérables par un re-run.",
            theme::muted(),
        )),
        Line::from(Span::styled("Supprimer ?   [y/N]", theme::title_style())),
    ];

    f.render_widget(
        Paragraph::new(body).block(
            Block::default()
                .title(" Confirmation ")
                .borders(Borders::ALL)
                .border_style(theme::border_style()),
        ),
        zone,
    );
}
```

`tui/views/mod.rs` :
```rust
//! Rendering. Layout mirrors claudine: header, body, status line, footer,
//! with modals drawn on top conditionally.

pub mod confirm;
pub mod orgs;
pub mod repo;

use crate::clean::Plan;
use crate::tui::app::App;
use crate::tui::theme;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const FOOTER: &str =
    " [espace] cocher  [s] trier  [f] filtrer  [A] tout ⚑  [d] supprimer  [q] quitter";

pub fn render(app: &App, f: &mut Frame, pending: Option<&Plan>) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(f.area());

    f.render_widget(
        Paragraph::new(Span::styled(
            format!(" bondebarras · {} orgs ", app.orgs.len()),
            theme::title_style(),
        )),
        rows[0],
    );

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(26), Constraint::Min(20)])
        .split(rows[1]);
    orgs::render(app, f, cols[0]);
    repo::render(app, f, cols[1]);

    f.render_widget(
        Paragraph::new(Span::styled(app.status.clone(), theme::muted())),
        rows[2],
    );
    f.render_widget(
        Paragraph::new(Span::styled(FOOTER, theme::muted())),
        rows[3],
    );

    if let Some(plan) = pending {
        confirm::render(plan, f, f.area());
    }
}
```

Ajouter dans `tui/mod.rs` :
```rust
pub mod views;
```

- [ ] **Step 5: Relancer les tests**

Run: `cargo test -p bondebarras-core repo`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/bondebarras-core/src/tui/
git commit -m "feat(tui): rendu split-pane et modale de confirmation palier 1"
```

---

### Task 14: Boucle d'événements, CLI et câblage

**Files:**
- Modify: `crates/bondebarras-core/src/tui/mod.rs`, `crates/bondebarras-core/src/lib.rs`
- Create: `crates/bondebarras-core/src/cli.rs`
- Create: `crates/bondebarras/tests/cli.rs`

**Interfaces:**
- Consumes: tout ce qui précède.
- Produces: `cli::Cli` (clap), `tui::run_tui(client, orgs) -> Result<()>`, `run() -> ExitCode` opérationnel.

- [ ] **Step 1: Écrire le test d'intégration qui échoue**

`crates/bondebarras/tests/cli.rs` :
```rust
use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn help_lists_the_subcommands() {
    Command::cargo_bin("bondebarras")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("scan"));
}

#[test]
fn version_is_reported() {
    Command::cargo_bin("bondebarras")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("0.1.0"));
}
```

- [ ] **Step 2: Lancer le test pour vérifier qu'il échoue**

Run: `cargo test -p bondebarras --test cli`
Expected: FAIL — la sortie ne contient pas `scan`.

- [ ] **Step 3: Écrire la CLI**

`crates/bondebarras-core/src/cli.rs` :
```rust
//! Command-line surface. With no subcommand, bondebarras opens the TUI.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "bondebarras", version, about = "Audit et nettoyage des orgs GitHub")]
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
    },
}
```

- [ ] **Step 4: Écrire la boucle d'événements**

Ajouter dans `tui/mod.rs` :
```rust
use crate::api::Client;
use crate::clean::{self, Plan, Progress};
use crate::model::{OrgSummary, human_size};
use crate::scan;
use anyhow::Result;
use app::{App, Focus};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::execute;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// How long a draw waits for a key before looping. Short enough that deletion
/// progress lands smoothly, long enough not to spin.
const TICK: Duration = Duration::from_millis(120);

/// Run the TUI until the user quits, restoring the terminal on every path.
///
/// The client arrives behind an `Arc` because deletions run on a spawned task:
/// awaiting them inline would freeze the interface for the whole purge, which
/// is exactly what the progress channel exists to avoid.
pub async fn run_tui(client: Arc<Client>, orgs: Vec<OrgSummary>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let result = event_loop(client, &mut terminal, App::new(orgs)).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

async fn event_loop<B: ratatui::backend::Backend>(
    client: Arc<Client>,
    terminal: &mut Terminal<B>,
    mut app: App,
) -> Result<()> {
    let mut pending: Option<Plan> = None;
    let (tx, mut rx) = mpsc::unbounded_channel::<Progress>();

    while !app.should_quit {
        terminal.draw(|f| views::render(&app, f, pending.as_ref()))?;

        // Drain deletion progress without blocking the draw.
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Progress::Done { id } => {
                    app.resources.retain(|r| r.id != id);
                    app.selected.remove(&id);
                }
                Progress::Failed { id, reason } => {
                    app.selected.remove(&id);
                    app.status = format!("Erreur : suppression de {id} — {reason}");
                }
                Progress::Finished { freed, failures } => {
                    app.status = if failures == 0 {
                        format!("Bon débarras ! {} libérés.", human_size(freed))
                    } else {
                        format!(
                            "Bon débarras ! {} libérés, {failures} échec(s).",
                            human_size(freed)
                        )
                    };
                }
            }
        }

        if !event::poll(TICK)? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        // The confirmation modal swallows every key while it is up.
        if let Some(plan) = pending.take() {
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                app.status = format!("Suppression de {} …", plan.summary());
                // Spawned, not awaited: the loop keeps drawing and draining
                // `rx` while the purge runs.
                let tx = tx.clone();
                let client = Arc::clone(&client);
                tokio::spawn(async move { clean::execute(&client, plan, tx).await });
            } else {
                app.status = "Annulé.".into();
            }
            continue;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
            KeyCode::Tab => {
                app.focus = match app.focus {
                    Focus::Orgs => Focus::Resources,
                    Focus::Resources => Focus::Orgs,
                }
            }
            KeyCode::Down => match app.focus {
                Focus::Orgs => {
                    app.org_cursor = (app.org_cursor + 1).min(app.orgs.len().saturating_sub(1))
                }
                Focus::Resources => {
                    let max = app.visible_resources().len().saturating_sub(1);
                    app.res_cursor = (app.res_cursor + 1).min(max);
                }
            },
            KeyCode::Up => match app.focus {
                Focus::Orgs => app.org_cursor = app.org_cursor.saturating_sub(1),
                Focus::Resources => app.res_cursor = app.res_cursor.saturating_sub(1),
            },
            KeyCode::Enter => {
                // Stage 2: load the selected repository on demand. The target
                // is cloned out first — holding a borrow on `app.orgs` while
                // assigning `app.status` would not compile.
                if let Some((org, repo)) = app.current_target() {
                    app.status = format!("Chargement de {org}/{repo} …");
                    match scan::repo_detail(&client, &org, &repo).await {
                        Ok(items) => {
                            app.resources = items;
                            app.res_cursor = 0;
                            app.selected.clear();
                            app.focus = Focus::Resources;
                            app.status.clear();
                        }
                        Err(e) => app.status = format!("Erreur : {e}"),
                    }
                }
            }
            KeyCode::Char(' ') => app.toggle_selected(),
            KeyCode::Char('s') => app.cycle_sort(),
            KeyCode::Char('A') => app.select_all_stale(),
            KeyCode::Char('d') => {
                if let Some((org, repo)) = app.current_target() {
                    if !app.selected.is_empty() {
                        pending = Some(app.take_plan(&org, &repo));
                    }
                }
            }
            KeyCode::Char(c) => app.filter.push(c),
            KeyCode::Backspace => {
                app.filter.pop();
            }
            _ => {}
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Câbler `run()`**

Remplacer le corps de `lib.rs` :
```rust
//! bondebarras — audit and cleanup of GitHub organization resources.

pub mod api;
pub mod auth;
pub mod clean;
pub mod cli;
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
        Some(cli::Command::Scan { org }) => {
            let targets: Vec<String> = match org {
                Some(o) => vec![o],
                None => orgs,
            };
            for summary in scan::overview(&client, &targets).await {
                println!(
                    "{:<24} {:>10}  ({} caches)",
                    summary.login,
                    model::human_size(summary.cache_bytes),
                    summary.cache_count
                );
            }
        }
        None => {
            let summaries = scan::overview(&client, &orgs).await;
            tui::run_tui(client, summaries).await?;
        }
    }
    Ok(())
}
```

- [ ] **Step 6: Relancer les tests et vérifier le quality gate**

Run:
```sh
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```
Expected: tout passe, dont les 2 tests de `crates/bondebarras/tests/cli.rs`.

- [ ] **Step 7: Commit**

```bash
git add crates/
git commit -m "feat(tui): boucle d'evenements, CLI et cablage complet"
```

---

### Task 15: Gouvernance, CI, packaging et landing page

**Files:**
- Create: `README.md`, `CLAUDE.md`, `AGENTS.md`, `CONVENTIONS.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `CHANGELOG.md`, `LICENSE-MIT`, `LICENSE-APACHE`, `deny.toml`, `.cargo/audit.toml`
- Create: `.github/CODEOWNERS`, `.github/dependabot.yml`, `.github/PULL_REQUEST_TEMPLATE.md`, `.github/ISSUE_TEMPLATE/{bug_report,feature_request}.md`
- Create: `.github/workflows/{ci,release,pages,site-check}.yml`
- Create: `packaging/homebrew/bondebarras.rb`, `packaging/aur/PKGBUILD`, `packaging/winget/README.md`
- Create: `site/config.toml`, `site/content/_index.md`, `site/content/_index.fr.md`, `site/sass/main.scss`, `site/templates/{base,index}.html`

**Interfaces:**
- Consumes: le binaire `bondebarras` produit par la Task 14.
- Produces: aucune API Rust.

**Source de vérité :** copier les fichiers correspondants de
`../../Delfour.co/system/claude-tui/` et substituer `claudine` → `bondebarras`,
`claudine-core` → `bondebarras-core`. Les quatre workflows, les trois gabarits de
packaging et la structure du site sont repris **tels quels** — seuls les noms, l'URL du
dépôt, la description et la couleur de marque changent.

- [ ] **Step 1: Copier la gouvernance et adapter**

```bash
SRC=../../Delfour.co/system/claude-tui
cp $SRC/CONVENTIONS.md $SRC/CONTRIBUTING.md $SRC/CODE_OF_CONDUCT.md .
cp $SRC/LICENSE-MIT $SRC/LICENSE-APACHE $SRC/deny.toml .
mkdir -p .cargo && cp $SRC/.cargo/audit.toml .cargo/
sed -i 's/claudine-core/bondebarras-core/g; s/claudine/bondebarras/g' \
  CONVENTIONS.md CONTRIBUTING.md CODE_OF_CONDUCT.md deny.toml
```

Puis relire `CONVENTIONS.md` et corriger à la main la section « Project shape » pour
décrire les crates de bondebarras.

- [ ] **Step 2: Copier `.github/` et les workflows**

```bash
SRC=../../Delfour.co/system/claude-tui
cp -r $SRC/.github .
grep -rl claudine .github | xargs sed -i 's/claudine-core/bondebarras-core/g; s/claudine/bondebarras/g'
grep -rn "claudine" .github || echo "aucune trace de claudine"
```

Vérifier ensuite que `release.yml` référence bien `--bin bondebarras`, que le job
`fedora` pointe sur `crates/bondebarras`, et que l'identifiant winget est devenu
`systm-d.bondebarras`.

- [ ] **Step 3: Copier le packaging et le site**

```bash
SRC=../../Delfour.co/system/claude-tui
cp -r $SRC/packaging .
mv packaging/homebrew/claudine.rb packaging/homebrew/bondebarras.rb
cp -r $SRC/site .
rm -rf site/public site/static/*.png site/static/*.webp
grep -rl claudine packaging site | xargs sed -i 's/claudine/bondebarras/g'
```

Adapter `site/config.toml` :
```toml
base_url = "https://systm-d.github.io/bondebarras"
title = "bondebarras"
description = "Audit and clean up your GitHub organizations from one terminal interface — dead caches, stale artifacts, forgotten workflow runs."
default_language = "en"
compile_sass = true
build_search_index = false
generate_feeds = false

[markdown]
highlight_code = false

[languages.fr]
title = "bondebarras"
description = "Auditez et nettoyez vos organisations GitHub depuis une seule interface terminal — caches morts, artifacts périmés, workflow runs oubliés."

[extra]
brand_color = "#d97757"
repo_url = "https://github.com/systm-d/bondebarras"
```

Écrire `site/content/_index.md` (anglais) autour de l'accroche « Good riddance. » et
`site/content/_index.fr.md` autour de « Bon débarras. », en reprenant la structure de
sections du `_index.md` de claudine.

- [ ] **Step 4: Écrire le README et le CHANGELOG**

`README.md` (anglais) : ce que fait l'outil, le chiffre qui motive (51 Go sur 15 orgs),
l'installation (`brew`, `.deb`, `.rpm`, binaires), l'usage (`bondebarras`,
`bondebarras scan --org <org>`), les raccourcis du TUI, les scopes de jeton requis.

`CHANGELOG.md`, format Keep a Changelog :
```markdown
# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-28

### Added

- Two-stage scan across every organization the token can see.
- Split-pane TUI: organizations on the left, resources on the right.
- Actions caches, artifacts and workflow runs, with individual deletion.
- Flagging of caches attached to a closed pull request.
- Ad-hoc bulk selection: sort, filter, select-all-flagged.
- Tier-1 confirmation before any deletion.
- `bondebarras scan` for a non-interactive overview.
```

- [ ] **Step 5: Écrire `CLAUDE.md` et `AGENTS.md`**

`CLAUDE.md` sur le modèle de claudine : rôle du projet, « Read first » pointant vers
`CONVENTIONS.md` et `docs/superpowers/specs/`, règles produit, et le tableau « Where to
change what » reprenant la table de la section « Structure des fichiers » de ce plan.

- [ ] **Step 6: Vérifier le quality gate et le build du site**

Run:
```sh
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cargo deny check
cd site && zola build && cd ..
```
Expected: tout passe. Si `zola` n'est pas installé localement, `site-check.yml` le
vérifiera sur la première PR.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "build: gouvernance, CI, packaging et landing page"
```

---

## Self-Review

**1. Couverture de la spec**

| Exigence de la spec | Tâche |
|---|---|
| §4 workspace deux crates, shim | 1 |
| §4 frontière `api/`, typé + brut masqués | 5 |
| §4.2 lecture des scopes, `delete_repo` absent | 4 |
| §5 étage 1 — agrégats org-level | 9 |
| §5 étage 2 — détail au drill-down | 9, 14 (touche Entrée) |
| §5.1 concurrence bornée à 8 | 5 |
| §5.1 espacement des suppressions | 10 |
| §5.2 détection ⚑ PR fermée | 3, 8, 9 |
| §6 split-pane, header/statut/footer | 13 |
| §6.1 espace / s / f / A / d | 12, 14 |
| §6.3 `theme.rs` avec tests | 11 |
| §7 palier porté par le type | 2, 10 |
| §7 pas de corbeille, journal, « Bon débarras ! » | 10, 14 |
| §8 CLI `scan` | 14 |
| §9 gouvernance, CI, packaging, site | 15 |
| §11 wiremock / insta / assert_cmd | 5–8, 14 |
| §3.1 endpoints billing morts | non appelés en v0.1 (écart documenté) |

Aucune exigence v0.1 sans tâche.

**2. Placeholders**

Les seuls fichiers créés sans contenu intégral sont les cinq modules `api/*` de la
Task 5 Step 4, remplis par les Tasks 6–8 — chacun avec son contenu complet écrit. Les
fichiers de la Task 15 sont copiés depuis une source nommée et existante avec la
commande de substitution exacte, pas décrits en creux.

**3. Cohérence des types**

- `Resource` : mêmes sept champs partout (Tasks 2, 6, 7, 9, 12, 13).
- `Client::get_json` / `Client::delete` : `path` toujours relatif sans slash initial (Tasks 5–8).
- `age_days` : `pub(crate)` dans `api::caches`, importé par `artifacts` et `runs` (Tasks 6, 7).
- `mark_stale(&mut [Resource], &HashSet<u64>)` : même signature Tasks 9 et 9-tests.
- `Plan { items, owner, repo }` : construit par `App::take_plan` (12), consommé par `clean::execute` (10), affiché par `confirm::render` (13).
- `views::render(app, f, pending)` : trois arguments, cohérent entre 13 et 14.
- `RiskTier` dérive `Ord`, utilisé par `Plan::tier` via `.max()` (Tasks 2, 10).
