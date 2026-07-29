# bondebarras v0.4 — branches, tags et releases

- **Date :** 2026-07-29
- **Statut :** Design (mesuré contre l'API réelle avant rédaction)
- **Auteur :** k@levilainpetit.dev (avec Claude Code)
- **Prérequis :** v0.3 livrée (packages GHCR), fusionnée dans `main`

## 1. Le gisement, mesuré

Contrairement à la v0.3 — où l'absence de taille dans l'API a forcé un recadrage en version
d'hygiène — **la v0.4 est une version de volume**, et le chiffre est vérifié.

Assets de releases sur quatre organisations, relevés le 2026-07-29 :

| Repo | Releases | Assets |
|---|---|---|
| `exec-d/terminus` | 25 | **1 453 Mo** |
| `delfour-co/githero` | 27 | **1 371 Mo** |
| `exec-d/terminus-32` | 19 | 1 128 Mo |
| `exec-d/dark-ink` | 6 | 846 Mo |
| `delfour-co/dashboard` | 12 | 534 Mo |
| `delfour-co/cavalio` | 5 | 296 Mo |
| `delfour-co/asteroids` | 11 | 263 Mo |
| 11 autres | — | 1 425 Mo |
| **Total (4 orgs)** | | **7 316 Mo** |

Les branches s'accumulent aussi : 17 sur `claudine`, 16 sur `repolens`, et
`SecondBrain-io/monolith-back` atteint le plafond de 100 par page.

**L'API expose bien `size` sur chaque asset** (`releases[].assets[].size`), donc aucune des
contorsions de la v0.3 n'est nécessaire ici.

## 2. Détecter une branche morte sans payer une requête par branche

C'est la question de conception centrale. Comparer chaque branche à la branche par défaut
coûterait un `compare` par branche — 100 requêtes sur un seul repo.

**Ce n'est pas nécessaire.** Une PR fermée expose `head.ref` et `merged_at` :

```
#31  head=claude/claudine-landing-positioning-3jbqk4  merged_at=2026-07-24T13:33:32Z
#30  head=claude/claudine-landing-positioning-3jbqk4  merged_at=2026-07-24T12:46:51Z
```

Or `prs::closed_numbers` **récupère déjà ces PR** depuis la v0.1, pour le drapeau ⚑ des
caches. Il suffit d'en extraire aussi les `head.ref` mergés. **Coût marginal : zéro
requête.**

Une branche dont une PR a été mergée est une branche morte. C'est exactement le même levier
que le cache rattaché à une PR fermée, appliqué à une autre ressource.

### 2.1 Trois nuances à ne pas écraser

- **Une branche peut porter plusieurs PR.** Ci-dessus, `claude/claudine-landing-…` est la
  source de #29, #30 **et** #31. Le jeu des branches mergées est donc un ensemble, pas une
  liste — dédupliquer.
- **Une PR fermée sans merge ne tue pas sa branche.** `merged_at` à `null` signifie
  rejetée : le travail peut être repris. Seul un `merged_at` non nul compte.
- **La branche par défaut n'est jamais candidate**, même si une PR mergée pointait vers
  elle. Ni aucune branche protégée — l'API expose `protected` sur chaque branche.

## 3. Ce que l'outil sait proposer

| Ressource | Détection | Palier | Taille |
|---|---|---|---|
| Branche mergée | `head.ref` d'une PR avec `merged_at` non nul | 2 | — |
| Tag | listé, jamais présélectionné | 2 | — |
| Asset de release | `releases[].assets[]` | 2 | **oui** |
| Release entière | — | **hors périmètre** | — |

**Les releases elles-mêmes ne sont pas supprimables.** Seuls leurs assets le sont. Une
release est un point d'histoire du dépôt — un tag, des notes, une date — et son poids est
entièrement dans ses binaires. Supprimer les assets d'une vieille release libère l'espace
sans effacer la trace.

**Les tags sont affichés, jamais présélectionnés**, et `protected` au sens de la v0.3 :
`select()` les refuse en headless. Un tag est ce sur quoi pointent les releases, les
`go get`, les `Cargo.toml` ; le supprimer casse des installations reproductibles.

## 4. Architecture

```
crates/bondebarras-core/src/
├─ api/refs.rs        branches, tags, suppression d'une ref
├─ api/releases.rs    releases, assets, suppression d'un asset
└─ refs.rs            classification pure : branche mergée, protégée, par défaut
```

`ResourceKind` gagne trois variantes : `Branch`, `Tag`, `ReleaseAsset`. Le `match`
exhaustif de `risk_tier` force à leur assigner un palier — toutes trois en palier 2.

`api::prs::closed_numbers` devient `closed_prs`, rendant à la fois les numéros (pour le ⚑
des caches) et les `head.ref` mergés (pour les branches). **Un seul appel, deux usages.**

### 4.1 Scopes

`repo` suffit pour les trois familles. Aucun nouveau scope.

## 5. Ce que l'interface doit dire

```
 systm-d / claudine                            17 branches · 5 releases
 ────────────────────────────────────────────────────────────────────────
 [x] branch claude/claudine-landing-positioning…    —   mergée #31 ⚑
 [x] asset  claudine-linux-x86_64.tar.gz         2.4 Mo  v0.1.1
 [ ] tag    v0.1.3                                  —   protégé
 [ ] branch main                                    —   par défaut
```

La branche par défaut et les branches protégées sont **affichées mais jamais cochables** —
les voir rassure sur le fait que l'outil les connaît et les épargne.

## 6. Tests

| Cible | Vérification |
|---|---|
| branche mergée | `merged_at` non nul → morte ; `merged_at` nul → **vivante** |
| plusieurs PR, une branche | dédupliqué, une seule entrée |
| branche par défaut | jamais candidate, même avec une PR mergée la visant |
| branche protégée | `protected: true` → `Resource.protected` |
| assets | taille sommée par release ; une release sans asset ne produit rien |
| tags | toujours `protected`, donc jamais pris en headless |
| paliers | les trois variantes en palier 2 |

Le test qui compte est le premier : une PR **fermée sans merge** ne doit pas tuer sa
branche. Sans le cas négatif, une classification marquant toute PR fermée passerait, et
l'outil proposerait de supprimer le travail d'une PR rejetée qu'on comptait reprendre.

## 7. Hors périmètre

- **Pas de suppression de release**, seulement de ses assets.
- Pas de `compare` par branche — la détection passe par les PR, ou ne se fait pas.
- Pas de suppression de branche par défaut ni protégée, à aucun palier.
- Pas de suppression de dépôt : hors périmètre définitivement (cf. spec v0.1 §12).
