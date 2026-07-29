# bondebarras v0.3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Nettoyer les versions de packages GHCR — couches sans tag et attestations orphelines — en assumant que GitHub n'en expose pas la taille.

**Architecture:** `packages.rs` classe les versions en calcul pur ; `api/packages.rs` les récupère et les supprime ; `ResourceKind` gagne une variante qui force l'assignation d'un palier ; le palier 2 obtient enfin sa modale.

**Tech Stack:** Inchangé. Tests : wiremock, assert_cmd, predicates.

## Global Constraints

- Rust **edition 2024**, MSRV **1.88**. `unsafe_code = "forbid"` ; clippy `all = warn`, CI en `-D warnings` ; rustfmt `max_width = 100`.
- **Doc comments en anglais. Chaînes user-facing en français**, accents inclus.
- Jamais `ERROR`/`FATAL`/`PANIC` en user-facing. `Erreur : ` est ajouté **une seule fois**, par `run()`.
- **Routes API avec slash initial.** Sans lui la requête part au mauvais hôte et les tests wiremock passent quand même.
- `cargo test <filtre>` prend une **sous-chaîne littérale**, pas une regex.
- `sort_by(|a,b| b.x.cmp(&a.x))` échoue sur `clippy::unnecessary_sort_by` → `sort_by_key(|r| std::cmp::Reverse(r.x))`.
- Conventional Commits. Multi-plateforme.
- **Preuve TDD : sortie terminale brute, redirigée vers un fichier puis collée depuis ce fichier.** Les relecteurs vérifient que le fichier existe et correspond. Trois rapports de ce projet ont présenté du texte reconstitué comme une capture littérale.
- **Tests qui ne peuvent pas échouer :** sept tests de ce projet ont nommé la bonne propriété sans pouvoir échouer dessus. Pour chaque test, se demander ce que renverrait une implémentation *fautive*, et construire le fixture pour que les deux divergent — puis le prouver en lançant contre le défaut.

## Le fait qui cadre toute la version

**L'API n'expose aucune taille pour une version de package.** Vérifié le 2026-07-29 : le
payload complet ne contient aucun champ de taille, et le relevé de facturation ne porte
aucun SKU de stockage de packages. `Resource.size_bytes` vaut donc **0**, comme pour un
workflow run, et l'interface doit le dire explicitement — une colonne de tirets dans un
outil qui affiche des octets partout ailleurs se lit sinon comme « vide ».

Gisement réel sur les 15 orgs : 7 packages, 45 versions, dont 23 sans tag.

## Structure des fichiers

| Fichier | Responsabilité |
|---|---|
| `crates/bondebarras-core/src/packages.rs` | classification pure : sans tag, attestation orpheline, taguée |
| `crates/bondebarras-core/src/api/packages.rs` | liste des packages, versions, suppression |
| `crates/bondebarras-core/src/model.rs` | `ResourceKind::PackageVersion`, palier 2 |
| `crates/bondebarras-core/src/scan.rs` | les versions rejoignent l'étage 2 |
| `crates/bondebarras-core/src/tui/views/{confirm,repo}.rs` | modale palier 2, ligne d'avertissement |
| `crates/bondebarras-core/src/{cli,commands/clean}.rs` | drapeau `--packages` |

---

### Task 1: Classification des versions (calcul pur)

**Files:**
- Create: `crates/bondebarras-core/src/packages.rs`
- Modify: `crates/bondebarras-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `struct PackageVersion { id: u64, digest: String, tags: Vec<String>, age_days: i64 }`
  - `enum VersionClass { Untagged, OrphanedAttestation, Tagged }`
  - `fn attested_digest(tag: &str) -> Option<String>`
  - `fn classify(versions: &[PackageVersion]) -> Vec<(u64, VersionClass)>`

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn v(id: u64, digest: &str, tags: &[&str]) -> PackageVersion {
        PackageVersion {
            id,
            digest: digest.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            age_days: 30,
        }
    }

    /// Real digests captured from `systm-d/repolens` on 2026-07-29.
    const IMG: &str = "sha256:1a65eb30f0e36fc41bb07724b11e53ada5e810382f39143698b00c470f019b80";
    const ATT_TAG: &str = "sha256-1a65eb30f0e36fc41bb07724b11e53ada5e810382f39143698b00c470f019b80";
    const OTHER: &str = "sha256:9a26c70801010123223adb5e73ff703aca86c15e19b30124ede5628a1e185826";

    #[test]
    fn an_attestation_tag_yields_the_digest_it_signs() {
        assert_eq!(attested_digest(ATT_TAG).as_deref(), Some(IMG));
    }

    #[test]
    fn ordinary_and_malformed_tags_are_not_attestations() {
        // Only 64 lowercase hex after the prefix counts. Anything else is a
        // tag someone chose, and deleting it would delete a real image.
        assert_eq!(attested_digest("latest"), None);
        assert_eq!(attested_digest("2.0.2"), None);
        assert_eq!(attested_digest("sha256-"), None);
        assert_eq!(attested_digest("sha256-zzz"), None);
        assert_eq!(attested_digest("sha256-1a65eb30"), None); // too short
        assert_eq!(attested_digest("sha256-1A65EB30F0E36FC41BB07724B11E53ADA5E810382F39143698B00C470F019B80"), None);
    }

    #[test]
    fn an_attestation_whose_subject_survives_is_not_orphaned() {
        // THE test of this task. Flagging a live image's signature would
        // offer to delete the proof that a deployed image is authentic.
        let versions = vec![
            v(1, "sha256:1d7018e5", &[ATT_TAG]),
            v(2, IMG, &["latest", "2.0.2"]),
        ];
        let classes = classify(&versions);
        assert_eq!(classes, vec![(1, VersionClass::Tagged), (2, VersionClass::Tagged)]);
    }

    #[test]
    fn an_attestation_whose_subject_is_gone_is_orphaned() {
        let versions = vec![
            v(1, "sha256:1d7018e5", &[ATT_TAG]),
            v(2, OTHER, &["latest"]),
        ];
        let classes = classify(&versions);
        assert_eq!(
            classes,
            vec![(1, VersionClass::OrphanedAttestation), (2, VersionClass::Tagged)]
        );
    }

    #[test]
    fn a_version_with_no_tag_is_untagged() {
        let classes = classify(&[v(1, OTHER, &[])]);
        assert_eq!(classes, vec![(1, VersionClass::Untagged)]);
    }
}
```

- [ ] **Step 2: Lancer les tests pour vérifier qu'ils échouent**

Déclarer `pub mod packages;` dans `lib.rs` **avant** ce run, sinon `cargo test` rapporte « 0 tests » au lieu d'une erreur de compilation — ce qui ne prouve rien.

Run: `cargo test -p bondebarras-core packages > /tmp/v3t1-red.txt 2>&1; cat /tmp/v3t1-red.txt`
Expected: FAIL — `cannot find type PackageVersion`.

- [ ] **Step 3: Écrire l'implémentation**

```rust
//! Classification of container package versions.
//!
//! GitHub exposes no size for a package version — not in the versions API, not
//! as a billing SKU. So unlike caches and artifacts, this family cannot be
//! ranked or justified by bytes. What it can be ranked by is *deadness*: a
//! layer no tag points at, and a signature whose image is gone.

/// One version of a container package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageVersion {
    pub id: u64,
    /// The version's own digest, in `sha256:<64 hex>` form.
    pub digest: String,
    pub tags: Vec<String>,
    pub age_days: i64,
}

/// What a version is, from safest to riskiest to delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionClass {
    /// No tag points at it. The bulk of the waste.
    Untagged,
    /// A signature or attestation whose subject image no longer exists.
    OrphanedAttestation,
    /// At least one real tag. Never preselected — deleting `latest` breaks
    /// deployments.
    Tagged,
}

/// The digest an attestation tag signs, if the tag is one.
///
/// Cosign and GitHub attach attestations by tagging them
/// `sha256-<digest of the signed image>`. The separator is the only difference
/// from a digest: `-` in the tag, `:` in the `name`.
///
/// The 64-lowercase-hex check is not pedantry. A tag someone chose that merely
/// starts with `sha256-` must not be read as an attestation, because the
/// consequence of getting it wrong is deleting a real image.
pub fn attested_digest(tag: &str) -> Option<String> {
    let hex = tag.strip_prefix("sha256-")?;
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return None;
    }
    Some(format!("sha256:{hex}"))
}

/// Classify every version, resolving attestations against the versions present.
pub fn classify(versions: &[PackageVersion]) -> Vec<(u64, VersionClass)> {
    let present: std::collections::HashSet<&str> =
        versions.iter().map(|v| v.digest.as_str()).collect();

    versions
        .iter()
        .map(|v| {
            let class = if v.tags.is_empty() {
                VersionClass::Untagged
            } else if v.tags.iter().all(|t| {
                attested_digest(t).is_some_and(|d| !present.contains(d.as_str()))
            }) {
                // Every tag is an attestation for something that is gone. If
                // even one tag is a real tag, or points at a live image, this
                // is not orphaned.
                VersionClass::OrphanedAttestation
            } else {
                VersionClass::Tagged
            };
            (v.id, class)
        })
        .collect()
}
```

- [ ] **Step 4: Relancer**

Run: `cargo test -p bondebarras-core packages > /tmp/v3t1-green.txt 2>&1; cat /tmp/v3t1-green.txt`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/bondebarras-core/src/packages.rs crates/bondebarras-core/src/lib.rs
git commit -m "feat(packages): classification des versions de conteneurs"
```

---

### Task 2: `ResourceKind::PackageVersion` et le palier 2

**Files:**
- Modify: `crates/bondebarras-core/src/model.rs`

**Interfaces:**
- Produces: la variante `ResourceKind::PackageVersion`, mappée sur `RiskTier::Medium`.

Le `match` exhaustif de `risk_tier` **ne compilera pas** tant que la nouvelle variante n'a
pas de palier. C'est le garde-fou pour lequel il existe : impossible d'ajouter une famille
destructive sans lui assigner consciemment une friction.

- [ ] **Step 1: Écrire le test qui échoue**

```rust
    #[test]
    fn deleting_a_package_version_is_medium_risk() {
        // Irreversible and not regenerable by a re-run, unlike a cache: the
        // layer is gone from the registry. But it is not the nuclear tier —
        // nothing here destroys a repository.
        assert_eq!(risk_tier(ResourceKind::PackageVersion), RiskTier::Medium);
        assert!(RiskTier::Low < risk_tier(ResourceKind::PackageVersion));
    }
```

Ajouter aussi `PackageVersion` à `ResourceKind::ALL`.

- [ ] **Step 2: Lancer le test**

Run: `cargo test -p bondebarras-core model > /tmp/v3t2-red.txt 2>&1; cat /tmp/v3t2-red.txt`
Expected: FAIL — `no variant named PackageVersion`.

- [ ] **Step 3: Implémenter**

Ajouter la variante, l'ajouter à `ALL` (et corriger la taille du tableau), et étendre le
`match` de `risk_tier` :

```rust
        ResourceKind::Cache | ResourceKind::Artifact | ResourceKind::WorkflowRun => RiskTier::Low,
        // Irreversible: the layer leaves the registry. A cache or an artifact
        // comes back with a re-run; this does not.
        ResourceKind::PackageVersion => RiskTier::Medium,
```

Le compilateur signalera tous les autres `match` sur `ResourceKind` à compléter —
`clean::execute`, `views/repo.rs`, `commands/clean.rs`. Les traiter dans les tâches
suivantes ; ici, il suffit que `model.rs` compile et que ses tests passent.

- [ ] **Step 4: Relancer et commiter**

```bash
cargo test -p bondebarras-core model > /tmp/v3t2-green.txt 2>&1; cat /tmp/v3t2-green.txt
git add crates/bondebarras-core/src/model.rs
git commit -m "feat(model): les versions de packages relevent du palier 2"
```

---

### Task 3: Endpoints des packages

**Files:**
- Create: `crates/bondebarras-core/src/api/packages.rs`
- Modify: `crates/bondebarras-core/src/api/mod.rs`

**Interfaces:**
- Produces:
  - `async fn versions(client: &Client, org: &str, package: &str) -> Result<Vec<PackageVersion>>`
  - `async fn delete_version(client: &Client, org: &str, package: &str, id: u64) -> Result<()>`

Routes réelles, vérifiées :
`/orgs/{org}/packages/container/{package}/versions?per_page=100`
`/orgs/{org}/packages/container/{package}/versions/{id}`

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn versions_carry_their_digest_and_tags() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/repolens/versions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "id": 862511118,
                  "name": "sha256:1d7018e5672547cced06883706367832e5f1be5fa90bc2038ad308e19958e80e",
                  "created_at": "2026-05-13T16:11:32Z",
                  "metadata": { "container": { "tags": ["sha256-1a65eb30f0e36fc41bb07724b11e53ada5e810382f39143698b00c470f019b80"] } } },
                { "id": 862511085,
                  "name": "sha256:9a26c70801010123223adb5e73ff703aca86c15e19b30124ede5628a1e185826",
                  "created_at": "2026-05-13T16:11:30Z",
                  "metadata": { "container": { "tags": [] } } },
                // No usable id: must be dropped, never coerced to 0.
                { "name": "sha256:deadbeef", "created_at": "2026-05-13T16:11:29Z",
                  "metadata": { "container": { "tags": [] } } }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = versions(&client, "systm-d", "repolens").await.unwrap();

        assert_eq!(out.len(), 2, "the version with no id must be dropped");
        assert_eq!(out[0].id, 862511118);
        assert_eq!(out[0].tags.len(), 1);
        assert_eq!(out[1].tags.len(), 0);
        assert!(out[1].digest.starts_with("sha256:9a26c7"));
    }

    #[tokio::test]
    async fn a_repo_without_a_package_yields_nothing_not_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/no-such/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        // Most repositories publish no image at all. A 404 is the normal case,
        // not a failure to report.
        assert!(versions(&client, "systm-d", "no-such").await.unwrap().is_empty());
    }
}
```

- [ ] **Step 2 à 5**

Même forme que les autres modules d'`api/` : implémenter en suivant `api/caches.rs`, avec
`filter_map` sur `item["id"].as_u64()?` (jamais `unwrap_or(0)`), un `404 → Vec::new()`, et
`age_days` importé depuis `api::caches`. Commit : `feat(api): endpoints des versions de packages`.

---

### Task 4: Les versions rejoignent le drill-down

**Files:**
- Modify: `crates/bondebarras-core/src/scan.rs`, `crates/bondebarras-core/src/clean.rs`

`repo_detail` interroge le package homonyme du repo — c'est la convention de tout ce compte
(`systm-d/repolens` publie `ghcr.io/systm-d/repolens`) — et convertit chaque version en
`Resource` avec `size_bytes: 0` et un `label` portant sa classification.

`clean::execute` gagne le bras `PackageVersion` appelant `api::packages::delete_version`.
Il lui faut le nom du package : le `Plan` porte déjà `owner` et `repo`, qui suffisent.

- [ ] **Step 1: Écrire le test qui échoue**

Un test wiremock sur `repo_detail` vérifiant qu'une version sans tag arrive en `Resource`
avec `kind == PackageVersion`, `size_bytes == 0`, et `stale_pr == false` (les packages n'ont
pas de ref git ; le drapeau ⚑ ne les concerne pas).

- [ ] **Step 2 à 5**

Implémenter, vérifier, commiter : `feat(scan): les versions de packages au drill-down`.

---

### Task 5: La modale du palier 2

**Files:**
- Modify: `crates/bondebarras-core/src/tui/views/confirm.rs`, `crates/bondebarras-core/src/tui/views/repo.rs`

Le palier 2 est défini depuis la v0.1 sans qu'aucune ressource n'y soit rattachée. La v0.3
est la première à l'utiliser.

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
    #[test]
    fn the_modal_matches_the_plans_tier() {
        // Tier 1 stays a bare [y/N]; tier 2 must add the itemised recap and
        // the irreversibility warning. A plan mixing both takes the higher.
        assert_eq!(modal_kind(RiskTier::Low), ModalKind::Simple);
        assert_eq!(modal_kind(RiskTier::Medium), ModalKind::Itemised);
    }

    #[test]
    fn a_package_row_says_its_size_is_unknown_not_zero() {
        // Every other screen shows bytes. A bare "0 o" here would read as
        // "empty", which is the opposite of the truth.
        let line = text(&row_spans(&package_resource(), false));
        assert!(!line.contains("0 o"), "got: {line}");
        assert!(line.contains('—'), "got: {line}");
    }
```

- [ ] **Step 2 à 5**

La modale palier 2 liste les éléments et porte l'avertissement d'irréversibilité, en
français accentué. Le panneau de droite affiche `—` au lieu d'une taille pour un
`PackageVersion`, et une ligne d'entête indiquant que GitHub n'expose pas la taille.
Commit : `feat(tui): modale palier 2 et taille inconnue des packages`.

---

### Task 6: CLI et documentation

**Files:**
- Modify: `crates/bondebarras-core/src/{cli,commands/clean}.rs`, `README.md`, `CHANGELOG.md`, `CLAUDE.md`, `site/content/_index*.md`

`--packages` rejoint les drapeaux de famille. Comme les autres, **son absence ne sélectionne
rien**.

- [ ] **Step 1: Écrire les tests qui échouent**

Étendre les tests de `select` : `--packages` seul ne prend que les `PackageVersion`, et un
`clean` sans aucun drapeau de famille n'en prend toujours aucun. **Le fixture doit contenir
au moins une ressource de chaque famille**, sinon il ne discrimine pas un bras de `match`
mal câblé.

- [ ] **Step 2 à 5**

Documentation : dire que la v0.3 nettoie les versions de packages, et **dire aussi que
GitHub n'en expose pas la taille**. Ne pas promettre de gain en octets. Ne pas annoncer les
branches/tags/releases (v0.4) ni l'archivage (v0.5). Commit séparé pour la doc.

## Self-Review

**Couverture de la spec v0.3**

| Exigence | Tâche |
|---|---|
| §1 aucune taille, `size_bytes = 0` | 4, 5 |
| §3.1 versions sans tag | 1 |
| §3.2 attestations orphelines | 1 |
| §3.3 versions taguées jamais présélectionnées | 4, 5 |
| §4 `ResourceKind::PackageVersion` | 2 |
| §5 palier 2 et sa modale | 2, 5 |
| §6 ligne d'avertissement sur la taille | 5 |
| §7 tests | chaque tâche |
| §8 pas de taille inventée | partout |

**Le test qui compte** est `an_attestation_whose_subject_survives_is_not_orphaned`. Sans le
cas négatif, la classification pourrait marquer orpheline toute attestation et le test
passerait — on proposerait alors de supprimer la signature d'une image en production.
