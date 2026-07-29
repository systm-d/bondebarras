# bondebarras v0.2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ajouter l'onglet Billing — le seul angle par lequel l'axe « minutes Actions » est adressable — et la CLI headless, puis solder trois dettes laissées par la v0.1.

**Architecture:** `billing.rs` fait l'agrégation en calcul pur, sans réseau ni ratatui ; `api/billing.rs` récupère le relevé et transforme un 403 en `None` ; l'étage 1 du scan passe de deux à trois requêtes par org ; le TUI gagne un onglet, la CLI gagne `--json` et une sous-commande `clean`.

**Tech Stack:** Inchangé — tokio, octocrab 0.41, ratatui 0.30, crossterm 0.29, clap 4, serde_json, anyhow. Tests : wiremock, assert_cmd, predicates.

## Global Constraints

- Rust **edition 2024**, MSRV **1.88** — plancher réel imposé par `ratatui 0.30.2`.
- `unsafe_code = "forbid"` ; clippy `all = { level = "warn", priority = -1 }`, CI en `-D warnings`.
- rustfmt `max_width = 100`. Le code pasté ici n'est pas forcément rustfmt-clean (`fn_call_width` vaut 60) : lancer `cargo fmt` et laisser reformater.
- **Doc comments en anglais. Chaînes user-facing en français**, accents inclus. Identifiants en anglais.
- Jamais `ERROR`/`FATAL`/`PANIC` en user-facing. `Erreur : ` est ajouté **une seule fois**, par `run()` — les valeurs d'erreur ne doivent pas le porter.
- **Routes API avec slash initial** : `/organizations/{org}/settings/billing/usage`. Sans lui, `Uri::from_str` lit le premier segment comme une autorité et la requête part ailleurs — et les tests wiremock passent quand même.
- `cargo test <filtre>` prend une **sous-chaîne littérale**, pas une regex.
- `sort_by(|a, b| b.x.cmp(&a.x))` échoue sur `clippy::unnecessary_sort_by` : utiliser `sort_by_key(|r| std::cmp::Reverse(r.x))`.
- Conventional Commits. Multi-plateforme.
- Preuve TDD : sortie terminale **brute et non retouchée**. Les relecteurs recoupent les numéros de ligne, et une transcription reconstituée a déjà été détectée sur ce projet.

## Ce qui existe déjà (v0.1, fusionnée dans `main`)

`Client::{get_json, delete}` (chemins à slash initial) · `api::{caches, artifacts, runs, prs, repos}` · `scan::{overview, repo_detail}` · `clean::{Plan, Progress, execute}` · `model::{Resource, ResourceKind, RiskTier, risk_tier, human_size, OrgSummary, RepoSummary}` · `tui::{app::App, theme, views::{orgs, repo, confirm}}` · `cli::Cli` avec la seule sous-commande `Scan { org }`. 48 tests.

## Structure des fichiers

| Fichier | Responsabilité |
|---|---|
| `crates/bondebarras-core/src/billing.rs` | agrégation pure : SKU, multiplicateurs, couvert/facturé |
| `crates/bondebarras-core/src/api/billing.rs` | récupération du relevé, 403 → `None` |
| `crates/bondebarras-core/src/model.rs` | `OrgSummary` gagne `billing: Option<BillingReport>` |
| `crates/bondebarras-core/src/scan.rs` | étage 1 passe à trois requêtes par org |
| `crates/bondebarras-core/src/tui/views/billing.rs` | rendu de l'onglet |
| `crates/bondebarras-core/src/tui/app.rs` | `Tab` (vue), navigation par mois |
| `crates/bondebarras-core/src/tui/mod.rs` | garde de sortie pendant une purge |
| `crates/bondebarras-core/src/cli.rs` | `--json`, sous-commande `clean` |
| `crates/bondebarras-core/src/commands/mod.rs`, `scan.rs`, `clean.rs` | exécution headless |

---

### Task 1: Agrégation de facturation (calcul pur)

**Files:**
- Create: `crates/bondebarras-core/src/billing.rs`
- Modify: `crates/bondebarras-core/src/lib.rs`

**Interfaces:**
- Consumes: rien.
- Produces:
  - `struct UsageItem { month: String, product: String, sku: String, quantity: f64, unit_type: String, gross: f64, discount: f64, net: f64, repo: String }`
  - `struct BillingReport { items: Vec<UsageItem> }`
  - `fn sku_multiplier(sku: &str) -> Option<u32>`
  - `BillingReport::months(&self) -> Vec<String>`
  - `BillingReport::included_minutes(&self, month: &str) -> u64`
  - `BillingReport::cost(&self, month: &str) -> (f64, f64, f64)`
  - `BillingReport::unknown_skus(&self, month: &str) -> Vec<String>`
  - `const FREE_MINUTES_PER_MONTH: u64 = 2_000;`

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn item(month: &str, sku: &str, qty: f64, gross: f64, discount: f64, repo: &str) -> UsageItem {
        UsageItem {
            month: month.into(),
            product: "actions".into(),
            sku: sku.into(),
            quantity: qty,
            unit_type: "Minutes".into(),
            gross,
            discount,
            net: gross - discount,
            repo: repo.into(),
        }
    }

    #[test]
    fn multipliers_follow_githubs_billing_ratios() {
        assert_eq!(sku_multiplier("Actions Linux"), Some(1));
        assert_eq!(sku_multiplier("Actions Windows"), Some(2));
        assert_eq!(sku_multiplier("Actions macOS 3-core"), Some(10));
        assert_eq!(sku_multiplier("Actions macOS XL"), Some(10));
        // An unknown runner must be reported, not silently counted as Linux.
        assert_eq!(sku_multiplier("Actions Quantum"), None);
    }

    #[test]
    fn included_minutes_apply_the_multipliers() {
        let r = BillingReport {
            items: vec![
                item("2026-07", "Actions Linux", 100.0, 0.6, 0.0, "a"),
                item("2026-07", "Actions Windows", 100.0, 1.0, 0.0, "a"),
                item("2026-07", "Actions macOS 3-core", 10.0, 0.62, 0.0, "a"),
            ],
        };
        // 100 + 100x2 + 10x10 = 400
        assert_eq!(r.included_minutes("2026-07"), 400);
    }

    #[test]
    fn a_fully_discounted_item_does_not_consume_the_allowance() {
        // Public repos bill nothing: gross == discount. Counting them would
        // make an open-source project look like it had blown the quota.
        let r = BillingReport {
            items: vec![
                item("2026-07", "Actions Linux", 5000.0, 30.0, 30.0, "public"),
                item("2026-07", "Actions Linux", 100.0, 0.6, 0.0, "private"),
            ],
        };
        assert_eq!(r.included_minutes("2026-07"), 100);
    }

    #[test]
    fn an_unknown_sku_is_reported_not_swallowed() {
        let r = BillingReport {
            items: vec![item("2026-07", "Actions Quantum", 42.0, 1.0, 0.0, "a")],
        };
        assert_eq!(r.unknown_skus("2026-07"), vec!["Actions Quantum".to_string()]);
        // It still counts, at x1, rather than vanishing from the total.
        assert_eq!(r.included_minutes("2026-07"), 42);
    }

    #[test]
    fn cost_splits_gross_into_covered_and_billed() {
        let r = BillingReport {
            items: vec![
                item("2026-07", "Actions Linux", 100.0, 10.0, 10.0, "a"),
                item("2026-07", "Actions Linux", 100.0, 4.0, 1.0, "b"),
            ],
        };
        let (gross, covered, billed) = r.cost("2026-07");
        assert!((gross - 14.0).abs() < 1e-9);
        assert!((covered - 11.0).abs() < 1e-9);
        assert!((billed - 3.0).abs() < 1e-9);
    }

    #[test]
    fn months_are_sorted_and_deduplicated() {
        let r = BillingReport {
            items: vec![
                item("2026-07", "Actions Linux", 1.0, 0.0, 0.0, "a"),
                item("2026-05", "Actions Linux", 1.0, 0.0, 0.0, "a"),
                item("2026-07", "Actions Linux", 1.0, 0.0, 0.0, "b"),
            ],
        };
        assert_eq!(r.months(), vec!["2026-05".to_string(), "2026-07".to_string()]);
    }

    #[test]
    fn an_empty_month_yields_zeroes_not_a_panic() {
        let r = BillingReport { items: vec![] };
        assert_eq!(r.included_minutes("2026-07"), 0);
        assert_eq!(r.cost("2026-07"), (0.0, 0.0, 0.0));
        assert!(r.months().is_empty());
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core billing`
Expected: FAIL — `cannot find type UsageItem`.

Le module doit être déclaré dans `lib.rs` avant ce run, sinon `cargo test` rapporte « 0 tests » au lieu d'une erreur de compilation, ce qui n'est pas une preuve RED.

- [ ] **Step 3: Écrire l'implémentation**

```rust
//! Billing aggregation. Pure calculation over a usage report — no network,
//! no rendering.
//!
//! Minutes cannot be cleaned up retroactively: once burnt they are burnt. All
//! this module can do is say *where they went*, which is the only useful
//! answer for the Actions-minutes axis of the problem.

/// One line of GitHub's usage report: a month × a repository × a SKU.
#[derive(Debug, Clone)]
pub struct UsageItem {
    /// `YYYY-MM`, derived from the report's RFC 3339 `date`.
    pub month: String,
    pub product: String,
    pub sku: String,
    pub quantity: f64,
    pub unit_type: String,
    pub gross: f64,
    /// The part absorbed by the free allowance.
    pub discount: f64,
    /// What is actually paid.
    pub net: f64,
    pub repo: String,
}

/// A whole organization's usage report.
#[derive(Debug, Clone, Default)]
pub struct BillingReport {
    pub items: Vec<UsageItem>,
}

/// Free Actions allowance for an organization, in Linux-equivalent minutes.
pub const FREE_MINUTES_PER_MONTH: u64 = 2_000;

/// How many Linux-equivalent minutes one minute of this runner costs.
///
/// `None` means the SKU is unknown — a new runner family GitHub added. The
/// caller counts it at ×1 *and* surfaces it, because a silent multiplier
/// would skew the gauge with no way to notice.
pub fn sku_multiplier(sku: &str) -> Option<u32> {
    match sku {
        "Actions Linux" => Some(1),
        "Actions Windows" => Some(2),
        s if s.starts_with("Actions macOS") => Some(10),
        _ => None,
    }
}

impl BillingReport {
    /// Every month present in the report, oldest first.
    pub fn months(&self) -> Vec<String> {
        let mut out: Vec<String> = self.items.iter().map(|i| i.month.clone()).collect();
        out.sort();
        out.dedup();
        out
    }

    /// Minutes charged against the free allowance, in Linux equivalents.
    ///
    /// Fully discounted items are excluded: a public repository bills nothing
    /// and consumes no allowance, so counting it would make an open-source
    /// project look like it had blown the quota.
    pub fn included_minutes(&self, month: &str) -> u64 {
        self.items
            .iter()
            .filter(|i| i.month == month && i.unit_type == "Minutes")
            .filter(|i| i.gross > i.discount)
            .map(|i| {
                let mult = sku_multiplier(&i.sku).unwrap_or(1) as f64;
                (i.quantity * mult).round() as u64
            })
            .sum()
    }

    /// `(gross, covered, billed)` for the month, all products together.
    pub fn cost(&self, month: &str) -> (f64, f64, f64) {
        self.items
            .iter()
            .filter(|i| i.month == month)
            .fold((0.0, 0.0, 0.0), |(g, c, b), i| {
                (g + i.gross, c + i.discount, b + i.net)
            })
    }

    /// SKUs in this month that `sku_multiplier` does not know.
    pub fn unknown_skus(&self, month: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .items
            .iter()
            .filter(|i| i.month == month && i.unit_type == "Minutes")
            .filter(|i| sku_multiplier(&i.sku).is_none())
            .map(|i| i.sku.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    }
}
```

Ajouter `pub mod billing;` dans `lib.rs`.

- [ ] **Step 4: Relancer les tests**

Run: `cargo test -p bondebarras-core billing`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/billing.rs crates/bondebarras-core/src/lib.rs
git commit -m "feat(billing): agregation des minutes et des couts"
```

---

### Task 2: Récupération du relevé de facturation

**Files:**
- Create: `crates/bondebarras-core/src/api/billing.rs`
- Modify: `crates/bondebarras-core/src/api/mod.rs`

**Interfaces:**
- Consumes: `api::Client`, `billing::{BillingReport, UsageItem}`.
- Produces: `async fn fetch(client: &Client, org: &str) -> Option<BillingReport>`

**Pourquoi `Option` et non `Result` :** l'endpoint renvoie **403** sur une org où l'utilisateur n'est pas propriétaire — constaté sur `le-vilain-petit-dev`. L'org reste entièrement navigable pour les caches, artifacts et runs ; seule la colonne Billing porte un ⚠. Un 403 dégrade, il n'interrompt pas.

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn fetch_maps_the_usage_items() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/organizations/systm-d/settings/billing/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "usageItems": [{
                    "date": "2026-07-01T00:00:00Z",
                    "product": "actions",
                    "sku": "Actions Linux",
                    "quantity": 3311.0,
                    "unitType": "Minutes",
                    "pricePerUnit": 0.006,
                    "grossAmount": 19.866,
                    "discountAmount": 19.866,
                    "netAmount": 0.0,
                    "organizationName": "systm-d",
                    "repositoryName": "josephine"
                }]
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let report = fetch(&client, "systm-d").await.expect("200 yields a report");

        assert_eq!(report.items.len(), 1);
        let it = &report.items[0];
        assert_eq!(it.month, "2026-07");
        assert_eq!(it.sku, "Actions Linux");
        assert_eq!(it.repo, "josephine");
        assert!((it.gross - 19.866).abs() < 1e-9);
        assert!((it.net - 0.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn a_403_degrades_to_none_rather_than_failing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/organizations/le-vilain-petit-dev/settings/billing/usage"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(fetch(&client, "le-vilain-petit-dev").await.is_none());
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core api::billing`
Expected: FAIL — `cannot find function fetch`.

- [ ] **Step 3: Écrire l'implémentation**

```rust
//! The organization usage report.
//!
//! The legacy billing endpoints (`/orgs/{org}/settings/billing/actions`,
//! `/packages`, `/shared-storage`) all return **410 Gone** — GitHub moved to a
//! unified billing platform. This one replaces them and is richer: it reports
//! per repository × per SKU × per month.

use super::Client;
use crate::billing::{BillingReport, UsageItem};

/// Fetch an org's usage report, or `None` when it is not readable.
///
/// A 403 means the user is not an owner of that org. That is not fatal: the
/// org stays navigable for caches, artifacts and runs, and only the billing
/// column is marked unavailable.
pub async fn fetch(client: &Client, org: &str) -> Option<BillingReport> {
    let v = client
        .get_json(&format!("/organizations/{org}/settings/billing/usage"))
        .await
        .ok()?;

    let items = v["usageItems"]
        .as_array()?
        .iter()
        .map(|i| {
            let date = i["date"].as_str().unwrap_or_default();
            UsageItem {
                // "2026-07-01T00:00:00Z" -> "2026-07". Anything shorter keeps
                // whatever is there rather than panicking on a slice.
                month: date.get(..7).unwrap_or(date).to_string(),
                product: i["product"].as_str().unwrap_or_default().to_string(),
                sku: i["sku"].as_str().unwrap_or_default().to_string(),
                quantity: i["quantity"].as_f64().unwrap_or(0.0),
                unit_type: i["unitType"].as_str().unwrap_or_default().to_string(),
                gross: i["grossAmount"].as_f64().unwrap_or(0.0),
                discount: i["discountAmount"].as_f64().unwrap_or(0.0),
                net: i["netAmount"].as_f64().unwrap_or(0.0),
                repo: i["repositoryName"].as_str().unwrap_or_default().to_string(),
            }
        })
        .collect();

    Some(BillingReport { items })
}
```

Ajouter `pub mod billing;` dans `api/mod.rs`.

- [ ] **Step 4: Relancer les tests**

Run: `cargo test -p bondebarras-core api::billing`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/api/
git commit -m "feat(api): releve de facturation, 403 degrade en None"
```

---

### Task 3: Le relevé entre dans l'étage 1

**Files:**
- Modify: `crates/bondebarras-core/src/model.rs`, `crates/bondebarras-core/src/scan.rs`

**Interfaces:**
- Produces: `OrgSummary.billing: Option<BillingReport>`

- [ ] **Step 1: Écrire le test qui échoue**

Ajouter dans `scan.rs` :

```rust
    #[tokio::test]
    async fn an_org_without_billing_access_is_still_scanned() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/actions/cache/usage-by-repository"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "repository_cache_usages": [
                    { "full_name": "systm-d/josephine",
                      "active_caches_size_in_bytes": 1000, "active_caches_count": 2 }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "josephine" }
            ])))
            .mount(&server)
            .await;
        // Billing refused: the org must survive with `billing: None`.
        Mock::given(method("GET"))
            .and(path("/organizations/systm-d/settings/billing/usage"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = overview(&client, &["systm-d".to_string()]).await;

        assert_eq!(out.len(), 1, "a billing 403 must not drop the org");
        assert_eq!(out[0].cache_bytes, 1000);
        assert!(out[0].billing.is_none());
    }
```

- [ ] **Step 2: Lancer le test pour vérifier qu'il échoue**

Run: `cargo test -p bondebarras-core an_org_without_billing`
Expected: FAIL — `no field billing on type OrgSummary`.

- [ ] **Step 3: Écrire l'implémentation**

Dans `model.rs`, ajouter le champ :

```rust
    /// The org's usage report, or `None` when billing is not readable —
    /// GitHub answers 403 to anyone who is not an owner. A 403 degrades this
    /// one column; it never drops the org.
    pub billing: Option<crate::billing::BillingReport>,
```

Dans `scan.rs`, à l'intérieur du futur par org, après les deux appels existants :

```rust
        // Third and last stage-1 request. Deliberately not `?`-propagated: an
        // org whose billing is refused is still worth showing.
        let billing = crate::api::billing::fetch(client, org).await;
```

et le passer au `OrgSummary`. Mettre à jour tous les autres sites de construction d'`OrgSummary` (les tests de `tui/app.rs` en construisent).

- [ ] **Step 4: Relancer la suite complète**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/
git commit -m "feat(scan): le releve de facturation rejoint l'etage 1"
```

---

### Task 4: L'onglet Billing

**Files:**
- Create: `crates/bondebarras-core/src/tui/views/billing.rs`
- Modify: `crates/bondebarras-core/src/tui/app.rs`, `crates/bondebarras-core/src/tui/views/mod.rs`, `crates/bondebarras-core/src/tui/mod.rs`

**Interfaces:**
- Produces:
  - `enum View { Orgs, Billing }` sur `App`, plus `App.month_cursor: usize`
  - `views::billing::render(app: &mut App, f: &mut Frame, area: Rect)`
  - `views::billing::gauge_line(used: u64, allowance: u64) -> String`

**Vue strictement diagnostique : aucune action destructive n'y est possible.**

- [ ] **Step 1: Écrire les tests qui échouent**

Dans `views/billing.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gauge_reports_overshoot_rather_than_capping_at_full() {
        // 818 % is the real figure measured on systm-d in July 2026. Clamping
        // it to 100 % would hide exactly the thing the tab exists to show.
        let line = gauge_line(16_369, 2_000);
        assert!(line.contains("818"), "got: {line}");
        assert!(line.contains("16 369") || line.contains("16369"), "got: {line}");
    }

    #[test]
    fn a_zero_allowance_does_not_divide_by_zero() {
        let line = gauge_line(100, 0);
        assert!(!line.contains("NaN"), "got: {line}");
        assert!(!line.contains("inf"), "got: {line}");
    }

    #[test]
    fn an_unused_month_reads_zero_percent() {
        assert!(gauge_line(0, 2_000).contains('0'));
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core billing::tests::the_gauge`
Expected: FAIL — `cannot find function gauge_line`.

- [ ] **Step 3: Écrire l'implémentation**

```rust
//! The Billing tab: strictly diagnostic, no destructive action.
//!
//! Minutes cannot be reclaimed retroactively, so the only useful thing this
//! view can do is name the repository burning them.

use crate::billing::FREE_MINUTES_PER_MONTH;
use crate::tui::app::App;
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

/// One line summarising allowance consumption.
///
/// Deliberately not clamped at 100 %: an org at 818 % of its included minutes
/// is exactly the situation the tab exists to surface, and a full bar would
/// say nothing.
pub fn gauge_line(used: u64, allowance: u64) -> String {
    let percent = if allowance == 0 {
        0
    } else {
        (used as f64 / allowance as f64 * 100.0).round() as u64
    };
    let filled = (percent as usize / 10).min(20);
    format!(
        "{} / {}   {}  {} %",
        thousands(used),
        thousands(allowance),
        "█".repeat(filled),
        percent
    )
}

/// Groups digits with a narrow space, as French convention wants.
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    let Some(org) = app.orgs.get(app.org_cursor) else {
        f.render_widget(
            Paragraph::new(Span::styled("Aucune organisation.", theme::muted())),
            area,
        );
        return;
    };

    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        org.login.clone(),
        theme::title_style(),
    ))];

    match &org.billing {
        None => lines.push(Line::from(Span::styled(
            "⚠ facturation illisible — vous n'êtes pas propriétaire de cette organisation",
            theme::status_warn(),
        ))),
        Some(report) => {
            let months = report.months();
            let month = months
                .get(app.month_cursor.min(months.len().saturating_sub(1)))
                .cloned()
                .unwrap_or_default();

            lines.push(Line::from(Span::styled(month.clone(), theme::muted())));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Minutes équivalent-inclus".to_string(),
                theme::text_style(),
            )));
            lines.push(Line::from(Span::styled(
                gauge_line(report.included_minutes(&month), FREE_MINUTES_PER_MONTH),
                theme::text_style(),
            )));

            let (gross, covered, billed) = report.cost(&month);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("Coûts   brut {gross:.2} €   couvert {covered:.2} €   facturé {billed:.2} €"),
                if billed > 0.0 {
                    theme::status_warn()
                } else {
                    theme::muted()
                },
            )));

            for sku in report.unknown_skus(&month) {
                lines.push(Line::from(Span::styled(
                    format!("⚠ SKU inconnu, compté ×1 : {sku}"),
                    theme::status_warn(),
                )));
            }
        }
    }

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

Dans `app.rs`, ajouter :

```rust
/// Which top-level view is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Orgs,
    Billing,
}
```

plus les champs `pub view: View` (initialisé à `View::Orgs`) et `pub month_cursor: usize` (à 0).

Dans `views/mod.rs`, `render` dispatche sur `app.view` : `View::Orgs` garde le split-pane actuel, `View::Billing` appelle `billing::render` sur tout le corps. Le titre de l'en-tête indique l'onglet actif.

Dans `tui/mod.rs`, ajouter les touches : `b` bascule `app.view`, et sous `View::Billing`, `←`/`→` déplacent `month_cursor`. Passer `app.view` dans le footer.

- [ ] **Step 4: Relancer**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/tui/
git commit -m "feat(tui): onglet Billing, diagnostic des minutes et des couts"
```

---

### Task 5: CLI headless — `scan --json`

**Files:**
- Modify: `crates/bondebarras-core/src/cli.rs`, `crates/bondebarras-core/src/lib.rs`
- Create: `crates/bondebarras-core/src/commands/mod.rs`, `crates/bondebarras-core/src/commands/scan.rs`
- Modify: `crates/bondebarras/tests/cli.rs`

**Interfaces:**
- Produces: `commands::scan::run(client: &Client, orgs: &[String], json: bool) -> Result<()>`

**Règle non négociable :** avec `--json`, **stdout ne porte que du JSON**. Progression et erreurs vont sur stderr, pour qu'un `| jq` fonctionne toujours.

- [ ] **Step 1: Écrire le test qui échoue**

Dans `crates/bondebarras/tests/cli.rs` :

```rust
#[test]
fn scan_json_is_documented_in_help() {
    Command::cargo_bin("bondebarras")
        .unwrap()
        .args(["scan", "--help"])
        .assert()
        .success()
        .stdout(contains("--json"));
}
```

- [ ] **Step 2: Lancer le test pour vérifier qu'il échoue**

Run: `cargo test -p bondebarras --test cli scan_json`
Expected: FAIL — la sortie ne contient pas `--json`.

- [ ] **Step 3: Écrire l'implémentation**

Dans `cli.rs`, `Scan` gagne `#[arg(long)] pub json: bool`.

`commands/scan.rs` :

```rust
//! Non-interactive overview.

use crate::api::Client;
use crate::model::human_size;
use crate::scan;
use anyhow::Result;

/// Print the stage-1 overview.
///
/// With `json`, **stdout carries JSON and nothing else** — every progress or
/// diagnostic line goes to stderr, so a `| jq` pipeline always parses.
pub async fn run(client: &Client, orgs: &[String], json: bool) -> Result<()> {
    let summaries = scan::overview(client, orgs).await;

    if json {
        let value: Vec<serde_json::Value> = summaries
            .iter()
            .map(|o| {
                serde_json::json!({
                    "org": o.login,
                    "cache_bytes": o.cache_bytes,
                    "cache_count": o.cache_count,
                    "billing_readable": o.billing.is_some(),
                    "repos": o.repos.iter().map(|r| serde_json::json!({
                        "name": r.name,
                        "cache_bytes": r.cache_bytes,
                        "cache_count": r.cache_count,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&value)?);
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
```

`commands/mod.rs` déclare `pub mod scan;`. `lib.rs` déclare `pub mod commands;` et délègue le bras `Scan`.

- [ ] **Step 4: Relancer**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/
git commit -m "feat(cli): scan --json pour un pipeline machine"
```

---

### Task 6: CLI headless — `clean`

**Files:**
- Modify: `crates/bondebarras-core/src/cli.rs`, `crates/bondebarras-core/src/lib.rs`
- Create: `crates/bondebarras-core/src/commands/clean.rs`
- Modify: `crates/bondebarras/tests/cli.rs`

**Interfaces:**
- Produces:
  - `struct CleanFilter { caches: bool, artifacts: bool, runs: bool, stale_pr: bool, older_than: Option<i64> }`
  - `fn select(items: &[Resource], filter: &CleanFilter) -> Vec<Resource>`
  - `async fn run(client, org, repo, filter, yes) -> Result<ExitCode>`

**Règles non négociables :**
- Sans `--yes`, `clean` affiche le plan et **ne supprime rien**.
- Le **palier 3 est refusé en headless**, sans drapeau de contournement. Aucune ressource n'y est rattachée aujourd'hui ; la règle se pose maintenant, pendant que la surface CLI se fige, pour qu'une opération future ne puisse pas se glisser dans un cron.
- Code de sortie `0` si tout a réussi, `1` si au moins une suppression a échoué.

- [ ] **Step 1: Écrire les tests qui échouent**

Dans `commands/clean.rs` — `select` est du calcul pur, donc testable sans réseau :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ResourceKind;

    fn res(kind: ResourceKind, id: u64, age: i64, stale: bool) -> Resource {
        Resource {
            kind,
            id,
            label: format!("r{id}"),
            size_bytes: 100,
            age_days: age,
            git_ref: None,
            stale_pr: stale,
        }
    }

    fn filter() -> CleanFilter {
        CleanFilter {
            caches: false,
            artifacts: false,
            runs: false,
            stale_pr: false,
            older_than: None,
        }
    }

    #[test]
    fn no_family_flag_selects_nothing() {
        // A `clean` with no family named must be a no-op, never "everything".
        let items = vec![res(ResourceKind::Cache, 1, 90, true)];
        assert!(select(&items, &filter()).is_empty());
    }

    #[test]
    fn families_are_cumulative() {
        let items = vec![
            res(ResourceKind::Cache, 1, 1, false),
            res(ResourceKind::Artifact, 2, 1, false),
            res(ResourceKind::WorkflowRun, 3, 1, false),
        ];
        let f = CleanFilter { caches: true, artifacts: true, ..filter() };
        let picked: Vec<u64> = select(&items, &f).iter().map(|r| r.id).collect();
        assert_eq!(picked, vec![1, 2]);
    }

    #[test]
    fn stale_pr_narrows_within_the_chosen_families() {
        let items = vec![
            res(ResourceKind::Cache, 1, 1, true),
            res(ResourceKind::Cache, 2, 1, false),
        ];
        let f = CleanFilter { caches: true, stale_pr: true, ..filter() };
        let picked: Vec<u64> = select(&items, &f).iter().map(|r| r.id).collect();
        assert_eq!(picked, vec![1]);
    }

    #[test]
    fn older_than_is_inclusive_of_the_boundary() {
        let items = vec![
            res(ResourceKind::Cache, 1, 30, false),
            res(ResourceKind::Cache, 2, 29, false),
        ];
        let f = CleanFilter { caches: true, older_than: Some(30), ..filter() };
        let picked: Vec<u64> = select(&items, &f).iter().map(|r| r.id).collect();
        assert_eq!(picked, vec![1]);
    }
}
```

Dans `crates/bondebarras/tests/cli.rs` :

```rust
#[test]
fn clean_requires_yes_to_delete() {
    Command::cargo_bin("bondebarras")
        .unwrap()
        .args(["clean", "--help"])
        .assert()
        .success()
        .stdout(contains("--yes"))
        .stdout(contains("--stale-pr"));
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core clean::tests && cargo test -p bondebarras --test cli clean_requires`
Expected: FAIL — `cannot find type CleanFilter`.

- [ ] **Step 3: Écrire l'implémentation**

Dans `cli.rs` :

```rust
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
```

`commands/clean.rs` :

```rust
//! Non-interactive cleanup, for a monthly cron.

use crate::api::Client;
use crate::clean::{self, Plan, Progress};
use crate::model::{Resource, ResourceKind, RiskTier, human_size};
use crate::scan;
use anyhow::{Result, bail};
use std::process::ExitCode;
use tokio::sync::mpsc;

/// Which resources a headless run should touch.
pub struct CleanFilter {
    pub caches: bool,
    pub artifacts: bool,
    pub runs: bool,
    pub stale_pr: bool,
    pub older_than: Option<i64>,
}

/// Resources matching the filter.
///
/// Naming no family selects **nothing**. A `clean` that quietly meant
/// "everything" would be the worst possible default for an irreversible
/// operation running unattended.
pub fn select(items: &[Resource], filter: &CleanFilter) -> Vec<Resource> {
    items
        .iter()
        .filter(|r| match r.kind {
            ResourceKind::Cache => filter.caches,
            ResourceKind::Artifact => filter.artifacts,
            ResourceKind::WorkflowRun => filter.runs,
        })
        .filter(|r| !filter.stale_pr || r.stale_pr)
        .filter(|r| filter.older_than.is_none_or(|d| r.age_days >= d))
        .cloned()
        .collect()
}

/// Run the cleanup. Returns the process exit code: failure if any deletion did.
pub async fn run(
    client: &Client,
    org: &str,
    repo: &str,
    filter: &CleanFilter,
    yes: bool,
) -> Result<ExitCode> {
    let items = scan::repo_detail(client, org, repo).await?;
    let picked = select(&items, filter);

    let plan = Plan {
        items: picked,
        owner: org.to_string(),
        repo: repo.to_string(),
    };

    // The nuclear tier demands typing the target's name, which no headless
    // run can do. There is deliberately no flag to bypass this.
    if plan.tier() >= RiskTier::Nuclear {
        bail!("le palier 3 exige une confirmation interactive et ne peut pas s'exécuter sans interface");
    }

    if plan.items.is_empty() {
        eprintln!("Rien à supprimer.");
        return Ok(ExitCode::SUCCESS);
    }

    if !yes {
        eprintln!("Plan ({}) — relancez avec --yes pour l'appliquer :", plan.summary());
        for r in &plan.items {
            eprintln!("  {:<40} {:>10}", r.label, human_size(r.size_bytes));
        }
        return Ok(ExitCode::SUCCESS);
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Progress>();
    clean::execute(client, plan, tx).await;

    let mut failures = 0usize;
    while let Ok(msg) = rx.try_recv() {
        match msg {
            Progress::Failed { id, reason, .. } => {
                failures += 1;
                eprintln!("Erreur : suppression de {id} — {reason}");
            }
            Progress::Finished { freed, failures: f } => {
                failures = f;
                eprintln!("Bon débarras ! {} libérés.", human_size(freed));
            }
            Progress::Done { .. } => {}
        }
    }

    Ok(if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
```

Câbler le bras `Clean` dans `lib.rs`, en propageant le `ExitCode`.

- [ ] **Step 4: Relancer**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/
git commit -m "feat(cli): sous-commande clean headless"
```

---

### Task 7: Solder les dettes de la v0.1

**Files:**
- Modify: `crates/bondebarras-core/src/tui/mod.rs`
- Modify: `crates/bondebarras-core/src/api/{caches,artifacts,runs}.rs`

Trois éléments reportés lors de la revue finale de la v0.1.

**Interfaces:** aucune nouvelle surface publique.

- [ ] **Step 1: Écrire les tests qui échouent**

Un test par lister, sur le modèle des tests wiremock existants — voici celui des caches, à décliner pour `artifacts` et `runs` avec leurs clés de tableau et leurs champs respectifs :

```rust
    #[tokio::test]
    async fn an_item_without_a_usable_id_is_dropped() {
        // An item we cannot address is an item we must not offer to delete.
        // Coercing a missing id to 0 would collide every such item into one
        // selection slot.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "actions_caches": [
                    { "id": 9, "ref": "refs/heads/main", "key": "ok",
                      "size_in_bytes": 10, "last_accessed_at": "2026-06-01T00:00:00Z" },
                    { "ref": "refs/heads/main", "key": "no-id",
                      "size_in_bytes": 20, "last_accessed_at": "2026-06-01T00:00:00Z" },
                    { "id": "12", "ref": "refs/heads/main", "key": "string-id",
                      "size_in_bytes": 30, "last_accessed_at": "2026-06-01T00:00:00Z" }
                ]
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = list(&client, "systm-d", "claudine").await.unwrap();

        assert_eq!(items.len(), 1, "only the addressable item survives");
        assert_eq!(items[0].id, 9);
    }
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p bondebarras-core an_item_without_a_usable_id`
Expected: les trois tests doivent **passer** immédiatement — le comportement a été implanté pendant la vague de correctifs de la v0.1, seuls les tests manquaient. Si l'un échoue, c'est une vraie régression : le signaler avant d'aller plus loin.

C'est le seul endroit de ce plan où un test n'a pas de phase RED, et c'est délibéré : il verrouille un comportement existant non couvert.

- [ ] **Step 3: Garde de sortie pendant une purge**

Quitter pendant une purge abandonne silencieusement les suppressions restantes et n'affiche aucun récapitulatif. Dans `tui/mod.rs`, le bras de sortie devient :

```rust
            KeyCode::Char('q') | KeyCode::Esc => {
                // A purge runs on a spawned task; quitting drops whatever is
                // still queued, with no summary. Say so once and let a second
                // press through — an unattended quit must not silently cut an
                // irreversible operation short.
                if app.purging_org.is_some() && !app.quit_armed {
                    app.quit_armed = true;
                    app.status =
                        "Purge en cours — [q] à nouveau pour quitter sans l'achever.".into();
                } else {
                    app.should_quit = true;
                }
            }
```

Ajouter `pub quit_armed: bool` à `App`, initialisé à `false`, et le remettre à `false` sur `Progress::Finished`.

- [ ] **Step 4: Vérifier**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/
git commit -m "fix(tui): garde de sortie pendant une purge, tests des ids inutilisables"
```

---

### Task 8: Documentation de la v0.2

**Files:**
- Modify: `README.md`, `CHANGELOG.md`, `CLAUDE.md`, `site/content/_index.md`, `site/content/_index.fr.md`

- [ ] **Step 1: Mettre à jour le CHANGELOG**

Format Keep a Changelog, nouvelle section `## [0.2.0] - 2026-07-29` listant : onglet Billing, `scan --json`, sous-commande `clean`, garde de sortie pendant une purge.

- [ ] **Step 2: Mettre à jour le README**

Ajouter la section CLI headless avec les deux exemples de la spec, le tableau des drapeaux, et la règle « sans `--yes`, rien n'est supprimé ». Ajouter l'onglet Billing aux raccourcis (`b`, `←`/`→`).

**Ne pas promettre ce que la v0.2 ne fait pas** : ni packages GHCR, ni branches/tags/releases, ni archivage de repos — ce sont les v0.3 à v0.5.

- [ ] **Step 3: Mettre à jour `CLAUDE.md`**

Ajouter au tableau « Where to change what » : `billing.rs`, `api/billing.rs`, `commands/{scan,clean}.rs`, `tui/views/billing.rs`.

- [ ] **Step 4: Mettre à jour les deux pages du site**

Mentionner l'onglet Billing et la CLI headless, dans les deux langues, en gardant la structure de sections existante.

- [ ] **Step 5: Vérifier et commiter**

```bash
cd site && zola build && cd ..
git add -A
git commit -m "docs: v0.2 — onglet Billing et CLI headless"
```

## Self-Review

**Couverture de la spec v0.2**

| Exigence | Tâche |
|---|---|
| §3 relevé d'usage, mapping des trois montants | 1, 2 |
| §3.2 403 → `None`, org toujours navigable | 2, 3 |
| §4 multiplicateurs, SKU inconnu signalé | 1 |
| §4 repos publics exclus de la jauge | 1 |
| §5 onglet Billing, navigation par mois | 4 |
| §6 `scan --json`, stdout JSON pur | 5 |
| §6 `clean` + drapeaux, dry-run sans `--yes` | 6 |
| §6 palier 3 refusé en headless | 6 |
| §6 code de sortie 1 si échec | 6 |
| §7 découpage des modules | 1-6 |
| §8 tests | chaque tâche |
| Dettes v0.1 (garde de sortie, tests des ids) | 7 |
| Documentation | 8 |

**Cohérence des types**

- `UsageItem`/`BillingReport` : construits en Task 1, remplis en Task 2, consommés en Tasks 3-5.
- `OrgSummary.billing: Option<BillingReport>` : ajouté Task 3, lu Tasks 4 et 5.
- `CleanFilter`/`select` : Task 6 seulement.
- `View`/`month_cursor`/`quit_armed` : ajoutés Tasks 4 et 7, lus dans `tui/mod.rs`.
- `Progress::Failed` porte `kind` depuis la v0.1 — le `..` du motif en Task 6 l'ignore volontairement.
