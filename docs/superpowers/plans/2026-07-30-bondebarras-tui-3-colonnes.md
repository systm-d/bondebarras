# TUI trois colonnes, marquage de sûreté et jauges — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rendre la navigation visible — trois colonnes au lieu d'un arbre replié — marquer sur trois niveaux ce qui est supprimable sans risque, et montrer où en sont les deux quotas mesurables.

**Architecture:** `safety.rs` classe en calcul pur ; `Resource` gagne un champ rempli au scan ; le TUI passe d'un arbre à trois colonnes qui se replient selon la largeur ; la colonne 3 se charge après une pause du curseur, avec annulation des résultats périmés.

**Tech Stack:** Inchangé — ratatui 0.30, crossterm 0.29, tokio.

## Global Constraints

- Rust **edition 2024**, MSRV **1.88**. `unsafe_code = "forbid"` ; clippy `all = warn`, CI en `-D warnings` ; rustfmt `max_width = 100`.
- **Doc comments en anglais. Chaînes user-facing en français**, accents inclus.
- Jamais `ERROR`/`FATAL`/`PANIC` en user-facing. `Erreur : ` ajouté **une seule fois**, par `run()`.
- `cargo test <filtre>` prend une **sous-chaîne littérale**. `repos` matche déjà trois modules — choisir des noms de test distinctifs.
- `sort_by(|a,b| b.x.cmp(&a.x))` → `sort_by_key(|r| std::cmp::Reverse(r.x))`.
- Conventional Commits. Multi-plateforme.
- **Preuve TDD : sortie brute, redirigée vers un fichier puis relue et collée telle quelle.** Cinq rapports de ce projet ont présenté du texte reformaté comme une capture littérale.
- **Tests qui ne peuvent pas échouer :** dix tests de ce projet ont nommé la bonne propriété sans pouvoir échouer dessus. Construire chaque fixture pour que l'implémentation correcte et la fautive divergent, puis le prouver contre le défaut.
- **Tests de rendu : rendre le vrai `Rect`, pas la frame entière, et balayer les tailles.** Le dernier défaut de modale se reproduisait à une hauteur précise par largeur ; trois tailles échantillonnées l'avaient manqué. Un test de rendu ancré sur `f.area()` au lieu du rect du panneau ne peut pas échouer.

## Ce qui existe (v0.5, fusionnée) — 310 tests

Sept `ResourceKind` plus `Repository`. `Resource { kind, id, label, size_bytes, age_days, git_ref, stale_pr, protected }`. `Focus { Orgs, Repos, Resources }` désignant des niveaux d'un arbre. `scan::repo_detail_with_warnings`. `packages::classify`, `refs::classify_branch`, `repos::classify_repo`. `App` avec `org_cursor`, `repo_cursor`, `res_cursor`, `selected`, `selected_repo`, `org_state`, `res_state`, `loaded`.

## Structure des fichiers

| Fichier | Responsabilité |
|---|---|
| `crates/bondebarras-core/src/safety.rs` | classification pure des trois niveaux |
| `crates/bondebarras-core/src/model.rs` | `Resource.safety` |
| `crates/bondebarras-core/src/scan.rs` | remplit `safety` au scan |
| `crates/bondebarras-core/src/tui/views/gauges.rs` | les deux jauges |
| `crates/bondebarras-core/src/tui/views/repos.rs` | colonne 2 |
| `crates/bondebarras-core/src/tui/views/orgs.rs` | colonne 1, réduite |
| `crates/bondebarras-core/src/tui/views/mod.rs` | disposition responsive, pied de page |
| `crates/bondebarras-core/src/tui/app.rs` | curseur de colonne, cache, génération de chargement |
| `crates/bondebarras-core/src/tui/mod.rs` | pause, annulation, touches |

---

### Task 1: Classification de sûreté (calcul pur)

**Files:**
- Create: `crates/bondebarras-core/src/safety.rs`
- Modify: `crates/bondebarras-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `enum Safety { Safe, Check, Keep }`
  - `struct RepoContext { merged_refs: HashSet<String>, live_branches: HashSet<String>, default_branch: String, release_tags: Vec<String> }`
  - `fn classify(r: &Resource, ctx: &RepoContext) -> Safety`

`release_tags` est ordonné de la plus récente à la plus ancienne, tel que l'API les rend.

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ResourceKind;

    fn ctx() -> RepoContext {
        RepoContext {
            merged_refs: ["claude/landing".to_string()].into_iter().collect(),
            live_branches: ["main".to_string(), "wip".to_string()].into_iter().collect(),
            default_branch: "main".to_string(),
            release_tags: vec!["v0.12.0".into(), "v0.11.0".into(), "v0.10.0".into()],
        }
    }

    fn res(kind: ResourceKind, age: i64) -> Resource {
        Resource {
            kind,
            id: 1,
            label: String::new(),
            size_bytes: 0,
            age_days: age,
            git_ref: None,
            stale_pr: false,
            protected: false,
            safety: Safety::Keep,
        }
    }

    #[test]
    fn a_cache_on_a_closed_prs_ref_is_safe() {
        let mut c = res(ResourceKind::Cache, 12);
        c.git_ref = Some("claude/landing".into());
        assert_eq!(classify(&c, &ctx()), Safety::Safe);
    }

    #[test]
    fn a_cache_pinned_to_a_closed_pr_is_safe_via_stale_pr() {
        // A pull ref names no branch, so `merged_refs` — which holds
        // `head.ref` values — can never match it. This is the path that
        // covers the 320 caches on the author's own `josephine`.
        let mut c = res(ResourceKind::Cache, 12);
        c.git_ref = Some("refs/pull/54/merge".into());
        c.stale_pr = true;
        assert_eq!(classify(&c, &ctx()), Safety::Safe);

        // The same ref shape with an open PR must NOT be safe, or the tool
        // would offer to delete the cache of work in progress.
        let mut open = res(ResourceKind::Cache, 12);
        open.git_ref = Some("refs/pull/99/merge".into());
        assert_eq!(classify(&open, &ctx()), Safety::Check);
    }

    #[test]
    fn a_cache_on_a_vanished_branch_is_safe() {
        // Nothing can pull it by name any more, and no PR will resurrect it.
        let mut c = res(ResourceKind::Cache, 12);
        c.git_ref = Some("gone-branch".into());
        assert_eq!(classify(&c, &ctx()), Safety::Safe);
    }

    #[test]
    fn a_cache_on_the_default_branch_is_kept() {
        let mut c = res(ResourceKind::Cache, 12);
        c.git_ref = Some("main".into());
        assert_eq!(classify(&c, &ctx()), Safety::Keep);
    }

    #[test]
    fn a_cache_on_a_live_branch_is_only_worth_checking() {
        let mut c = res(ResourceKind::Cache, 12);
        c.git_ref = Some("wip".into());
        assert_eq!(classify(&c, &ctx()), Safety::Check);
    }

    #[test]
    fn an_expired_artifact_is_safe_but_a_recent_one_is_kept() {
        // GitHub already made an expired artifact undownloadable; it only
        // occupies a row until deleted. A recent one is still live.
        let mut expired = res(ResourceKind::Artifact, 2);
        expired.label = "github-pages (expiré)".into();
        assert_eq!(classify(&expired, &ctx()), Safety::Safe);

        let recent = res(ResourceKind::Artifact, 2);
        assert_eq!(classify(&recent, &ctx()), Safety::Keep);
    }

    #[test]
    fn an_asset_two_releases_back_is_safe_and_the_previous_one_is_not() {
        // THE test of this task. Collapsing these two into one level would
        // force an arbitrary call, and the tool would either offer to delete
        // the release someone is still on, or refuse to clean anything old.
        let mut old = res(ResourceKind::ReleaseAsset, 40);
        old.label = "josephine-linux (v0.10.0)".into();
        assert_eq!(classify(&old, &ctx()), Safety::Safe);

        let mut previous = res(ResourceKind::ReleaseAsset, 20);
        previous.label = "josephine-linux (v0.11.0)".into();
        assert_eq!(classify(&previous, &ctx()), Safety::Check);

        let mut latest = res(ResourceKind::ReleaseAsset, 2);
        latest.label = "josephine-linux (v0.12.0)".into();
        assert_eq!(classify(&latest, &ctx()), Safety::Keep);
    }

    #[test]
    fn a_protected_resource_is_never_safe() {
        // `protected` is the bulk-selection gate and outranks every other
        // signal. A tag carries it, and a tag is what releases point at.
        let mut t = res(ResourceKind::Tag, 400);
        t.protected = true;
        assert_eq!(classify(&t, &ctx()), Safety::Keep);
    }

    #[test]
    fn a_repository_is_never_marked_whatever_its_age() {
        // v0.5's rule: `pushed_at` is not proof of abandonment. A finished
        // library does not move for two years without being dead.
        let old = res(ResourceKind::Repository, 775);
        assert_eq!(classify(&old, &ctx()), Safety::Keep);
    }
}
```

- [ ] **Step 2: Lancer les tests**

Déclarer `pub mod safety;` dans `lib.rs` **avant** ce run, sinon `cargo test` rapporte « 0 tests » au lieu d'une erreur de compilation — ce qui ne prouve rien.

Run: `cargo test -p bondebarras-core safety:: > /tmp/t1-red.txt 2>&1; cat /tmp/t1-red.txt`
Expected: FAIL — `cannot find type Safety`.

- [ ] **Step 3: Écrire l'implémentation**

```rust
//! How safe a resource is to delete, on three levels.
//!
//! Two levels would force an arbitrary call. An asset from the release before
//! last and a cache from a closed pull request are not dead in the same way:
//! someone may still be pulling the first, while nothing references the
//! second. Collapsing them means lying in one direction or the other.

use crate::model::{Resource, ResourceKind};
use std::collections::HashSet;

/// How safe a resource is to delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Safety {
    /// Nothing live references it. `[A]` takes these.
    Safe,
    /// Plausibly dead, but a person should look. Shown, never preselected.
    Check,
    /// Live, or protected. Never offered in bulk.
    Keep,
}

/// What the repository's other listings say, so a resource can be judged
/// against them. Every field comes from `repo_detail`'s existing calls — this
/// costs no extra request.
#[derive(Debug, Clone, Default)]
pub struct RepoContext {
    /// `head.ref` of the pull requests that were actually merged.
    pub merged_refs: HashSet<String>,
    /// Branches that still exist.
    pub live_branches: HashSet<String>,
    pub default_branch: String,
    /// Release tags, newest first, as the API returns them.
    pub release_tags: Vec<String>,
}

/// How many releases back an asset must be before it counts as safe.
const SAFE_RELEASE_DEPTH: usize = 2;

/// An artifact this old is worth checking even if it has not expired.
const ARTIFACT_CHECK_DAYS: i64 = 30;

/// A workflow run this old is worth checking.
const RUN_CHECK_DAYS: i64 = 90;

/// Judge one resource against its repository's context.
///
/// `protected` outranks everything: it is the bulk-selection gate, and a
/// resource behind it must never be offered as safe regardless of its age or
/// its refs.
pub fn classify(r: &Resource, ctx: &RepoContext) -> Safety {
    if r.protected {
        return Safety::Keep;
    }

    match r.kind {
        // Never marked at any level. `pushed_at` is not proof of abandonment.
        ResourceKind::Repository => Safety::Keep,
        ResourceKind::Tag => Safety::Keep,

        ResourceKind::Cache | ResourceKind::WorkflowRun => classify_by_ref(r, ctx),
        ResourceKind::Artifact => {
            if r.label.contains("(expiré)") {
                Safety::Safe
            } else if r.age_days >= ARTIFACT_CHECK_DAYS {
                Safety::Check
            } else {
                Safety::Keep
            }
        }
        ResourceKind::ReleaseAsset => classify_asset(r, ctx),
        // Untagged and orphaned attestations already carry `protected: false`
        // from `packages::classify`; a tagged version carries `protected`.
        ResourceKind::PackageVersion => Safety::Safe,
        ResourceKind::Branch => {
            let name = r.label.as_str();
            if ctx.merged_refs.contains(name) {
                Safety::Safe
            } else {
                Safety::Check
            }
        }
    }
}

/// A cache or a run, judged by the ref it is attached to.
fn classify_by_ref(r: &Resource, ctx: &RepoContext) -> Safety {
    // A cache pinned to a closed pull request carries `refs/pull/32/merge`,
    // which names no branch — `merged_refs` holds `head.ref` values and would
    // never match it. `stale_pr` already resolves that case, from the same
    // closed-PR listing, and has since v0.1. Reuse it rather than re-deriving.
    if r.stale_pr {
        return Safety::Safe;
    }

    let Some(name) = r.git_ref.as_deref().map(strip_ref_prefix) else {
        return if r.kind == ResourceKind::WorkflowRun && r.age_days >= RUN_CHECK_DAYS {
            Safety::Check
        } else {
            Safety::Keep
        };
    };

    if name == ctx.default_branch {
        return Safety::Keep;
    }
    if ctx.merged_refs.contains(name) {
        return Safety::Safe;
    }
    // A ref naming a branch that no longer exists: nothing pulls it by name,
    // and no pull request will bring it back.
    if !ctx.live_branches.contains(name) && !name.starts_with("refs/pull/") {
        return Safety::Safe;
    }
    Safety::Check
}

/// `refs/heads/foo` and `refs/tags/foo` both name `foo`; a pull ref keeps its
/// full form, since it names no branch.
fn strip_ref_prefix(git_ref: &str) -> &str {
    git_ref
        .strip_prefix("refs/heads/")
        .or_else(|| git_ref.strip_prefix("refs/tags/"))
        .unwrap_or(git_ref)
}

/// An asset, judged by how far back its release is.
fn classify_asset(r: &Resource, ctx: &RepoContext) -> Safety {
    let Some(tag) = tag_in_label(&r.label) else {
        return Safety::Check;
    };
    match ctx.release_tags.iter().position(|t| t == tag) {
        Some(0) => Safety::Keep,
        Some(n) if n < SAFE_RELEASE_DEPTH => Safety::Check,
        Some(_) => Safety::Safe,
        // A tag absent from the listing: its release is older than the page we
        // fetched, so it is at least as old as the oldest we know.
        None => Safety::Safe,
    }
}

/// The release tag an asset's label carries, written `… (v1.2.3)`.
fn tag_in_label(label: &str) -> Option<&str> {
    let start = label.rfind('(')? + 1;
    let end = label.rfind(')')?;
    (start < end).then(|| &label[start..end])
}
```

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core safety:: > /tmp/t1-green.txt 2>&1; cat /tmp/t1-green.txt`
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/safety.rs crates/bondebarras-core/src/lib.rs
git commit -m "feat(safety): classification des ressources sur trois niveaux"
```

---

### Task 2: `Resource.safety`, rempli au scan

**Files:** `crates/bondebarras-core/src/model.rs`, `crates/bondebarras-core/src/scan.rs`

**Interfaces:**
- Produces: `Resource.safety: Safety`

`repo_detail` construit déjà tout ce que `RepoContext` demande — branches, PR mergées, branche par défaut, releases. Il suffit de les assembler et de classer avant de rendre la liste.

- [ ] **Step 1: Écrire le test qui échoue**

Dans `scan.rs`, un test wiremock vérifiant qu'un cache rattaché à une PR mergée ressort `Safety::Safe` et qu'un cache sur `main` ressort `Safety::Keep` — **le fixture doit contenir les deux**, sinon il ne distingue pas une implémentation qui marquerait tout.

- [ ] **Step 2: Lancer le test**

Run: `cargo test -p bondebarras-core scan:: > /tmp/t2-red.txt 2>&1; cat /tmp/t2-red.txt`
Expected: FAIL — `no field safety on type Resource`.

- [ ] **Step 3: Implémenter**

Ajouter le champ à `Resource` :

```rust
    /// How safe this is to delete. Filled by `scan`, which is the only place
    /// that holds the whole repository's context at once.
    pub safety: crate::safety::Safety,
```

Chaque site de construction de `Resource` le pose à `Safety::Keep` — le compilateur les nommera tous. Puis, à la fin de `repo_detail`, construire le `RepoContext` depuis les listes déjà en main et reclasser :

```rust
    let ctx = safety::RepoContext {
        merged_refs: closed.merged_refs.clone(),
        live_branches: branches.iter().map(|b| b.name.clone()).collect(),
        default_branch: default_branch.clone(),
        release_tags: release_tags.clone(),
    };
    for item in items.iter_mut() {
        item.safety = safety::classify(item, &ctx);
    }
```

Adapter aux noms réels des variables locales de `repo_detail`.

- [ ] **Step 4 et 5 : relancer, commiter**

`feat(scan): le niveau de sureté est calculé au scan`

**Amendement (2026-09-11, contrôleur).** Le champ `Resource.safety` existe déjà : la Task 1 (`99c9b14`) l'a ajouté et posé à `Safety::Keep` sur chaque site de construction. Cette tâche se réduit au câblage — construire le `RepoContext` à la fin de `repo_detail` et reclasser — et à son test wiremock. Le reclassement vit dans **le seul chemin de code** qui assemble les neuf listes : la Task 6 ajoutera `repo_detail_ticking`, qui devra passer par ce chemin, pas le recopier.

---

### Task 3: Les deux jauges

**Files:** Create `crates/bondebarras-core/src/tui/views/gauges.rs` ; modify `tui/views/mod.rs`

**Interfaces:**
- Produces:
  - `fn cache_gauge_line(used: u64, width: u16) -> Vec<Line<'static>>`
  - `fn minutes_gauge_line(used: u64, is_public: bool, width: u16) -> Vec<Line<'static>>`
  - `const CACHE_CEILING_BYTES: u64 = 10 * 1024 * 1024 * 1024;`

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect()
    }

    #[test]
    fn the_cache_gauge_reports_overshoot_rather_than_capping() {
        // josephine sits at 11.5 GiB against a 10 GB ceiling. Clamping to
        // 100 % would hide the one fact the gauge exists to show: GitHub is
        // already evicting, and it evicts by least-recently-read, so it takes
        // main's caches to make room for closed PRs'.
        let line = text(&cache_gauge_line(12_360_000_000, 60));
        assert!(line.contains("123"), "got: {line}");
        assert!(line.contains("évince"), "got: {line}");
    }

    #[test]
    fn the_cache_gauge_stays_quiet_below_the_ceiling() {
        let line = text(&cache_gauge_line(4_000_000_000, 60));
        assert!(!line.contains("évince"), "got: {line}");
    }

    #[test]
    fn a_public_repo_reads_zero_with_its_reason() {
        // A bare 0 % would read as comfortable headroom. It means this
        // repository cannot consume the allowance at all.
        let line = text(&minutes_gauge_line(0, true, 60));
        assert!(line.contains("public"), "got: {line}");
    }

    #[test]
    fn a_zero_ceiling_does_not_divide_by_zero() {
        let line = text(&cache_gauge_line(0, 60));
        assert!(!line.contains("NaN") && !line.contains("inf"), "got: {line}");
        assert!(line.contains(" 0 %"), "got: {line}");
    }
}
```

- [ ] **Step 2 à 5**

Implémenter sur le modèle de `views/billing.rs`'s `gauge_line`, qui ne plafonne déjà pas. La ligne d'avertissement n'apparaît qu'au-dessus de 100 %. Commit : `feat(tui): jauges de cache et de minutes`.

**Amendements (2026-09-11, contrôleur).**

- `CACHE_CEILING_BYTES` reste `10 * 1024 * 1024 * 1024` : le §2 de la spec montre josephine à **115 %** pour 11,5 Gio. Le test `the_cache_gauge_reports_overshoot_rather_than_capping` attend donc `"115"` pour 12 360 000 000 octets, pas `"123"` — 123 % supposerait un plafond décimal de 10 000 000 000, contraire à la constante imposée.
- Le pourcentage passe par une fonction pure qui prend `(used, ceiling)`. `a_zero_ceiling_does_not_divide_by_zero` l'appelle avec un plafond **nul** : tel qu'écrit plus haut, il passait un usage nul contre un plafond constant et ne pouvait pas échouer sur la propriété qu'il nomme.
- Le plafond de minutes vient de `billing::FREE_MINUTES_PER_MONTH`, sans nouveau littéral : l'issue #11 remplacera cette source unique par le quota de la formule de l'organisation.
- La colonne 3 n'existe qu'à la Task 4 : dessiner les deux lignes en tête du panneau de ressources actuel (`views/repo.rs`) ; la Task 4 les garde en tête de la colonne 3.

---

### Task 4: Trois colonnes et disposition responsive

**Files:** `tui/views/mod.rs`, `tui/views/orgs.rs`, Create `tui/views/repos.rs`

**Interfaces:**
- Produces:
  - `enum Columns { Three, Two, One }`
  - `fn columns_for(width: u16) -> Columns`
  - `views::repos::render(app: &mut App, f: &mut Frame, area: Rect)`

Seuils : `≥ 100` → `Three`, `72..100` → `Two`, `< 72` → `One`.

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
    #[test]
    fn the_layout_degrades_from_the_left() {
        // The resources column is where deletion happens; it is never the one
        // dropped. Context is recalled in the header instead.
        assert_eq!(columns_for(120), Columns::Three);
        assert_eq!(columns_for(100), Columns::Three);
        assert_eq!(columns_for(99), Columns::Two);
        assert_eq!(columns_for(72), Columns::Two);
        assert_eq!(columns_for(71), Columns::One);
        assert_eq!(columns_for(40), Columns::One);
    }
```

Plus un test de rendu **balayant les largeurs de 60 à 200**, rendant le vrai `Rect` de chaque panneau et vérifiant qu'aucune ligne n'est tronquée et que la colonne des ressources est toujours présente :

```rust
    #[test]
    fn the_resources_column_survives_every_width() {
        // Three tests at three sampled sizes is how this project shipped a
        // modal whose prompt vanished at exactly one height per width.
        for width in 60..=200u16 {
            let backend = ratatui::backend::TestBackend::new(width, 30);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            // … render with a fixture carrying one resource …
            let rendered: String = /* buffer content */;
            assert!(
                rendered.contains("RESSOURCES"),
                "resources column missing at width {width}"
            );
        }
    }
```

- [ ] **Step 2 à 5**

`orgs.rs` perd le dépliage — plus de dépôts indentés. `repos.rs` rend la colonne 2 depuis `app.orgs[app.org_cursor].repos`. `views::render` compose selon `columns_for(f.area().width)` et met le contexte manquant dans l'en-tête quand une colonne disparaît.

`←`/`→` changent de colonne, `Tab` reste synonyme cyclique. **Le pied annonce les déplacements** — c'est la correction du défaut d'origine.

**Amendement (2026-09-11, contrôleur) — le pied dépend de la colonne active** (spec §2), au lieu d'une constante unique. Chaque variante annonce `[←/→] colonne` et `[↑/↓] ligne`, puis les actions valables dans cette colonne, et se termine par `[q] quitter` (le test de la Task 6 s'y ancre). Le pied n'annonce **que des touches qui existent** : `[A]` garde son libellé actuel jusqu'à la Task 7, qui introduit `[V]`. Un pied qui promet une touche absente est exactement le défaut que ce plan corrige.

**Amendement 2 (2026-09-11, contrôleur) — largeur de la colonne des dépôts.** La ligne de dépôt de la v0.5 (case 4, nom ≥ 10, âge ou classe jusqu'à ` déjà archivé` 13, taille 8) demande 35 cellules ; `Length(26)` n'en laisse que 24, et `a_repository_row_stays_legible_across_swept_widths` — issu de la revue v0.5, qui a refusé un panneau de 26 amputé de sa taille — échoue à 26. La colonne 2 prend donc la largeur du panneau v0.5 : **`Length(38)`** (`orgs::PANE_WIDTH`). La colonne des ressources garde **`Min(40)` dans tous les modes à plusieurs colonnes**, et les seuils en découlent au lieu d'être posés à la main :

- `Three` si `width >= 22 + 38 + 40` (= 100) ;
- `Two` si `width >= 38 + 40` (= 78) ;
- `One` sinon.

Les tests de bornes deviennent : `columns_for(100) == Three`, `columns_for(99) == Two`, `columns_for(78) == Two`, `columns_for(77) == One`, `columns_for(40) == One`. Un terminal de 80 colonnes garde ses deux colonnes.

En mode deux colonnes, la colonne de gauche suit le focus : orgs + ressources quand le focus est sur les orgs, dépôts + ressources sinon ; l'en-tête rappelle le contexte masqué. `←`/`→`/`Tab` parcourent les trois colonnes dans tous les modes. En mode une colonne, seule la colonne active est dessinée : le balayage de largeurs place le focus sur les ressources, et un test séparé vérifie que c'est bien la colonne active qui est montrée.

Le pied s'adapte à la largeur : `[←/→] colonne`, `[↑/↓] ligne` et `[q] quitter` toujours visibles, les actions de la colonne ajoutées dans un ordre fixe tant qu'elles tiennent. Les libellés de ressources se raccourcissent à la largeur de la colonne pour que taille et drapeau ne soient jamais coupés.

`a_repository_row_stays_legible_across_swept_widths` garde sa propriété (âge ou classe **et** taille visibles) ; seul le calcul de son `Rect` suit la disposition en colonnes.

Commit : `feat(tui): trois colonnes repliables, navigation annoncée`.

---

### Task 5: Chargement après pause, cache, annulation

**Files:** `tui/app.rs`, `tui/mod.rs`

**Interfaces:**
- Produces:
  - `App.repo_cache: HashMap<(String, String), Vec<Resource>>`
  - `App.load_generation: u64`
  - `App.pending_since: Option<Instant>`

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
    #[test]
    fn a_result_that_arrives_after_the_cursor_moved_is_ignored() {
        // Stop on three repositories in a row and the first result must not
        // appear under the third one's name. This is the same shape as the
        // v0.5 defect where a purge resolved against whatever `app` pointed
        // at, rather than against its own identity.
        let mut a = App::new(vec![]);
        let stale = a.begin_load();          // generation 1
        let _current = a.begin_load();       // generation 2
        assert!(!a.accepts_load(stale), "a superseded load must be dropped");
    }

    #[test]
    fn a_cached_repo_is_served_without_a_request() {
        let mut a = App::new(vec![]);
        a.remember(("org".into(), "repo".into()), vec![]);
        assert!(a.cached(("org", "repo")).is_some());
    }

    #[test]
    fn a_purge_invalidates_the_repos_cache() {
        // Otherwise the screen keeps showing what was just deleted.
        let mut a = App::new(vec![]);
        a.remember(("org".into(), "repo".into()), vec![]);
        a.forget(("org", "repo"));
        assert!(a.cached(("org", "repo")).is_none());
    }
```

- [ ] **Step 2 à 5**

Le minuteur : `pending_since` est posé à chaque déplacement dans la colonne 2 ; la boucle d'événements, dont le `poll` bat déjà à 120 ms, déclenche le chargement quand `pending_since.elapsed() >= 300 ms`.

L'annulation par génération : `begin_load` incrémente et rend la génération, la tâche la porte, `accepts_load` compare à l'arrivée. Même forme que l'identité portée par `Progress` en v0.5 — c'est un type, pas une convention.

Pendant le chargement, la colonne affiche `(chargement…)`. Une colonne vide se lirait « ce dépôt n'a rien ».

Commit : `feat(tui): chargement apres pause, cache de depots, annulation`.

---

### Task 6: La barre de progression du pied

**Files:** Create `crates/bondebarras-core/src/tui/views/progress.rs` ; modify `tui/app.rs`, `tui/views/mod.rs`, `tui/mod.rs`, `scan.rs`

Demande utilisateur : « pour les chargements des dépôts et caches, ajoute une barre de
chargement. Et pour la suppression et etc ajoute des barres de progression également, tu n'as
qu'à ajouter cela dans une barre en footer de la fenêtre. »

**Une ligne à elle, qui n'existe que pendant un travail.** La disposition passe de quatre
rangées à cinq : en-tête, corps, ligne d'état, **progression**, pied.

```
Constraint::Length(1)                                    en-tête
Constraint::Min(1)                                       corps
Constraint::Length(1)                                    ligne d'état
Constraint::Length(if app.progress.is_some() {1} else {0})   progression
Constraint::Length(1)                                    pied
```

Elle ne remplace **ni** le pied **ni** la ligne d'état, et c'est le point important :

- Le pied porte les déplacements. Les lui prendre pendant une purge rejouerait le défaut du
  §1 — l'utilisateur qui ne voit plus comment naviguer — au pire moment.
- La ligne d'état porte les erreurs (`Erreur : suppression de 9 — 404`). Une barre qui les
  recouvre ferait disparaître l'échec derrière la progression de l'échec.

À `Length(0)` ratatui ne dessine rien : la rangée n'existe pas quand rien ne tourne.

**Interfaces :**

```rust
/// A unit of work in flight, and how far along it is.
///
/// Both denominators are counted, never estimated: a purge knows its item
/// count from its own `Plan`, and a repository load knows it makes exactly
/// nine calls. This project has twice published a percentage its data could
/// not support — the "818 %" of v0.2 and the "890 Mo" of v0.3 — and a bar
/// that animates without measuring anything would be the same lie in a
/// prettier shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    pub label: String,
    pub done: usize,
    pub total: usize,
}

impl Work {
    /// Ratio in 0..=1. `total == 0` yields 0, never a division by zero.
    pub fn ratio(&self) -> f64 { … }
}
```

`App` gagne **deux** créneaux, pas un :

```rust
    /// The purge or archive in flight, if any.
    pub purge: Option<Work>,
    /// The repository drill-down in flight, if any.
    pub loading: Option<Work>,
```

Deux, parce que les deux se chevauchent réellement : le curseur reste libre pendant une
purge, donc un chargement peut partir alors qu'une suppression court. Un créneau unique
ferait écraser l'un par l'autre, et la barre annoncerait la fin d'un travail qui tourne
encore. **La purge l'emporte à l'affichage** — c'est celle qui détruit des données.

`fn shown(app: &App) -> Option<&Work>` rend `purge` d'abord, `loading` sinon.

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_plan_does_not_divide_by_zero() {
        // A plan can be empty — `[A]` on a repository whose every resource
        // is protected selects nothing. 0/0 must be 0, not NaN, and NaN
        // reaches ratatui's Gauge as a panic in debug.
        let w = Work { label: "purge".into(), done: 0, total: 0 };
        assert_eq!(w.ratio(), 0.0);
    }

    #[test]
    fn a_finished_work_is_exactly_one() {
        let w = Work { label: "purge".into(), done: 9, total: 9 };
        assert_eq!(w.ratio(), 1.0);
    }

    #[test]
    fn a_purge_outranks_a_load_that_started_under_it() {
        // The cursor stays live during a purge, so a drill-down can start
        // while deletions are still landing. With one slot the load would
        // overwrite the purge and the bar would report the wrong work as
        // finished. The fixture needs BOTH present, or it proves nothing.
        let mut app = App::default();
        app.purge = Some(Work { label: "suppression".into(), done: 2, total: 7 });
        app.loading = Some(Work { label: "chargement".into(), done: 8, total: 9 });
        let shown = shown(&app).unwrap();
        assert_eq!(shown.label, "suppression");
        assert_eq!(shown.done, 2);
    }

    #[test]
    fn a_load_shows_when_no_purge_runs() {
        let mut app = App::default();
        app.loading = Some(Work { label: "chargement".into(), done: 3, total: 9 });
        assert_eq!(shown(&app).unwrap().label, "chargement");
    }

    #[test]
    fn nothing_running_draws_no_row() {
        assert!(shown(&App::default()).is_none());
    }

    /// The row must appear and disappear, and taking it must never cost the
    /// footer its keys — that footer is what this whole plan exists to fix.
    #[test]
    fn the_progress_row_never_costs_the_footer_its_keys() {
        for height in 6u16..=40 {
            for width in [72u16, 100, 140] {
                let mut app = App::default();
                app.purge = Some(Work { label: "suppression".into(), done: 1, total: 4 });
                let buf = render_to_backend(&mut app, width, height);
                let text = buffer_text(&buf);
                assert!(
                    text.contains("[q] quitter"),
                    "footer lost its keys at {width}x{height} with the bar shown"
                );
            }
        }
    }
}
```

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core progress:: > /tmp/t6-red.txt 2>&1; cat /tmp/t6-red.txt`
Expected: FAIL — `cannot find type Work`.

⚠️ Le filtre `progress` est une **sous-chaîne littérale** : il attrapera aussi les tests
existants qui nomment `Progress` dans `clean.rs`. `progress::` restreint au module neuf.

- [ ] **Step 3: Alimenter les deux compteurs**

**La purge.** Tout est déjà là. `clean::execute` envoie un `Progress::Done` ou
`Progress::Failed` par élément, et la boucle de `tui/mod.rs` les draine déjà. Poser
`app.purge = Some(Work { total: plan.items.len(), done: 0, … })` au lancement, incrémenter
`done` sur chaque `Done` **et** chaque `Failed` — un échec est un élément traité, pas un
élément en attente — et remettre à `None` sur `Finished`, qui écrit déjà son récapitulatif
dans `app.status`.

⚠️ `total: 0` ne doit pas poser de barre du tout : une purge vide se termine avant d'être
dessinée, et une barre à 0 % qui disparaît aussitôt est un clignotement, pas une information.

**Le chargement.** Les neuf appels de `repo_detail_with_warnings` partent dans un
`futures::join!` et ne rendent la main qu'ensemble : sans changement, il n'y a rien à compter
entre 0 et 9. Ajouter un canal de tics :

```rust
/// `repo_detail_with_warnings`, plus one tick per completed call.
///
/// The nine listings are joined, so without this the caller sees nothing
/// between "started" and "all nine done" — a bar over that has two states
/// and is worth less than the `(chargement…)` text it would replace.
/// Each future sends its tick as it lands; ticks arrive in completion
/// order, which is the order the user is actually waiting on.
pub async fn repo_detail_ticking(
    client: &Client,
    owner: &str,
    repo: &str,
    tick: UnboundedSender<()>,
) -> Result<(Vec<Resource>, Vec<&'static str>)>
```

Chaque future est enveloppée dans un `async { let r = fut.await; let _ = tick.send(()); r }`.
Le `let _` est délibéré : un canal fermé signifie que l'utilisateur a quitté l'écran, et une
erreur d'envoi de tic ne doit pas faire échouer un chargement qui, lui, a réussi.

`TOTAL_CALLS = 9`, en constante nommée à côté du `join!`, avec un commentaire disant qu'elle
doit suivre le nombre de futures. Un test l'ancre : compter les tics reçus sur un
`repo_detail_ticking` complet et vérifier qu'il en arrive exactement `TOTAL_CALLS`. Sans lui,
ajouter une dixième famille laisserait la barre plafonner à 90 %.

⚠️ **L'annulation par génération de la Task 5 s'applique aussi aux tics.** Un chargement
abandonné continue d'émettre : ses tics doivent être ignorés comme l'est son résultat, sinon
la barre du dépôt regardé avance au rythme de celui qu'on a quitté. Faire porter la
génération au créneau `loading` et la comparer à chaque tic.

- [ ] **Step 4: Dessiner**

`ratatui::widgets::Gauge`, `ratatui_unicode` non requis. Étiquette à gauche, ratio à droite :

```
 suppression  ████████████░░░░░░░░░░░░░░  4/11
```

Le libellé est **user-facing, donc en français** : `suppression`, `archivage`, `chargement`.
Le compte `done/total` est écrit en clair à côté de la barre — un pourcentage seul ne dit pas
s'il reste deux éléments ou deux cents.

Couleurs : `theme::` existant, comme les jauges de la Task 3. Ne pas introduire de palette.

- [ ] **Step 5: Vérifier et commiter**

`cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

Commit : `feat(tui): barre de progression en pied pour les purges et les chargements`.

---

### Task 7: Marqueurs ⛑ / • et sélection par niveau

Ajoutée le 2026-09-11 : la spec §4.2 et §7 exigent que `[A]` coche les ⛑ et qu'une seconde touche ajoute les •, et le §2 montre les marqueurs dans les lignes ; aucune tâche ne les portait.

**Files:** `crates/bondebarras-core/src/tui/app.rs`, `tui/mod.rs`, le module qui rend la colonne des ressources après la Task 4, `tui/views/mod.rs` (pied)

**Interfaces:**
- Consumes: `Resource.safety` (Task 2) ; pied par colonne (Task 4)
- Produces: `App::select_safe()` derrière `[A]` (remplace `select_all_stale`), `App::select_safe_and_check()` derrière `[V]`

**Touches.** Dans un terminal, `KeyCode::Char('A')` *est* Maj+a : la « Maj+A » de la spec n'est pas une frappe distincte de `[A]`. `[A]` garde sa touche et ne prend que les ⛑ (les caches ⚑ en font partie : `stale_pr` rend `Safe`). « + à vérifier » passe sur `[V]` — vérifier dans `tui/mod.rs` que `V` est libre, sinon s'arrêter et le signaler.

**Règles inchangées.** Un `Repository` n'est jamais pris, ni par `[A]` ni par `[V]`. Une ressource `protected` n'est jamais prise en masse, quel que soit son niveau. Les lignes masquées par le filtre ne sont pas prises (la propriété de `select_all_stale_does_not_select_rows_hidden_by_the_filter` reste vraie sous le nouveau nom). La sélection individuelle (`espace`) n'est jamais bornée par `Safety`. La CLI headless n'est pas touchée.

**Marqueurs.** Dans chaque ligne de ressource : `⛑` pour `Safe`, `•` pour `Check`, une espace pour `Keep`, dans une colonne de largeur fixe pour que les libellés restent alignés.

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
    #[test]
    fn select_safe_takes_only_the_safe_rows() {
        // One row of each level: a key that also took Check would pass a
        // fixture holding only Safe rows.
        let mut a = app_with_levels(&[Safety::Safe, Safety::Check, Safety::Keep]);
        a.select_safe();
        assert_eq!(selected_levels(&a), vec![Safety::Safe]);
    }

    #[test]
    fn select_safe_and_check_adds_check_and_nothing_else() {
        let mut a = app_with_levels(&[Safety::Safe, Safety::Check, Safety::Keep]);
        a.select_safe_and_check();
        assert_eq!(selected_levels(&a), vec![Safety::Safe, Safety::Check]);
    }

    #[test]
    fn a_protected_row_is_never_taken_in_bulk_even_if_marked_safe() {
        // classify never returns Safe for a protected row today; this pins the
        // selection's own guard, so a future classifier bug cannot reach a
        // bulk delete.
        let mut a = app_with_levels(&[Safety::Safe]);
        protect_all_resources(&mut a);
        a.select_safe_and_check();
        assert!(selected_levels(&a).is_empty());
    }
```

`app_with_levels`, `selected_levels` et `protect_all_resources` sont des helpers de test à écrire dans le même module, sur le modèle des fixtures de `select_all_stale_takes_only_flagged_items`. Ajouter un test de rendu qui trouve `⛑` et `•` dans le `Rect` réel de la colonne des ressources, et un test que le pied de cette colonne annonce `[V]`.

- [ ] **Step 2: Lancer les tests**

Run: `cargo test -p bondebarras-core select_safe > /tmp/t7-red.txt 2>&1; cat /tmp/t7-red.txt`
Expected: FAIL — `no method named select_safe`.

- [ ] **Step 3: Implémenter** — renommer `select_all_stale` en `select_safe` (critère : `safety == Safety::Safe`), ajouter `select_safe_and_check`, brancher `[V]`, dessiner les marqueurs, annoncer `[A] sûrs  [V] +à vérifier` dans le pied de la colonne des ressources.

- [ ] **Step 4 et 5 : relancer, commiter** — `feat(tui): marqueurs de surete, [A] surs et [V] a verifier`.

**Amendement (2026-09-11, contrôleur) — ce que la Task 4 laisse ouvert.**

- **Ligne compacte.** Dans la colonne des ressources, les champs fixes d'une ligne (case, genre, taille, âge, drapeau) prennent aujourd'hui 33 cellules : à 80 colonnes (colonne des ressources de 42), il reste 7 caractères de libellé, et le marqueur de cette tâche en prendrait 2 de plus. Compacter ces champs en ajoutant le marqueur — le schéma du §2 de la spec montre des tailles courtes (`467 M`) et pas d'âge à côté d'un drapeau — de sorte qu'**à 80 colonnes chaque ligne garde au moins 12 caractères de libellé**, et qu'**une version de paquet montre son suffixe de classe complet**, `(sans tag)` comme `(attestation orpheline)`. Si la classe la plus longue ne peut pas tenir, s'arrêter et le signaler plutôt que de raccourcir un libellé de classe.
- **Test.** `a_package_row_survives_at_eighty_columns` (nom conservé : `scan.rs` le cite) redevient une garantie à 80 colonnes : il affirme `(sans tag)` **et** `(attestation orpheline)` dans le `Rect` réel de la colonne des ressources à une largeur de terminal de 80, en plus du balayage 60..=200. Un test de balayage vérifie les 12 caractères de libellé minimum à 80 colonnes et au-delà.
- **Touches.** Depuis `f6ab57d`, `espace`, `A`, `f` et `s` n'agissent que dans la colonne active. `[A]` et `[V]` suivent la même règle : ils n'agissent que lorsque le focus est sur la colonne des ressources, et le pied de cette colonne seule annonce `[A] sûrs  [V] +à vérifier`.
- **`d`.** `d` n'agit que depuis la colonne qui possède son plan : la colonne des ressources supprime les ressources cochées, la colonne des dépôts archive les dépôts cochés, la colonne des orgs ne fait rien et son pied n'annonce pas `[d]`. Sans cela, en mode une colonne avec le focus sur les orgs, `d` construit un plan à partir de ressources cochées plus tôt et désormais hors écran, et la confirmation de Tier 1 ne les liste pas. Test : mode une colonne, focus sur les orgs, ressources cochées auparavant — `d` n'ouvre aucune confirmation et ne prend aucun plan ; le pied de la colonne des orgs ne contient pas `[d]` à aucune largeur.

---

### Task 8: Documentation

Ajoutée le 2026-09-11 : aucune tâche ne mettait à jour la documentation, alors que la disposition, les touches et le sens de `[A]` changent.

**Files:** `README.md`, `CHANGELOG.md`, `CLAUDE.md`

- [ ] **Step 1:** `README.md` — l'usage du TUI décrit les trois colonnes et leur repli (≥ 100 / 72–99 / < 72), les touches `←`/`→`/`Tab`/`↑`/`↓`/`Entrée`, les marqueurs ⛑ / •, `[A]` (sûrs) et `[V]` (+ à vérifier), les deux jauges et la barre de progression. Aucune capture inventée : décrire, ou reprendre le schéma du §2 de la spec.
- [ ] **Step 2:** `CHANGELOG.md` — section `## [Unreleased]` au format Keep a Changelog : *Added* (trois colonnes, marquage de sûreté, jauges, chargement après pause, barre de progression, `[V]`), *Changed* (`[A]` prend les ⛑ au lieu des ⚑).
- [ ] **Step 3:** `CLAUDE.md` — table « Where to change what » : `safety.rs`, `tui/views/gauges.rs`, `tui/views/repos.rs`, `tui/views/progress.rs` ; la ligne du panneau gauche devient la colonne 1 ; la règle produit qui cite `select_all_stale` (`[A]`) suit le nouveau nom.
- [ ] **Step 3 bis:** les noms rendus faux par la Task 4 — `CLAUDE.md` cite `orgs::repo_row_spans` (désormais `repos::repo_row_spans`) et « Left pane » / « Right pane » (désormais les colonnes 1 à 3) ; quelques commentaires d'`app.rs` et un nom de test disent encore qu'un dépôt « vit dans l'arbre » ; le doc comment de `scan.rs` qui cite `a_package_row_survives_at_eighty_columns` doit rester vrai après la Task 7.
- [ ] **Step 3 ter:** la table « Where to change what » de `CLAUDE.md` gagne `views/repos.rs` (colonne 2) et `views/shown.rs` (Task 5 : quel dépôt la colonne des ressources montre, et son `(chargement…)`) ; la règle produit sur les versions de paquet cite `column_head` au lieu de `list_title`, qui ne décide plus de l'explication des tailles inconnues.
- [ ] **Step 4:** `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo build --release`, puis commit `docs: trois colonnes, marquage de surete et nouvelles touches`.

## Self-Review

**Couverture de la spec**

| Exigence | Tâche |
|---|---|
| §1 le pied annonce les déplacements | 4 |
| §2 trois colonnes, `Focus` = colonnes, `←`/`→` | 4 |
| §2.1 repli par la gauche, seuils 100 / 72 | 4 |
| §3 pause 300 ms, cache, annulation, `(chargement…)` | 5 |
| §3 invalidation après purge | 5 |
| §4 les trois niveaux, table par famille | 1 |
| §4.1 branche disparue et PR mergée sans coût | 1, 2 |
| §4.3 `Safety` ≠ `protected` | 1 |
| §5 les deux jauges, plafond codé en dur annoncé | 3 |
| §7 tests, dont le balayage de largeurs | 1-6 |
| barre de progression (demande hors spec initiale) | 6 |
| §4.2 `[A]` ⛑ seuls, seconde touche + •, marqueurs dans les lignes | 7 |
| documentation (README, CHANGELOG, CLAUDE.md) | 8 |

**Cohérence des types**

- `Safety` : défini en 1, champ en 2, lu en 4.
- `RepoContext` : construit en 2 depuis les listes de `repo_detail`, consommé en 1.
- `Work` : défini en 6, alimenté depuis `Progress` (v0.5, inchangé) et depuis les tics de
  `repo_detail_ticking` (6). La génération de la Task 5 le borne — d'où l'ordre 5 puis 6.
- `Columns` / `columns_for` : 4 seulement.
- `begin_load` / `accepts_load` / `remember` / `cached` / `forget` : 5 seulement.

**Le test qui compte** est `an_asset_two_releases_back_is_safe_and_the_previous_one_is_not`. Sans les trois cas dans le même fixture, une implémentation à deux niveaux passerait, et l'outil proposerait soit de supprimer la release qu'on utilise encore, soit rien du tout.
