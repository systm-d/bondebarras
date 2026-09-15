# Quotas de la formule, stockage Actions, budget et rétention — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Faire dire vrai à l'onglet Billing — montants en dollars, quota de la formule réelle, stockage Actions en GB-heures, budget Actions, rétention des artefacts — sans jamais afficher un chiffre que l'API ne donne pas.

**Architecture:** Le calcul reste pur dans `billing.rs`. Trois lectures dégradables neuves dans `api/` (`orgs`, `budgets`, `retention`) sont jointes à l'étage 1 dans `scan::overview` et portées par `OrgSummary`. L'onglet Billing devient une suite de blocs construits par des fonctions pures, testés par rendu à travers `tui::views::render`. `scan --json` passe par une fonction pure.

**Tech Stack:** Inchangé — Rust 2024, tokio, octocrab 0.41 derrière `Client::get_json`, ratatui 0.30, chrono 0.4, wiremock 0.6.

**Spec:** `docs/superpowers/specs/2026-09-11-bondebarras-billing-quotas-design.md`

## Global Constraints

- Rust **edition 2024**, MSRV **1.88**. `unsafe_code = "forbid"` ; clippy `all = warn`, CI en `-D warnings` ; rustfmt `max_width = 100`.
- **Doc comments en anglais. Chaînes user-facing en français**, accents inclus.
- Jamais `ERROR`/`FATAL`/`PANIC` en user-facing. `Erreur : ` ajouté **une seule fois**, par `run()`.
- `cargo test <filtre>` prend une **sous-chaîne littérale**. `billing` matche trois modules, `repos` aussi — filtrer sur les noms de test distinctifs donnés dans chaque tâche.
- Tri : `sort_by_key(|r| std::cmp::Reverse(r.x))` pour une clé `Ord` ; `sort_by(|a, b| b.x.total_cmp(&a.x))` pour un `f64`.
- Conventional Commits. **Le commit qui termine une issue porte `(#N)` dans son sujet.** Multi-plateforme.
- **Preuve TDD : sortie brute, redirigée vers un fichier puis relue et collée telle quelle.** Cinq rapports de ce projet ont présenté du texte reformaté comme une capture littérale.
- **Tests qui ne peuvent pas échouer :** dix tests de ce projet ont nommé la bonne propriété sans pouvoir échouer dessus. Construire chaque fixture pour que l'implémentation correcte et la fautive divergent, puis le prouver contre le défaut.
- **Tests de rendu : rendre le vrai `Rect`, pas la frame entière, et balayer les tailles.** Ici : passer par `tui::views::render` (la vraie disposition), largeurs 60 → 200, puis hauteurs depuis la première qui peut montrer la ligne.
- **Ne jamais estimer ni inventer un nombre que l'API ne donne pas.** Pas de quota connu → pas de pourcentage ; pas de montant de budget par défaut ; une réponse incomplète se lit « illisible », jamais « rien ».
- **`api/` reste le seul module qui connaît HTTP/octocrab.** Chaque endpoint neuf passe par `Client::get_json` et a ses tests wiremock, **chemin de dégradation compris**.
- **Lecture seule** : aucun `PUT`, `PATCH` ni `DELETE` n'est ajouté par ce plan.
- Toute ligne de l'onglet Billing qu'un test vérifie fait **au plus 56 caractères** (visible dans le cadre à 60 colonnes).
- Chaque issue : entrée `## [Unreleased]` du `CHANGELOG.md` (Keep a Changelog), README mis à jour là où le comportement visible ou les scopes changent.

## Structure des fichiers

| Fichier | Responsabilité | Tâches |
|---|---|---|
| `crates/bondebarras-core/src/billing.rs` | quotas par formule, stockage GB-heures, budgets, seuil de rétention (pur) | 2, 6, 10, 13 |
| `crates/bondebarras-core/src/model.rs` | `OrgSummary` (`Default`, `plan`, `budgets`, `retention`) ; `ArtifactRetention` | 4, 11, 13 |
| `crates/bondebarras-core/src/api/orgs.rs` | **neuf** — `GET /orgs/{org}` → `plan.name` | 3 |
| `crates/bondebarras-core/src/api/budgets.rs` | **neuf** — budgets paginés, tout échec → `None` | 11 |
| `crates/bondebarras-core/src/api/retention.rs` | **neuf** — rétention, tout échec → `None` | 13 |
| `crates/bondebarras-core/src/api/mod.rs` | déclare les trois modules | 3, 11, 13 |
| `crates/bondebarras-core/src/scan.rs` | `overview` : lectures dégradables jointes | 4, 11, 13 |
| `crates/bondebarras-core/src/commands/scan.rs` | `overview_json` pur | 4, 7, 11, 13 |
| `crates/bondebarras-core/src/tui/views/billing.rs` | l'onglet, en blocs purs | 1, 5, 8, 12, 14 |
| `crates/bondebarras-core/src/tui/views/gauges.rs` | `percent` réutilisé tel quel (déjà `pub(crate)`) ; quota de minutes en paramètre ; `cache_over_ceiling` | 5, 9 |
| `crates/bondebarras-core/src/tui/views/repos.rs` | ⚠ plafond de cache, ligne de détail GB-h | 9 |
| le module qui porte `repo_gauge_lines` (colonne 3) | passe le quota de la formule | 5 |
| `README.md`, `CHANGELOG.md`, `CLAUDE.md` | documentation | 1, 3, 5, 9, 11, 12, 13, 14 |

---

## Préalable d'exécution (avant la Task 1)

Ce plan s'exécute **après** `feat/tui-3-colonnes` (tâches 1 à 8 et revue finale), sur une branche empilée :

```bash
git switch feat/tui-3-colonnes
git switch -c feat/billing-quotas
cargo test --workspace > /tmp/bq-baseline.txt 2>&1; tail -5 /tmp/bq-baseline.txt
```

Noter le nombre de tests dans le ledger. Puis vérifier les interfaces sur lesquelles ce plan s'appuie, et **consigner les noms réels dans le ledger** :

```bash
grep -n "pub const CACHE_CEILING_BYTES\|fn percent\|pub fn minutes_gauge_line\|FREE_MINUTES_PER_MONTH" crates/bondebarras-core/src/tui/views/gauges.rs
grep -rn "gauges::percent\|pub(crate) fn percent" crates/bondebarras-core/src/tui/
grep -rn "minutes_gauge_line(" crates/bondebarras-core/src/tui/
grep -rn "fn repo_gauge_lines" crates/bondebarras-core/src/tui/
grep -n "pub fn render\|ListItem::new\|human_size" crates/bondebarras-core/src/tui/views/repos.rs
grep -n "pub fn render\|Constraint::Length" crates/bondebarras-core/src/tui/views/mod.rs
grep -rn "OrgSummary {" crates/bondebarras-core/src/
grep -n "## \[Unreleased\]" CHANGELOG.md
```

| Interface | Constatée à `e635f36` (Task 3 de tui-3-colonnes et son refactor) | Attendue après la Task 8 |
|---|---|---|
| `gauges::CACHE_CEILING_BYTES` | `pub const … = 10 * 1024 * 1024 * 1024` | inchangée |
| `gauges::percent` | `pub(crate) fn percent(used: u64, ceiling: u64) -> u64`, appelée par les deux jauges de `gauges.rs` **et** par `views::billing::gauge_line` (`use crate::tui::views::gauges;`) — la seule arithmétique de pourcentage du crate | inchangée |
| `gauges::minutes_gauge_line` | `pub fn minutes_gauge_line(used: u64, is_public: bool, width: u16) -> Vec<Line<'static>>`, lit `FREE_MINUTES_PER_MONTH` | inchangée |
| appel de la jauge de minutes | `repo_gauge_lines(app: &App, width: u16)` dans `views/repo.rs`, `org: &OrgSummary` en portée | peut avoir déménagé avec la colonne 3 |
| colonne 2 | — | `tui/views/repos.rs`, `pub fn render(app: &mut App, f: &mut Frame, area: Rect)`, un `ListItem` par dépôt de `app.orgs[app.org_cursor].repos`, ligne construite par une fonction de spans qui affiche `human_size(repo.cache_bytes)` |
| disposition | — | `views::render(app: &mut App, f: &mut Frame, pending: Option<&Plan>)` ; en `View::Billing`, `billing::render(app, f, corps)` sur toute la largeur ; sous le corps : ligne d'état, progression (hauteur 0 au repos), pied |
| `Focus::Repos` | — | la colonne 2 est visible dans les trois dispositions |
| `CHANGELOG.md` | — | une section `## [Unreleased]` existe (Task 8 de tui-3-colonnes) |

**Règle :** si seul un **nom** diffère, substituer le nom réel dans le code de ce plan et le consigner. Si une **forme** diffère (la jauge de minutes n'existe plus, la colonne 2 n'est pas une liste de `ListItem`, le corps de l'onglet Billing n'est pas pleine largeur, il y a plus de deux rangées fixes sous le corps), s'arrêter et le signaler avant la Task 1 : les tests de rendu de ce plan en dépendent.

---

### Task 1: #12 — les coûts en dollars

**Files:**
- Modify: `crates/bondebarras-core/src/tui/views/billing.rs` (la ligne de coûts dans `render` ; module `tests`)
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: `tui::views::render(app, f, None)` (disposition de tui-3-colonnes) ; `App::new`, `App.view`, `View::Billing`.
- Produces (utilisés par les Tasks 5, 8, 12, 14) :
  - `fn usd(amount: f64) -> String` — `"{amount:.2} $"`
  - `fn cost_line(gross: f64, covered: f64, billed: f64) -> String`
  - helpers de test dans `tui::views::billing::tests` : `usage(month, sku, unit, quantity, gross, repo) -> UsageItem`, `private_repo(name) -> RepoSummary`, `exec_d_september() -> OrgSummary`, `billing_app(org) -> App`, `screen(app, width, height) -> String`, `assert_shown_at_every_size(app, needle)`, `assert_absent_at_every_width(app, needle)`

- [ ] **Step 1: Écrire le test de rendu qui échoue**

Ajouter en tête du module `tests` de `crates/bondebarras-core/src/tui/views/billing.rs`, après `use super::*;` :

```rust
    use crate::billing::{BillingReport, UsageItem};
    use crate::model::{OrgSummary, RepoSummary};
    use crate::tui::app::View;

    /// One usage-report line with its unit spelled out: minutes and storage
    /// share the report, and later tests need both. Fully discounted, as
    /// exec-d's September report was.
    fn usage(month: &str, sku: &str, unit: &str, quantity: f64, gross: f64, repo: &str) -> UsageItem {
        UsageItem {
            month: month.into(),
            product: "actions".into(),
            sku: sku.into(),
            quantity,
            unit_type: unit.into(),
            gross,
            discount: gross,
            net: 0.0,
            repo: repo.into(),
        }
    }

    fn private_repo(name: &str) -> RepoSummary {
        RepoSummary {
            name: name.into(),
            cache_bytes: 0,
            cache_count: 0,
            private: true,
            age_days: 0,
            class: crate::repos::RepoClass::Archivable,
        }
    }

    /// exec-d in September 2026, measured on 2026-09-10: 1 004 private
    /// Linux-equivalent minutes (the org's real monthly total, attributed to
    /// one repository for the fixture's sake) and 371.85 GB-hours of Actions
    /// storage, both fully discounted.
    fn exec_d_september() -> OrgSummary {
        OrgSummary {
            login: "exec-d".into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![private_repo("disconnected")],
            billing: Some(BillingReport {
                items: vec![
                    usage("2026-09", "Actions Linux", "Minutes", 1_004.0, 6.024, "disconnected"),
                    usage(
                        "2026-09",
                        "Actions storage",
                        "GigabyteHours",
                        371.85,
                        0.1249,
                        "disconnected",
                    ),
                ],
            }),
        }
    }

    fn billing_app(org: OrgSummary) -> App {
        let mut app = App::new(vec![org]);
        app.view = View::Billing;
        app
    }

    /// The whole screen, row by row, rendered through `views::render` — the
    /// real layout, so the Billing panel gets the `Rect` it gets in
    /// production, never the bare frame.
    fn screen(app: &mut App, width: u16, height: u16) -> String {
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| crate::tui::views::render(app, f, None))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(usize::from(width))
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// `needle` must be on screen at every width from 60 to 200 (at a height
    /// that fits the whole tab), then at every height from the first one
    /// that can hold its row up to 50 (at width 100). Sweeps, never samples:
    /// this project once shipped a modal whose prompt vanished at exactly one
    /// height per width.
    fn assert_shown_at_every_size(app: &mut App, needle: &str) {
        for width in 60..=200u16 {
            let s = screen(app, width, 50);
            assert!(s.contains(needle), "{needle:?} missing at {width}x50:\n{s}");
        }
        let tall = screen(app, 100, 50);
        let row = tall
            .lines()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} missing at 100x50:\n{tall}"));
        // Below the needle's row: the panel's bottom border, the status row
        // and the footer — the progress row is zero-high while nothing runs.
        let floor = u16::try_from(row).expect("a row index fits in u16") + 1 + 3;
        for height in floor..=50 {
            let s = screen(app, 100, height);
            assert!(s.contains(needle), "{needle:?} missing at 100x{height}:\n{s}");
        }
    }

    /// `needle` must appear nowhere on screen, at any width from 60 to 200.
    fn assert_absent_at_every_width(app: &mut App, needle: &str) {
        for width in 60..=200u16 {
            let s = screen(app, width, 50);
            assert!(!s.contains(needle), "{needle:?} present at {width}x50:\n{s}");
        }
    }

    /// GitHub's amounts are US dollars (`pricePerUnit` 0.006 for Actions
    /// Linux). The tab printed the right figure with the wrong currency.
    /// Looks for the figure *with* its sign, so a stray `$` elsewhere cannot
    /// satisfy it.
    #[test]
    fn the_rendered_cost_line_is_in_dollars_at_every_size() {
        let mut app = billing_app(exec_d_september());
        // 6.024 + 0.1249 = 6.1489, gross and covered alike.
        assert_shown_at_every_size(&mut app, "brut 6.15 $");
        assert_shown_at_every_size(&mut app, "facturé 0.00 $");
        assert_absent_at_every_width(&mut app, "€");
    }
```

- [ ] **Step 2: Lancer le test**

Run: `cargo test -p bondebarras-core the_rendered_cost_line_is_in_dollars > /tmp/bq-t1-red.txt 2>&1; cat /tmp/bq-t1-red.txt`
Expected: FAIL — `"brut 6.15 $" missing at 60x50` (l'écran porte `brut 6.15 €`).

- [ ] **Step 3: Implémenter et ajouter le test unitaire**

Dans `crates/bondebarras-core/src/tui/views/billing.rs`, au-dessus de `pub fn render` :

```rust
/// A usage-report amount, in the report's own currency.
///
/// `grossAmount`, `discountAmount` and `netAmount` are US dollars:
/// `pricePerUnit` is 0.006 for `Actions Linux`, GitHub's published
/// per-minute dollar rate. Suffixed like the `€` this replaces, never
/// converted — this crate has no exchange rate and must not invent one.
fn usd(amount: f64) -> String {
    format!("{amount:.2} $")
}

/// The month's cost line: gross, covered by the allowance, actually billed.
fn cost_line(gross: f64, covered: f64, billed: f64) -> String {
    format!(
        "Coûts   brut {}   couvert {}   facturé {}",
        usd(gross),
        usd(covered),
        usd(billed)
    )
}
```

Dans `render`, remplacer :

```rust
                format!(
                    "Coûts   brut {gross:.2} €   couvert {covered:.2} €   facturé {billed:.2} €"
                ),
```

par :

```rust
                cost_line(gross, covered, billed),
```

Ajouter au module `tests` :

```rust
    #[test]
    fn cost_line_reads_dollars_never_euros() {
        let line = cost_line(6.1489, 6.1489, 0.0);
        assert_eq!(line, "Coûts   brut 6.15 $   couvert 6.15 $   facturé 0.00 $");
    }
```

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core dollars > /tmp/bq-t1-green.txt 2>&1; cat /tmp/bq-t1-green.txt`
Expected: PASS — `cost_line_reads_dollars_never_euros` et `the_rendered_cost_line_is_in_dollars_at_every_size`.

- [ ] **Step 5: CHANGELOG**

Sous `## [Unreleased]` de `CHANGELOG.md`, ajouter (créer la sous-section `### Fixed` si elle n'existe pas) :

```markdown
### Fixed

- Billing tab: amounts are shown in US dollars (`6.15 $`), the currency of
  GitHub's usage report (`pricePerUnit` is 0.006 for Actions Linux). They were
  printed with a `€` sign — the right figure in the wrong currency. Nothing is
  converted: bondebarras has no exchange rate and does not invent one. (#12)
```

- [ ] **Step 6: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t1-gate.txt 2>&1; tail -5 /tmp/bq-t1-gate.txt
git add crates/bondebarras-core/src/tui/views/billing.rs CHANGELOG.md
git commit -m "fix(billing): montants en dollars, jamais en euros (#12)"
```

---
### Task 2: #11 — quota de minutes par formule (calcul pur)

**Files:**
- Modify: `crates/bondebarras-core/src/billing.rs` (après `FREE_MINUTES_PER_MONTH` ; module `tests`)

**Interfaces:**
- Consumes: rien de neuf.
- Produces (Tasks 4, 5, 7, 12) :
  - `pub fn included_minutes_for(plan: Option<&str>) -> Option<u64>`

**Pas de second pourcentage.** `tui::views::gauges::percent(used: u64, ceiling: u64) -> u64` est `pub(crate)` depuis `e635f36`, et `views::billing::gauge_line` l'appelle déjà : c'est la seule arithmétique de pourcentage du crate, testée par `a_zero_ceiling_does_not_divide_by_zero`. Ce plan la réutilise telle quelle (Tasks 5, 8, 12) et ne la déplace pas : aucun module pur n'en a besoin — le JSON ne publie aucun pourcentage, et `billing::nears_blocking_budget` (Task 10) reçoit celui que la jauge affiche.

`FREE_MINUTES_PER_MONTH` reste en place jusqu'à la Task 5, qui retire ses deux lecteurs dans le même commit.

- [ ] **Step 1: Écrire les tests qui échouent**

Ajouter au module `tests` de `crates/bondebarras-core/src/billing.rs` :

```rust
    #[test]
    fn included_minutes_follow_the_plan() {
        assert_eq!(included_minutes_for(Some("free")), Some(2_000));
        assert_eq!(included_minutes_for(Some("team")), Some(3_000));
        assert_eq!(included_minutes_for(Some("enterprise")), Some(50_000));
    }

    #[test]
    fn an_unknown_or_unread_plan_has_no_minutes_allowance() {
        // `pro` is a real GitHub plan — for a personal account, never an org.
        // No figure here, so no percentage anywhere.
        assert_eq!(included_minutes_for(Some("pro")), None);
        assert_eq!(included_minutes_for(Some("")), None);
        assert_eq!(included_minutes_for(None), None);
    }
```

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core -- included_minutes_follow_the_plan an_unknown_or_unread_plan_has_no_minutes_allowance > /tmp/bq-t2-red.txt 2>&1; cat /tmp/bq-t2-red.txt`
Expected: FAIL — `cannot find function 'included_minutes_for' in this scope`.

- [ ] **Step 3: Implémenter**

Dans `crates/bondebarras-core/src/billing.rs`, juste après la constante `FREE_MINUTES_PER_MONTH` :

```rust
/// Included Actions minutes per month for an organization's GitHub plan, in
/// Linux-equivalent minutes — GitHub's own table.
///
/// `None` for a plan this crate has no figure for, and for no plan at all:
/// `GET /orgs/{org}` only returns `plan` to an owner. Never a default. A
/// guessed allowance made exec-d, on Team, read 50 % for 1 004 minutes when
/// 33 % was true, and SecondBrain-io, on Enterprise, read 901 % for 36 %.
pub fn included_minutes_for(plan: Option<&str>) -> Option<u64> {
    match plan? {
        "free" => Some(2_000),
        "team" => Some(3_000),
        "enterprise" => Some(50_000),
        _ => None,
    }
}
```

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core -- included_minutes_follow_the_plan an_unknown_or_unread_plan_has_no_minutes_allowance > /tmp/bq-t2-green.txt 2>&1; cat /tmp/bq-t2-green.txt`
Expected: PASS (2 tests).

- [ ] **Step 5: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t2-gate.txt 2>&1; tail -5 /tmp/bq-t2-gate.txt
git add crates/bondebarras-core/src/billing.rs
git commit -m "feat(billing): quota de minutes par formule"
```

---

### Task 3: #11 — lire la formule de l'organisation

**Files:**
- Create: `crates/bondebarras-core/src/api/orgs.rs`
- Modify: `crates/bondebarras-core/src/api/mod.rs` (liste des modules)
- Modify: `CLAUDE.md` (table « Where to change what »)

**Interfaces:**
- Consumes: `Client::get_json(&self, path: &str) -> Result<serde_json::Value>`.
- Produces (Task 4) : `pub async fn api::orgs::plan(client: &Client, org: &str) -> Option<String>`.

- [ ] **Step 1: Écrire les tests qui échouent**

Dans `crates/bondebarras-core/src/api/mod.rs`, ajouter `pub mod orgs;` entre `pub mod caches;` et `pub mod packages;` **avant** de lancer quoi que ce soit — sinon `cargo test` rapporte « 0 tests » au lieu d'une erreur, ce qui ne prouve rien.

Créer `crates/bondebarras-core/src/api/orgs.rs` avec seulement son module de tests :

```rust
//! Organization details — today, only the plan name.

use super::Client;

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn plan_reads_the_orgs_plan_name() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/exec-d"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "login": "exec-d",
                "plan": { "name": "team" }
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert_eq!(plan(&client, "exec-d").await.as_deref(), Some("team"));
    }

    /// Not an owner: GitHub still answers 200 with the organization, minus
    /// `plan`. That must read as "unknown", not as any plan in particular.
    #[tokio::test]
    async fn plan_is_none_when_the_field_is_absent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/le-vilain-petit-dev"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "login": "le-vilain-petit-dev"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(plan(&client, "le-vilain-petit-dev").await.is_none());
    }

    /// Modelled on `api::billing::a_403_degrades_to_none_rather_than_failing`.
    #[tokio::test]
    async fn plan_is_none_when_the_org_read_is_refused() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/maxds-lyon-archives"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(plan(&client, "maxds-lyon-archives").await.is_none());
    }
}
```

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core plan_ > /tmp/bq-t3-red.txt 2>&1; cat /tmp/bq-t3-red.txt`
Expected: FAIL — `cannot find function 'plan' in this scope`.

- [ ] **Step 3: Implémenter**

Dans `crates/bondebarras-core/src/api/orgs.rs`, entre `use super::Client;` et `#[cfg(test)]` :

```rust
/// The organization's plan name (`free`, `team`, `enterprise`), or `None`.
///
/// GitHub only includes `plan` in `GET /orgs/{org}` for an owner of the org.
/// A missing field, a refusal, or any other failure all read the same way —
/// plan unknown — and never drop the org: the plan only decides whether a
/// percentage can be shown, the same degradation as `api::billing::fetch`.
pub async fn plan(client: &Client, org: &str) -> Option<String> {
    let v = client.get_json(&format!("/orgs/{org}")).await.ok()?;
    v["plan"]["name"].as_str().map(str::to_string)
}
```

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core plan_ > /tmp/bq-t3-green.txt 2>&1; cat /tmp/bq-t3-green.txt`
Expected: PASS — les trois tests de `api::orgs` (le filtre attrape aussi des tests existants de `app.rs` comme `take_plan_…` : ils doivent passer aussi).

- [ ] **Step 5: CLAUDE.md**

Dans la table « Where to change what » de `CLAUDE.md`, juste après la ligne `Billing usage-report fetch (403 degrades to `None`, not an error)` :

```markdown
| Organization plan name (`GET /orgs/{org}`; refused or absent → `None`, never a default) | `crates/bondebarras-core/src/api/orgs.rs` |
```

- [ ] **Step 6: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t3-gate.txt 2>&1; tail -5 /tmp/bq-t3-gate.txt
git add crates/bondebarras-core/src/api/orgs.rs crates/bondebarras-core/src/api/mod.rs CLAUDE.md
git commit -m "feat(api): formule de l'organisation, illisible sans echec"
```

---

### Task 4: #11 — la formule dans `OrgSummary`, `scan::overview` et `scan --json`

**Files:**
- Modify: `crates/bondebarras-core/src/model.rs` (`OrgSummary`)
- Modify: `crates/bondebarras-core/src/scan.rs` (doc du module ; fin de la closure de `overview` ; module `tests`)
- Modify: `crates/bondebarras-core/src/commands/scan.rs` (`run` ; nouvelles `overview_json`, `org_json` ; module `tests`)
- Modify: tous les sites de test qui construisent un `OrgSummary` (liste par `grep`, Step 3)

**Interfaces:**
- Consumes: `api::orgs::plan` (Task 3) ; `billing::included_minutes_for` (Task 2).
- Produces :
  - `OrgSummary` dérive `Default` ; champ `pub plan: Option<String>`
  - `pub fn commands::scan::overview_json(summaries: &[OrgSummary]) -> serde_json::Value` (la Task 7 lui ajoute `month: &str`)
  - JSON par organisation : `plan`, `minutes_allowance`

- [ ] **Step 1: Écrire les tests qui échouent**

Dans le module `tests` de `crates/bondebarras-core/src/scan.rs`, après `an_org_without_billing_access_is_still_scanned` :

```rust
    /// #11: the plan decides whether any percentage can be shown at all, so
    /// `overview` must carry it from `api::orgs::plan` onto the summary.
    #[tokio::test]
    async fn overview_carries_the_orgs_plan() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/exec-d/actions/cache/usage-by-repository"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "repository_cache_usages": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/exec-d/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/exec-d"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "login": "exec-d",
                "plan": { "name": "team" }
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = overview(&client, &["exec-d".to_string()]).await;

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].plan.as_deref(), Some("team"));
    }

    /// A refused plan costs the plan, never the org — same rule as billing.
    #[tokio::test]
    async fn overview_keeps_an_org_whose_plan_is_refused() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/SecondBrain-io/actions/cache/usage-by-repository"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "repository_cache_usages": [
                    { "full_name": "SecondBrain-io/monolith-back",
                      "active_caches_size_in_bytes": 1000, "active_caches_count": 2 }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/SecondBrain-io/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/SecondBrain-io"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = overview(&client, &["SecondBrain-io".to_string()]).await;

        assert_eq!(out.len(), 1, "a refused plan must not drop the org");
        assert_eq!(out[0].cache_bytes, 1000);
        assert!(out[0].plan.is_none());
    }
```

À la fin de `crates/bondebarras-core/src/commands/scan.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn org(login: &str, plan: Option<&str>) -> OrgSummary {
        OrgSummary {
            login: login.into(),
            plan: plan.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn scan_json_carries_plan_and_minutes_allowance() {
        let v = overview_json(&[org("exec-d", Some("team")), org("le-vilain-petit-dev", None)]);

        assert_eq!(v[0]["org"], "exec-d");
        assert_eq!(v[0]["plan"], "team");
        // Team's 3 000 — not the 2 000 every org used to be measured against.
        assert_eq!(v[0]["minutes_allowance"], 3_000);
        assert!(v[1]["plan"].is_null(), "got: {}", v[1]);
        assert!(v[1]["minutes_allowance"].is_null(), "got: {}", v[1]);
        // The fields that existed before stay.
        assert_eq!(v[1]["billing_readable"], false);
        assert!(v[1]["repos"].as_array().is_some_and(|r| r.is_empty()));
    }
}
```

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core -- overview_carries_the_orgs_plan overview_keeps_an_org_whose_plan_is_refused scan_json_carries_plan_and_minutes_allowance > /tmp/bq-t4-red.txt 2>&1; cat /tmp/bq-t4-red.txt`
Expected: FAIL — `no field 'plan' on type 'OrgSummary'` et `cannot find function 'overview_json'`.

- [ ] **Step 3: Implémenter le modèle et mettre à jour les fixtures**

Dans `crates/bondebarras-core/src/model.rs`, remplacer la déclaration de `OrgSummary` par :

```rust
/// Stage-1 view of one organization.
///
/// `Default` exists for test fixtures (`..Default::default()`) only. The one
/// production construction site, `scan::overview`, names every field, so a
/// field added later can never be silently defaulted there.
#[derive(Debug, Clone, Default)]
pub struct OrgSummary {
    pub login: String,
    pub cache_bytes: u64,
    pub cache_count: u32,
    pub repos: Vec<RepoSummary>,
    /// The org's usage report, or `None` when billing is not readable —
    /// GitHub answers 403 to anyone who is not an owner. A 403 degrades this
    /// one column; it never drops the org.
    pub billing: Option<crate::billing::BillingReport>,
    /// The org's plan name (`free`, `team`, `enterprise`), or `None` when
    /// `GET /orgs/{org}` did not say — GitHub only tells an owner. Decides
    /// whether any allowance percentage can be shown at all (see
    /// `billing::included_minutes_for`).
    pub plan: Option<String>,
}
```

Lister les sites de construction :

```bash
grep -rn "OrgSummary {" crates/bondebarras-core/src/
```

Chaque site **de test** (tous sauf `Some(OrgSummary {` dans `scan.rs`) reçoit `..Default::default()` comme dernière ligne entre ses accolades. Exemple, dans `tui/app.rs` :

```rust
        let mut a = App::new(vec![OrgSummary {
            login: "systm-d".into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![repo("josephine"), repo("claudine")],
            billing: None,
            ..Default::default()
        }]);
```

À `3712c60` ces sites sont : `tui/app.rs` (7), `tui/views/orgs.rs` (1), `tui/views/repo.rs` (`org_with_gauged_repo`), `tui/views/billing.rs` (`exec_d_september`, Task 1) ; les Tasks 4 à 8 de tui-3-colonnes ont pu en ajouter. Le site de production ne reçoit **pas** `..Default::default()` : il nomme le champ (Step 4).

- [ ] **Step 4: Câbler `overview` et le JSON**

Dans `crates/bondebarras-core/src/scan.rs`, remplacer ces deux lignes du doc de module (si les Tasks 2 ou 6 de tui-3-colonnes les ont reformulées, garder leur sens et y substituer le texte ci-dessous) :

```rust
//! Stage 1 runs at launch and only touches org-level aggregates — three
//! requests per org, so fifteen orgs still land in a few seconds. Stage 2
```

par :

```rust
//! Stage 1 runs at launch and only touches org-level data: the two reads
//! that define an org (cache usage, repository list), and degradable reads
//! that only enrich it — each refused on its own, never dropping the org.
//! Fifteen orgs still land in a few seconds. Stage 2
```

Puis, dans `overview`, remplacer :

```rust
        // Third and last stage-1 request. Deliberately not `?`-propagated: an
        // org whose billing is refused is still worth showing.
        let billing = crate::api::billing::fetch(client, org).await;

        Some(OrgSummary {
            login: org.clone(),
            cache_bytes,
            cache_count,
            repos: repos_out,
            billing,
        })
```

par :

```rust
        // The degradable stage-1 reads, joined: none depends on another, and
        // none is `?`-propagated — an org whose billing or plan is refused is
        // still worth showing.
        let (billing, plan) = futures::join!(
            crate::api::billing::fetch(client, org),
            crate::api::orgs::plan(client, org),
        );

        Some(OrgSummary {
            login: org.clone(),
            cache_bytes,
            cache_count,
            repos: repos_out,
            billing,
            plan,
        })
```

Dans `crates/bondebarras-core/src/commands/scan.rs`, remplacer tout ce qui précède le module de tests par :

```rust
//! Non-interactive overview.

use crate::api::Client;
use crate::billing;
use crate::model::{OrgSummary, human_size};
use crate::scan;
use anyhow::Result;

/// Print the stage-1 overview.
///
/// With `json`, **stdout carries JSON and nothing else** — every progress or
/// diagnostic line goes to stderr, so a `| jq` pipeline always parses.
pub async fn run(client: &Client, orgs: &[String], json: bool) -> Result<()> {
    let summaries = scan::overview(client, orgs).await;

    if json {
        println!("{}", serde_json::to_string_pretty(&overview_json(&summaries))?);
    } else {
        for o in &summaries {
            println!(
                "{:<24} {:>10}  ({} caches)",
                o.login,
                human_size(o.cache_bytes),
                o.cache_count
            );
        }
    }
    Ok(())
}

/// The `--json` document: one object per organization.
///
/// Pure — no network, no stdout — so every field can be asserted on directly.
/// A figure the API did not give is `null`, never a default:
/// `minutes_allowance` is `null` for an unreadable or unknown plan, not the
/// Free plan's 2 000.
pub fn overview_json(summaries: &[OrgSummary]) -> serde_json::Value {
    serde_json::Value::Array(summaries.iter().map(org_json).collect())
}

fn org_json(o: &OrgSummary) -> serde_json::Value {
    serde_json::json!({
        "org": o.login,
        "cache_bytes": o.cache_bytes,
        "cache_count": o.cache_count,
        "billing_readable": o.billing.is_some(),
        "plan": o.plan,
        "minutes_allowance": billing::included_minutes_for(o.plan.as_deref()),
        "repos": o.repos.iter().map(|r| serde_json::json!({
            "name": r.name,
            "cache_bytes": r.cache_bytes,
            "cache_count": r.cache_count,
        })).collect::<Vec<_>>(),
    })
}
```

- [ ] **Step 5: Relancer**

Run: `cargo test -p bondebarras-core -- overview_carries_the_orgs_plan overview_keeps_an_org_whose_plan_is_refused scan_json_carries_plan_and_minutes_allowance > /tmp/bq-t4-green.txt 2>&1; cat /tmp/bq-t4-green.txt`
Expected: PASS (3 tests).

- [ ] **Step 6: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t4-gate.txt 2>&1; tail -5 /tmp/bq-t4-gate.txt
git add -A crates/bondebarras-core/src
git commit -m "feat(scan): la formule de chaque organisation, jusque dans scan --json"
```

---

### Task 5: #11 — l'onglet et la colonne 3 au quota de la formule

**Files:**
- Modify: `crates/bondebarras-core/src/tui/views/billing.rs` (`gauge_line`, `render` découpé en blocs ; module `tests`)
- Modify: `crates/bondebarras-core/src/tui/views/gauges.rs` (`minutes_gauge_line` ; `use` ; module `tests`)
- Modify: le module qui porte `repo_gauge_lines` (à `3712c60` : `crates/bondebarras-core/src/tui/views/repo.rs`) — l'appel et la fixture `org_with_gauged_repo`
- Modify: `crates/bondebarras-core/src/billing.rs` (retrait de `FREE_MINUTES_PER_MONTH`)
- Modify: `README.md`, `CHANGELOG.md`, `CLAUDE.md`

**Interfaces:**
- Consumes: `billing::included_minutes_for` (Task 2) ; `gauges::percent(used: u64, ceiling: u64) -> u64` (existant, `pub(crate)` depuis `e635f36`) ; `OrgSummary.plan` (Task 4) ; helpers de test de la Task 1.
- Produces (Tasks 8, 12, 14) :
  - `pub fn gauge_line(used: u64, allowance: Option<u64>) -> String`
  - `fn bar(percent: u64) -> String`
  - `fn tab_lines(org: &OrgSummary, month_cursor: usize) -> Vec<Line<'static>>` — tout l'onglet, de haut en bas
  - `fn header_line(login: &str, plan: Option<&str>) -> Line<'static>`, `fn month_line(month: &str) -> Line<'static>`, `fn enterprise_lines(plan: Option<&str>) -> Vec<Line<'static>>`, `fn unreadable_line() -> Line<'static>`
  - `fn displayed_month(report: &BillingReport, month_cursor: usize) -> String`, `fn private_repos(org: &OrgSummary) -> HashSet<String>`
  - `fn minutes_block(report: &BillingReport, month: &str, private: &HashSet<String>, plan: Option<&str>) -> Vec<Line<'static>>`
  - `fn cost_block(report: &BillingReport, month: &str) -> Vec<Line<'static>>`
  - `pub fn gauges::minutes_gauge_line(used: u64, is_public: bool, allowance: Option<u64>, width: u16) -> Vec<Line<'static>>`

- [ ] **Step 1: Écrire les tests qui échouent**

Dans le module `tests` de `crates/bondebarras-core/src/tui/views/billing.rs`, ajouter :

```rust
    #[test]
    fn gauge_line_without_an_allowance_has_no_percentage() {
        assert_eq!(gauge_line(1_004, None), "1 004 min   formule inconnue, pas de quota");
    }

    /// #11's real figures: exec-d, on Team since 2026-09-10, burnt 1 004
    /// private minutes in September. Against the Free plan's 2 000 every org
    /// used to get, the tab read 50 %; against Team's 3 000, 33 % is right.
    /// Both are asserted: a constant allowance satisfies neither.
    #[test]
    fn exec_d_september_reads_33_percent_on_team() {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        let mut app = billing_app(org);
        assert_shown_at_every_size(&mut app, "1 004 / 3 000");
        assert_shown_at_every_size(&mut app, " 33 %");
        assert_absent_at_every_width(&mut app, "50 %");
    }

    /// No plan, no percentage — anywhere in the tab, not only on the minutes
    /// line. Later blocks (storage, budget warnings) must keep this green.
    #[test]
    fn an_unknown_plan_shows_no_percentage_anywhere_in_the_tab() {
        let mut app = billing_app(exec_d_september());
        assert_shown_at_every_size(&mut app, "formule inconnue, pas de quota");
        assert_absent_at_every_width(&mut app, "%");
    }

    #[test]
    fn the_billing_header_names_the_current_plan() {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        let mut app = billing_app(org);
        assert_shown_at_every_size(&mut app, "exec-d · formule team");
        assert_shown_at_every_size(&mut app, "2026-09 · quota de la formule actuelle");
    }

    #[test]
    fn an_enterprise_org_says_its_quota_is_shared() {
        let mut org = exec_d_september();
        org.login = "SecondBrain-io".into();
        org.plan = Some("enterprise".into());
        let mut app = billing_app(org);
        assert_shown_at_every_size(&mut app, "Formule enterprise : quota partagé par tout le compte");
        assert_shown_at_every_size(&mut app, "entreprise, ces pourcentages sont des minimums.");

        // Said on enterprise only.
        let mut team = exec_d_september();
        team.plan = Some("team".into());
        let mut app = billing_app(team);
        assert_absent_at_every_width(&mut app, "Formule enterprise");
    }
```

Dans le même module, adapter les trois tests existants de `gauge_line` à la nouvelle signature — seul l'appel change :

```rust
        let line = gauge_line(16_369, Some(2_000));
```

```rust
        let line = gauge_line(100, Some(0));
```

```rust
        assert!(gauge_line(0, Some(2_000)).ends_with(" 0 %"));
```

Dans le module `tests` de `crates/bondebarras-core/src/tui/views/gauges.rs`, remplacer l'appel de `a_public_repo_reads_zero_with_its_reason` par `minutes_gauge_line(0, true, Some(3_000), 60)` et ajouter :

```rust
    #[test]
    fn minutes_gauge_divides_by_the_plans_allowance() {
        // exec-d on Team: 1 004 of 3 000. Against the old 2 000, 50 %.
        let line = text(&minutes_gauge_line(1_004, false, Some(3_000), 60));
        assert!(line.contains(" 33 %"), "got: {line}");
        assert!(line.contains("1004 / 3000"), "got: {line}");
    }

    #[test]
    fn minutes_gauge_without_a_plan_shows_no_percentage() {
        let line = text(&minutes_gauge_line(1_004, false, None, 60));
        assert!(!line.contains('%'), "got: {line}");
        assert!(line.contains("formule inconnue"), "got: {line}");
    }
```

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core -- gauge_line_without_an_allowance exec_d_september_reads_33_percent_on_team an_unknown_plan_shows_no_percentage_anywhere the_billing_header_names_the_current_plan an_enterprise_org_says_its_quota_is_shared minutes_gauge_divides_by_the_plans_allowance minutes_gauge_without_a_plan > /tmp/bq-t5-red.txt 2>&1; cat /tmp/bq-t5-red.txt`
Expected: FAIL — erreurs de compilation `this function takes 2 arguments but …` / `expected u64, found Option<…>` sur `gauge_line` et `minutes_gauge_line`.

- [ ] **Step 3: La jauge de minutes de la colonne 3**

Dans `crates/bondebarras-core/src/tui/views/gauges.rs`, supprimer `use crate::billing::FREE_MINUTES_PER_MONTH;` et remplacer `minutes_gauge_line` (doc comment compris) par :

```rust
/// Actions minutes against the allowance of the org's plan
/// (`billing::included_minutes_for`).
///
/// A public repository's Actions runs are free and unlimited — GitHub's own
/// billing report never lists them against the allowance (see
/// `billing::BillingReport::included_minutes`) — so it always reads 0 %, with
/// the reason spelled out: a bare 0 % would read as comfortable headroom.
///
/// With no known allowance — no plan read, or a plan this crate has no
/// figure for — the total is real and a percentage would be invented, so the
/// line gives the total and says why: the same rule as a package version's
/// size.
pub fn minutes_gauge_line(
    used: u64,
    is_public: bool,
    allowance: Option<u64>,
    width: u16,
) -> Vec<Line<'static>> {
    if is_public {
        return vec![Line::from(Span::styled(
            "Minutes    0 % (dépôt public : minutes Actions gratuites et illimitées, \
             hors plafond)"
                .to_string(),
            theme::muted(),
        ))];
    }
    let Some(allowance) = allowance else {
        return vec![Line::from(Span::styled(
            format!("Minutes  {used} min · formule inconnue, pas de quota"),
            theme::muted(),
        ))];
    };
    let pct = percent(used, allowance);
    vec![Line::from(Span::styled(
        format!("Minutes {}  {pct:>3} %   {used} / {allowance}", bar(pct, width)),
        theme::text_style(),
    ))]
}
```

Dans `repo_gauge_lines`, remplacer l'appel par :

```rust
    lines.extend(gauges::minutes_gauge_line(
        minutes_used,
        !repo.private,
        crate::billing::included_minutes_for(org.plan.as_deref()),
        width,
    ));
```

Dans la fixture `org_with_gauged_repo` du même module, ajouter `plan: Some("free".into()),` avant `..Default::default()` : le test de balayage des jauges (`the_two_gauges_render_at_the_head_of_the_real_resource_pane_across_swept_widths`) attend `50` pour 1 000 minutes, ce qui n'est vrai que contre les 2 000 de la formule Free — sans formule, la ligne n'a plus de pourcentage et ce test doit échouer, pas passer par hasard.

Dans `crates/bondebarras-core/src/billing.rs`, supprimer la constante `FREE_MINUTES_PER_MONTH` et son doc comment. Vérifier qu'il n'en reste aucun lecteur :

```bash
grep -rn "FREE_MINUTES_PER_MONTH" crates/
```

Expected: aucune sortie (des mentions dans des doc comments sont à reformuler vers `billing::included_minutes_for` — dont le doc de `gauges::percent`, dont le paragraphe sur `pub(crate)` reste vrai : `views::billing::gauge_line` l'appelle toujours).

- [ ] **Step 4: L'onglet Billing en blocs**

Dans `crates/bondebarras-core/src/tui/views/billing.rs`, remplacer les `use` du haut par :

```rust
use crate::billing::{BillingReport, MinuteLine, included_minutes_for, sku_multiplier};
use crate::model::OrgSummary;
use crate::tui::app::App;
use crate::tui::theme;
use crate::tui::views::gauges;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::collections::HashSet;
```

Remplacer `gauge_line` (doc comment compris) par :

```rust
/// One line summarising allowance consumption.
///
/// Deliberately not clamped at 100 %: an org well past its included minutes
/// is exactly the situation the tab exists to surface. Without a known
/// allowance — no plan read, or a plan this crate has no figure for — the
/// line gives the total and says why there is no percentage, rather than
/// dividing by a guess.
pub fn gauge_line(used: u64, allowance: Option<u64>) -> String {
    let Some(allowance) = allowance else {
        return format!("{} min   formule inconnue, pas de quota", thousands(used));
    };
    let percent = gauges::percent(used, allowance);
    format!(
        "{} / {}   {}  {} %",
        thousands(used),
        thousands(allowance),
        bar(percent),
        percent
    )
}

/// The gauge's bar: one cell per 10 %, at most 20 so overshoot stays legible.
fn bar(percent: u64) -> String {
    "█".repeat((percent as usize / 10).min(20))
}
```

Remplacer `pub fn render` en entier par :

```rust
/// `exec-d · formule team`, or `formule inconnue` when the plan was not read.
fn header_line(login: &str, plan: Option<&str>) -> Line<'static> {
    let plan = plan.unwrap_or("inconnue");
    Line::from(Span::styled(
        format!("{login} · formule {plan}"),
        theme::title_style(),
    ))
}

/// Every month the tab pages through is measured against today's plan:
/// `plan.name` is the only plan GitHub reports, and a mid-month change
/// (exec-d moved from Free to Team on 2026-09-10) is invisible to it.
fn month_line(month: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("{month} · quota de la formule actuelle"),
        theme::muted(),
    ))
}

/// On `enterprise`, the allowance belongs to the enterprise account and is
/// shared by its organizations. bondebarras only sees this one org's usage,
/// so every percentage below is a floor — and the tab says so.
fn enterprise_lines(plan: Option<&str>) -> Vec<Line<'static>> {
    if plan != Some("enterprise") {
        return Vec::new();
    }
    vec![
        Line::from(Span::styled(
            "Formule enterprise : quota partagé par tout le compte",
            theme::muted(),
        )),
        Line::from(Span::styled(
            "  entreprise, ces pourcentages sont des minimums.",
            theme::muted(),
        )),
    ]
}

fn unreadable_line() -> Line<'static> {
    Line::from(Span::styled(
        "⚠ facturation illisible — vous n'êtes pas propriétaire de cette organisation",
        theme::status_warn(),
    ))
}

/// The month under `month_cursor`, clamped: the cursor can outlive a switch
/// to an org with fewer months.
fn displayed_month(report: &BillingReport, month_cursor: usize) -> String {
    let months = report.months();
    months
        .get(month_cursor.min(months.len().saturating_sub(1)))
        .cloned()
        .unwrap_or_default()
}

/// The only signal that separates "public, free forever" from "private,
/// covered by the allowance": GitHub's usage report discounts both
/// identically, so the repo listing stage 1 already fetched is the sole
/// place this distinction survives.
fn private_repos(org: &OrgSummary) -> HashSet<String> {
    org.repos
        .iter()
        .filter(|r| r.private)
        .map(|r| r.name.clone())
        .collect()
}

/// The minutes gauge and the per-repository breakdown behind it.
///
/// The breakdown is the tab's reason to exist: minutes cannot be reclaimed
/// once burnt, so the actionable part is *which repository* burnt them.
fn minutes_block(
    report: &BillingReport,
    month: &str,
    private: &HashSet<String>,
    plan: Option<&str>,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled("Minutes équivalent-inclus", theme::text_style())),
        Line::from(Span::styled(
            gauge_line(
                report.included_minutes(month, private),
                included_minutes_for(plan),
            ),
            theme::text_style(),
        )),
    ];
    let minute_lines = report.minute_lines(month, private);
    for minute_line in minute_lines.iter().take(MAX_MINUTE_LINES) {
        lines.push(minute_line_row(minute_line));
    }
    // A truncation that leaves no trace would bury the count of hidden rows.
    // Counted in rows, not repositories: one repo can contribute several rows
    // (one per SKU).
    if minute_lines.len() > MAX_MINUTE_LINES {
        lines.push(Line::from(Span::styled(
            format!(
                "   … et {} autre(s) ligne(s)",
                minute_lines.len() - MAX_MINUTE_LINES
            ),
            theme::muted(),
        )));
    }
    lines
}

/// The month's costs, then any runner SKU `sku_multiplier` does not know.
fn cost_block(report: &BillingReport, month: &str) -> Vec<Line<'static>> {
    let (gross, covered, billed) = report.cost(month);
    let style = if billed > 0.0 {
        theme::status_warn()
    } else {
        theme::muted()
    };
    let mut lines = vec![Line::from(Span::styled(
        cost_line(gross, covered, billed),
        style,
    ))];
    for sku in report.unknown_skus(month) {
        lines.push(Line::from(Span::styled(
            format!("⚠ SKU inconnu, compté ×1 : {sku}"),
            theme::status_warn(),
        )));
    }
    lines
}

/// Every line of the tab for one org, top to bottom.
///
/// Built from owned lines so the borrow of `app.orgs` ends before rendering,
/// and so each block can be asserted on through the real render.
fn tab_lines(org: &OrgSummary, month_cursor: usize) -> Vec<Line<'static>> {
    let plan = org.plan.as_deref();
    let mut lines = vec![header_line(&org.login, plan)];

    let Some(report) = &org.billing else {
        lines.push(unreadable_line());
        return lines;
    };

    let month = displayed_month(report, month_cursor);
    let private = private_repos(org);
    lines.push(month_line(&month));
    lines.extend(enterprise_lines(plan));
    lines.push(Line::from(""));
    lines.extend(minutes_block(report, &month, &private, plan));
    lines.push(Line::from(""));
    lines.extend(cost_block(report, &month));
    lines
}

pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    let Some(org) = app.orgs.get(app.org_cursor) else {
        f.render_widget(
            Paragraph::new(Span::styled("Aucune organisation.", theme::muted())),
            area,
        );
        return;
    };
    let lines = tab_lines(org, app.month_cursor);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Billing ")
                .borders(Borders::ALL)
                .border_style(theme::border_style()),
        ),
        area,
    );
}
```

- [ ] **Step 5: Relancer**

Run: `cargo test -p bondebarras-core -- gauge_line_without_an_allowance exec_d_september_reads_33_percent_on_team an_unknown_plan_shows_no_percentage_anywhere the_billing_header_names_the_current_plan an_enterprise_org_says_its_quota_is_shared minutes_gauge_divides_by_the_plans_allowance minutes_gauge_without_a_plan the_two_gauges_render_at_the_head the_rendered_cost_line_is_in_dollars > /tmp/bq-t5-green.txt 2>&1; cat /tmp/bq-t5-green.txt`
Expected: PASS — les sept tests neufs, le balayage des jauges de la colonne 3 et celui des coûts de la Task 1.

Preuve que le balayage des jauges peut encore échouer : retirer temporairement `plan: Some("free".into()),` de `org_with_gauged_repo`, relancer `cargo test -p bondebarras-core the_two_gauges_render_at_the_head > /tmp/bq-t5-mutant.txt 2>&1`, constater l'échec sur `"50"`, remettre la ligne.

- [ ] **Step 6: Documentation**

`README.md` — dans la puce `- **Billing tab** —`, remplacer :

```markdown
- **Billing tab** — per-organization Actions-minutes usage against the free
  allowance, month by month, with a per-repository breakdown of what is
  burning it.
```

par :

```markdown
- **Billing tab** — per-organization Actions-minutes usage against the
  allowance of the organization's **current plan** (`free` 2,000, `team`
  3,000, `enterprise` 50,000 minutes a month), month by month, with a
  per-repository breakdown of what is burning it. The plan comes from
  `GET /orgs/{org}`, which only tells an owner: when it cannot be read, or
  names a plan bondebarras has no figure for, the tab shows the total and
  says `formule inconnue` — **never a percentage against a guessed
  allowance**. Every month the tab pages through is measured against today's
  plan, and the tab says so (`quota de la formule actuelle`). On
  `enterprise`, the allowance belongs to the enterprise account and is shared
  by its organizations, so the percentage is a minimum.
```

et, plus loin dans la même puce, remplacer :

```markdown
  fact. (That org is on a different plan, so no allowance percentage is
  given here.)
```

par :

```markdown
  fact. (That org is on `enterprise`: 49 % of its 50,000 included minutes —
  a minimum, since that allowance is shared across the enterprise.)
```

Juste après le tableau des drapeaux de la section « CLI subcommands » (dernière ligne `| --yes | Confirms without a prompt |`), ajouter une ligne vide puis :

```markdown
`scan --json` prints one object per organization: `org`, `cache_bytes`,
`cache_count`, `billing_readable` and `repos`, plus `plan` (the plan name, or
`null` when it cannot be read) and `minutes_allowance` (that plan's included
minutes, or `null` — never a default).
```

`CHANGELOG.md` — sous `## [Unreleased]` (créer les sous-sections absentes) :

```markdown
### Changed

- Billing tab: the minutes gauge measures against the allowance of the
  organization's current plan — `free` 2,000, `team` 3,000, `enterprise`
  50,000 — read from `GET /orgs/{org}` at stage 1, instead of the Free plan's
  2,000 for every organization. Measured on 2026-09-10: exec-d (Team) read
  50 % for 1,004 minutes where 33 % is right; SecondBrain-io (Enterprise) read
  901 % for 18,016 where 36 % is right. The per-repository minutes gauge
  follows the same allowance. An unreadable or unknown plan shows the total
  and `formule inconnue`, with no percentage anywhere. (#11)

### Added

- `scan --json`: `plan` and `minutes_allowance` per organization, `null` when
  unknown. (#11)
```

`CLAUDE.md` — dans « Product rules », juste après la puce `- **Package versions carry no size, ever.** …` :

```markdown
- **No allowance, no percentage.** An organization's included Actions
  minutes come from `billing::included_minutes_for` — `free`, `team`,
  `enterprise`, nothing else — fed by `OrgSummary.plan`, which GitHub only
  returns to an owner. An unread or unknown plan yields `None`, and every
  gauge then shows the total and `formule inconnue`, never a percentage
  against a guessed figure. `tui::views::gauges::percent` is the crate's
  only percentage arithmetic, shared by the column-3 gauges and the Billing
  tab.
```

- [ ] **Step 7: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t5-gate.txt 2>&1; tail -5 /tmp/bq-t5-gate.txt
git add -A crates/bondebarras-core/src README.md CHANGELOG.md CLAUDE.md
git commit -m "feat(billing): quota de la formule de l'organisation, jamais devine (#11)"
```

---

### Task 6: #13 — stockage Actions en GB-heures (calcul pur)

**Files:**
- Modify: `crates/bondebarras-core/src/billing.rs` (doc du module ; constantes et fonctions libres après `included_minutes_for` ; bloc `impl BillingReport` après `unknown_skus` ; module `tests`)

**Interfaces:**
- Consumes: `UsageItem`, `BillingReport` (existants) ; `chrono` (déjà une dépendance).
- Produces (Tasks 7, 8, 9, 12, 13, 14) :
  - `pub struct StorageLine { pub repo: String, pub gbh: f64 }` (`Debug, Clone, PartialEq`)
  - `pub struct StorageQuota { pub gbh: f64, pub hours: u32 }` (`Debug, Clone, Copy, PartialEq`)
  - `pub fn included_storage_gb_for(plan: Option<&str>) -> Option<f64>`
  - `pub fn hours_in_month(month: &str) -> Option<u32>`
  - `pub fn storage_quota(plan: Option<&str>, month: &str) -> Option<StorageQuota>`
  - `pub fn month_of(now: chrono::DateTime<chrono::Utc>) -> String`
  - `BillingReport::storage_gbh(&self, month: &str) -> f64`
  - `BillingReport::storage_gbh_for_repo(&self, month: &str, repo: &str) -> f64`
  - `BillingReport::storage_lines(&self, month: &str) -> Vec<StorageLine>`

- [ ] **Step 1: Écrire les tests qui échouent**

Dans le module `tests` de `crates/bondebarras-core/src/billing.rs`, après le helper `private_repos` :

```rust
    /// One `Actions storage` line, in GigabyteHours, as GitHub reports it.
    fn storage(month: &str, gbh: f64, repo: &str) -> UsageItem {
        UsageItem {
            month: month.into(),
            product: "actions".into(),
            sku: "Actions storage".into(),
            quantity: gbh,
            unit_type: "GigabyteHours".into(),
            gross: 0.0,
            discount: 0.0,
            net: 0.0,
            repo: repo.into(),
        }
    }
```

Puis, en fin de module :

```rust
    /// The counterpart of `storage_line_items_do_not_pollute_the_minutes_gauge`:
    /// the storage total must not pick up minutes, another month, or another
    /// product's GigabyteHours.
    #[test]
    fn storage_gbh_sums_only_actions_storage_gigabyte_hours() {
        // An illustrative GigabyteHours line of another product: its exact SKU
        // name is not under test, only that it is not Actions storage.
        let mut other_product = storage("2026-09", 500.0, "disconnected");
        other_product.product = "codespaces".into();
        other_product.sku = "Codespaces storage".into();
        let r = BillingReport {
            items: vec![
                storage("2026-09", 359.88, "disconnected"),
                storage("2026-09", 11.21, "ptitjardinier-app"),
                item("2026-09", "Actions Linux", 1_004.0, 6.024, 6.024, "disconnected"),
                storage("2026-08", 100.0, "disconnected"),
                other_product,
            ],
        };
        // exec-d's real September lines for the two repositories #13 names.
        // The report's 371.85 total also counts 0.76 GB-h the issue does not
        // attribute; the fixture does not invent a repository for them.
        let got = r.storage_gbh("2026-09");
        assert!((got - 371.09).abs() < 1e-9, "got {got}");
    }

    #[test]
    fn storage_lines_rank_exec_d_september() {
        let r = BillingReport {
            items: vec![
                // Lightest first, so the ranking has work to do.
                storage("2026-09", 11.21, "ptitjardinier-app"),
                storage("2026-09", 359.88, "disconnected"),
                // Minutes must never rank a repository.
                item("2026-09", "Actions Linux", 5_000.0, 30.0, 30.0, "ptitjardinier-app"),
            ],
        };
        assert_eq!(
            r.storage_lines("2026-09"),
            vec![
                StorageLine { repo: "disconnected".into(), gbh: 359.88 },
                StorageLine { repo: "ptitjardinier-app".into(), gbh: 11.21 },
            ]
        );
    }

    /// One row per repository: two storage items of the same repository and
    /// month add up rather than producing two rows.
    #[test]
    fn storage_lines_merge_items_of_one_repo() {
        let r = BillingReport {
            items: vec![
                storage("2026-09", 1.5, "alertU"),
                storage("2026-09", 2.25, "alertU"),
            ],
        };
        let lines = r.storage_lines("2026-09");
        assert_eq!(lines.len(), 1);
        assert!((lines[0].gbh - 3.75).abs() < 1e-9, "got {}", lines[0].gbh);
    }

    #[test]
    fn storage_gbh_for_repo_reads_one_repo() {
        let r = BillingReport {
            items: vec![
                storage("2026-09", 359.88, "disconnected"),
                storage("2026-09", 11.21, "ptitjardinier-app"),
                storage("2026-08", 100.0, "disconnected"),
            ],
        };
        let got = r.storage_gbh_for_repo("2026-09", "disconnected");
        assert!((got - 359.88).abs() < 1e-9, "got {got}");
        let quiet = r.storage_gbh_for_repo("2026-09", "quiet");
        assert!(quiet.abs() < 1e-9, "got {quiet}");
    }

    #[test]
    fn included_storage_follows_the_plan() {
        assert_eq!(included_storage_gb_for(Some("free")), Some(0.5));
        assert_eq!(included_storage_gb_for(Some("team")), Some(2.0));
        assert_eq!(included_storage_gb_for(Some("enterprise")), Some(50.0));
        assert_eq!(included_storage_gb_for(Some("pro")), None);
        assert_eq!(included_storage_gb_for(None), None);
    }

    /// The hour base is the displayed month's own — days × 24, GitHub's
    /// documented formula. A 720 constant fails on July, a 744 constant on
    /// September. See the spec's open measurement on 720 vs 744 before
    /// changing this.
    #[test]
    fn hours_in_month_counts_the_displayed_months_days() {
        assert_eq!(hours_in_month("2026-09"), Some(720));
        assert_eq!(hours_in_month("2026-07"), Some(744));
        assert_eq!(hours_in_month("2026-02"), Some(672));
        assert_eq!(hours_in_month("2028-02"), Some(696));
        assert_eq!(hours_in_month("2026-12"), Some(744));
        assert_eq!(hours_in_month("2026-13"), None);
        assert_eq!(hours_in_month(""), None);
    }

    #[test]
    fn storage_quota_is_included_gb_times_month_hours() {
        // exec-d in September on Free, its plan until 2026-09-10: 0.5 × 720.
        assert_eq!(
            storage_quota(Some("free"), "2026-09"),
            Some(StorageQuota { gbh: 360.0, hours: 720 })
        );
        // On Team, its current plan: 2 × 720.
        assert_eq!(
            storage_quota(Some("team"), "2026-09"),
            Some(StorageQuota { gbh: 1_440.0, hours: 720 })
        );
        assert_eq!(
            storage_quota(Some("free"), "2026-07"),
            Some(StorageQuota { gbh: 372.0, hours: 744 })
        );
        assert_eq!(storage_quota(None, "2026-09"), None);
        assert_eq!(storage_quota(Some("team"), ""), None);
    }

    #[test]
    fn month_of_formats_year_and_month() {
        let t = chrono::DateTime::parse_from_rfc3339("2026-09-10T08:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(month_of(t), "2026-09");
    }
```

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core -- storage_gbh_sums_only storage_lines_rank_exec_d storage_lines_merge_items storage_gbh_for_repo_reads included_storage_follows hours_in_month_counts storage_quota_is_included month_of_formats > /tmp/bq-t6-red.txt 2>&1; cat /tmp/bq-t6-red.txt`
Expected: FAIL — `cannot find struct 'StorageLine'`, `no method named 'storage_gbh'`, etc.

- [ ] **Step 3: Implémenter**

Dans `crates/bondebarras-core/src/billing.rs`, ajouter au doc du module, après son premier paragraphe :

```rust
//!
//! Actions storage is the other axis, and unlike minutes it can be acted on:
//! it is billed in GB-hours — every hour a gigabyte of artifacts exists — so
//! deleting artifacts stops the accumulation, though never the hours already
//! counted. The usage report already carries it per repository; naming the
//! repository holding it is the same job as naming the one burning minutes.
```

Après `included_minutes_for` :

```rust
/// The usage report's Actions storage SKU.
const ACTIONS_STORAGE_SKU: &str = "Actions storage";

/// The unit GitHub bills Actions storage in.
const GIGABYTE_HOURS: &str = "GigabyteHours";

/// One row of the storage breakdown: which repository held how many
/// GB-hours in the month.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageLine {
    pub repo: String,
    pub gbh: f64,
}

/// A plan's included Actions storage for one month, in GB-hours, with the
/// hour base it was computed on — the Billing tab states that base, because
/// it is an open measurement (720 or 744 hours, see the design spec).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StorageQuota {
    pub gbh: f64,
    pub hours: u32,
}

/// Included Actions storage for an organization's plan, in GB — GitHub's
/// table. `None` for no plan or an unknown one, exactly like
/// `included_minutes_for`.
pub fn included_storage_gb_for(plan: Option<&str>) -> Option<f64> {
    match plan? {
        "free" => Some(0.5),
        "team" => Some(2.0),
        "enterprise" => Some(50.0),
        _ => None,
    }
}

/// Hours in a `YYYY-MM` month: its days × 24.
///
/// GitHub's documentation converts GB-hours to GB-months "by dividing by the
/// hours in the month (usually 720 hours for a 30-day month)". exec-d's
/// September 2026 report suggests a 744-hour base instead; that is an open
/// measurement, and this is the one place to change if it is confirmed.
/// `None` for anything that is not a real month.
pub fn hours_in_month(month: &str) -> Option<u32> {
    let (year, mon) = month.split_once('-')?;
    let year: i32 = year.parse().ok()?;
    let mon: u32 = mon.parse().ok()?;
    let first = chrono::NaiveDate::from_ymd_opt(year, mon, 1)?;
    let next = if mon == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        chrono::NaiveDate::from_ymd_opt(year, mon + 1, 1)?
    };
    u32::try_from((next - first).num_days() * 24).ok()
}

/// A plan's included storage for `month`, in GB-hours: included GB × the
/// month's hours. `None` when either is unknown — and then no percentage.
pub fn storage_quota(plan: Option<&str>, month: &str) -> Option<StorageQuota> {
    let gb = included_storage_gb_for(plan)?;
    let hours = hours_in_month(month)?;
    Some(StorageQuota {
        gbh: gb * f64::from(hours),
        hours,
    })
}

/// `YYYY-MM` for an instant, in UTC — how the "current month" of the repos
/// column and `scan --json` is named. Takes the instant rather than reading
/// the clock, so it stays testable.
pub fn month_of(now: chrono::DateTime<chrono::Utc>) -> String {
    now.format("%Y-%m").to_string()
}

fn is_actions_storage(item: &UsageItem) -> bool {
    item.sku == ACTIONS_STORAGE_SKU && item.unit_type == GIGABYTE_HOURS
}
```

Dans `impl BillingReport`, après `unknown_skus` :

```rust
    /// Actions storage used in the month, in GB-hours, public repositories
    /// included: the documentation says a public repository's *minutes* are
    /// free, but says nothing of its storage, and the report discounts both
    /// kinds alike. Counting it is the cautious reading, and the tab says so.
    pub fn storage_gbh(&self, month: &str) -> f64 {
        self.items
            .iter()
            .filter(|i| i.month == month && is_actions_storage(i))
            .map(|i| i.quantity)
            .sum()
    }

    /// One repository's Actions storage in the month, in GB-hours. A
    /// repository the report does not list for that month held none.
    pub fn storage_gbh_for_repo(&self, month: &str, repo: &str) -> f64 {
        self.items
            .iter()
            .filter(|i| i.month == month && i.repo == repo && is_actions_storage(i))
            .map(|i| i.quantity)
            .sum()
    }

    /// Storage per repository for the month, heaviest first — the storage
    /// counterpart of `minute_lines`: an alert says the quota is nearly
    /// used, this says which repository to go and clean.
    pub fn storage_lines(&self, month: &str) -> Vec<StorageLine> {
        let mut out: Vec<StorageLine> = Vec::new();
        for i in self
            .items
            .iter()
            .filter(|i| i.month == month && is_actions_storage(i))
        {
            match out.iter_mut().find(|l| l.repo == i.repo) {
                Some(line) => line.gbh += i.quantity,
                None => out.push(StorageLine {
                    repo: i.repo.clone(),
                    gbh: i.quantity,
                }),
            }
        }
        out.sort_by(|a, b| b.gbh.total_cmp(&a.gbh));
        out
    }
```

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core -- storage_gbh_sums_only storage_lines_rank_exec_d storage_lines_merge_items storage_gbh_for_repo_reads included_storage_follows hours_in_month_counts storage_quota_is_included month_of_formats storage_line_items_do_not_pollute > /tmp/bq-t6-green.txt 2>&1; cat /tmp/bq-t6-green.txt`
Expected: PASS (9 tests, dont le test existant sur les minutes).

- [ ] **Step 5: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t6-gate.txt 2>&1; tail -5 /tmp/bq-t6-gate.txt
git add crates/bondebarras-core/src/billing.rs
git commit -m "feat(billing): stockage Actions en GB-heures et quota par formule"
```

---

### Task 7: #13 — le stockage du mois dans `scan --json`

**Files:**
- Modify: `crates/bondebarras-core/src/commands/scan.rs` (`run`, `overview_json`, `org_json` ; module `tests`)

**Interfaces:**
- Consumes: `billing::{storage_quota, month_of}`, `BillingReport::{storage_gbh, storage_gbh_for_repo}` (Task 6).
- Produces : `pub fn overview_json(summaries: &[OrgSummary], month: &str) -> serde_json::Value` ; JSON `billing_month`, `storage_gbh`, `storage_allowance_gbh` par organisation, `storage_gbh` par dépôt.

- [ ] **Step 1: Écrire le test qui échoue**

Dans le module `tests` de `crates/bondebarras-core/src/commands/scan.rs`, remplacer `use super::*;` par :

```rust
    use super::*;
    use crate::billing::{BillingReport, UsageItem};
    use crate::model::RepoSummary;

    fn repo(name: &str) -> RepoSummary {
        RepoSummary {
            name: name.into(),
            cache_bytes: 0,
            cache_count: 0,
            private: true,
            age_days: 0,
            class: crate::repos::RepoClass::Archivable,
        }
    }

    fn storage(month: &str, gbh: f64, repo: &str) -> UsageItem {
        UsageItem {
            month: month.into(),
            product: "actions".into(),
            sku: "Actions storage".into(),
            quantity: gbh,
            unit_type: "GigabyteHours".into(),
            gross: 0.0,
            discount: 0.0,
            net: 0.0,
            repo: repo.into(),
        }
    }

    fn number(v: &serde_json::Value) -> f64 {
        v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
    }
```

Dans `scan_json_carries_plan_and_minutes_allowance`, l'appel devient `overview_json(&[org("exec-d", Some("team")), org("le-vilain-petit-dev", None)], "2026-09")`. Puis ajouter :

```rust
    #[test]
    fn scan_json_carries_storage_figures() {
        let exec_d = OrgSummary {
            login: "exec-d".into(),
            plan: Some("free".into()),
            repos: vec![repo("disconnected"), repo("quiet")],
            billing: Some(BillingReport {
                items: vec![
                    storage("2026-09", 359.88, "disconnected"),
                    storage("2026-09", 11.21, "ptitjardinier-app"),
                    // August must not leak into September's figures.
                    storage("2026-08", 100.0, "disconnected"),
                ],
            }),
            ..Default::default()
        };
        let unreadable = OrgSummary {
            login: "le-vilain-petit-dev".into(),
            repos: vec![repo("private-thing")],
            ..Default::default()
        };

        let v = overview_json(&[exec_d, unreadable], "2026-09");

        assert_eq!(v[0]["billing_month"], "2026-09");
        assert!((number(&v[0]["storage_gbh"]) - 371.09).abs() < 1e-9, "got: {}", v[0]);
        // Free's 0.5 GB × September's 720 hours.
        assert!((number(&v[0]["storage_allowance_gbh"]) - 360.0).abs() < 1e-9, "got: {}", v[0]);
        assert!((number(&v[0]["repos"][0]["storage_gbh"]) - 359.88).abs() < 1e-9);
        // A readable report that lists nothing for this repository: a real zero.
        assert!(number(&v[0]["repos"][1]["storage_gbh"]).abs() < 1e-9);
        // Unreadable billing, unknown plan: nulls, never zeros.
        assert!(v[1]["storage_gbh"].is_null(), "got: {}", v[1]);
        assert!(v[1]["storage_allowance_gbh"].is_null(), "got: {}", v[1]);
        assert!(v[1]["repos"][0]["storage_gbh"].is_null(), "got: {}", v[1]);
    }
```

- [ ] **Step 2: Lancer le test**

Run: `cargo test -p bondebarras-core scan_json_carries > /tmp/bq-t7-red.txt 2>&1; cat /tmp/bq-t7-red.txt`
Expected: FAIL — `this function takes 1 argument but 2 arguments were supplied`.

- [ ] **Step 3: Implémenter**

Dans `crates/bondebarras-core/src/commands/scan.rs`, la branche JSON de `run` devient :

```rust
    if json {
        let month = billing::month_of(chrono::Utc::now());
        println!(
            "{}",
            serde_json::to_string_pretty(&overview_json(&summaries, &month))?
        );
    } else {
```

Remplacer `overview_json` et `org_json` par :

```rust
/// The `--json` document: one object per organization.
///
/// Pure — no network, no stdout — so every field can be asserted on directly.
/// A figure the API did not give is `null`, never a default:
/// `minutes_allowance` is `null` for an unreadable or unknown plan, not the
/// Free plan's 2 000, and storage is `null` when billing is unreadable, not 0.
/// `month` is the month storage is read for; `run` passes the current UTC
/// month, and the document names it (`billing_month`).
pub fn overview_json(summaries: &[OrgSummary], month: &str) -> serde_json::Value {
    serde_json::Value::Array(summaries.iter().map(|o| org_json(o, month)).collect())
}

fn org_json(o: &OrgSummary, month: &str) -> serde_json::Value {
    let plan = o.plan.as_deref();
    serde_json::json!({
        "org": o.login,
        "cache_bytes": o.cache_bytes,
        "cache_count": o.cache_count,
        "billing_readable": o.billing.is_some(),
        "plan": o.plan,
        "minutes_allowance": billing::included_minutes_for(plan),
        "billing_month": month,
        "storage_gbh": o.billing.as_ref().map(|b| b.storage_gbh(month)),
        "storage_allowance_gbh": billing::storage_quota(plan, month).map(|q| q.gbh),
        "repos": o.repos.iter().map(|r| serde_json::json!({
            "name": r.name,
            "cache_bytes": r.cache_bytes,
            "cache_count": r.cache_count,
            "storage_gbh": o.billing.as_ref().map(|b| b.storage_gbh_for_repo(month, &r.name)),
        })).collect::<Vec<_>>(),
    })
}
```

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core scan_json_carries > /tmp/bq-t7-green.txt 2>&1; cat /tmp/bq-t7-green.txt`
Expected: PASS (2 tests).

- [ ] **Step 5: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t7-gate.txt 2>&1; tail -5 /tmp/bq-t7-gate.txt
git add crates/bondebarras-core/src/commands/scan.rs
git commit -m "feat(scan): stockage Actions du mois dans scan --json"
```

---

### Task 8: #13 — le bloc « Stockage Actions » de l'onglet

**Files:**
- Modify: `crates/bondebarras-core/src/tui/views/billing.rs` (`MAX_MINUTE_LINES` renommée ; `use` ; nouvelles fonctions ; `tab_lines` ; module `tests`)

**Interfaces:**
- Consumes: `billing::{StorageLine, StorageQuota, storage_quota}`, `BillingReport::{storage_gbh, storage_lines}` (Task 6) ; `gauges::percent` (existant) ; `bar`, `thousands`, `tab_lines`, helpers de test (Tasks 1, 5).
- Produces (Task 12) :
  - `const MAX_BREAKDOWN_LINES: usize = 8` (remplace `MAX_MINUTE_LINES`)
  - `fn centi_gbh(gbh: f64) -> u64`, `fn storage_percent(used: f64, quota: StorageQuota) -> u64` — le ratio de stockage passe par `gauges::percent`, en centièmes de GB-heure
  - `pub fn storage_gauge_line(used: f64, quota: Option<StorageQuota>) -> String`
  - `fn storage_line_row(line: &StorageLine) -> Line<'static>`
  - `fn storage_block(report: &BillingReport, month: &str, plan: Option<&str>) -> Vec<Line<'static>>`
  - `const DELETION_DOES_NOT_REFUND: [&str; 2]`

La jauge de stockage vit ici, au niveau de l'organisation. Pour l'onglet Billing seulement, cela remplace la phrase du §5 de la spec `tui-3-colonnes` (« le stockage Actions est un débit en gigaoctet-heures, pas un niveau ») ; la colonne 3 garde ses deux jauges.

- [ ] **Step 1: Écrire les tests qui échouent**

Dans le module `tests` de `crates/bondebarras-core/src/tui/views/billing.rs` :

```rust
    /// exec-d's September lines per repository, on Team. Listed lightest
    /// first so the ranking has work to do; storage amounts are not under
    /// test here and are left at zero.
    fn exec_d_september_by_repo() -> OrgSummary {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        org.billing = Some(BillingReport {
            items: vec![
                usage("2026-09", "Actions Linux", "Minutes", 1_004.0, 6.024, "disconnected"),
                usage("2026-09", "Actions storage", "GigabyteHours", 11.21, 0.0, "ptitjardinier-app"),
                usage("2026-09", "Actions storage", "GigabyteHours", 359.88, 0.0, "disconnected"),
            ],
        });
        org
    }

    /// exec-d in September on Free: 371.85 GB-h against 0.5 GB × 720 h. GitHub
    /// discounted all of it; the 103 % shown is the spec's open measurement
    /// (720 or 744 hours) — stated on the line, not hidden.
    #[test]
    fn storage_gauge_states_its_hour_base() {
        let september =
            storage_gauge_line(371.85, billing::storage_quota(Some("free"), "2026-09"));
        assert_eq!(september, "371.85 / 360 GB-h   ██████████  103 %   base 720 h");

        let july = storage_gauge_line(371.85, billing::storage_quota(Some("free"), "2026-07"));
        assert!(july.starts_with("371.85 / 372 GB-h"), "got: {july}");
        assert!(july.ends_with("100 %   base 744 h"), "got: {july}");
    }

    #[test]
    fn storage_gauge_without_a_plan_has_no_percentage() {
        assert_eq!(
            storage_gauge_line(371.85, None),
            "371.85 GB-h   formule inconnue, pas de quota"
        );
    }

    #[test]
    fn the_storage_block_says_public_repos_count() {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        let mut app = billing_app(org);
        assert_shown_at_every_size(&mut app, "Stockage Actions · GB-heures, dépôts publics compris");
        // Team: 371.85 of 2 GB × 720 h.
        assert_shown_at_every_size(&mut app, "371.85 / 1 440 GB-h");
        assert_shown_at_every_size(&mut app, "base 720 h");
    }

    /// Ranked by row position of the *storage figures*: "disconnected" also
    /// names a minutes row above the block, so searching for the repository
    /// name would find that row and pass whatever the storage order.
    #[test]
    fn the_storage_block_names_the_heaviest_repo_first() {
        let mut app = billing_app(exec_d_september_by_repo());
        assert_shown_at_every_size(&mut app, "359.88 GB-h");
        assert_shown_at_every_size(&mut app, "11.21 GB-h");

        let s = screen(&mut app, 100, 50);
        let row_of = |needle: &str| {
            s.lines()
                .position(|l| l.contains(needle))
                .unwrap_or_else(|| panic!("{needle:?} missing:\n{s}"))
        };
        assert!(row_of("359.88 GB-h") < row_of("11.21 GB-h"), "heaviest first:\n{s}");
        let heaviest = s.lines().nth(row_of("359.88 GB-h")).unwrap();
        assert!(heaviest.contains("disconnected"), "got: {heaviest}");
    }

    /// GitHub's documentation: deleting artifacts "does not remove charges
    /// already recorded". Mandatory, and both halves must survive a narrow
    /// frame, or the line says only the reassuring half.
    #[test]
    fn the_storage_block_says_deleting_does_not_refund() {
        let mut app = billing_app(exec_d_september());
        assert_shown_at_every_size(&mut app, "Supprimer des artefacts arrête l'accumulation,");
        assert_shown_at_every_size(&mut app, "mais ne rend pas les GB-heures déjà comptées.");
    }
```

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core -- storage_gauge_states_its_hour_base storage_gauge_without_a_plan the_storage_block_ > /tmp/bq-t8-red.txt 2>&1; cat /tmp/bq-t8-red.txt`
Expected: FAIL — `cannot find function 'storage_gauge_line' in this scope`.

- [ ] **Step 3: Implémenter**

Dans `crates/bondebarras-core/src/tui/views/billing.rs`, le `use crate::billing::…` devient :

```rust
use crate::billing::{
    self, BillingReport, MinuteLine, StorageLine, StorageQuota, included_minutes_for,
    sku_multiplier,
};
```

Remplacer la constante `MAX_MINUTE_LINES` (doc comment compris) par la suivante, puis remplacer ses trois usages dans `minutes_block` par `MAX_BREAKDOWN_LINES` :

```rust
/// The breakdowns — minutes and storage — are the tab's reason to exist, but
/// the area is bounded: past this many rows the tail is noise the user came
/// here to avoid, not signal. Both blocks share it, as #13 asks.
const MAX_BREAKDOWN_LINES: usize = 8;

/// What GitHub's documentation insists on, split in two so both halves
/// survive a narrow frame: deleting artifacts stops the accumulation but
/// refunds nothing already counted.
const DELETION_DOES_NOT_REFUND: [&str; 2] = [
    "Supprimer des artefacts arrête l'accumulation,",
    "  mais ne rend pas les GB-heures déjà comptées.",
];
```

Après `gauge_line` et `bar` :

```rust
/// A storage figure in hundredths of a GB-hour — the usage report's own
/// precision — so it can go through `gauges::percent`, the crate's single
/// percentage, which counts in whole units.
fn centi_gbh(gbh: f64) -> u64 {
    (gbh * 100.0).round() as u64
}

/// The storage ratio as a whole percentage: `gauges::percent` on hundredths
/// of a GB-hour, never a second formula. Every reader of that ratio goes
/// through here, so none can round it differently.
fn storage_percent(used: f64, quota: StorageQuota) -> u64 {
    gauges::percent(centi_gbh(used), centi_gbh(quota.gbh))
}

/// Actions storage consumed against the plan's included GB-hours, with the
/// hour base written out — the base is an open measurement, so the line
/// never lets a percentage stand without it. Not clamped, like the minutes.
pub fn storage_gauge_line(used: f64, quota: Option<StorageQuota>) -> String {
    let Some(quota) = quota else {
        return format!("{used:.2} GB-h   formule inconnue, pas de quota");
    };
    let percent = storage_percent(used, quota);
    format!(
        "{used:.2} / {} GB-h   {}  {} %   base {} h",
        thousands(quota.gbh.round() as u64),
        bar(percent),
        percent,
        quota.hours
    )
}

/// One row of the storage breakdown: the repository, then its GB-hours.
fn storage_line_row(line: &StorageLine) -> Line<'static> {
    let amount = format!("{:.2} GB-h", line.gbh);
    Line::from(Span::styled(
        format!("   {:<20}{amount:>12}", line.repo),
        theme::muted(),
    ))
}

/// The storage gauge, the repositories holding the storage, and what
/// deleting can and cannot do about it. No request of its own: the usage
/// report stage 1 loaded already carries every line.
fn storage_block(report: &BillingReport, month: &str, plan: Option<&str>) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "Stockage Actions · GB-heures, dépôts publics compris",
            theme::text_style(),
        )),
        Line::from(Span::styled(
            storage_gauge_line(report.storage_gbh(month), billing::storage_quota(plan, month)),
            theme::text_style(),
        )),
    ];
    let storage_lines = report.storage_lines(month);
    for line in storage_lines.iter().take(MAX_BREAKDOWN_LINES) {
        lines.push(storage_line_row(line));
    }
    if storage_lines.len() > MAX_BREAKDOWN_LINES {
        lines.push(Line::from(Span::styled(
            format!(
                "   … et {} autre(s) dépôt(s)",
                storage_lines.len() - MAX_BREAKDOWN_LINES
            ),
            theme::muted(),
        )));
    }
    for text in DELETION_DOES_NOT_REFUND {
        lines.push(Line::from(Span::styled(text, theme::muted())));
    }
    lines
}
```

Dans `tab_lines`, remplacer :

```rust
    lines.extend(minutes_block(report, &month, &private, plan));
    lines.push(Line::from(""));
    lines.extend(cost_block(report, &month));
```

par :

```rust
    lines.extend(minutes_block(report, &month, &private, plan));
    lines.push(Line::from(""));
    lines.extend(storage_block(report, &month, plan));
    lines.push(Line::from(""));
    lines.extend(cost_block(report, &month));
```

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core -- storage_gauge_states_its_hour_base storage_gauge_without_a_plan the_storage_block_ an_unknown_plan_shows_no_percentage_anywhere exec_d_september_reads_33_percent the_rendered_cost_line_is_in_dollars > /tmp/bq-t8-green.txt 2>&1; cat /tmp/bq-t8-green.txt`
Expected: PASS — les six tests neufs, et les trois tests de rendu des Tasks 1 et 5 (dont « aucun `%` sans formule », qui couvre maintenant la jauge de stockage).

- [ ] **Step 5: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t8-gate.txt 2>&1; tail -5 /tmp/bq-t8-gate.txt
git add crates/bondebarras-core/src/tui/views/billing.rs
git commit -m "feat(billing): bloc stockage Actions de l'onglet, base horaire affichee"
```

---

### Task 9: #13 — la colonne des dépôts : GB-heures du mois et ⚠ du plafond de cache

**Files:**
- Modify: `crates/bondebarras-core/src/tui/views/gauges.rs` (`cache_over_ceiling` ; module `tests`)
- Modify: `crates/bondebarras-core/src/tui/views/repos.rs` (fonctions neuves ; `render` ; la fonction qui construit la ligne d'un dépôt ; module `tests`)
- Modify: le doc comment de la constante de largeur de la colonne 2 (à `main` : `orgs::PANE_WIDTH` dans `tui/views/orgs.rs`)
- Modify: `README.md`, `CHANGELOG.md`, `CLAUDE.md`

**Interfaces:**
- Consumes: `gauges::CACHE_CEILING_BYTES` ; `billing::month_of`, `BillingReport::storage_gbh_for_repo` (Task 6) ; `views::render`, `Focus::Repos`.
- Produces :
  - `pub fn gauges::cache_over_ceiling(cache_bytes: u64) -> bool`
  - `pub fn repos::ceiling_mark(cache_bytes: u64) -> Span<'static>`
  - `pub fn repos::storage_detail_line(gbh: f64) -> Line<'static>`
  - `pub fn repos::repo_item(row: Vec<Span<'static>>, storage_gbh: Option<f64>) -> ListItem<'static>`
  - `fn repos::repo_storage_this_month(org: &OrgSummary, repo: &str, month: &str) -> Option<f64>`

La colonne 2 fait `Length(38)` (`orgs::PANE_WIDTH`, amendement 2 de tui-3-colonnes, `6dd71df`) : 36 cellules intérieures. La ligne de dépôt de la v0.5 en prend 35 (case 4, nom 10, âge ou classe 13, taille 8) ; la cellule du ⚠ — une espace ou ⚠, toujours présente, devant la taille des caches — prend la 36e. La ligne est alors **pleine** : les GB-heures vont sur une **ligne de détail** sous les seuls dépôts qui en ont ce mois-ci (spec, décision 18), et aucune colonne existante ne rétrécit pour eux.

- [ ] **Step 1: Écrire les tests qui échouent**

Dans le module `tests` de `crates/bondebarras-core/src/tui/views/gauges.rs` :

```rust
    /// #13: exactly 10 GiB is still inside the included cache storage; one
    /// byte more is not.
    #[test]
    fn cache_over_ceiling_is_strictly_above_ten_gibibytes() {
        assert!(!cache_over_ceiling(CACHE_CEILING_BYTES));
        assert!(cache_over_ceiling(CACHE_CEILING_BYTES + 1));
        assert!(!cache_over_ceiling(0));
    }
```

Dans le module `tests` de `crates/bondebarras-core/src/tui/views/repos.rs` (le créer avec `use super::*;` s'il n'existe pas) :

```rust
    /// exec-d's `disconnected`, holding this month's storage, beside a quiet
    /// repository. `cache_bytes` of the first is the only parameter.
    fn exec_d_with_storage(disconnected_cache_bytes: u64) -> App {
        let month = crate::billing::month_of(chrono::Utc::now());
        let repo = |name: &str, cache_bytes: u64| crate::model::RepoSummary {
            name: name.into(),
            cache_bytes,
            cache_count: 1,
            private: true,
            age_days: 3,
            class: crate::repos::RepoClass::Archivable,
        };
        let storage = crate::billing::UsageItem {
            month,
            product: "actions".into(),
            sku: "Actions storage".into(),
            quantity: 359.88,
            unit_type: "GigabyteHours".into(),
            gross: 0.0,
            discount: 0.0,
            net: 0.0,
            repo: "disconnected".into(),
        };
        let mut app = App::new(vec![crate::model::OrgSummary {
            login: "exec-d".into(),
            repos: vec![
                repo("disconnected", disconnected_cache_bytes),
                repo("quiet", 1_000),
            ],
            billing: Some(crate::billing::BillingReport { items: vec![storage] }),
            ..Default::default()
        }]);
        app.focus = crate::tui::app::Focus::Repos;
        app
    }

    fn whole_screen(app: &mut App, width: u16, height: u16) -> String {
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| crate::tui::views::render(app, f, None))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn repo_storage_detail_line_shows_gigabyte_hours() {
        let text: String = storage_detail_line(359.88)
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(text, "  ↳ 359.9 GB-h ce mois");
        let quiet: String = ceiling_mark(gauges::CACHE_CEILING_BYTES)
            .content
            .to_string();
        assert_eq!(quiet, " ", "exactly at the ceiling: no mark");
    }

    /// #13: find the repository holding the storage without opening it, and
    /// see a cache past 10 GiB marked — in the real layout, at every width
    /// (three, two and one column all have the repos column on screen with
    /// focus on it) and at every height from 8. The one-byte-under twin
    /// proves the ⚠ comes from the mark and nowhere else on screen.
    #[test]
    fn the_repos_column_shows_storage_and_the_ceiling_mark_at_every_width() {
        let sizes = (60..=200u16).map(|w| (w, 30)).chain((8..=30u16).map(|h| (100, h)));
        let mut over = exec_d_with_storage(gauges::CACHE_CEILING_BYTES + 1);
        let mut at = exec_d_with_storage(gauges::CACHE_CEILING_BYTES);
        for (width, height) in sizes {
            let s = whole_screen(&mut over, width, height);
            assert!(
                s.contains("↳ 359.9 GB-h ce mois"),
                "storage detail missing at {width}x{height}: {s}"
            );
            assert!(s.contains('⚠'), "ceiling mark missing at {width}x{height}: {s}");

            let s = whole_screen(&mut at, width, height);
            assert!(!s.contains('⚠'), "a cache at exactly 10 GiB is marked at {width}x{height}: {s}");
        }
    }
```

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core -- cache_over_ceiling_is_strictly repo_storage_detail_line the_repos_column_shows_storage > /tmp/bq-t9-red.txt 2>&1; cat /tmp/bq-t9-red.txt`
Expected: FAIL — `cannot find function 'cache_over_ceiling'`, `cannot find function 'storage_detail_line'`.

- [ ] **Step 3: Implémenter**

Dans `crates/bondebarras-core/src/tui/views/gauges.rs`, après `CACHE_CEILING_BYTES` :

```rust
/// Whether a repository's caches are past the included 10 GiB.
///
/// Strictly above: exactly `CACHE_CEILING_BYTES` is still inside it. Past it,
/// GitHub evicts the least recently read caches — or, if the repository's
/// cache limit was raised above the included 10 GB, bills the excess at its
/// hourly peak (GitHub's Actions billing documentation).
pub fn cache_over_ceiling(cache_bytes: u64) -> bool {
    cache_bytes > CACHE_CEILING_BYTES
}
```

Dans `crates/bondebarras-core/src/tui/views/repos.rs`, ajouter aux `use` ce qui manque parmi :

```rust
use crate::model::OrgSummary;
use crate::tui::theme;
use crate::tui::views::gauges;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;
```

puis, avant `pub fn render` :

```rust
/// `⚠` before the cache figure of a repository past the included 10 GiB
/// (`gauges::cache_over_ceiling`), a space otherwise, so figures stay
/// aligned. The row has no room to say why; column 3's cache gauge does, the
/// moment the repository is opened.
pub fn ceiling_mark(cache_bytes: u64) -> Span<'static> {
    if gauges::cache_over_ceiling(cache_bytes) {
        Span::styled("⚠", theme::status_warn())
    } else {
        Span::raw(" ")
    }
}

/// The detail line under a repository that held Actions storage this month,
/// so the one holding it is found without opening every repository. On its
/// own line because the repository row has no cell left once `ceiling_mark`
/// takes its one.
pub fn storage_detail_line(gbh: f64) -> Line<'static> {
    Line::from(Span::styled(
        format!("  ↳ {gbh:.1} GB-h ce mois"),
        theme::muted(),
    ))
}

/// A repository's list item: its row, plus the storage detail line when it
/// held any this month. Only those repositories take a second line; the
/// cursor still moves one repository at a time.
pub fn repo_item(row: Vec<Span<'static>>, storage_gbh: Option<f64>) -> ListItem<'static> {
    match storage_gbh {
        Some(gbh) => ListItem::new(vec![Line::from(row), storage_detail_line(gbh)]),
        None => ListItem::new(Line::from(row)),
    }
}

/// This month's GB-hours for one repository, when there is something to
/// show: `None` when billing is unreadable (unknown, not zero) and when the
/// repository held none (nothing to say).
fn repo_storage_this_month(org: &OrgSummary, repo: &str, month: &str) -> Option<f64> {
    org.billing
        .as_ref()
        .map(|b| b.storage_gbh_for_repo(month, repo))
        .filter(|gbh| *gbh > 0.0)
}
```

Dans la fonction qui construit les spans de la ligne d'un dépôt (à `3712c60`, `orgs::repo_row_spans` ; après la Task 4 de tui-3-colonnes, son équivalent dans `repos.rs`), insérer `ceiling_mark(repo.cache_bytes)` **juste avant** le span qui formate `human_size(repo.cache_bytes)` :

```rust
        ceiling_mark(repo.cache_bytes),
        Span::styled(
            format!("{:>8}", human_size(repo.cache_bytes)),
            theme::muted(),
        ),
```

(Garder le format réel du span de taille tel que la Task 4 l'a laissé ; seule l'insertion compte.)

Dans `repos::render`, calculer le mois une fois avant la construction des éléments :

```rust
    let month = crate::billing::month_of(chrono::Utc::now());
```

et remplacer la construction de chaque élément `ListItem::new(Line::from(spans))` par :

```rust
repo_item(spans, repo_storage_this_month(org, &repo.name, &month))
```

où `org` est l'organisation courante déjà lue par `render` et `spans` la ligne du dépôt.

La cellule du ⚠ consomme la dernière cellule libre de la colonne 2 (35 + 1 = 36 sur 36). Dans le même commit, mettre à jour le doc comment de la constante de largeur de la colonne (à `main` : `orgs::PANE_WIDTH`, qui dit qu'une des deux cellules de marge « stays unused ») : cette cellule porte désormais le ⚠, et la ligne n'a plus de marge.

**Si un test de balayage existant de la colonne 2 échoue** (nom, âge, classe ou taille coupés) — par exemple parce que la Task 4 de tui-3-colonnes a ajouté une cellule à la ligne —, **s'arrêter et le signaler**. Ne pas réduire la largeur du nom (10 cellules, le budget de l'amendement 2 de tui-3-colonnes) ni élargir la colonne (les seuils 100 / 78 en dérivent) sans arbitrage.

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core -- cache_over_ceiling_is_strictly repo_storage_detail_line the_repos_column_shows_storage > /tmp/bq-t9-green.txt 2>&1; cat /tmp/bq-t9-green.txt`
Expected: PASS (3 tests). Puis `cargo test -p bondebarras-core stays_legible > /tmp/bq-t9-legible.txt 2>&1; cat /tmp/bq-t9-legible.txt` : les balayages existants de la colonne 2 passent.

- [ ] **Step 5: Documentation**

`README.md` — dans la puce `- **Billing tab** —`, après la ligne `a minimum, since that allowance is shared across the enterprise.)`, ajouter :

```markdown
  Below the minutes, **Actions storage**, billed in GB-hours — every hour a
  gigabyte of artifacts exists — against the plan's included storage
  (`free` 0.5 GB, `team` 2 GB, `enterprise` 50 GB) times the hours of the
  displayed month, a base the gauge states (`base 720 h`). Public
  repositories' storage is counted: GitHub's documentation says their
  minutes are free, and says nothing of their storage. The repositories
  holding it are named, heaviest first, and the tab says what deleting
  artifacts can and cannot do: it stops the accumulation, it does not refund
  hours already counted. The repositories column shows, under a repository's
  row, its GB-hours for the current month, and marks with ⚠ a repository
  whose caches exceed the included 10 GiB — past which GitHub evicts, or
  bills the excess at its hourly peak if the repository's cache limit was
  raised.
```

Dans le paragraphe `scan --json` ajouté par la Task 5, remplacer `minutes, or `null` — never a default).` par :

```markdown
minutes, or `null` — never a default). `billing_month` names the current
month (`YYYY-MM`), for which `storage_gbh` and `storage_allowance_gbh` are
given per organization and `storage_gbh` per repository — `null` when billing
or the plan cannot be read, never zero.
```

`CHANGELOG.md` — sous `## [Unreleased]`, `### Added` :

```markdown
- Billing tab: Actions storage, in GB-hours, against the plan's included
  storage (`free` 0.5 GB, `team` 2 GB, `enterprise` 50 GB) times the
  displayed month's hours — the base is written on the gauge — with the
  heaviest repositories named and a fixed line saying deleting artifacts
  stops the accumulation but refunds nothing already counted. Public
  repositories' storage is counted, and the tab says so. No extra request:
  the usage report loaded at stage 1 already carried it. Measured on
  2026-09-10: exec-d at 371.85 GB-hours in September, 359.88 of them in
  `disconnected`. (#13)
- Repositories column: a repository's GB-hours for the current month, on a
  detail line under its row, and a ⚠ before a cache footprint past the
  included 10 GiB. (#13)
- `scan --json`: `billing_month`, `storage_gbh` and `storage_allowance_gbh`
  per organization, `storage_gbh` per repository. (#13)
```

`CLAUDE.md` — dans la puce `- **No allowance, no percentage.**` ajoutée par la Task 5, après `never a percentage against a guessed figure.`, ajouter :

```markdown
  Included Actions storage follows the same rule through
  `billing::storage_quota`: included GB × the displayed month's hours
  (`billing::hours_in_month`, days × 24 — the 720-vs-744 question is an open
  measurement recorded in the billing-quotas design spec).
```

- [ ] **Step 6: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t9-gate.txt 2>&1; tail -5 /tmp/bq-t9-gate.txt
git add -A crates/bondebarras-core/src README.md CHANGELOG.md CLAUDE.md
git commit -m "feat(tui): stockage Actions par depot et plafond de cache dans la colonne des depots (#13)"
```

---

### Task 10: #14 — le budget Actions (sélection pure)

**Files:**
- Modify: `crates/bondebarras-core/src/billing.rs` (type et fonctions après `is_actions_storage` ; module `tests`)

**Interfaces:**
- Consumes: rien de neuf.
- Produces (Tasks 11, 12) :
  - `pub struct Budget { pub budget_type: String, pub sku: String, pub scope: String, pub amount: u64, pub blocking: bool }` (`Debug, Clone, PartialEq, Eq`)
  - `pub const BUDGET_WARNING_PERCENT: u64 = 90`
  - `pub fn actions_budget(budgets: &[Budget]) -> Option<&Budget>`
  - `pub fn actions_sku_budgets(budgets: &[Budget]) -> Vec<&Budget>`
  - `pub fn nears_blocking_budget(percent: u64, budget: Option<&Budget>) -> bool`

- [ ] **Step 1: Écrire les tests qui échouent**

Dans le module `tests` de `crates/bondebarras-core/src/billing.rs` :

```rust
    fn budget(budget_type: &str, sku: &str, amount: u64, blocking: bool) -> Budget {
        Budget {
            budget_type: budget_type.into(),
            sku: sku.into(),
            scope: "organization".into(),
            amount,
            blocking,
        }
    }

    /// exec-d's real response on 2026-09-10: four org-level product budgets,
    /// all 0 $ and blocking. Identical amounts, so the test must check *which*
    /// budget came back — and the Actions one is third, so "the first
    /// budget" fails.
    #[test]
    fn actions_budget_picks_the_org_actions_product_budget() {
        let exec_d = vec![
            budget("ProductPricing", "codespaces", 0, true),
            budget("ProductPricing", "packages", 0, true),
            budget("ProductPricing", "actions", 0, true),
            budget("ProductPricing", "git_lfs", 0, true),
        ];
        let b = actions_budget(&exec_d).expect("exec-d has an Actions budget");
        assert_eq!(b.sku, "actions");
        assert_eq!((b.amount, b.blocking), (0, true));

        // cloudalpes: 5 $, blocking.
        let cloudalpes = vec![budget("ProductPricing", "actions", 5, true)];
        assert_eq!(actions_budget(&cloudalpes).map(|b| b.amount), Some(5));

        // SecondBrain-io: no budget at all.
        assert!(actions_budget(&[]).is_none());

        // An Actions budget of another scope is not the organization's.
        let mut repo_scoped = budget("ProductPricing", "actions", 5, true);
        repo_scoped.scope = "repository".into();
        assert!(actions_budget(&[repo_scoped]).is_none());
    }

    /// Never observed, so never interpreted — and never silently ignored.
    /// SKU names here are illustrative: the real shape is an open
    /// measurement.
    #[test]
    fn a_sku_pricing_actions_budget_is_surfaced_not_ignored() {
        let budgets = vec![
            budget("SkuPricing", "actions_linux", 5, true),
            budget("SkuPricing", "codespaces_storage", 5, true),
            budget("ProductPricing", "actions", 0, true),
        ];
        let skus: Vec<&str> = actions_sku_budgets(&budgets)
            .iter()
            .map(|b| b.sku.as_str())
            .collect();
        assert_eq!(skus, vec!["actions_linux"]);
        // It does not stand in for the product budget.
        assert_eq!(actions_budget(&budgets).map(|b| b.sku.as_str()), Some("actions"));
    }

    #[test]
    fn nears_blocking_budget_needs_ninety_percent_and_a_blocking_budget() {
        let blocking = budget("ProductPricing", "actions", 0, true);
        let alert_only = budget("ProductPricing", "actions", 5, false);
        assert!(nears_blocking_budget(90, Some(&blocking)));
        assert!(nears_blocking_budget(103, Some(&blocking)));
        assert!(!nears_blocking_budget(89, Some(&blocking)));
        assert!(!nears_blocking_budget(95, Some(&alert_only)));
        assert!(!nears_blocking_budget(95, None));
    }
```

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core -- actions_budget_picks a_sku_pricing_actions_budget nears_blocking_budget > /tmp/bq-t10-red.txt 2>&1; cat /tmp/bq-t10-red.txt`
Expected: FAIL — `cannot find struct, variant or union type 'Budget'`.

- [ ] **Step 3: Implémenter**

Dans `crates/bondebarras-core/src/billing.rs`, après `fn is_actions_storage` :

```rust
/// One budget from `GET /organizations/{org}/settings/billing/budgets`.
///
/// Read-only, permanently: changing a budget commits money. Every field is
/// required — an entry missing one makes the whole listing unreadable (see
/// `api::budgets::fetch`), since the dropped entry could be the Actions
/// budget, and "no budget: overage billed" would then be said of an org
/// GitHub actually blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    /// `budget_type`: `ProductPricing` or `SkuPricing`.
    pub budget_type: String,
    /// `budget_product_sku`: a product (`actions`) or, for `SkuPricing`, a SKU.
    pub sku: String,
    /// `budget_scope`: `organization`, `repository`, `enterprise`, …
    pub scope: String,
    /// `budget_amount`, whole US dollars — an integer in GitHub's schema.
    pub amount: u64,
    /// `prevent_further_usage`: GitHub stops the usage once the budget is spent.
    pub blocking: bool,
}

/// From this percentage of a gauge, a blocking budget earns a warning.
pub const BUDGET_WARNING_PERCENT: u64 = 90;

/// The organization's Actions budget: scope `organization`, type
/// `ProductPricing`, product `actions`. `None` when there is none — which,
/// on a readable listing, means overage is billed with no ceiling.
pub fn actions_budget(budgets: &[Budget]) -> Option<&Budget> {
    budgets.iter().find(|b| {
        b.scope == "organization" && b.budget_type == "ProductPricing" && b.sku == "actions"
    })
}

/// Organization budgets on a single Actions SKU (`SkuPricing`). Never
/// observed on the author's organizations, so never interpreted: the tab
/// names each one instead of folding it into the product budget's meaning.
pub fn actions_sku_budgets(budgets: &[Budget]) -> Vec<&Budget> {
    budgets
        .iter()
        .filter(|b| {
            b.scope == "organization"
                && b.budget_type == "SkuPricing"
                && b.sku.to_ascii_lowercase().starts_with("actions")
        })
        .collect()
}

/// Whether a gauge at `percent` should warn that GitHub will stop Actions
/// once the allowance runs out: at least `BUDGET_WARNING_PERCENT`, with a
/// blocking Actions budget. Takes the percentage the gauge displays, so the
/// warning can never contradict it.
pub fn nears_blocking_budget(percent: u64, budget: Option<&Budget>) -> bool {
    percent >= BUDGET_WARNING_PERCENT && budget.is_some_and(|b| b.blocking)
}
```

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core -- actions_budget_picks a_sku_pricing_actions_budget nears_blocking_budget > /tmp/bq-t10-green.txt 2>&1; cat /tmp/bq-t10-green.txt`
Expected: PASS (3 tests).

- [ ] **Step 5: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t10-gate.txt 2>&1; tail -5 /tmp/bq-t10-gate.txt
git add crates/bondebarras-core/src/billing.rs
git commit -m "feat(billing): budget Actions de l'organisation, selection pure"
```

---

### Task 11: #14 — lire les budgets à l'étage 1, jusque dans `scan --json`

**Files:**
- Create: `crates/bondebarras-core/src/api/budgets.rs`
- Modify: `crates/bondebarras-core/src/api/mod.rs`, `crates/bondebarras-core/src/model.rs` (`OrgSummary`), `crates/bondebarras-core/src/scan.rs` (`overview` ; `tests`), `crates/bondebarras-core/src/commands/scan.rs` (`org_json` ; `tests`)
- Modify: `CLAUDE.md` (table)

**Interfaces:**
- Consumes: `Client::get_json` ; `billing::{Budget, actions_budget, actions_sku_budgets}` (Task 10).
- Produces (Task 12) :
  - `pub async fn api::budgets::fetch(client: &Client, org: &str) -> Option<Vec<Budget>>`
  - `OrgSummary.budgets: Option<Vec<crate::billing::Budget>>`
  - JSON : `budgets_readable`, `actions_budget`, `actions_sku_budgets`

- [ ] **Step 1: Écrire les tests qui échouent**

Ajouter `pub mod budgets;` entre `pub mod billing;` et `pub mod caches;` dans `crates/bondebarras-core/src/api/mod.rs`, puis créer `crates/bondebarras-core/src/api/budgets.rs` :

```rust
//! Organization budgets — read-only, permanently.

use super::Client;
use crate::billing::Budget;

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const BUDGETS: &str = "/organizations/exec-d/settings/billing/budgets";

    /// exec-d's real response on 2026-09-10, identifiers masked as in #14.
    fn exec_d_response() -> serde_json::Value {
        serde_json::json!({
          "budgets": [
            {"id": "…", "budget_type": "ProductPricing", "budget_product_sku": "codespaces", "budget_scope": "organization", "budget_amount": 0, "prevent_further_usage": true, "budget_entity_name": "exec-d", "budget_alerting": {"will_alert": true, "alert_recipients": ["…"]}},
            {"id": "…", "budget_type": "ProductPricing", "budget_product_sku": "packages", "budget_scope": "organization", "budget_amount": 0, "prevent_further_usage": true, "budget_entity_name": "exec-d", "budget_alerting": {"will_alert": true, "alert_recipients": ["…"]}},
            {"id": "…", "budget_type": "ProductPricing", "budget_product_sku": "actions", "budget_scope": "organization", "budget_amount": 0, "prevent_further_usage": true, "budget_entity_name": "exec-d", "budget_alerting": {"will_alert": true, "alert_recipients": ["…"]}},
            {"id": "…", "budget_type": "ProductPricing", "budget_product_sku": "git_lfs", "budget_scope": "organization", "budget_amount": 0, "prevent_further_usage": true, "budget_entity_name": "exec-d", "budget_alerting": {"will_alert": true, "alert_recipients": ["…"]}}
          ],
          "has_next_page": false,
          "total_count": 4
        })
    }

    fn budget_json(sku: &str, amount: u64) -> serde_json::Value {
        serde_json::json!({
            "budget_type": "ProductPricing", "budget_product_sku": sku,
            "budget_scope": "organization", "budget_amount": amount,
            "prevent_further_usage": true
        })
    }

    #[tokio::test]
    async fn budgets_fetch_maps_every_budget_of_the_sample() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(BUDGETS))
            .respond_with(ResponseTemplate::new(200).set_body_json(exec_d_response()))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let budgets = fetch(&client, "exec-d").await.expect("a 200 is readable");

        assert_eq!(budgets.len(), 4);
        assert_eq!(budgets[2].sku, "actions");
        assert_eq!(budgets[2].budget_type, "ProductPricing");
        assert_eq!(budgets[2].scope, "organization");
        assert_eq!((budgets[2].amount, budgets[2].blocking), (0, true));
        let actions = crate::billing::actions_budget(&budgets).expect("one Actions budget");
        assert_eq!(actions.sku, "actions");
    }

    /// Observed on the three orgs the account does not own: 400, although
    /// the documentation announces 403, 404 or 500. Any non-2xx degrades.
    #[tokio::test]
    async fn budgets_fetch_degrades_a_400_to_unreadable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/organizations/le-vilain-petit-dev/settings/billing/budgets"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(serde_json::json!({ "message": "Unable to get budgets." })),
            )
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(fetch(&client, "le-vilain-petit-dev").await.is_none());
    }

    /// The Actions budget sits on page 2: a single-page read finds only
    /// codespaces, and would call exec-d an org with no Actions budget.
    #[tokio::test]
    async fn budgets_fetch_reads_the_next_page() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(BUDGETS))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "budgets": [budget_json("codespaces", 0)],
                "has_next_page": true,
                "total_count": 2
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(BUDGETS))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "budgets": [budget_json("actions", 5)],
                "has_next_page": false,
                "total_count": 2
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let budgets = fetch(&client, "exec-d").await.expect("readable");

        assert_eq!(budgets.len(), 2);
        assert_eq!(crate::billing::actions_budget(&budgets).map(|b| b.amount), Some(5));
    }

    /// Dropping the malformed entry would leave `Some([codespaces])` — "no
    /// Actions budget, overage billed" — for an org whose Actions budget
    /// merely lacked an amount. Unreadable is the only honest answer.
    #[tokio::test]
    async fn budgets_fetch_treats_a_malformed_entry_as_unreadable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(BUDGETS))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "budgets": [
                    budget_json("codespaces", 0),
                    { "budget_type": "ProductPricing", "budget_product_sku": "actions",
                      "budget_scope": "organization", "prevent_further_usage": true }
                ],
                "has_next_page": false
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(fetch(&client, "exec-d").await.is_none());
    }

    /// A tenth page that still announces a next one: a partial list could
    /// miss the Actions budget, so it reads as unreadable, not as truncated.
    #[tokio::test]
    async fn budgets_fetch_gives_up_rather_than_truncate() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(BUDGETS))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "budgets": [budget_json("codespaces", 0)],
                "has_next_page": true
            })))
            .expect(10)
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(fetch(&client, "exec-d").await.is_none());
    }
}
```

Dans le module `tests` de `crates/bondebarras-core/src/scan.rs` :

```rust
    /// #14: budgets ride along at stage 1, and a refusal — observed as a 400
    /// — costs the budgets only, never the org.
    #[tokio::test]
    async fn overview_carries_budgets_and_keeps_the_org_when_refused() {
        let server = MockServer::start().await;
        for org in ["exec-d", "le-vilain-petit-dev"] {
            Mock::given(method("GET"))
                .and(path(format!("/orgs/{org}/actions/cache/usage-by-repository")))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "repository_cache_usages": [] })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path(format!("/orgs/{org}/repos")))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/organizations/exec-d/settings/billing/budgets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "budgets": [{
                    "budget_type": "ProductPricing", "budget_product_sku": "actions",
                    "budget_scope": "organization", "budget_amount": 0,
                    "prevent_further_usage": true
                }],
                "has_next_page": false
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/organizations/le-vilain-petit-dev/settings/billing/budgets"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(serde_json::json!({ "message": "Unable to get budgets." })),
            )
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = overview(
            &client,
            &["exec-d".to_string(), "le-vilain-petit-dev".to_string()],
        )
        .await;

        let find = |login: &str| out.iter().find(|o| o.login == login).unwrap();
        assert_eq!(out.len(), 2, "a refused budgets read must not drop the org");
        assert_eq!(find("exec-d").budgets.as_ref().map(Vec::len), Some(1));
        assert!(find("le-vilain-petit-dev").budgets.is_none());
    }
```

Dans le module `tests` de `crates/bondebarras-core/src/commands/scan.rs` :

```rust
    fn budget(budget_type: &str, sku: &str, amount: u64) -> crate::billing::Budget {
        crate::billing::Budget {
            budget_type: budget_type.into(),
            sku: sku.into(),
            scope: "organization".into(),
            amount,
            blocking: true,
        }
    }

    /// "No budget" and "budgets unreadable" must never produce the same
    /// object: the first means overage is billed without a ceiling, the
    /// second means nobody knows.
    #[test]
    fn scan_json_tells_no_budget_from_unreadable_budgets() {
        let blocked = OrgSummary {
            login: "exec-d".into(),
            budgets: Some(vec![
                budget("ProductPricing", "codespaces", 0),
                budget("ProductPricing", "actions", 0),
                budget("SkuPricing", "actions_linux", 5),
            ]),
            ..Default::default()
        };
        let no_budget = OrgSummary {
            login: "SecondBrain-io".into(),
            budgets: Some(vec![]),
            ..Default::default()
        };
        let unreadable = OrgSummary {
            login: "le-vilain-petit-dev".into(),
            ..Default::default()
        };

        let v = overview_json(&[blocked, no_budget, unreadable], "2026-09");

        assert_eq!(v[0]["budgets_readable"], true);
        assert_eq!(v[0]["actions_budget"], serde_json::json!({ "amount": 0, "blocking": true }));
        assert_eq!(
            v[0]["actions_sku_budgets"],
            serde_json::json!([{ "sku": "actions_linux", "amount": 5, "blocking": true }])
        );

        assert_eq!(v[1]["budgets_readable"], true);
        assert!(v[1]["actions_budget"].is_null(), "got: {}", v[1]);
        assert_eq!(v[1]["actions_sku_budgets"], serde_json::json!([]));

        assert_eq!(v[2]["budgets_readable"], false);
        assert!(v[2]["actions_budget"].is_null(), "got: {}", v[2]);
        assert!(v[2]["actions_sku_budgets"].is_null(), "got: {}", v[2]);
    }
```

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core -- budgets_fetch_ overview_carries_budgets scan_json_tells_no_budget > /tmp/bq-t11-red.txt 2>&1; cat /tmp/bq-t11-red.txt`
Expected: FAIL — `cannot find function 'fetch' in this scope`, `no field 'budgets' on type 'OrgSummary'`.

- [ ] **Step 3: Implémenter**

Dans `crates/bondebarras-core/src/api/budgets.rs`, entre les `use` et `#[cfg(test)]` :

```rust
/// GitHub's maximum page size for this endpoint (its default is 10).
const PAGE_SIZE: usize = 100;

/// Hard stop on pagination: a thousand budgets. Past it, the listing reads
/// as unreadable rather than truncated — see `fetch`.
const MAX_BUDGET_PAGES: u32 = 10;

/// Every budget of an organization, or `None` when they cannot be read.
///
/// GitHub reserves the endpoint to organization admins and billing managers.
/// Its documentation announces 403, 404 or 500 for anyone else; the real
/// answer on three orgs, on 2026-09-10, was a 400 `Unable to get budgets.`.
/// Any failure degrades the same way, like `api::billing::fetch`.
///
/// `None` — unreadable — is never collapsed into an empty list — no budget.
/// A failed page, an entry missing one of the five fields read, and a tenth
/// page still announcing a next one all read as unreadable: each could hide
/// the Actions budget, and "no budget: overage billed" would then be said of
/// an org GitHub actually blocks. An absent `has_next_page` is the last page.
pub async fn fetch(client: &Client, org: &str) -> Option<Vec<Budget>> {
    let mut out = Vec::new();
    for page in 1..=MAX_BUDGET_PAGES {
        let v = client
            .get_json(&format!(
                "/organizations/{org}/settings/billing/budgets?per_page={PAGE_SIZE}&page={page}"
            ))
            .await
            .ok()?;
        for item in v["budgets"].as_array()? {
            out.push(parse_budget(item)?);
        }
        if !v["has_next_page"].as_bool().unwrap_or(false) {
            return Some(out);
        }
    }
    None
}

/// One budget, or `None` when any of the five fields read is missing or
/// mistyped.
fn parse_budget(item: &serde_json::Value) -> Option<Budget> {
    Some(Budget {
        budget_type: item["budget_type"].as_str()?.to_string(),
        sku: item["budget_product_sku"].as_str()?.to_string(),
        scope: item["budget_scope"].as_str()?.to_string(),
        amount: item["budget_amount"].as_u64()?,
        blocking: item["prevent_further_usage"].as_bool()?,
    })
}
```

Dans `crates/bondebarras-core/src/model.rs`, ajouter à `OrgSummary` après `plan` :

```rust
    /// The org's budgets, or `None` when they cannot be read — GitHub
    /// reserves them to admins and billing managers. `Some(vec![])` is a
    /// readable org with no budget at all; the two must never be confused.
    pub budgets: Option<Vec<crate::billing::Budget>>,
```

Dans `scan::overview`, la jointure et la construction deviennent :

```rust
        let (billing, plan, budgets) = futures::join!(
            crate::api::billing::fetch(client, org),
            crate::api::orgs::plan(client, org),
            crate::api::budgets::fetch(client, org),
        );

        Some(OrgSummary {
            login: org.clone(),
            cache_bytes,
            cache_count,
            repos: repos_out,
            billing,
            plan,
            budgets,
        })
```

Dans `crates/bondebarras-core/src/commands/scan.rs`, `org_json` devient :

```rust
fn org_json(o: &OrgSummary, month: &str) -> serde_json::Value {
    let plan = o.plan.as_deref();
    let budgets = o.budgets.as_deref();
    serde_json::json!({
        "org": o.login,
        "cache_bytes": o.cache_bytes,
        "cache_count": o.cache_count,
        "billing_readable": o.billing.is_some(),
        "plan": o.plan,
        "minutes_allowance": billing::included_minutes_for(plan),
        "billing_month": month,
        "storage_gbh": o.billing.as_ref().map(|b| b.storage_gbh(month)),
        "storage_allowance_gbh": billing::storage_quota(plan, month).map(|q| q.gbh),
        "budgets_readable": budgets.is_some(),
        "actions_budget": budgets.and_then(billing::actions_budget).map(|b| serde_json::json!({
            "amount": b.amount,
            "blocking": b.blocking,
        })),
        "actions_sku_budgets": budgets.map(|all| billing::actions_sku_budgets(all)
            .into_iter()
            .map(|b| serde_json::json!({ "sku": b.sku, "amount": b.amount, "blocking": b.blocking }))
            .collect::<Vec<_>>()),
        "repos": o.repos.iter().map(|r| serde_json::json!({
            "name": r.name,
            "cache_bytes": r.cache_bytes,
            "cache_count": r.cache_count,
            "storage_gbh": o.billing.as_ref().map(|b| b.storage_gbh_for_repo(month, &r.name)),
        })).collect::<Vec<_>>(),
    })
}
```

`CLAUDE.md` — dans la table, après la ligne de `api/orgs.rs` :

```markdown
| Organization budgets (read-only; paginated; any failure, malformed entry or truncation → unreadable, never "no budget") | `crates/bondebarras-core/src/api/budgets.rs` |
```

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core -- budgets_fetch_ overview_carries_budgets scan_json_tells_no_budget > /tmp/bq-t11-green.txt 2>&1; cat /tmp/bq-t11-green.txt`
Expected: PASS (7 tests).

- [ ] **Step 5: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t11-gate.txt 2>&1; tail -5 /tmp/bq-t11-gate.txt
git add -A crates/bondebarras-core/src CLAUDE.md
git commit -m "feat(api): budgets de l'organisation a l'etage 1, illisibles sans confusion"
```

---

### Task 12: #14 — le budget dans l'onglet, et l'avertissement sous les jauges

**Files:**
- Modify: `crates/bondebarras-core/src/tui/views/billing.rs` (`use` ; `budget_lines`, `budget_warning_lines` neuves ; `minutes_block`, `storage_block`, `tab_lines` ; module `tests`)
- Modify: `README.md`, `CHANGELOG.md`, `CLAUDE.md`

**Interfaces:**
- Consumes: `billing::{Budget, actions_budget, actions_sku_budgets, nears_blocking_budget}` (Task 10) ; `gauges::percent` (existant) et `storage_percent` (Task 8) ; `OrgSummary.budgets` (Task 11) ; `usd`, `tab_lines`, `minutes_block`, `storage_block`, helpers de test (Tasks 1, 5, 8).
- Produces (Task 14) :
  - `fn budget_lines(budgets: Option<&[Budget]>) -> Vec<Line<'static>>`
  - `fn budget_warning_lines(quota: &str, percent: u64, budget: Option<&Budget>) -> Vec<Line<'static>>`
  - `fn minutes_block(report: &BillingReport, month: &str, private: &HashSet<String>, plan: Option<&str>, budget: Option<&Budget>) -> Vec<Line<'static>>`
  - `fn storage_block(report: &BillingReport, month: &str, plan: Option<&str>, budget: Option<&Budget>) -> Vec<Line<'static>>`

- [ ] **Step 1: Écrire les tests qui échouent**

Dans le module `tests` de `crates/bondebarras-core/src/tui/views/billing.rs`, ajouter `use crate::billing::Budget;` puis :

```rust
    fn with_budgets(mut org: OrgSummary, budgets: Option<Vec<Budget>>) -> App {
        org.budgets = budgets;
        billing_app(org)
    }

    fn actions(amount: u64, blocking: bool) -> Budget {
        Budget {
            budget_type: "ProductPricing".into(),
            sku: "actions".into(),
            scope: "organization".into(),
            amount,
            blocking,
        }
    }

    /// 2 850 of Team's 3 000 minutes: 95 %. An illustrative figure — #14
    /// gives budgets, not a 95 % month.
    fn team_at_95_percent() -> OrgSummary {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        org.billing = Some(BillingReport {
            items: vec![usage("2026-09", "Actions Linux", "Minutes", 2_850.0, 17.1, "disconnected")],
        });
        org
    }

    #[test]
    fn budget_line_reads_zero_dollars_blocking() {
        let mut app = with_budgets(exec_d_september(), Some(vec![actions(0, true)]));
        assert_shown_at_every_size(&mut app, "Budget Actions : 0.00 $ · bloquant");
    }

    #[test]
    fn budget_line_reads_five_dollars_blocking() {
        let mut org = exec_d_september();
        org.login = "cloudalpes".into();
        let mut app = with_budgets(org, Some(vec![actions(5, true)]));
        assert_shown_at_every_size(&mut app, "Budget Actions : 5.00 $ · bloquant");
    }

    #[test]
    fn budget_line_reads_no_budget_as_billed_overage() {
        let mut org = exec_d_september();
        org.login = "SecondBrain-io".into();
        let mut app = with_budgets(org, Some(vec![]));
        assert_shown_at_every_size(&mut app, "Budget Actions : aucun, dépassement facturé sans plafond");
        assert_shown_at_every_size(&mut app, "(si un moyen de paiement est enregistré)");
        assert_absent_at_every_width(&mut app, "Budget Actions : illisible"); // scoped: Task 14's retention line may say "illisible" on its own
    }

    #[test]
    fn budget_line_reads_unreadable_budgets() {
        let mut org = exec_d_september();
        org.login = "le-vilain-petit-dev".into();
        let mut app = with_budgets(org, None);
        assert_shown_at_every_size(&mut app, "Budget Actions : illisible");
        assert_shown_at_every_size(&mut app, "(réservé aux admins et gestionnaires de facturation)");
        assert_absent_at_every_width(&mut app, "aucun, dépassement");
    }

    #[test]
    fn a_sku_budget_line_is_signalled_not_interpreted() {
        let sku = Budget {
            budget_type: "SkuPricing".into(),
            sku: "actions_linux".into(),
            scope: "organization".into(),
            amount: 5,
            blocking: true,
        };
        let mut app = with_budgets(exec_d_september(), Some(vec![actions(0, true), sku]));
        assert_shown_at_every_size(&mut app, "Budget SKU actions_linux : 5.00 $ · bloquant");
        assert_shown_at_every_size(&mut app, "signalé, non pris en compte par les avertissements");
    }

    #[test]
    fn a_blocking_budget_at_95_percent_warns_under_the_gauge() {
        let mut app = with_budgets(team_at_95_percent(), Some(vec![actions(0, true)]));
        assert_shown_at_every_size(&mut app, "⚠ 95 % du quota de minutes, budget 0.00 $ bloquant :");
        assert_shown_at_every_size(&mut app, "GitHub bloquera l'usage Actions au quota atteint.");
    }

    #[test]
    fn a_five_dollar_blocking_budget_warns_it_bills_then_blocks() {
        let mut app = with_budgets(team_at_95_percent(), Some(vec![actions(5, true)]));
        assert_shown_at_every_size(&mut app, "⚠ 95 % du quota de minutes, budget 5.00 $ bloquant :");
        assert_shown_at_every_size(&mut app, "facturé jusqu'à 5.00 $, puis usage Actions bloqué.");
    }

    /// exec-d's real September storage, on Free, with its real 0 $ blocking
    /// Actions budget: 103 % of 360 GB-h — the storage gauge warns too.
    #[test]
    fn the_storage_gauge_warns_on_a_blocking_budget_too() {
        let mut org = exec_d_september();
        org.plan = Some("free".into());
        let mut app = with_budgets(org, Some(vec![actions(0, true)]));
        assert_shown_at_every_size(&mut app, "⚠ 103 % du quota de stockage, budget 0.00 $ bloquant :");
        // 1 004 of Free's 2 000 minutes is 50 %: no minutes warning.
        assert_absent_at_every_width(&mut app, "du quota de minutes");
    }

    /// The same 95 % month must stay quiet when nothing will be blocked, or
    /// when nobody can say: no budget, an alert-only budget, unreadable.
    #[test]
    fn no_budget_warning_without_a_blocking_budget() {
        for budgets in [Some(vec![]), Some(vec![actions(5, false)]), None] {
            let mut app = with_budgets(team_at_95_percent(), budgets);
            assert_absent_at_every_width(&mut app, "du quota de");
        }
    }
```

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core -- budget_line_reads a_sku_budget_line a_blocking_budget_at_95 a_five_dollar_blocking the_storage_gauge_warns no_budget_warning > /tmp/bq-t12-red.txt 2>&1; cat /tmp/bq-t12-red.txt`
Expected: FAIL — `"Budget Actions : 0.00 $ · bloquant" missing at 60x50` (et les autres, sauf `no_budget_warning_without_a_blocking_budget`, qui passe déjà : il protège contre la suite, pas contre l'état actuel).

- [ ] **Step 3: Implémenter**

Le `use crate::billing::…` de `crates/bondebarras-core/src/tui/views/billing.rs` devient :

```rust
use crate::billing::{
    self, BillingReport, Budget, MinuteLine, StorageLine, StorageQuota, included_minutes_for,
    sku_multiplier,
};
```

Ajouter, après `cost_line` :

```rust
/// The organization's Actions budget and what it does past the allowance —
/// or why nothing can be said. "No budget" and "unreadable" never share a
/// line: the first means overage is billed, the second that nobody knows.
fn budget_lines(budgets: Option<&[Budget]>) -> Vec<Line<'static>> {
    let Some(budgets) = budgets else {
        return vec![
            Line::from(Span::styled("Budget Actions : illisible", theme::muted())),
            Line::from(Span::styled(
                "  (réservé aux admins et gestionnaires de facturation)",
                theme::muted(),
            )),
        ];
    };
    let mut lines = match billing::actions_budget(budgets) {
        Some(b) if b.blocking => vec![Line::from(Span::styled(
            format!("Budget Actions : {} · bloquant", usd(b.amount as f64)),
            theme::text_style(),
        ))],
        Some(b) => vec![Line::from(Span::styled(
            format!("Budget Actions : {} · alerte seule, sans blocage", usd(b.amount as f64)),
            theme::text_style(),
        ))],
        None => vec![
            Line::from(Span::styled(
                "Budget Actions : aucun, dépassement facturé sans plafond",
                theme::text_style(),
            )),
            Line::from(Span::styled(
                "  (si un moyen de paiement est enregistré)",
                theme::muted(),
            )),
        ],
    };
    // Never observed, so named rather than interpreted.
    for b in billing::actions_sku_budgets(budgets) {
        let mode = if b.blocking { "bloquant" } else { "alerte seule" };
        lines.push(Line::from(Span::styled(
            format!("Budget SKU {} : {} · {mode}", b.sku, usd(b.amount as f64)),
            theme::muted(),
        )));
        lines.push(Line::from(Span::styled(
            "  signalé, non pris en compte par les avertissements",
            theme::muted(),
        )));
    }
    lines
}

/// Under a gauge at `billing::BUDGET_WARNING_PERCENT` or more with a
/// blocking Actions budget: what GitHub will do once the allowance runs out.
/// Takes the percentage the gauge displays, so the two never disagree.
fn budget_warning_lines(quota: &str, percent: u64, budget: Option<&Budget>) -> Vec<Line<'static>> {
    if !billing::nears_blocking_budget(percent, budget) {
        return Vec::new();
    }
    let Some(b) = budget else {
        return Vec::new();
    };
    let consequence = if b.amount == 0 {
        "  GitHub bloquera l'usage Actions au quota atteint.".to_string()
    } else {
        format!(
            "  facturé jusqu'à {}, puis usage Actions bloqué.",
            usd(b.amount as f64)
        )
    };
    vec![
        Line::from(Span::styled(
            format!(
                "⚠ {percent} % du quota de {quota}, budget {} bloquant :",
                usd(b.amount as f64)
            ),
            theme::status_warn(),
        )),
        Line::from(Span::styled(consequence, theme::status_warn())),
    ]
}
```

Remplacer `minutes_block` en entier par :

```rust
/// The minutes gauge, the budget warning it may carry, and the
/// per-repository breakdown behind it.
///
/// The breakdown is the tab's reason to exist: minutes cannot be reclaimed
/// once burnt, so the actionable part is *which repository* burnt them.
fn minutes_block(
    report: &BillingReport,
    month: &str,
    private: &HashSet<String>,
    plan: Option<&str>,
    budget: Option<&Budget>,
) -> Vec<Line<'static>> {
    let used = report.included_minutes(month, private);
    let allowance = included_minutes_for(plan);
    let mut lines = vec![
        Line::from(Span::styled("Minutes équivalent-inclus", theme::text_style())),
        Line::from(Span::styled(gauge_line(used, allowance), theme::text_style())),
    ];
    if let Some(allowance) = allowance {
        let percent = gauges::percent(used, allowance);
        lines.extend(budget_warning_lines("minutes", percent, budget));
    }
    let minute_lines = report.minute_lines(month, private);
    for minute_line in minute_lines.iter().take(MAX_BREAKDOWN_LINES) {
        lines.push(minute_line_row(minute_line));
    }
    // A truncation that leaves no trace would bury the count of hidden rows.
    // Counted in rows, not repositories: one repo can contribute several rows
    // (one per SKU).
    if minute_lines.len() > MAX_BREAKDOWN_LINES {
        lines.push(Line::from(Span::styled(
            format!(
                "   … et {} autre(s) ligne(s)",
                minute_lines.len() - MAX_BREAKDOWN_LINES
            ),
            theme::muted(),
        )));
    }
    lines
}
```

Remplacer `storage_block` en entier par :

```rust
/// The storage gauge, the budget warning it may carry, the repositories
/// holding the storage, and what deleting can and cannot do about it. No
/// request of its own: the usage report stage 1 loaded carries every line.
fn storage_block(
    report: &BillingReport,
    month: &str,
    plan: Option<&str>,
    budget: Option<&Budget>,
) -> Vec<Line<'static>> {
    let used = report.storage_gbh(month);
    let quota = billing::storage_quota(plan, month);
    let mut lines = vec![
        Line::from(Span::styled(
            "Stockage Actions · GB-heures, dépôts publics compris",
            theme::text_style(),
        )),
        Line::from(Span::styled(
            storage_gauge_line(used, quota),
            theme::text_style(),
        )),
    ];
    if let Some(quota) = quota {
        let percent = storage_percent(used, quota);
        lines.extend(budget_warning_lines("stockage", percent, budget));
    }
    let storage_lines = report.storage_lines(month);
    for line in storage_lines.iter().take(MAX_BREAKDOWN_LINES) {
        lines.push(storage_line_row(line));
    }
    if storage_lines.len() > MAX_BREAKDOWN_LINES {
        lines.push(Line::from(Span::styled(
            format!(
                "   … et {} autre(s) dépôt(s)",
                storage_lines.len() - MAX_BREAKDOWN_LINES
            ),
            theme::muted(),
        )));
    }
    for text in DELETION_DOES_NOT_REFUND {
        lines.push(Line::from(Span::styled(text, theme::muted())));
    }
    lines
}
```

Remplacer `tab_lines` en entier par :

```rust
/// Every line of the tab for one org, top to bottom.
///
/// Built from owned lines so the borrow of `app.orgs` ends before rendering,
/// and so each block can be asserted on through the real render.
fn tab_lines(org: &OrgSummary, month_cursor: usize) -> Vec<Line<'static>> {
    let plan = org.plan.as_deref();
    let budgets = org.budgets.as_deref();
    let mut lines = vec![header_line(&org.login, plan)];

    let Some(report) = &org.billing else {
        lines.push(unreadable_line());
        lines.extend(budget_lines(budgets));
        return lines;
    };

    let month = displayed_month(report, month_cursor);
    let private = private_repos(org);
    let budget = budgets.and_then(billing::actions_budget);
    lines.push(month_line(&month));
    lines.extend(enterprise_lines(plan));
    lines.extend(budget_lines(budgets));
    lines.push(Line::from(""));
    lines.extend(minutes_block(report, &month, &private, plan, budget));
    lines.push(Line::from(""));
    lines.extend(storage_block(report, &month, plan, budget));
    lines.push(Line::from(""));
    lines.extend(cost_block(report, &month));
    lines
}
```

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core -- budget_line_reads a_sku_budget_line a_blocking_budget_at_95 a_five_dollar_blocking the_storage_gauge_warns no_budget_warning an_unknown_plan_shows_no_percentage_anywhere the_storage_block_ exec_d_september_reads_33_percent > /tmp/bq-t12-green.txt 2>&1; cat /tmp/bq-t12-green.txt`
Expected: PASS — les neuf tests neufs et les tests de rendu des Tasks 5 et 8.

- [ ] **Step 5: Documentation**

`README.md` — dans la puce `- **Billing tab** —`, après la ligne `raised.` qui termine le paragraphe ajouté par la Task 9, ajouter :

```markdown
  The tab also says what happens once an allowance runs out, from the
  organization's **Actions budget**: `0.00 $ · bloquant` (GitHub stops
  Actions at the allowance), `5.00 $ · bloquant` (billed up to 5 $, then
  stopped), or no budget at all (overage billed with no ceiling, if a payment
  method is on file). A gauge at 90 % or more with a blocking budget carries a
  warning under it. A budget on a single Actions SKU is named as such, never
  interpreted. Budgets are read, never changed: changing one commits money,
  and that is permanently out of scope.
```

Dans le paragraphe `scan --json`, remplacer `or the plan cannot be read, never zero.` par :

```markdown
or the plan cannot be read, never zero. `budgets_readable`, `actions_budget`
(`{"amount", "blocking"}`, or `null` when the organization has no Actions
budget) and `actions_sku_budgets` (`null`, not `[]`, when budgets cannot be
read) keep "no budget" and "unreadable" apart.
```

À la fin de la section « Required token scopes », ajouter un paragraphe :

```markdown
Reading **budgets** is not a scope question: GitHub reserves the budgets
endpoint for organization admins and billing managers. Anyone else is refused
— observed as a 400, not the 403 the documentation announces — and the
Billing tab reads `Budget Actions : illisible` instead of guessing, while
`scan --json` reports `budgets_readable: false`. The rest of the tab, and the
organization itself, are unaffected.
```

`CHANGELOG.md` — sous `## [Unreleased]`, `### Added` :

```markdown
- Billing tab: the organization's Actions budget — its amount and whether it
  blocks — read at stage 1, and what it means past the allowance: Actions
  stopped at the allowance (0 $, as on exec-d), billed up to the budget then
  stopped (5 $, as on cloudalpes), or billed without a ceiling (no budget, as
  on SecondBrain-io). A gauge at 90 % or more with a blocking budget carries a
  warning. A per-SKU Actions budget is named, not interpreted. Budgets that
  cannot be read — a 400 on organizations the account does not own — read
  `illisible`, never "no budget". Read-only, permanently. (#14)
- `scan --json`: `budgets_readable`, `actions_budget` and
  `actions_sku_budgets`. (#14)
```

`CLAUDE.md` — dans la puce `- Required token scopes: …`, après `so `delete_repo` is never needed.`, ajouter :

```markdown
  Reading budgets (the Billing tab's budget line) is a role, not a scope:
  organization admin or billing manager — anyone else reads `illisible`, and
  a budget is never written.
```

- [ ] **Step 6: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t12-gate.txt 2>&1; tail -5 /tmp/bq-t12-gate.txt
git add crates/bondebarras-core/src/tui/views/billing.rs README.md CHANGELOG.md CLAUDE.md
git commit -m "feat(billing): budget Actions et ce qu'il fait au-dela du quota (#14)"
```

---

### Task 13: #15 — lire la rétention des artefacts et journaux

**Files:**
- Create: `crates/bondebarras-core/src/api/retention.rs`
- Modify: `crates/bondebarras-core/src/api/mod.rs`, `crates/bondebarras-core/src/model.rs` (`ArtifactRetention`, `OrgSummary`), `crates/bondebarras-core/src/billing.rs` (seuil ; `tests`), `crates/bondebarras-core/src/scan.rs` (`overview` ; `tests`), `crates/bondebarras-core/src/commands/scan.rs` (`org_json` ; `tests`)
- Modify: `CLAUDE.md` (table)

**Interfaces:**
- Consumes: `Client::get_json`.
- Produces (Task 14) :
  - `pub struct model::ArtifactRetention { pub days: u32, pub maximum_allowed_days: Option<u32> }` (`Debug, Clone, Copy, PartialEq, Eq`)
  - `OrgSummary.retention: Option<ArtifactRetention>`
  - `pub async fn api::retention::fetch(client: &Client, org: &str) -> Option<ArtifactRetention>`
  - `pub const billing::RETENTION_FLAG_DAYS: u32 = 90`, `pub const billing::NOTABLE_STORAGE_GBH: f64 = 36.0`
  - `pub fn billing::retention_worth_flagging(days: u32, storage_gbh: Option<f64>) -> bool`
  - JSON : `artifact_retention_days`

**Lecture seule.** Le `PUT` du même chemin existe (204) ; il n'est ni écrit ni appelé (spec, décision 17).

- [ ] **Step 1: Écrire les tests qui échouent**

Ajouter `pub mod retention;` entre `pub mod repos;` et `pub mod runs;` dans `crates/bondebarras-core/src/api/mod.rs`. Créer `crates/bondebarras-core/src/api/retention.rs` :

```rust
//! The organization's artifact and log retention setting — read, never written.

use super::Client;
use crate::model::ArtifactRetention;

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn retention_fetch_maps_days_and_maximum() {
        let server = MockServer::start().await;
        // exec-d's real response, before it moved to 7 days.
        Mock::given(method("GET"))
            .and(path("/orgs/exec-d/actions/permissions/artifact-and-log-retention"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "days": 90,
                "maximum_allowed_days": 400
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert_eq!(
            fetch(&client, "exec-d").await,
            Some(ArtifactRetention { days: 90, maximum_allowed_days: Some(400) })
        );
    }

    /// A token without `admin:org` — the README's own scopes: refused, and
    /// never fatal. Modelled on `api::billing::a_403_degrades_to_none_rather_than_failing`.
    #[tokio::test]
    async fn retention_fetch_degrades_a_refusal_to_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/actions/permissions/artifact-and-log-retention"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(fetch(&client, "systm-d").await.is_none());
    }

    /// No `days`, no retention: GitHub's 90-day default is never assumed.
    #[tokio::test]
    async fn retention_fetch_never_assumes_a_default() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/exec-d/actions/permissions/artifact-and-log-retention"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "maximum_allowed_days": 400 })),
            )
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(fetch(&client, "exec-d").await.is_none());
    }
}
```

Dans le module `tests` de `crates/bondebarras-core/src/billing.rs` :

```rust
    #[test]
    fn retention_worth_flagging_needs_ninety_days_and_notable_storage() {
        // exec-d before its change: 90 days, 371.85 GB-h in September.
        assert!(retention_worth_flagging(90, Some(371.85)));
        // The maximum accumulates even more.
        assert!(retention_worth_flagging(400, Some(371.85)));
        // exec-d after its change: 7 days.
        assert!(!retention_worth_flagging(7, Some(371.85)));
        // 90 days on an org holding about what systm-d/josephine alone holds.
        assert!(!retention_worth_flagging(90, Some(12.9)));
        // At the threshold itself.
        assert!(retention_worth_flagging(90, Some(36.0)));
        // Billing unreadable: nobody knows whether storage counts.
        assert!(!retention_worth_flagging(90, None));
    }
```

Dans le module `tests` de `crates/bondebarras-core/src/scan.rs` :

```rust
    /// #15: retention rides along at stage 1; a token without `admin:org`
    /// costs the retention only.
    #[tokio::test]
    async fn overview_carries_retention() {
        let server = MockServer::start().await;
        for org in ["exec-d", "systm-d"] {
            Mock::given(method("GET"))
                .and(path(format!("/orgs/{org}/actions/cache/usage-by-repository")))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "repository_cache_usages": [] })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path(format!("/orgs/{org}/repos")))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/orgs/exec-d/actions/permissions/artifact-and-log-retention"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "days": 7 })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/actions/permissions/artifact-and-log-retention"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = overview(&client, &["exec-d".to_string(), "systm-d".to_string()]).await;

        let find = |login: &str| out.iter().find(|o| o.login == login).unwrap();
        assert_eq!(out.len(), 2, "a refused retention read must not drop the org");
        assert_eq!(find("exec-d").retention.map(|r| r.days), Some(7));
        assert!(find("systm-d").retention.is_none());
    }
```

Dans le module `tests` de `crates/bondebarras-core/src/commands/scan.rs` :

```rust
    #[test]
    fn scan_json_carries_artifact_retention_days() {
        let exec_d = OrgSummary {
            login: "exec-d".into(),
            retention: Some(crate::model::ArtifactRetention {
                days: 7,
                maximum_allowed_days: None,
            }),
            ..Default::default()
        };
        let v = overview_json(&[exec_d, org("systm-d", None)], "2026-09");
        assert_eq!(v[0]["artifact_retention_days"], 7);
        assert!(v[1]["artifact_retention_days"].is_null(), "got: {}", v[1]);
    }
```

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core -- retention_fetch_ retention_worth_flagging overview_carries_retention scan_json_carries_artifact_retention > /tmp/bq-t13-red.txt 2>&1; cat /tmp/bq-t13-red.txt`
Expected: FAIL — `cannot find type 'ArtifactRetention' in module 'crate::model'`.

- [ ] **Step 3: Implémenter**

Dans `crates/bondebarras-core/src/model.rs`, avant `OrgSummary` :

```rust
/// An organization's artifact and log retention setting, read-only.
///
/// It is the tap: every artifact a workflow uploads is kept this long unless
/// the workflow's own `retention-days` asks for less. A change only applies
/// to new artifacts and logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactRetention {
    /// Days a new artifact or log is kept by default.
    pub days: u32,
    /// The ceiling `days` may be raised to, when GitHub says.
    pub maximum_allowed_days: Option<u32>,
}
```

et à `OrgSummary`, après `budgets` :

```rust
    /// The org's artifact and log retention, or `None` when it cannot be
    /// read — GitHub requires `admin:org` (or the fine-grained "Actions
    /// policies" permission), which this tool treats as optional.
    pub retention: Option<ArtifactRetention>,
```

Dans `crates/bondebarras-core/src/api/retention.rs`, entre les `use` et `#[cfg(test)]` :

```rust
/// The retention setting, or `None` when it cannot be read.
///
/// GitHub requires the classic `admin:org` scope or the fine-grained
/// "Actions policies" permission, and bondebarras requires neither: a
/// refusal reads as "rétention illisible" and never drops the org. A body
/// without an integer `days` reads the same way — GitHub's 90-day default is
/// never assumed. The `PUT` on the same path exists and is deliberately not
/// called: this round is read-only.
pub async fn fetch(client: &Client, org: &str) -> Option<ArtifactRetention> {
    let v = client
        .get_json(&format!(
            "/orgs/{org}/actions/permissions/artifact-and-log-retention"
        ))
        .await
        .ok()?;
    Some(ArtifactRetention {
        days: u32::try_from(v["days"].as_u64()?).ok()?,
        maximum_allowed_days: v["maximum_allowed_days"]
            .as_u64()
            .and_then(|d| u32::try_from(d).ok()),
    })
}
```

Dans `crates/bondebarras-core/src/billing.rs`, après `nears_blocking_budget` :

```rust
/// Retention, in days, from which the setting is worth pointing out.
/// GitHub's default is 90, and `maximum_allowed_days` goes up to 400 — longer
/// accumulates more, hence "at least".
pub const RETENTION_FLAG_DAYS: u32 = 90;

/// GB-hours in a month from which an org's Actions storage is not
/// negligible: 10 % of the smallest plan's included storage (0.5 GB × 720 h
/// = 360 GB-h). Fixed, independent of the org's plan, so the highlight still
/// works when the plan cannot be read.
pub const NOTABLE_STORAGE_GBH: f64 = 36.0;

/// Whether the retention setting deserves to stand out: at least
/// `RETENTION_FLAG_DAYS`, on an org whose storage this month is at least
/// `NOTABLE_STORAGE_GBH`. Unknown storage (billing unreadable) never
/// highlights: nobody knows whether it counts.
pub fn retention_worth_flagging(days: u32, storage_gbh: Option<f64>) -> bool {
    days >= RETENTION_FLAG_DAYS && storage_gbh.is_some_and(|gbh| gbh >= NOTABLE_STORAGE_GBH)
}
```

Dans `scan::overview`, la jointure et la construction deviennent :

```rust
        let (billing, plan, budgets, retention) = futures::join!(
            crate::api::billing::fetch(client, org),
            crate::api::orgs::plan(client, org),
            crate::api::budgets::fetch(client, org),
            crate::api::retention::fetch(client, org),
        );

        Some(OrgSummary {
            login: org.clone(),
            cache_bytes,
            cache_count,
            repos: repos_out,
            billing,
            plan,
            budgets,
            retention,
        })
```

Dans `org_json` (`crates/bondebarras-core/src/commands/scan.rs`), ajouter juste avant la clé `"repos"` :

```rust
        "artifact_retention_days": o.retention.map(|r| r.days),
```

`CLAUDE.md` — dans la table, après la ligne de `api/budgets.rs` :

```markdown
| Artifact and log retention setting (read-only; refused → `None`; the `PUT` is never called) | `crates/bondebarras-core/src/api/retention.rs` |
```

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core -- retention_fetch_ retention_worth_flagging overview_carries_retention scan_json_carries_artifact_retention > /tmp/bq-t13-green.txt 2>&1; cat /tmp/bq-t13-green.txt`
Expected: PASS (6 tests).

- [ ] **Step 5: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t13-gate.txt 2>&1; tail -5 /tmp/bq-t13-gate.txt
git add -A crates/bondebarras-core/src CLAUDE.md
git commit -m "feat(api): retention des artefacts de chaque organisation, en lecture seule"
```

---

### Task 14: #15 — la rétention dans l'onglet, à côté du stockage

**Files:**
- Modify: `crates/bondebarras-core/src/tui/views/billing.rs` (`use` ; `retention_lines`, `RETENTION_NOTES`, `retention_notes` neuves ; `tab_lines` ; module `tests`)
- Modify: `README.md`, `CHANGELOG.md`, `CLAUDE.md`

**Interfaces:**
- Consumes: `model::ArtifactRetention`, `OrgSummary.retention`, `billing::retention_worth_flagging` (Task 13) ; `BillingReport::storage_gbh` (Task 6) ; `tab_lines` et ses blocs (Task 12) ; helpers de test (Task 1).
- Produces :
  - `fn retention_lines(retention: Option<ArtifactRetention>, storage_gbh: Option<f64>) -> Vec<Line<'static>>`
  - `const RETENTION_NOTES: [&str; 4]`, `fn retention_notes() -> Vec<Line<'static>>`

- [ ] **Step 1: Écrire les tests qui échouent**

Dans le module `tests` de `crates/bondebarras-core/src/tui/views/billing.rs`, ajouter `use crate::model::ArtifactRetention;` puis :

```rust
    fn with_retention(mut org: OrgSummary, days: Option<u32>) -> App {
        org.retention = days.map(|days| ArtifactRetention {
            days,
            maximum_allowed_days: Some(400),
        });
        billing_app(org)
    }

    /// exec-d before its change: 90 days, 371.85 GB-h in September — the
    /// setting that made its storage overflow.
    #[test]
    fn retention_line_flags_ninety_days_when_storage_counts() {
        let mut app = with_retention(exec_d_september(), Some(90));
        assert_shown_at_every_size(&mut app, "⚠ Rétention artefacts et journaux : 90 j (max. 400 j)");
        assert_shown_at_every_size(&mut app, "c'est ce réglage qui fait durer le stockage");
    }

    /// exec-d after its change. Asserting the highlighted form is absent —
    /// not merely that "7 j" is present — is what fails a highlight that
    /// ignores `days`.
    #[test]
    fn retention_line_leaves_seven_days_quiet() {
        let mut app = with_retention(exec_d_september(), Some(7));
        assert_shown_at_every_size(&mut app, "Rétention artefacts et journaux : 7 j (max. 400 j)");
        assert_absent_at_every_width(&mut app, "⚠ Rétention");
        assert_absent_at_every_width(&mut app, "fait durer le stockage");
    }

    /// 90 days on an org holding what systm-d/josephine alone held in
    /// September (12.9 GB-h): below 36, no highlight. The storage half of
    /// the rule must be able to fail too.
    #[test]
    fn retention_line_leaves_ninety_days_quiet_when_storage_is_negligible() {
        let mut org = exec_d_september();
        org.billing = Some(BillingReport {
            items: vec![usage("2026-09", "Actions storage", "GigabyteHours", 12.9, 0.0, "josephine")],
        });
        let mut app = with_retention(org, Some(90));
        assert_shown_at_every_size(&mut app, "Rétention artefacts et journaux : 90 j (max. 400 j)");
        assert_absent_at_every_width(&mut app, "⚠ Rétention");
    }

    #[test]
    fn retention_line_reads_unreadable() {
        let mut app = with_retention(exec_d_september(), None);
        assert_shown_at_every_size(&mut app, "Rétention artefacts et journaux : illisible");
        assert_shown_at_every_size(&mut app, "(scope admin:org requis pour la lire)");
    }

    /// The two exact points of #15, on a readable and an unreadable billing
    /// report alike: they are true whatever the tab can read.
    #[test]
    fn the_tab_carries_both_retention_notes() {
        let mut unreadable = exec_d_september();
        unreadable.billing = None;
        for org in [exec_d_september(), unreadable] {
            let mut app = with_retention(org, Some(7));
            for note in RETENTION_NOTES {
                assert_shown_at_every_size(&mut app, note.trim_start());
            }
        }
    }
```

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core -- retention_line_ the_tab_carries_both_retention_notes > /tmp/bq-t14-red.txt 2>&1; cat /tmp/bq-t14-red.txt`
Expected: FAIL — `cannot find value 'RETENTION_NOTES' in this scope`.

- [ ] **Step 3: Implémenter**

Dans `crates/bondebarras-core/src/tui/views/billing.rs`, `use crate::model::OrgSummary;` devient `use crate::model::{ArtifactRetention, OrgSummary};`. Après `DELETION_DOES_NOT_REFUND` :

```rust
/// The two exact points #15 asks the tab to state, split to stay legible in
/// a narrow frame. True whatever the setting, so always shown.
const RETENTION_NOTES: [&str; 4] = [
    "Note : retention-days, dans un workflow, fixe la durée",
    "  de cet artefact, dans la limite de ce réglage.",
    "Note : un changement de rétention ne vaut que pour",
    "  les nouveaux artefacts et journaux.",
];
```

Après `storage_block` :

```rust
/// The retention setting, beside the storage it governs. Highlighted — ⚠,
/// warning colour, and the reason — when it is at least 90 days on an org
/// whose storage counts (`billing::retention_worth_flagging`). Read-only:
/// the tab shows the tap, it does not turn it.
fn retention_lines(
    retention: Option<ArtifactRetention>,
    storage_gbh: Option<f64>,
) -> Vec<Line<'static>> {
    let Some(r) = retention else {
        return vec![
            Line::from(Span::styled(
                "Rétention artefacts et journaux : illisible",
                theme::muted(),
            )),
            Line::from(Span::styled(
                "  (scope admin:org requis pour la lire)",
                theme::muted(),
            )),
        ];
    };
    let maximum = r
        .maximum_allowed_days
        .map(|m| format!(" (max. {m} j)"))
        .unwrap_or_default();
    let text = format!("Rétention artefacts et journaux : {} j{maximum}", r.days);
    if billing::retention_worth_flagging(r.days, storage_gbh) {
        vec![
            Line::from(Span::styled(format!("⚠ {text}"), theme::status_warn())),
            Line::from(Span::styled(
                "  c'est ce réglage qui fait durer le stockage",
                theme::status_warn(),
            )),
        ]
    } else {
        vec![Line::from(Span::styled(text, theme::text_style()))]
    }
}

fn retention_notes() -> Vec<Line<'static>> {
    RETENTION_NOTES
        .iter()
        .map(|text| Line::from(Span::styled(*text, theme::muted())))
        .collect()
}
```

Remplacer `tab_lines` en entier par :

```rust
/// Every line of the tab for one org, top to bottom.
///
/// Built from owned lines so the borrow of `app.orgs` ends before rendering,
/// and so each block can be asserted on through the real render.
fn tab_lines(org: &OrgSummary, month_cursor: usize) -> Vec<Line<'static>> {
    let plan = org.plan.as_deref();
    let budgets = org.budgets.as_deref();
    let mut lines = vec![header_line(&org.login, plan)];

    let Some(report) = &org.billing else {
        lines.push(unreadable_line());
        lines.extend(budget_lines(budgets));
        // Storage unknown: the retention is shown, never highlighted.
        lines.extend(retention_lines(org.retention, None));
        lines.push(Line::from(""));
        lines.extend(retention_notes());
        return lines;
    };

    let month = displayed_month(report, month_cursor);
    let private = private_repos(org);
    let budget = budgets.and_then(billing::actions_budget);
    lines.push(month_line(&month));
    lines.extend(enterprise_lines(plan));
    lines.extend(budget_lines(budgets));
    lines.push(Line::from(""));
    lines.extend(minutes_block(report, &month, &private, plan, budget));
    lines.push(Line::from(""));
    lines.extend(storage_block(report, &month, plan, budget));
    lines.extend(retention_lines(
        org.retention,
        Some(report.storage_gbh(&month)),
    ));
    lines.push(Line::from(""));
    lines.extend(cost_block(report, &month));
    lines.push(Line::from(""));
    lines.extend(retention_notes());
    lines
}
```

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core -- retention_line_ the_tab_carries_both_retention_notes an_unknown_plan_shows_no_percentage_anywhere budget_line_reads the_storage_block_ > /tmp/bq-t14-green.txt 2>&1; cat /tmp/bq-t14-green.txt`
Expected: PASS — les cinq tests neufs et les tests de rendu des Tasks 5, 8 et 12.

- [ ] **Step 5: Documentation**

`README.md` — dans la puce `- **Billing tab** —`, après la ligne `and that is permanently out of scope.` ajoutée par la Task 12 :

```markdown
  Beside the storage, the organization's **artifact and log retention**
  (90 days is GitHub's default), highlighted when it is 90 days or more on an
  organization holding at least 36 GB-hours of storage that month — 10 % of
  the smallest plan's included storage. It is the tap: every artifact a
  workflow uploads is kept that long. The tab states the two things worth
  knowing before changing it: a workflow's `retention-days` sets that one
  artifact's duration, within this setting; and a change only applies to new
  artifacts and logs. bondebarras only reads the setting.
```

Dans le paragraphe `scan --json`, remplacer `keep "no budget" and "unreadable" apart.` par :

```markdown
keep "no budget" and "unreadable" apart. `artifact_retention_days` is the
retention setting, or `null` when it cannot be read.
```

Dans « Required token scopes », remplacer :

```markdown
`repo`, `read:org`, `read:packages`, and `delete:packages` are enough for
everything bondebarras does — reading and deleting caches, artifacts,
```

par :

```markdown
`repo`, `read:org`, `read:packages`, and `delete:packages` are enough for
everything bondebarras does but one optional display (see `admin:org`
below) — reading and deleting caches, artifacts,
```

et ajouter à la fin de la section :

```markdown
`admin:org` is **optional**, and needed for one thing only: displaying an
organization's artifact and log retention, which GitHub only reveals to that
scope (or to the fine-grained "Actions policies" permission). bondebarras
never changes the setting. Without it, the Billing tab reads
`Rétention artefacts et journaux : illisible` and `scan --json` reports
`artifact_retention_days: null`; everything else works unchanged.
```

`CHANGELOG.md` — sous `## [Unreleased]`, `### Added` :

```markdown
- Billing tab: each organization's artifact and log retention, beside the
  storage it governs, highlighted at 90 days or more when the organization
  holds at least 36 GB-hours that month. The tab states that a workflow's
  `retention-days` is bounded by this setting, and that a change only applies
  to new artifacts and logs — verified on 2026-09-10, when an APK uploaded the
  day before exec-d moved to 7 days kept its 2026-12-08 expiry. Read-only:
  nothing here changes the setting. (#15)
- `scan --json`: `artifact_retention_days`. (#15)
```

puis, toujours sous `## [Unreleased]` :

```markdown
### Note on scopes

`admin:org` becomes an **optional** scope: only the retention display needs
it, and without it that one line reads `illisible`. Reading budgets needs no
scope but a role — organization admin or billing manager. No scope is added
to the required list.
```

`CLAUDE.md` — dans la puce `- Required token scopes: …`, après la phrase sur les budgets ajoutée par la Task 12 :

```markdown
  `admin:org` is optional, needed only to display artifact and log retention
  (`api::retention`); without it that line reads `illisible`. The retention
  `PUT` is never called.
```

et, dans la table, remplacer la ligne `| Billing aggregation: SKU multipliers, allowance math, per-repo/-month rollups | … |` par :

```markdown
| Billing aggregation: SKU multipliers, per-plan allowances (minutes, storage GB-hours), budget selection, retention highlight threshold, per-repo/-month rollups | `crates/bondebarras-core/src/billing.rs` |
```

- [ ] **Step 6: Vérifier et commiter**

```bash
cargo fmt && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace > /tmp/bq-t14-gate.txt 2>&1; tail -5 /tmp/bq-t14-gate.txt
cargo build --release > /tmp/bq-t14-release.txt 2>&1; tail -3 /tmp/bq-t14-release.txt
git add crates/bondebarras-core/src/tui/views/billing.rs README.md CHANGELOG.md CLAUDE.md
git commit -m "feat(billing): retention des artefacts a cote du stockage, en lecture seule (#15)"
```

---

## Self-Review

**Couverture de la spec**

| Exigence (spec) | Tâche |
|---|---|
| §3 / décision 2 — dollars, jamais convertis | 1 |
| décision 3 — `GET /orgs/{org}` → `plan.name`, dégradable | 3, 4 |
| décision 4 — `included_minutes_for` remplace la constante | 2, 5 |
| décision 5 — pas de quota, pas de `%`, nulle part | 5 (test sur tout l'onglet), 8, 12 le gardent vert |
| décision 6 — « quota de la formule actuelle », ligne enterprise | 5 |
| décision 7 — jauge de minutes de la colonne 3 | 5 |
| décision 8 — JSON `plan`, `minutes_allowance` | 4 |
| décisions 9, 10 — GB-heures, base du mois affiché, publics comptés | 6, 8 |
| décision 11 — jauge de stockage dans l'onglet, colonne 3 inchangée | 8 |
| décisions 12, 18, 19 — colonne 2, ligne de détail, mois calendaire UTC | 6 (`month_of`), 9 |
| décision 13 — ⚠ sur `CACHE_CEILING_BYTES`, strictement | 9 |
| décision 14 — ligne fixe sur la suppression | 8 |
| décision 15 — JSON stockage (+ `billing_month`) | 7 |
| décisions 16, 20, 21, 23 — budgets : pagination, tout échec, entrée mal formée, troncature, `SkuPricing` signalé, montant entier | 10, 11, 12 |
| décision 16 — alerte à 90 % avec budget bloquant | 10, 12 |
| décision 17, 22 — rétention lecture seule, `admin:org` optionnel, seuil 90 j ou plus / 36 GB-h, deux notes | 13, 14 |
| décision 24 — lignes vérifiées ≤ 56 caractères | 5, 8, 12, 14 (chaque texte asserté est compté) |
| §2 décision 1 — `(#N)` sur le commit qui termine l'issue | 1, 5, 9, 12, 14 |
| README / CHANGELOG / CLAUDE.md | 1, 3, 5, 9, 11, 12, 13, 14 |
| §9 mesures ouvertes — rendues visibles, jamais tranchées en silence | 8 (`base 720 h`, test 720/744), 10 (SKU illustratifs), 5 (enterprise « minimum ») |

**Cohérence des types**

- `included_minutes_for(Option<&str>) -> Option<u64>` : défini en 2 ; lu en 4, 5, 7, 12. Aucun pourcentage neuf : `gauges::percent(u64, u64) -> u64` (existant, `pub(crate)` depuis `e635f36`) sert en 5 (`gauge_line`, jauge de minutes), en 8 (`storage_percent`, en centièmes de GB-heure) et en 12 (seuils d'alerte).
- `OrgSummary` : `Default` et `plan` en 4, `budgets` en 11, `retention` en 13 ; le seul site de production (`scan::overview`) nomme chaque champ à chaque ajout.
- `StorageLine`, `StorageQuota`, `storage_quota`, `hours_in_month`, `month_of`, `storage_gbh`, `storage_gbh_for_repo`, `storage_lines` : définis en 6 ; lus en 7, 8, 9, 12, 14.
- `Budget { budget_type, sku, scope, amount: u64, blocking }`, `actions_budget`, `actions_sku_budgets`, `nears_blocking_budget`, `BUDGET_WARNING_PERCENT` : définis en 10 ; lus en 11, 12.
- `ArtifactRetention { days: u32, maximum_allowed_days: Option<u32> }` (Copy), `retention_worth_flagging`, `RETENTION_FLAG_DAYS`, `NOTABLE_STORAGE_GBH` : définis en 13 ; lus en 14.
- `overview_json(&[OrgSummary])` en 4, `overview_json(&[OrgSummary], &str)` à partir de la 7 (la 7 met à jour le test de la 4).
- `gauge_line(u64, Option<u64>)`, `bar`, `tab_lines`, `minutes_block`, `cost_block` en 5 ; `storage_gauge_line`, `storage_block`, `MAX_BREAKDOWN_LINES` en 8 ; `minutes_block` et `storage_block` prennent `budget: Option<&Budget>` en 12 ; `tab_lines` réécrit en entier en 8 (extrait), 12 et 14.
- `minutes_gauge_line(u64, bool, Option<u64>, u16)` à partir de la 5.

**Les tests qui comptent**

- `an_unknown_plan_shows_no_percentage_anywhere_in_the_tab` : il balaie tout l'onglet, pas une ligne, et reste vert à travers les Tasks 8, 12 et 14 — c'est la règle de #11 tenue par chaque bloc ajouté ensuite.
- `hours_in_month_counts_the_displayed_months_days` : une constante 720 échoue sur juillet, une constante 744 sur septembre ; la mesure ouverte ne peut pas être tranchée par accident.
- `budgets_fetch_treats_a_malformed_entry_as_unreadable` et `scan_json_tells_no_budget_from_unreadable_budgets` : sans eux, « aucun budget : dépassement facturé » pourrait être dit d'une organisation que GitHub bloque.

**Points de vigilance pour l'exécutant**

- `assert_shown_at_every_size` suppose deux rangées fixes sous le corps (ligne d'état, pied) quand la progression est au repos : à vérifier au Préalable ; si la disposition diffère, corriger le `+ 3` du plancher une fois, dans le helper.
- La Task 9 insère dans une fonction de ligne dont la Task 4 de tui-3-colonnes fixe le nom et le format : seul l'emplacement (avant la taille des caches) est prescrit. La ligne est alors pleine (36 cellules sur 36) : un balayage rouge de la colonne 2 se signale, il ne se corrige pas en rognant le nom.
- La Task 12 cherche l'absence de `Budget Actions : illisible`, pas de `illisible` seul : la ligne de rétention de la Task 14 peut légitimement dire « illisible ».
- `cargo test -p bondebarras-core -- a b c` accepte plusieurs filtres, chacun une sous-chaîne littérale.
