# bondebarras v0.3 — packages GHCR

- **Date :** 2026-07-29
- **Statut :** Design (recadré après mesure sur l'API réelle)
- **Auteur :** k@levilainpetit.dev (avec Claude Code)
- **Prérequis :** v0.2 livrée (Billing, CLI headless), fusionnée dans `main`

## 1. Ce que la mesure a changé

La spec v0.1 annonçait la v0.3 comme « stockage packages », avec une maquette montrant
« 41 versions untagged · 890 Mo ». **Ce chiffre n'est pas récupérable.** Trois constats
établis contre l'API réelle le 2026-07-29 :

### 1.1 L'API n'expose aucune taille

Le payload complet d'une version de package conteneur :

```json
{ "id": 862511118,
  "name": "sha256:1d7018e5672547cced06883706367832e5f1be5fa90bc2038ad308e19958e80e",
  "metadata": { "container": { "tags": ["sha256-1a65eb30f0e36…"] }, "package_type": "container" },
  "created_at": "2026-05-13T16:11:32Z", "updated_at": "2026-05-13T16:11:32Z",
  "html_url": "…", "package_html_url": "…", "url": "…" }
```

Aucun champ de taille, sous aucun nom. Une recherche sur `*size*` dans la réponse complète
de toutes les versions ne renvoie rien.

### 1.2 Le relevé de facturation ne mentionne pas les packages

Les SKU présents sur les quatre organisations interrogées : `Actions Linux`,
`Actions Windows`, `Actions macOS 3-core`, `Actions storage`, et divers `copilot`/`ghec`.
**Aucun SKU de stockage de packages.** La facturation ne fournit donc pas non plus de
substitut au champ manquant.

### 1.3 Le gisement est petit

Relevé sur les 15 organisations :

| Org | Packages | Versions | Untagged |
|---|---|---|---|
| `maxds-lyon` | 1 | 28 | **20** |
| `delfour-co` | 5 | 12 | 0 |
| `systm-d` | 1 | 5 | 3 |
| 12 autres | 0 | 0 | 0 |
| **Total** | **7** | **45** | **23** |

## 2. Conséquence : une version d'hygiène, pas de volume

La v0.1 et la v0.2 se justifiaient par des octets — 51,4 Go de caches, 24 632 minutes
privées. **La v0.3 ne peut pas se justifier ainsi**, et prétendre le contraire serait
répéter l'erreur du « 818 % » corrigée en fin de v0.2.

Ce qu'elle apporte réellement : sur `maxds-lyon`, **20 versions sur 28 sont des couches
sans tag** — 71 % de déchet dans un registre qu'on ne voit jamais. C'est du ménage utile,
mesuré en versions, pas en octets.

**Décision de conception : `Resource.size_bytes` vaut 0 pour une version de package**,
comme pour un workflow run. Le TUI affiche déjà `0 o` dans ce cas ; la v0.3 doit rendre
cette absence explicite plutôt que de la laisser lire comme « vide ».

## 3. Ce que l'outil sait détecter

Trois catégories, par ordre de sûreté décroissante.

### 3.1 Les versions sans tag — le gros du gisement

`metadata.container.tags` vide. Une couche que plus aucun tag ne référence : elle reste
tirable par digest, mais plus personne ne la tire par nom. C'est l'équivalent packages du
cache rattaché à une PR fermée.

⚠️ **Nuance importante, à ne pas écraser :** une version untagged peut encore être
référencée comme couche d'une image multi-architecture. Supprimer la couche casse le
manifeste parent. L'API ne permet pas de le vérifier — d'où le palier 2 (cf. §5).

### 3.2 Les attestations orphelines

GitHub et cosign attachent signatures et attestations en les taguant
`sha256-<digest-de-l-image-signée>`. Exemple réel sur `systm-d/repolens` :

```
version 862511118  tag  sha256-1a65eb30f0e36fc41bb07724b11e53ada5e810382f39143698b00c470f019b80
version 862508365  tags latest, 2.0, 2.0.2   (nom : sha256:1a65eb30f0e36…)
```

Le tag de la première **encode le digest de la seconde**. Quand l'image signée disparaît,
son attestation reste — et n'est plus rattachable à rien. C'est détectable en pur calcul :
un tag de la forme `sha256-<64 hexa>` dont le digest ne correspond au `name` d'aucune
version encore présente.

### 3.3 Les versions taguées

Affichées, jamais présélectionnées. Supprimer `latest` ou `2.0.2` casse des déploiements.

## 4. Architecture

Ajouts au découpage existant :

```
crates/bondebarras-core/src/
├─ api/packages.rs      liste des packages, versions, suppression d'une version
└─ packages.rs          classification pure : untagged, attestation orpheline, taguée
```

`packages.rs` ne connaît ni réseau ni ratatui : il prend un `Vec<PackageVersion>` et rend
une classification. Toute la logique d'attestation orpheline y vit, donc s'y teste.

`ResourceKind` gagne une variante `PackageVersion`. Le `match` exhaustif de `risk_tier`
force à lui assigner un palier — c'est exactement le garde-fou pour lequel il existe.

### 4.1 Scopes

`read:packages` pour lister, `delete:packages` pour supprimer. **Le jeton de l'utilisateur
porte déjà les deux.** Aucun nouveau scope n'est requis.

## 5. Paliers

| Catégorie | Palier | Friction |
|---|---|---|
| Attestation orpheline | 1 | `[y/N]` — plus rien ne la référence, par construction |
| Version sans tag | **2** | récapitulatif chiffré + `[y/N]`, avec l'avertissement multi-arch |
| Version taguée | **2** | idem, jamais présélectionnée |

Le palier 2 était défini depuis la v0.1 sans qu'aucune ressource n'y soit rattachée. La
v0.3 est la première à l'utiliser, et sa modale — récapitulatif chiffré avant confirmation
— reste à écrire.

## 6. Ce que l'interface doit dire

Le drill-down d'un repo gagne les versions de son package homonyme, s'il existe.

```
 systm-d / repolens                        5 versions · taille inconnue
 ──────────────────────────────────────────────────────────────────────
 [x] pkg    sha256:9a26c7080…              —      sans tag        ⚑
 [x] pkg    sha256:1d7018e56…              —      attestation orpheline ⚑
 [ ] pkg    sha256:1a65eb30f…              —      latest, 2.0, 2.0.2
 ──────────────────────────────────────────────────────────────────────
 ⚠ GitHub n'expose pas la taille des versions de packages.
```

**La ligne d'avertissement n'est pas optionnelle.** Sans elle, une colonne de tirets dans
un outil dont chaque autre écran affiche des octets se lit comme « ces éléments sont
vides », c'est-à-dire l'inverse de la vérité.

## 7. Tests

| Cible | Vérification |
|---|---|
| classification | untagged, taguée, attestation reconnues séparément |
| attestation orpheline | tag `sha256-<hex>` sans version correspondante → orpheline ; **avec** version correspondante → **pas** orpheline |
| tag malformé | `sha256-zzz`, `sha256-`, tag court → traité comme un tag ordinaire, jamais comme une attestation |
| paliers | `PackageVersion` mappe sur le palier 2, `Plan::tier` le remonte |
| API | liste, versions, suppression ; 404 sur un repo sans package → aucune ressource, pas d'erreur |
| CLI | `--packages` sélectionne la famille ; sans lui, aucune version n'est touchée |

Le deuxième test est le seul qui compte vraiment : une attestation dont l'image existe
encore **ne doit pas** être marquée orpheline, sinon on propose de supprimer la signature
d'une image vivante. Le fixture doit contenir les deux cas, sans quoi il ne discrimine rien.

## 8. Hors périmètre

- **Pas de taille**, ni estimée, ni extrapolée. L'API ne la donne pas ; l'inventer serait mentir.
- Pas de suppression de package entier — seulement des versions.
- Pas de types autres que `container` : `npm`, `maven`, `nuget` ont d'autres sémantiques et
  aucun de ces comptes n'en publie.
- Pas de vérification des références multi-architecture — l'API ne le permet pas. D'où le
  palier 2 et son avertissement explicite.
