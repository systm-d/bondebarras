# bondebarras v0.5 — archivage de dépôts

- **Date :** 2026-07-29
- **Statut :** Design (mesuré contre l'API réelle avant rédaction)
- **Auteur :** k@levilainpetit.dev (avec Claude Code)
- **Prérequis :** v0.4 livrée (branches, tags, assets), fusionnée dans `main`
- **Dernière version prévue.**

## 1. Ce que l'archivage fait, et ne fait pas

**Il ne libère aucun octet.** Le `size` d'un dépôt archivé est inchangé. Prétendre le
contraire répéterait l'erreur du « 818 % » de la v0.2 et celle des « 890 Mo » de la v0.3.

Ce qu'il fait, et qui justifie sa présence dans cet outil : **un dépôt archivé a ses Actions
désactivées.** Il cesse donc de produire des caches, des artifacts et des workflow runs —
exactement les trois familles que la v0.1 existe pour nettoyer. Archiver un dépôt mort n'est
pas du rangement cosmétique : c'est fermer le robinet plutôt que d'éponger indéfiniment.

**L'archivage est réversible** — un dépôt se désarchive. C'est ce qui le place au palier 2
et non au palier 3, et c'est aussi ce qui rend la suppression de dépôts inutile : tout ce
qu'on voudrait en obtenir, l'archivage le donne sans l'irréversibilité.

## 2. Le gisement, mesuré

Dépôts sans push depuis plus d'un an, non archivés, sur cinq organisations (2026-07-29) :

| Dépôt | Dernier push | Taille |
|---|---|---|
| `maxds-lyon/.github` | 775 j | 479 Ko |
| `maxds-lyon/NotionProspectingAutomation` | 750 j | 67 Ko |
| `maxds-lyon/Kata-DevOps` | 742 j | 3 Ko |
| `maxds-lyon/Kata-Terraform` | 742 j | 6 Ko |
| `maxds-lyon/.github-private` | 715 j | 469 Ko |
| `maxds-lyon/lokiprint` | 685 j | 8 397 Ko |
| `maxds-lyon/max-space` | 630 j | 5 813 Ko |
| … et d'autres | | |

**Un seul dépôt déjà archivé** sur les cinq organisations. Le tri n'a jamais été fait.

## 3. Ce que l'outil propose

Une seule opération : **archiver**. Pas de désarchivage — le rendre trivial inviterait à
archiver à la légère, et l'inverse se fait en deux clics sur github.com quand le besoin est
réel.

| Critère | Rôle |
|---|---|
| `pushed_at` | l'âge affiché ; seul signal de mort disponible |
| `archived` | déjà archivé → affiché, jamais candidat |
| `fork` | affiché, marqué ; un fork mort est le cas le plus sûr |

⚠️ **`pushed_at` n'est pas une preuve d'abandon.** Une bibliothèque stable et finie ne bouge
pas pendant deux ans sans être morte. L'outil **n'auto-sélectionne jamais** un dépôt, quel
que soit son âge : il trie par ancienneté et laisse l'humain décider. C'est la seule famille
de tout le produit où aucune présélection n'existe, y compris avec `[A]`.

## 4. Palier et garde-fous

Palier **2** : récapitulatif chiffré nommant chaque dépôt, puis confirmation.

Trois refus absolus, dans le type et non dans l'interface :

- un dépôt **déjà archivé** n'est pas candidat ;
- un dépôt dont l'utilisateur n'est pas administrateur n'est pas candidat — l'API renverrait
  403, et proposer une action qu'elle refusera est un mensonge ;
- **le headless refuse l'archivage entièrement.** Pas de drapeau `--archive`. Archiver
  bascule un dépôt entier en lecture seule ; cela ne se décide pas dans un cron.

Cette dernière règle est la première fois que le produit refuse une famille en headless. La
règle « le palier 3 est refusé en headless » existe depuis la v0.2 sans ressource attachée ;
celle-ci est plus étroite et plus concrète : ce n'est pas le palier qui refuse, c'est
l'opération.

## 5. Architecture

```
crates/bondebarras-core/src/
├─ api/archive.rs     PATCH /repos/{o}/{r} { "archived": true }
└─ repos.rs           classification pure : archivable, déjà archivé, sans droits
```

`ResourceKind` gagne `Repository`. Le `match` exhaustif de `risk_tier` force son palier.

**Le dépôt n'est pas une ressource du drill-down** — il *est* le niveau au-dessus. Il
apparaît donc dans le panneau de gauche, sur la ligne du dépôt lui-même, et non dans la
liste de droite. C'est la première ressource dont la sélection vit dans l'arbre.

### 5.1 Scopes

`repo` suffit. **`delete_repo` n'est jamais requis** — la suppression reste hors périmètre
définitivement (spec v0.1 §12).

## 6. Dettes de la v0.4 à solder

Quatre points relevés par la revue finale de la v0.4, à traiter ici :

1. `has_known_size` est un `matches!` nié plutôt qu'un `match` exhaustif, et son test sur
   `ResourceKind::ALL` recopie l'implémentation — une nouvelle famille sans taille passerait
   sans échec. Le rendre exhaustif, avec une table attendue écrite en dur.
2. Un quatrième `kind == PackageVersion` survit dans `list_title` (`repo.rs`), incohérent
   avec le nouveau prédicat.
3. Si la récupération de la branche par défaut échoue, `main` retombe en `Live` et le garde
   de sélection cesse de la couvrir — précisément quand la ligne est la plus dangereuse.
4. Depuis que les sept familles dégradent uniformément, un listing refusé se lit « rien à
   supprimer » en headless, sans aucun signal. Faire remonter un avertissement.

## 7. Tests

| Cible | Vérification |
|---|---|
| classification | archivable / déjà archivé / sans droits, séparément |
| jamais d'auto-sélection | `[A]` ne prend aucun dépôt, quel que soit son âge |
| headless | `clean` ne peut pas archiver, sans drapeau ni contournement |
| palier | `Repository` en palier 2 ; `Plan::tier` le remonte |
| API | le `PATCH` porte `{"archived": true}` ; un 403 remonte comme erreur, pas comme succès |
| dette 1 | une nouvelle variante sans taille fait échouer le test `ALL` |

Le test qui compte est **`[A]` ne prend aucun dépôt**. Sans lui, une extension future du
raccourci ferait basculer une organisation entière en lecture seule sur une frappe.

## 8. Hors périmètre

- **Pas de suppression de dépôt**, définitivement.
- Pas de désarchivage.
- Pas de critère d'abandon autre que `pushed_at` — l'API n'en offre pas de meilleur, et en
  inventer un donnerait une fausse assurance.
- Pas d'archivage en masse sur une organisation entière : dépôt par dépôt, vu par un humain.
