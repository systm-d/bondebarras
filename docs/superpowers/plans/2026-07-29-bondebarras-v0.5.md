# bondebarras v0.5 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Archiver les dépôts morts — non pour libérer des octets, mais parce qu'un dépôt archivé a ses Actions désactivées et cesse donc de produire les caches, artifacts et runs que la v0.1 existe pour nettoyer. Puis solder quatre dettes de la v0.4.

**Architecture:** Le dépôt est le seul « ressource » qui vit dans l'arbre de gauche plutôt que dans la liste de droite. Aucune auto-sélection, jamais, et refus total en headless.

## Global Constraints

- Rust **edition 2024**, MSRV **1.88**. `unsafe_code = "forbid"` ; clippy `all = warn`, CI en `-D warnings` ; rustfmt `max_width = 100`.
- **Doc comments en anglais. Chaînes user-facing en français**, accents inclus.
- Jamais `ERROR`/`FATAL`/`PANIC` en user-facing. `Erreur : ` ajouté **une seule fois**, par `run()`.
- **Routes API avec slash initial**, et **noms de refs percent-encodés** (cf. v0.4).
- `cargo test <filtre>` prend une **sous-chaîne littérale**.
- Conventional Commits. Multi-plateforme.
- **Preuve TDD : sortie brute, redirigée vers un fichier puis relue et collée telle quelle.**
- **Tests qui ne peuvent pas échouer :** huit tests de ce projet ont nommé la bonne propriété sans pouvoir échouer dessus. Construire chaque fixture pour que l'implémentation correcte et la fautive divergent, puis le prouver contre le défaut.
- **Tests de rendu : balayer, ne pas échantillonner.** Le défaut de modale de la v0.3 se reproduisait à une hauteur précise par largeur ; trois tailles échantillonnées l'ont manqué.

## Ce qui existe (v0.1-v0.4, fusionnées) — 205 tests

Sept `ResourceKind`, `RiskTier` à trois paliers dont le 3 reste sans ressource, `Resource.protected` comme grille de sélection en masse, un garde de sélection individuelle sur `Branch` pour `Default`/`Protected`, `has_known_size`, la modale palier 2 avec pied inclippable, la CLI headless à six drapeaux de famille.

---

### Task 1: Classification des dépôts et dettes de la v0.4

**Files:** Create `crates/bondebarras-core/src/repos.rs` ; modify `model.rs`, `tui/views/repo.rs`, `scan.rs`, `commands/clean.rs`

**Interfaces:**
- `enum RepoClass { Archivable, AlreadyArchived, NoAdminRights }`
- `fn classify_repo(archived: bool, admin: bool) -> RepoClass`
- `ResourceKind::Repository`, en `RiskTier::Medium`

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untouched_repo_the_user_administers_is_archivable() {
        assert_eq!(classify_repo(false, true), RepoClass::Archivable);
    }

    #[test]
    fn an_already_archived_repo_is_not_a_candidate() {
        assert_eq!(classify_repo(true, true), RepoClass::AlreadyArchived);
    }

    #[test]
    fn a_repo_without_admin_rights_is_not_a_candidate() {
        // The API would answer 403. Offering an action it will refuse is a
        // lie the API then contradicts, in front of the user.
        assert_eq!(classify_repo(false, false), RepoClass::NoAdminRights);
    }

    #[test]
    fn already_archived_outranks_missing_rights() {
        // Both disqualify; the message should say the useful one.
        assert_eq!(classify_repo(true, false), RepoClass::AlreadyArchived);
    }
}
```

- [ ] **Step 2: Lancer les tests**

Déclarer `pub mod repos;` dans `lib.rs` **avant** ce run.

Run: `cargo test -p bondebarras-core repos:: > /tmp/v5t1-red.txt 2>&1; cat /tmp/v5t1-red.txt`
Expected: FAIL — `cannot find function classify_repo`.

⚠️ `repos` est déjà un nom de module sous `api/`. Le filtre littéral `repos` attrapera les deux — utiliser `repos::` ou un nom de test distinctif.

- [ ] **Step 3: Implémenter, puis solder les quatre dettes de la v0.4**

`repos.rs` : la classification, avec des doc comments expliquant *pourquoi* chaque cas
disqualifie.

`ResourceKind::Repository` + son palier. Le `match` exhaustif de `risk_tier` refusera de
compiler sans — c'est le garde-fou. Les autres `match` cassés se remplissent en Task 3 ;
bras **honnêtes** en attendant, comme en v0.3 et v0.4.

Puis les quatre dettes, relevées par la revue finale de la v0.4 :

1. **`has_known_size` devient un `match` exhaustif**, et son test sur `ResourceKind::ALL`
   compare à une **table écrite en dur**, pas à une recopie de l'implémentation. Aujourd'hui
   les deux côtés recalculent la même expression, donc une famille sans taille ajoutée plus
   tard passerait à `true` sans échec. `Repository` en est justement une — écrire d'abord la
   table, voir le test échouer, puis corriger le prédicat.
2. **`list_title` dans `tui/views/repo.rs`** teste encore `kind == PackageVersion` pour son
   avertissement de taille. Le passer à `has_known_size`.
3. **Si la récupération de la branche par défaut échoue**, `default_branch` retombe à `""` et
   `main` se classe `Live` : le garde de sélection cesse de la couvrir précisément quand la
   ligne est la plus dangereuse. Faire échouer la classification vers `Protected` plutôt que
   `Live` quand le nom de la branche par défaut est inconnu — se tromper vers plus de
   prudence.
4. **Un listing refusé se lit « rien à supprimer » en headless**, sans signal, depuis que les
   sept familles dégradent uniformément. Faire remonter un avertissement sur stderr nommant
   la famille dont le listing a échoué. Ne pas changer la dégradation elle-même : elle est
   voulue.

- [ ] **Step 4 et 5 : relancer, commiter**

Deux commits : `feat(repos): classification des depots archivables` puis
`fix: soldes des dettes de la v0.4`.

---

### Task 2: L'endpoint d'archivage

**Files:** Create `crates/bondebarras-core/src/api/archive.rs`

**Interface:** `async fn archive(client: &Client, owner: &str, repo: &str) -> Result<()>`

Route : `PATCH /repos/{owner}/{repo}` avec le corps `{"archived": true}`.

`Client` n'a pas de primitive `PATCH` — il expose `get_json`, `get_json_or_missing` et
`delete`. En ajouter une, sur le modèle de `delete` : passer par `octocrab`'s `_patch` pour
garder l'accès au statut, et vérifier `is_success` plutôt que de désérialiser.

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
    #[tokio::test]
    async fn archive_sends_the_archived_flag() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/repos/maxds-lyon/lokiprint"))
            .and(body_json(serde_json::json!({ "archived": true })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        archive(&client, "maxds-lyon", "lokiprint").await.unwrap();
    }

    #[tokio::test]
    async fn a_403_is_an_error_not_a_silent_success() {
        // The user is not an admin. Reporting success would tell them a repo
        // is read-only when it is not.
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/repos/maxds-lyon/lokiprint"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let err = archive(&client, "maxds-lyon", "lokiprint").await.unwrap_err();
        assert!(err.to_string().contains("403"));
    }
```

Le `body_json` matcher est ce qui rend le premier test non tautologique : sans lui, un
`PATCH` au corps vide passerait.

- [ ] **Step 2 à 5**

Implémenter, vérifier, commiter : `feat(api): archivage d'un depot`.

---

### Task 3: L'arbre, le garde et le refus headless

**Files:** `scan.rs`, `tui/app.rs`, `tui/views/orgs.rs`, `tui/mod.rs`, `clean.rs`, `commands/clean.rs`

Le dépôt est le seul candidat qui vit dans **l'arbre de gauche**, pas dans la liste de
droite. `RepoSummary` porte sa classe et son âge ; la ligne du dépôt les affiche.

**Trois règles, dans le type autant que possible :**

- **Aucune auto-sélection, jamais.** `[A]` ne prend aucun dépôt, quel que soit son âge.
  `pushed_at` n'est pas une preuve d'abandon : une bibliothèque finie ne bouge pas pendant
  deux ans sans être morte.
- **Un dépôt déjà archivé ou sans droits d'admin n'est pas cochable.**
- **Le headless refuse l'archivage entièrement** — pas de drapeau, pas de contournement.
  `select()` ne doit jamais rendre un `Repository`. C'est la première fois que le produit
  refuse une famille entière en headless ; le dire dans un commentaire.

`clean::execute` gagne le bras `Repository` appelant `api::archive::archive`.

- [ ] **Step 1: Écrire les tests qui échouent**

Au minimum :

```rust
    #[test]
    fn bulk_selection_never_takes_a_repository() {
        // Without this, a future extension of [A] would flip a whole
        // organisation to read-only on one keystroke.
        // The fixture must contain an archivable repo of every age, or it
        // proves nothing.
    }

    #[test]
    fn headless_select_never_returns_a_repository() {
        // Archiving turns a whole repository read-only. That is not a cron
        // decision. The fixture needs a Repository resource present, or the
        // arm is never exercised — the mistake made twice already on this
        // project, in v0.3 and v0.4.
    }
```

- [ ] **Step 2 à 5**

Implémenter, vérifier, commiter : `feat(tui): archivage depuis l'arbre, jamais en masse`.

---

### Task 4: Rendu, CLI et documentation

**Files:** `tui/views/orgs.rs`, `tui/views/confirm.rs`, `README.md`, `CHANGELOG.md`, `CLAUDE.md`, `site/content/_index*.md`

La ligne d'un dépôt affiche son âge et sa classe : `775 j`, `déjà archivé`, `sans droits`.
La modale palier 2 nomme chaque dépôt — un récapitulatif qui dirait « 3 dépôts » sans les
nommer serait insuffisant pour une bascule en lecture seule.

**Un test de rendu `TestBackend` balayant les largeurs** sur la ligne de dépôt.

Documentation : dire que l'archivage **ne libère aucun octet**, et pourquoi il est là malgré
tout — un dépôt archivé a ses Actions désactivées, donc il cesse de produire des caches, des
artifacts et des runs. Dire aussi qu'il est **réversible**, et que la suppression de dépôts
reste hors périmètre définitivement.

Citer le gisement réel : sur cinq organisations, une douzaine de dépôts sans push depuis 500
à 775 jours, et **un seul déjà archivé**.

Bumper la version à `0.5.0` — **les deux littéraux**, la racine et la dépendance de chemin
dans `crates/bondebarras/Cargo.toml`, celle-ci étant load-bearing pour que le workspace
résolve.

## Self-Review

| Exigence spec | Tâche |
|---|---|
| §1 ne libère rien, désactive les Actions | 4 (doc) |
| §3 `pushed_at` seul signal, jamais d'auto-sélection | 3 |
| §4 déjà archivé / sans droits non candidats | 1, 3 |
| §4 refus headless total | 3 |
| §5 le dépôt vit dans l'arbre | 3 |
| §6 quatre dettes de la v0.4 | 1 |
| §7 tests | chaque tâche |

**Le test qui compte** est `bulk_selection_never_takes_a_repository`. Sans lui, une
extension future de `[A]` basculerait une organisation entière en lecture seule sur une
frappe.
