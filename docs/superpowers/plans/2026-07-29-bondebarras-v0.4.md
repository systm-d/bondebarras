# bondebarras v0.4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Nettoyer branches mergées, tags et assets de releases — **7,3 Go mesurés** sur quatre organisations.

**Architecture:** La détection des branches mortes réutilise l'appel PR qui alimente déjà le drapeau ⚑ depuis la v0.1, pour un coût marginal nul. `Resource.protected`, ajouté en v0.3, sert aux tags et aux branches protégées sans nouveau mécanisme.

**Tech Stack:** Inchangé.

## Global Constraints

- Rust **edition 2024**, MSRV **1.88**. `unsafe_code = "forbid"` ; clippy `all = warn`, CI en `-D warnings` ; rustfmt `max_width = 100`.
- **Doc comments en anglais. Chaînes user-facing en français**, accents inclus.
- Jamais `ERROR`/`FATAL`/`PANIC` en user-facing. `Erreur : ` ajouté **une seule fois**, par `run()`.
- **Routes API avec slash initial.**
- `cargo test <filtre>` prend une **sous-chaîne littérale**.
- `sort_by(|a,b| b.x.cmp(&a.x))` → `sort_by_key(|r| std::cmp::Reverse(r.x))`.
- Conventional Commits. Multi-plateforme.
- **Preuve TDD : sortie brute, redirigée vers un fichier puis relue et collée telle quelle.** Cinq rapports de ce projet ont présenté du texte reformaté comme une capture littérale.
- **Tests qui ne peuvent pas échouer :** huit tests de ce projet ont nommé la bonne propriété sans pouvoir échouer dessus. Pour chacun, se demander ce que renverrait une implémentation fautive et construire le fixture pour que les deux divergent — puis le prouver contre le défaut.
- **Tests de rendu :** la v0.3 a livré une modale correcte et invisible parce que tous ses tests assertaient sur des `Span` et des `Line`, jamais sur un buffer. Toute nouvelle vue se teste avec `TestBackend`, en **balayant** les tailles plutôt qu'en échantillonnant : le dernier défaut se reproduisait à une hauteur précise par largeur.

## Ce qui existe déjà (v0.1-v0.3, fusionnées)

`Client::{get_json, get_json_or_missing, delete}` · `api::{caches, artifacts, runs, prs, repos, packages, billing}` · `scan::{overview, repo_detail}` · `clean::{Plan, Progress, execute}` · `model::{Resource, ResourceKind, RiskTier, risk_tier, human_size, size_display}` avec `Resource.protected` · `packages::classify` · le TUI complet. 136 tests.

`Resource` porte déjà les sept champs plus `protected`. `ResourceKind` a quatre variantes.

---

### Task 1: Détection des branches mortes (calcul pur) et `closed_prs`

**Files:**
- Create: `crates/bondebarras-core/src/refs.rs`
- Modify: `crates/bondebarras-core/src/api/prs.rs`, `crates/bondebarras-core/src/lib.rs`

**Interfaces:**
- `struct ClosedPrs { numbers: HashSet<u64>, merged_refs: HashSet<String> }`
- `api::prs::closed_prs(client, owner, repo) -> Result<ClosedPrs>` (remplace `closed_numbers`)
- `struct BranchRef { name: String, protected: bool }`
- `fn branch_is_dead(b: &BranchRef, default_branch: &str, merged_refs: &HashSet<String>) -> bool`

**Le levier :** une PR fermée expose `head.ref` et `merged_at`. `closed_numbers` récupère déjà ces PR depuis la v0.1 pour le drapeau ⚑ des caches. En extraire aussi les refs mergées ne coûte **aucune requête supplémentaire**. Comparer chaque branche à la branche par défaut coûterait un `compare` par branche — 100 sur un repo réel.

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn b(name: &str, protected: bool) -> BranchRef {
        BranchRef { name: name.to_string(), protected }
    }

    fn merged(refs: &[&str]) -> HashSet<String> {
        refs.iter().map(|r| r.to_string()).collect()
    }

    #[test]
    fn a_branch_whose_pr_was_merged_is_dead() {
        let m = merged(&["claude/landing-3jbqk4"]);
        assert!(branch_is_dead(&b("claude/landing-3jbqk4", false), "main", &m));
    }

    #[test]
    fn a_branch_with_no_merged_pr_is_alive() {
        // THE test. A PR closed *without* merging leaves its branch alive —
        // the work was rejected, not integrated, and may still be resumed.
        // Without this case a classifier keying on "closed" alone would pass
        // and the tool would offer to delete work someone meant to revisit.
        let m = merged(&["claude/landing-3jbqk4"]);
        assert!(!branch_is_dead(&b("feature/rejected", false), "main", &m));
    }

    #[test]
    fn the_default_branch_is_never_dead() {
        // Even if a merged PR targeted it — a PR merged *into* main puts
        // main nowhere near the head refs, but a mis-shaped fixture could.
        let m = merged(&["main"]);
        assert!(!branch_is_dead(&b("main", false), "main", &m));
    }

    #[test]
    fn a_protected_branch_is_never_dead() {
        let m = merged(&["release/2.0"]);
        assert!(!branch_is_dead(&b("release/2.0", true), "main", &m));
    }
}
```

- [ ] **Step 2: Lancer les tests**

Déclarer `pub mod refs;` dans `lib.rs` **avant** ce run, sinon `cargo test` rapporte « 0 tests » au lieu d'une erreur de compilation.

Run: `cargo test -p bondebarras-core refs > /tmp/v4t1-red.txt 2>&1; cat /tmp/v4t1-red.txt`
Expected: FAIL — `cannot find function branch_is_dead`.

- [ ] **Step 3: Implémenter**

`refs.rs` :

```rust
//! Which branches are dead, decided without a request per branch.
//!
//! Comparing every branch against the default would cost one `compare` call
//! each — a hundred on a real repository. It is not needed: a closed pull
//! request carries `head.ref` and `merged_at`, and the closed-PR listing is
//! already fetched for the caches' ⚑ flag. A branch a merged PR came from is
//! a branch nobody works on any more.

use std::collections::HashSet;

/// A branch as the listing endpoint reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRef {
    pub name: String,
    pub protected: bool,
}

/// True when the branch can be offered for deletion.
///
/// Three exclusions, in order of how badly getting them wrong would hurt:
/// the default branch (deleting it breaks the repository), a protected branch
/// (someone deliberately said no), and a branch whose pull request was closed
/// *without* merging — that work was rejected, not integrated, and deleting it
/// throws away something a person may still intend to revisit.
pub fn branch_is_dead(b: &BranchRef, default_branch: &str, merged_refs: &HashSet<String>) -> bool {
    if b.name == default_branch || b.protected {
        return false;
    }
    merged_refs.contains(&b.name)
}
```

Dans `api/prs.rs`, remplacer `closed_numbers` par `closed_prs`, qui rend les deux jeux depuis la même pagination :

```rust
/// Everything the closed-PR listing tells us, from one paginated fetch.
#[derive(Debug, Clone, Default)]
pub struct ClosedPrs {
    /// Numbers, for the caches' ⚑ flag.
    pub numbers: HashSet<u64>,
    /// `head.ref` of the PRs that were actually merged, for dead branches.
    /// A PR closed without merging contributes nothing here.
    pub merged_refs: HashSet<String>,
}
```

Boucler comme aujourd'hui, en accumulant `item["number"].as_u64()` dans `numbers` et, **uniquement si `item["merged_at"]` n'est pas `null`**, `item["head"]["ref"].as_str()` dans `merged_refs`.

Mettre à jour l'appelant dans `scan.rs` (`mark_stale` prend `&closed.numbers`).

- [ ] **Step 4: Relancer, puis Step 5: Commit**

`feat(refs): detection des branches mortes via les PR mergees`

---

### Task 2: Trois variantes et leur palier

**Files:** `crates/bondebarras-core/src/model.rs`

`ResourceKind` gagne `Branch`, `Tag`, `ReleaseAsset`. Le `match` exhaustif de `risk_tier` refusera de compiler tant qu'elles n'ont pas de palier — **les trois en `RiskTier::Medium`**.

Comme en v0.3, l'ajout casse tous les `match` exhaustifs du crate. Faire le **minimum** pour compiler, avec des bras **honnêtes** : une erreur nommant la tâche qui les remplira, jamais un comportement silencieusement emprunté à une autre famille. Rapporter précisément lesquels ont été bouchonnés.

Ajouter les trois à `ResourceKind::ALL` et un test par variante sur son palier.

⚠️ `every_v01_kind_is_low_risk` énumère explicitement les trois familles v0.1 depuis la v0.3 — il ne cassera pas, mais vérifier.

Commit : `feat(model): branches, tags et assets relevent du palier 2`

---

### Task 3: Endpoints refs et releases

**Files:** Create `crates/bondebarras-core/src/api/refs.rs`, `crates/bondebarras-core/src/api/releases.rs`

**Interfaces:**
- `refs::branches(client, owner, repo) -> Result<Vec<BranchRef>>`
- `refs::tags(client, owner, repo) -> Result<Vec<String>>`
- `refs::delete_branch(client, owner, repo, name) -> Result<()>`
- `refs::delete_tag(client, owner, repo, name) -> Result<()>`
- `releases::assets(client, owner, repo) -> Result<Vec<ReleaseAsset>>` avec `struct ReleaseAsset { id: u64, name: String, size: u64, release_tag: String, age_days: i64 }`
- `releases::delete_asset(client, owner, repo, id) -> Result<()>`

Routes, vérifiées :
```
GET    /repos/{o}/{r}/branches?per_page=100      -> [{ name, protected }]
GET    /repos/{o}/{r}/tags?per_page=100          -> [{ name }]
DELETE /repos/{o}/{r}/git/refs/heads/{branch}
DELETE /repos/{o}/{r}/git/refs/tags/{tag}
GET    /repos/{o}/{r}/releases?per_page=100      -> [{ tag_name, assets: [{ id, name, size, created_at }] }]
DELETE /repos/{o}/{r}/releases/assets/{id}
```

**`releases[].assets[].size` existe** — contrairement aux packages de la v0.3, cette famille se mesure en octets. Sur `exec-d/terminus`, 25 releases pèsent 1 453 Mo.

**Paginer `branches` et `tags`** sur l'idiome de `prs::closed_prs` : `monolith-back` atteint le plafond de 100.

**Aucun `unwrap_or(0)` sur un id.** `App.selected` est clé sur `(kind, id)`. Les branches et tags n'ayant pas d'identifiant numérique, leur `Resource.id` doit être **stable et unique par nom** — utiliser un hachage du nom, et le documenter. Une collision ferait supprimer la mauvaise branche.

Tests wiremock sur le modèle de `api/caches.rs`, plus un test que deux noms distincts ne collident pas.

Commit : `feat(api): endpoints des branches, tags et assets de releases`

---

### Task 4: Intégration au drill-down et à la suppression

**Files:** `crates/bondebarras-core/src/scan.rs`, `crates/bondebarras-core/src/clean.rs`

`repo_detail` ajoute les trois familles à son `futures::join!`. Comme pour les packages, **un échec sur l'une ne doit pas coûter les autres** — dégrader avec `unwrap_or_default()`, pas `?`.

Conversion en `Resource` :

| Famille | `size_bytes` | `protected` | `stale_pr` |
|---|---|---|---|
| Branche morte | 0 | `false` | `false` |
| Branche vivante / défaut / protégée | 0 | **`true`** | `false` |
| Tag | 0 | **`true`** | `false` |
| Asset de release | **`size`** | `false` | `false` |

Les branches vivantes et les tags sont **affichés mais jamais cochables en masse** — `protected` s'en charge, sans nouveau mécanisme.

`clean::execute` gagne les trois bras. Une branche et un tag se suppriment par **nom**, pas par id : le `Resource.label` porte le nom, et le bras doit l'utiliser plutôt que l'id haché.

Commit : `feat(scan): branches, tags et assets au drill-down`

---

### Task 5: CLI, rendu et documentation

**Files:** `cli.rs`, `commands/clean.rs`, `tui/views/repo.rs`, `README.md`, `CHANGELOG.md`, `CLAUDE.md`, `site/content/_index*.md`

Trois drapeaux : `--branches`, `--tags`, `--assets`. Comme les autres, **leur absence ne sélectionne rien**. `CleanFilter` gagne trois champs.

Le panneau droit affiche la classification : `mergée #31 ⚑` pour une branche morte, `par défaut` / `protégée` pour une vivante, le tag de release pour un asset.

**Un test de rendu `TestBackend` balayant les largeurs** pour vérifier qu'une ligne de branche reste lisible — la v0.3 a livré des lignes correctes et tronquées faute d'un tel test.

Documentation : annoncer les trois familles et **les 7,3 Go mesurés**, en citant les repos réels (`exec-d/terminus` 1 453 Mo, `delfour-co/githero` 1 371 Mo). Ne pas annoncer la v0.5. Ne jamais promettre la suppression d'une release entière — seuls ses assets partent.

Commit : `feat(cli): branches, tags et assets rejoignent les drapeaux` puis un commit doc séparé.

## Self-Review

| Exigence spec | Tâche |
|---|---|
| §2 détection sans requête par branche | 1 |
| §2.1 dédup, PR non mergée, branche par défaut, protégée | 1 |
| §3 trois familles, palier 2 | 2, 3 |
| §3 releases non supprimables, seulement leurs assets | 3, 5 |
| §3 tags jamais présélectionnés | 4 |
| §4 `closed_prs` un appel deux usages | 1 |
| §5 rendu | 5 |
| §6 tests | chaque tâche |

**Le test qui compte** est `a_branch_with_no_merged_pr_is_alive`. Sans le cas négatif, une classification marquant toute PR fermée passerait, et l'outil proposerait de supprimer le travail d'une PR rejetée qu'on comptait reprendre.
