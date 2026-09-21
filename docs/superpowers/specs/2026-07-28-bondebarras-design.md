# bondebarras — TUI Rust pour auditer et nettoyer les orgs GitHub

- **Date :** 2026-07-28
- **Statut :** Design validé (en attente de relecture utilisateur)
- **Auteur :** k@levilainpetit.dev (avec Claude Code)

## 1. Contexte & objectif

Le compte `kdelfour` administre **15 organisations GitHub**. Faute de nettoyage,
plusieurs dépassent les quotas du forfait gratuit : stockage saturé, minutes Actions
épuisées, et une accumulation de repos abandonnés.

Le relevé effectué le 2026-07-28 chiffre le problème (tailles en Go décimaux) :

| Org | Caches Actions | Nb caches |
|---|---|---|
| `systm-d` | **37,2 Go** | 132 |
| `SecondBrain-io` | **13,9 Go** | 43 |
| `Tech-Work-events` | 190 Mo | 1 |
| `exec-d` | 71 Mo | 2 |
| `delfour-co` | 37 Mo | 3 |
| 10 autres | 0 | 0 |

Le détail de `systm-d` est instructif : `josephine` 12,4 Go (30 caches), `alertU`
11,5 Go (23), `claudine` 11,1 Go (69), `anonymous` 2,2 Go (10). Sur `claudine`, les plus gros caches
sont ceux de `Swatinem/rust-cache` — un jeu par job de matrice CI (`coverage`,
`ubuntu-22.04-test`, `ubuntu-24.04-test`) **multiplié par chaque ref**, dont
`refs/pull/25/merge` et `refs/pull/32/merge`, des PR fermées depuis longtemps. GitHub
n'évince qu'à 10 Go par repo ou après 7 jours sans accès : ces caches stagnent.

**Objectif :** un outil qui donne l'état des ressources consommées sur toutes les orgs
d'un coup, puis permet de nettoyer sélectivement, vite et sans se tromper.

## 2. Décisions clés (cadrage)

| Sujet | Décision |
|---|---|
| Axes du problème | Stockage **et** minutes Actions **et** repos morts |
| Nettoyage des minutes | **Impossible** rétroactivement → l'axe minutes est purement diagnostique |
| Périmètre destructif | Caches, artifacts, runs, packages GHCR, branches/tags/releases, **archivage** de repos |
| Modèle d'interaction | **Navigation manuelle pure** — pas de moteur de règles, pas de config persistée |
| Sélection en masse | Primitives **ad hoc** dans le TUI (tri, filtre, « tout cocher ⚑ »), rien de persisté |
| Garde-fou | **3 paliers gradués** selon le risque |
| Couche API | **octocrab**, cohérence avec `claudettes` |
| Auth | `gh auth token` → `$GITHUB_TOKEN` → erreur |
| Structure projet | Kit complet `claude-tui` : workspace, site Zola, CI, packaging |
| Visibilité du repo | **Public** — minutes Actions gratuites et illimitées |
| Nom | `bondebarras` — l'expression dit le geste et le soulagement |
| Orthographe | binaire et crates en ASCII (`bondebarras`), textes d'interface accentués (« Bon débarras ! ») |

## 3. Contraintes d'API découvertes

Trois constats issus des tests sur l'API réelle, qui cadrent ce qui est faisable.

### 3.1 Les endpoints billing historiques sont morts

`GET /orgs/{org}/settings/billing/actions`, `/packages` et `/shared-storage` renvoient
tous **410 Gone** — `{"message": "This endpoint has been moved."}`. GitHub a basculé sur
la plateforme de facturation unifiée.

**Conséquence directe : la jauge « X % de 500 Mo » n'est plus calculable.** Aucun endpoint
n'expose le quota total alloué.

### 3.2 Le nouvel endpoint est plus riche

`GET /organizations/{org}/settings/billing/usage` fonctionne et retourne un relevé
**par repo × par SKU × par mois** :

```json
{ "date": "2026-07-01T00:00:00Z", "product": "actions", "sku": "Actions Linux",
  "quantity": 3311.0, "unitType": "Minutes", "pricePerUnit": 0.006,
  "grossAmount": 19.866, "discountAmount": 19.866, "netAmount": 0.0,
  "organizationName": "systm-d", "repositoryName": "josephine" }
```

La jauge devient donc **« couvert par le forfait » vs « réellement facturé »** :
`grossAmount` = coût brut, `discountAmount` = absorbé par le forfait,
`netAmount` = ce qui est effectivement payé. C'est plus précis que l'ancienne jauge,
et ça nomme le repo fautif.

Cet endpoint peut renvoyer **403** sur une org où l'utilisateur n'est pas propriétaire
(constaté sur `le-vilain-petit-dev`). Ce n'est pas une erreur fatale : l'org reste
navigable pour tout le reste, avec un marqueur ⚠ sur la ligne.

### 3.3 Les agrégats org-level sont gratuits

`GET /orgs/{org}/actions/cache/usage-by-repository` retourne en **un seul appel** la
taille et le nombre de caches de chaque repo de l'org. C'est ce qui rend la vue
d'ensemble instantanée (cf. §5).

### 3.4 Note sur les caches

Les caches Actions ne sont **pas facturés** — ils sont gratuits, plafonnés à 10 Go par
repo, évincés en LRU. Les 37 Go de `systm-d` ne coûtent donc rien directement. Ils
restent la cible prioritaire pour deux raisons : ils masquent la visibilité sur ce qui
compte vraiment, et l'éviction LRU à 10 Go **détruit les caches utiles** (ceux de `main`)
au profit de caches morts de PR fermées, ce qui rallonge les CI.

Ce qui consomme réellement le quota facturable, c'est **artifacts + packages**.

## 4. Architecture

Workspace calqué sur `claude-tui` : cœur testé + binaire shim.

```
bondebarras/                      (workspace, le repo)
├─ crates/
│  ├─ bondebarras-core/  (lib)    ← toute la logique, la CLI et le TUI
│  └─ bondebarras/       (bin)    ← fn main() -> ExitCode { bondebarras_core::run() }
```

```
crates/bondebarras-core/src/
├─ lib.rs                run() -> ExitCode
├─ cli.rs                parsing clap
├─ commands/             scan.rs, clean.rs, tui.rs
├─ auth.rs               résolution du token, lecture des scopes
├─ api/
│  ├─ mod.rs             client, pagination, retry, concurrence
│  ├─ billing.rs         relevé d'usage
│  ├─ caches.rs          usage-by-repository, list, delete
│  ├─ artifacts.rs       list, delete
│  ├─ runs.rs            list, delete
│  ├─ packages.rs        packages, versions, delete
│  └─ repos.rs           repos, branches, tags, releases, archive
├─ model.rs              Org, Repo, Resource, ResourceKind, RiskTier, Selection
├─ scan.rs               orchestration deux étages
├─ clean.rs              planificateur + exécuteur, événements de progression
└─ tui/
   ├─ mod.rs             boucle d'événements
   ├─ app.rs             état + saisie
   ├─ theme.rs           palette et styles
   └─ views/             orgs.rs, repo.rs, billing.rs, confirm.rs
```

### 4.1 La frontière `api/`

**`api/` est le seul module qui connaît octocrab.** Le choix d'octocrab impose une
réalité : une partie des endroits nécessaires n'existe pas dans sa surface typée.

| Endpoint | Accès |
|---|---|
| repos, artifacts, packages, branches, releases | typé |
| `settings/billing/usage` | brut — `octocrab._get()` |
| `actions/cache/usage-by-repository` | brut — `octocrab._get()` |
| `actions/caches` (list, delete) | brut — `_get()` / `_delete()` |

Ce mélange **ne fuit pas** : `api/` expose des fonctions homogènes qui retournent les
types de `model.rs`. Le reste du code ignore quelles réponses sont typées et lesquelles
sont désérialisées à la main.

### 4.2 Vérification des scopes au démarrage

Le header `X-OAuth-Scopes` de la première réponse API donne les permissions réelles du
token. Elles sont mises en cache dans l'état de l'app, et **les actions indisponibles
sont grisées dans le TUI avec leur motif** plutôt que d'échouer au moment du delete.

État actuel du token de l'utilisateur : `admin:public_key`, `delete:packages`, `gist`,
`project`, `read:org`, `read:packages`, `repo`, `workflow`.

| Opération | Scope requis | Disponible |
|---|---|---|
| Caches, artifacts, runs | `repo` | ✅ |
| Versions de packages | `delete:packages` | ✅ |
| Branches, tags, releases | `repo` | ✅ |
| Archiver un repo | `repo` | ✅ |

La **suppression de repos est hors périmètre** (cf. §12), donc le scope `delete_repo`
n'est jamais requis : tout ce que l'outil sait faire tient dans les scopes déjà accordés.

## 5. Stratégie de scan — deux étages

Un scan complet naïf des 15 orgs (~100 repos × 4 familles de ressources) coûte environ
400 appels. Le design évite ce coût en exploitant les agrégats org-level.

**Étage 1 — vue d'ensemble, chargé au lancement :**

| Appel | Coût | Rendu |
|---|---|---|
| `orgs/{o}/actions/cache/usage-by-repository` | 1/org | taille + nombre de caches par repo |
| `organizations/{o}/settings/billing/usage` | 1/org | minutes + stockage par repo × SKU × mois |
| `orgs/{o}/repos` | 1-2/org | liste des repos |
| `orgs/{o}/packages` | 1/org | packages GHCR |

→ **≈ 60 appels pour 15 orgs, ~3 s.**

**Étage 2 — détail, chargé au drill-down uniquement :** caches individuels, artifacts,
runs, versions de packages, branches, tags, releases du repo sélectionné.

Ce découpage épouse le modèle de navigation manuelle : on ne paye que ce qu'on regarde.

### 5.1 Concurrence et limites de débit

- Lectures : sémaphore borné à **8** requêtes simultanées.
- Suppressions : débit volontairement plus bas, avec retry honorant `Retry-After` sur
  429 et 403 (secondary rate limit). Une purge de 132 caches lancée à plein régime se
  fait jeter par GitHub.
- Le quota principal (5 000 req/h) n'est jamais menacé par ce profil d'usage.

### 5.2 Détection « ⚑ PR fermée »

Un cache Actions porte un `ref` (`refs/pull/32/merge`, `refs/heads/main`). En croisant
avec l'état des PR du repo, on marque **⚑** les caches rattachés à une PR fermée ou
mergée : suppression sans aucun risque, et c'est là qu'est le volume.

C'est du calcul pur, testable sans réseau.

## 6. Le TUI

Squelette identique à claudine : header (onglets + logo en demi-blocs) / corps / ligne
de statut / footer de raccourcis, modales rendues par-dessus en conditionnel.

```
 bondebarras · kdelfour         [ Orgs ]  Billing   Aide

 ORGS                  │ systm-d / claudine              69 caches · 11.1 Go
 ───────────────────── │ ──────────────────────────────────────────────────
 systm-d      37.2 G ▸ │ [x] cache  v0-rust-coverage-Linux-x64   261 M  PR#32 ⚑
 SecondBrain  13.9 G   │ [x] cache  v0-rust-coverage-Linux-x64   261 M  PR#25 ⚑
 Tech-Work     190 M   │ [ ] cache  v0-rust-coverage-Linux-x64   261 M  main
 exec-d         71 M   │ [x] cache  v0-rust-ubuntu-22.04-test    257 M  PR#32 ⚑
 delfour-co     37 M   │ [ ] artif  github-pages                 1.1 M  1j
 le-vilain…      ⚠     │ [x] artif  github-pages                 1.1 M  expiré
                       │
 ⚠ billing 403         │ ⚑ = ref de PR fermée   sélection : 1.0 Go
 ─────────────────────────────────────────────────────────────────────────
 [espace] cocher  [s] trier  [f] filtrer  [A] tout ⚑  [d] supprimer  [?] aide
```

### 6.1 Navigation et sélection

Pas de moteur de règles, pas de fichier de config. Les primitives de sélection en masse
sont **ad hoc et non persistées** :

| Touche | Effet |
|---|---|
| `↑` `↓` `←` `→` | navigation dans l'arbre orgs → repo → ressources |
| `espace` | cocher / décocher une ligne |
| `s` | cycle de tri : taille ↓, âge ↓, nom |
| `f` | filtre textuel incrémental sur la liste |
| `A` | cocher toutes les lignes marquées ⚑ |
| `d` | supprimer la sélection (déclenche le palier adapté) |
| `Tab` | onglet suivant (Orgs / Billing) |
| `?` | aide |

### 6.2 Onglet Billing

Vue diagnostique, sans action : minutes converties en équivalent-inclus
(Linux ×1, Windows ×2, macOS ×10) face aux 2 000/mois, et coûts par repo avec la
répartition couvert / facturé issue de `discountAmount` / `netAmount`.

### 6.3 Thème

`tui/theme.rs` sur le modèle de `claudettes` : constantes `Color::Rgb(…)` puis helpers
`title_style()`, `muted()`, `status_warn()`, `selection_style()`, avec tests unitaires
sur la palette. La couleur de marque sera alignée sur celle du site Zola.

## 7. Garde-fous — 3 paliers

```
── palier 1 : caches / artifacts / runs ──────────
 Supprimer 84 artifacts (2.3 Go) ?          [y/N]

── palier 2 : packages / branches / tags ─────────
 41 versions • 3 repos • 890 Mo
 ⚠ irréversible, les pulls par digest casseront
 Confirmer ?                                [y/N]

── palier 3 : réservé ───────────────────────────
 Aucune ressource ne relève de ce palier dans le
 périmètre actuel. Il reste défini pour le jour où
 une opération vraiment irréversible entrera dans
 l'outil (cf. §12).
```

**Le palier est porté par le type, pas par l'UI.** `model.rs` définit
`fn risk_tier(kind: ResourceKind) -> RiskTier`, un `match` exhaustif : ajouter une
ressource destructive sans lui assigner de palier ne compile pas.

Aucune de ces opérations n'étant annulable côté GitHub, il n'y a **pas de corbeille ni
d'undo** — la promesse serait mensongère. À la place :

- la suppression s'exécute en tâche de fond, TUI non bloqué, avec barre de progression ;
- chaque ligne reçoit son résultat (`✓` / `✗` + motif) ;
- un journal de fin de run récapitule ce qui est parti, et se referme sur
  « Bon débarras ! ».

## 8. CLI headless

Le même cœur sans TUI, pour un cron mensuel :

```sh
bondebarras scan --org systm-d --json
bondebarras clean --org systm-d --caches --stale-pr --yes
```

Le **palier 3 est refusé en headless**, sans exception ni drapeau de contournement.
Aucune ressource n'y est rattachée aujourd'hui ; la règle est posée d'avance pour qu'une
opération future ne puisse pas se glisser dans un cron par inadvertance.

## 9. Infrastructure — kit `claude-tui` complet

### 9.1 Gouvernance et configuration

`CLAUDE.md`, `AGENTS.md`, `CONVENTIONS.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`,
`CHANGELOG.md`, `LICENSE-MIT` + `LICENSE-APACHE`, `deny.toml`, `rustfmt.toml`
(`max_width = 100`), `.cargo/audit.toml`, `.github/` (CODEOWNERS, `dependabot.yml`,
templates issue et PR).

Rust **edition 2024**, MSRV **1.88**, `unsafe_code = "forbid"`,
clippy `all = { level = "warn", priority = -1 }`.

### 9.2 Workflows

| Workflow | Contenu |
|---|---|
| `ci.yml` | lint (fmt + clippy `-D warnings`) · test matrix ubuntu-22.04/24.04 + fedora:40/41 + macos-latest · coverage tarpaulin → codecov (informatif, `continue-on-error`) · security `cargo-audit` + `cargo-deny` |
| `release.yml` | sur tag `v*` : binaires linux-x86_64 / windows-x86_64-msvc / macos-aarch64, `.deb` (cargo-deb), `.rpm` (cargo-generate-rpm), release GitHub, rendu de la formule Homebrew et du PKGBUILD AUR, manifestes winget, crates.io opt-in via `vars.PUBLISH_CRATES` |
| `pages.yml` | build Zola 0.21 → GitHub Pages, sur push `main` touchant `site/**` |
| `site-check.yml` | build Zola sur les PR touchant `site/**` |

### 9.3 Packaging

`packaging/homebrew/bondebarras.rb`, `packaging/aur/PKGBUILD`,
`packaging/winget/README.md`, `Formula/bondebarras.rb` (retiré du dépôt jusqu'à la
première stable, #21 ; la CI le propose alors en pull request contre `main` — elle
n'y pousse pas, `main` étant protégée sans dérogation, #35).

### 9.4 Landing page

Site Zola dans `site/` : `config.toml`, `content/_index.md` + `_index.fr.md`
(anglais à la racine, français sous `/fr/`), `sass/main.scss`,
`templates/{base,index}.html`, `static/` (favicons, hero, preview).

### 9.5 Note sur la visibilité du repo

Cette matrice de CI est précisément ce qui a rempli 11,1 Go de caches sur `claudine`.
Le repo `bondebarras` doit rester **public** : les minutes Actions y sont gratuites et
illimitées. En privé, cette matrice consommerait le quota que l'outil est censé
préserver.

## 10. Découpage en versions

| Version | Contenu | Gain |
|---|---|---|
| **v0.1** | Scan deux étages, TUI orgs + drill-down, caches + artifacts + runs, palier 1, détection ⚑ PR fermée | **~51 Go** récupérables immédiatement |
| **v0.2** | Onglet Billing (minutes + coûts par repo), CLI headless | diagnostic minutes |
| **v0.3** | Packages GHCR, priorité aux versions untagged — palier 2 | stockage packages |
| **v0.4** | Branches / tags / releases — palier 2 | ménage repo |
| **v0.5** | Archivage de repos — palier 2, scope `repo` | repos morts |

Chaque version fait l'objet de sa propre spec dans `docs/superpowers/specs/`.

## 11. Tests

| Cible | Outil |
|---|---|
| Réponses d'API | `wiremock` (fixtures figées des endpoints réels) |
| Rendu TUI | `insta` (snapshots) |
| CLI | `assert_cmd` + `predicates` |

La logique métier est conçue pour être testable **sans réseau** :

- détection « ⚑ PR fermée » à partir d'un `ref` de cache et d'un état de PR ;
- conversion des minutes en équivalent-inclus (multiplicateurs par SKU) ;
- agrégation du relevé de facturation (couvert / facturé par repo) ;
- mapping `ResourceKind → RiskTier` ;
- formatage des tailles.

Le `match` exhaustif de `risk_tier` est vérifié par un test qui énumère toutes les
variantes de `ResourceKind`.

## 12. Points explicitement hors périmètre

- **Pas de moteur de règles ni de config persistée** — choix assumé de navigation manuelle.
- **Pas de corbeille ni d'undo** — GitHub ne le permet pas, le promettre serait mentir.
- **Pas de gestion du LFS** — l'API ne permet pas de purger les objets LFS.
- **Pas de nettoyage rétroactif des minutes** — elles sont consommées, l'axe est diagnostique.
- **Pas de jauge « % du quota de stockage »** — l'endpoint qui la fournissait est mort (§3.1).
- **Pas de suppression de dépôts.** L'outil sait *archiver* un repo, ce qui est réversible
  et tient dans le scope `repo` déjà accordé. La suppression définitive est écartée : elle
  n'apporte rien que l'archivage ne règle pour un dépôt abandonné, et elle exigerait à la
  fois un scope supplémentaire (`delete_repo`) et le palier de confirmation le plus lourd
  pour un gain nul en stockage.
