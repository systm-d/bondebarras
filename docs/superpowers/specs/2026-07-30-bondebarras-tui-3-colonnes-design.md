# bondebarras — trois colonnes, marquage de sûreté, jauges de quota

- **Date :** 2026-07-30
- **Statut :** Design validé (en attente de relecture utilisateur)
- **Auteur :** k@levilainpetit.dev (avec Claude Code)
- **Prérequis :** v0.5 livrée, fusionnée dans `main`

## 1. Le défaut constaté

Sur une session réelle, l'utilisateur voit ses organisations, le curseur se pose sur un
dépôt, le panneau droit charge ses 320 éléments — et il rapporte ne pas arriver à naviguer
dans les dépôts.

La navigation fonctionne. **Elle est invisible.** Le pied de page annonce
`[espace] cocher [s] trier [f] filtrer [A] tout ⚑ [d] archiver [b] billing [q] quitter` :
que des actions, aucune touche de déplacement. `Tab`, `←`/`→`, `↑`/`↓` et `Entrée` sont
tous gérés et aucun n'est annoncé.

C'est la même classe de défaut que celui trouvé en revue finale de la v0.4, où le pied
disait « supprimer » pendant que `d` archivait : **le pied ne dit pas ce que l'outil fait.**

L'arbre de la colonne unique aggrave le problème. Les dépôts s'affichent indentés sous leur
organisation, donc rien ne distingue visuellement « je suis sur une org » de « je suis sur
un dépôt » sinon la couleur du surlignage.

## 2. Trois colonnes

```
 ORGS              DÉPÔTS               RESSOURCES
 ───────────────── ──────────────────── ──────────────────────────────
 systm-d    36.4 G josephine     12.4 G  Cache   ████████████▓ 115 %
 SecondBr…  14.9 G claudine      11.8 G  Minutes ▓▓▓▓▓▓▓▓▓▓▓▓▓   0 %
 delfour-co  161 M alertU         8.0 G  ──────────────────────────────
 exec-d       71 M anonymous      4.3 G  ⛑ cache v0-rust-ubu…  467 M PR#54
 maxds-lyon    0 o repolens         0 o  ⛑ cache v0-rust-cov…  416 M PR#51
                                         • artif github-pages  1.1 M expiré
                                           asset josephine-0.…  4.0 M v0.12.0
```

Chaque colonne est un niveau, visible en permanence. L'arbre déplié disparaît.

**`Focus` garde ses trois valeurs mais change de sens** : `Orgs`, `Repos` et `Resources`
désignent désormais des colonnes, plus des niveaux d'un arbre replié.

| Touche | Effet |
|---|---|
| `←` / `→` | colonne précédente / suivante |
| `Tab` | synonyme de `→`, cyclique |
| `↑` / `↓` | déplacement dans la colonne courante |
| `espace` | cocher la ligne |
| `Entrée` | forcer le chargement immédiat (voir §3) |

**Le pied de page annonce les déplacements**, et son contenu dépend de la colonne active.
C'est la correction du défaut du §1 ; le reste du design en découle.

### 2.1 Largeurs, et ce qui se passe quand elles ne rentrent pas

Trois colonnes à 24 + 28 + 30 font 82 caractères. **Un terminal de 80 colonnes est une
taille courante et le compte ne tombe pas.**

Ce projet a déjà livré une modale correcte en code et invisible à l'écran, puis un
correctif dont le défaut se reproduisait à une hauteur précise par largeur. La règle qui en
est sortie s'applique ici : **on balaie les largeurs, on n'en échantillonne pas trois.**

| Largeur du terminal | Disposition |
|---|---|
| ≥ 100 | trois colonnes : `Length(22)`, `Length(26)`, `Min(40)` |
| 72 à 99 | **deux colonnes** : dépôts + ressources ; l'organisation courante passe dans l'en-tête |
| < 72 | **une colonne** : la colonne active seule ; `←`/`→` la changent |

La dégradation se fait en retirant les colonnes de **gauche**, jamais celle de droite : les
ressources sont ce qu'on vient supprimer, et le contexte se rappelle dans l'en-tête.

## 3. Chargement de la colonne 3

Charger le détail d'un dépôt coûte **neuf appels** (caches, artifacts, runs, packages, PR,
releases, branches, tags, branche par défaut). Descendre le curseur sur les 33 dépôts de
`SecondBrain-io` en déclencherait 297.

**Le chargement part quand le curseur s'immobilise 300 ms**, et le résultat est gardé pour
la durée de la session. Traverser une liste au clavier ne déclenche rien ; s'arrêter sur un
dépôt le charge une fois.

- Un dépôt déjà chargé s'affiche instantanément, sans requête.
- `Entrée` force le chargement immédiat, sans attendre la pause.
- Pendant le chargement, la colonne affiche `(chargement…)` plutôt que de rester vide — une
  colonne vide se lit « ce dépôt n'a rien », ce qui est faux.
- Un chargement en vol est **abandonné** si le curseur repart : son résultat est ignoré à
  l'arrivée. Sans ça, s'arrêter successivement sur trois dépôts ferait afficher le contenu
  du premier arrivé, pas celui du dépôt regardé.

Le cache est invalidé pour un dépôt après une purge le concernant, sinon l'écran montrerait
ce qui vient d'être supprimé.

## 4. Le marquage de sûreté

Trois niveaux, portés par un champ `Resource.safety`, calculé au scan.

| Famille | ⛑ sûr | • à vérifier | rien |
|---|---|---|---|
| **cache** | PR fermée, ou branche disparue | autre ref | branche par défaut |
| **artifact** | expiré | > 30 j | récent |
| **workflow run** | PR de sa branche mergée | > 90 j | récent |
| **asset de release** | release ≥ 2 en arrière | release précédente | dernière release |
| **version de package** | sans tag, attestation orpheline | — | tagué |
| **branche** | mergée | vivante non mergée | par défaut, protégée |
| **tag** | — | — | toujours |
| **dépôt** | — | — | **jamais marqué** |

Le dépôt n'est jamais marqué, à aucun niveau : la règle de la v0.5 tient — `pushed_at`
n'est pas une preuve d'abandon, et une bibliothèque finie ne bouge pas pendant deux ans
sans être morte.

⚠️ **Le cache et le workflow run ne partagent pas leurs règles**, malgré des lignes d'allure
voisine. Un cache est une optimisation : rien ne casse quand il disparaît, donc une branche
effacée suffit à le dire sûr. Un run est un **compte rendu** — le journal de ce qui s'est
passé — et la disparition d'une branche ne dit rien de l'envie qu'on aura de le relire. Seul
un fait positif le rend sûr : sa PR a mergé. Sinon, l'âge tranche.

La première implémentation les a fait passer par la même fonction, et la revue de la tâche 1
a mesuré ce que ça donnait : **un run d'un jour, sur une branche supprimée sans jamais
merger, ressortait ⛑** — donc présélectionné par `[A]`, donc supprimable d'une frappe. Deux
lignes du tableau qui se ressemblent ne sont pas la même règle.

### 4.1 Deux critères qui ne coûtent rien

- **« branche disparue »** pour un cache croise la liste des branches, que la v0.4 récupère
  déjà dans le même `repo_detail`.
- **« PR mergée »** vient de `closed_prs`, le même appel qui alimente le drapeau actuel
  depuis la v0.1.

Aucune requête supplémentaire pour l'un ni pour l'autre.

### 4.2 Pourquoi trois niveaux et pas deux

Un asset de l'avant-dernière release et un cache de PR fermée ne sont pas morts de la même
façon. Le premier peut encore être téléchargé par quelqu'un qui suit une version ; le second
n'est référencé par rien. Les confondre sous un même ⛑ forcerait à trancher arbitrairement,
et à mentir dans un sens ou dans l'autre.

`[A]` coche les ⛑. `Maj+A` ajoute les •. Le niveau intermédiaire est **affiché mais jamais
coché par défaut**.

### 4.3 Rapport avec `protected`

`Safety` et `Resource.protected` sont deux choses distinctes et le restent :

- `protected` est la **grille de sélection en masse**, appliquée par `select()` en headless
  et par `[A]` dans le TUI. Elle ne change pas.
- `Safety` est un **affichage** et le critère de ce que `[A]` et `Maj+A` proposent.

Une ressource `protected` n'est jamais ⛑ — et c'est **tout** ce que `protected` décide de
son niveau. La première rédaction de ce paragraphe en tirait un exemple faux : « une branche
vivante non mergée est • sans être `protected` ». Elle *est* `protected`, depuis la v0.4 —
`scan::branch_resources` pose `protected: class != BranchClass::Merged` précisément pour
qu'un cron ne supprime jamais de travail non mergé.

La conséquence a été trouvée en revue de la tâche 1 : lire `protected` comme « donc rien à
afficher » rendait le niveau • des branches **inatteignable**, et le rangeait avec la branche
par défaut alors que les deux ne se ressemblent pas. La règle correcte est celle du haut,
appliquée à la lettre — `protected` interdit le ⛑ et rien de plus. Chaque famille décide de
son niveau par sa propre ligne du tableau du §4, et la branche le fait via `branch_class`
(`Merged` / `Default` / `Protected` / `Live`), qui porte depuis la v0.4 la distinction à
quatre voies que `protected` seul ne peut pas rendre.

Ce qui reste vrai sans réserve : **la sélection individuelle (`espace`) n'est jamais bornée
par `Safety`.** Une ligne visible est cochable, quel que soit son niveau.

## 5. Les deux jauges

En tête de la colonne 3, pour le dépôt courant.

**Cache / 10 Go.** Le plafond est celui documenté par GitHub. **L'API ne l'expose pas** —
`actions/cache/usage` ne renvoie que les totaux — donc il est codé en dur, et la jauge le
dit. Au-delà de 100 %, elle avertit que l'éviction est en cours : GitHub supprime alors les
caches les moins récemment lus, y compris ceux de la branche par défaut, au profit de ceux
des PR fermées.

Constaté le 2026-07-30 : `systm-d/josephine` à 11,5 Gio et `systm-d/claudine` à 11,0 Gio,
tous deux au-dessus du plafond.

**Minutes / 2 000.** Depuis le relevé de facturation, déjà agrégé en v0.2. Sur un dépôt
public elle affiche 0 % **avec la raison** — sans elle, un 0 % se lirait comme une marge
confortable alors qu'il signifie « cette famille ne consomme rien par nature ».

Aucune troisième jauge : le stockage de packages n'a ni taille ni SKU de facturation
(établi en v0.3), et le stockage Actions est un débit en gigaoctet-heures, pas un niveau
comparable à un plafond.

## 6. Architecture

```
crates/bondebarras-core/src/
├─ safety.rs               classification pure des trois niveaux
├─ tui/views/orgs.rs       colonne 1, réduite à la liste des orgs
├─ tui/views/repos.rs      colonne 2 (neuf)
├─ tui/views/gauges.rs     les deux jauges (neuf)
└─ tui/app.rs              curseur de colonne, cache de dépôts, minuteur
```

`Resource` gagne `safety: Safety`. `safety.rs` ne connaît ni réseau ni ratatui : il prend
les données d'un dépôt et rend un niveau par ressource, donc s'y teste.

**Ce que ça ne touche pas :** la couche `api/`, `clean.rs`, `commands/clean.rs`. La CLI
headless continue de filtrer sur `protected`, sans rien savoir de `Safety`.

## 7. Tests

| Cible | Vérification |
|---|---|
| classification | chaque famille, chaque niveau ; **une PR fermée sans merge ne rend pas sa branche ⛑** |
| `protected` vs ⛑ | une ressource `protected` n'est jamais ⛑ |
| `[A]` / `Maj+A` | `[A]` ne prend que les ⛑ ; `Maj+A` ajoute les • et rien d'autre |
| dépôt | jamais marqué, quel que soit son âge |
| jauges | 115 % ne se plafonne pas à 100 ; dénominateur nul ne divise pas par zéro |
| chargement | un résultat arrivé après un déplacement du curseur est ignoré |
| cache | un dépôt déjà chargé ne relance aucune requête ; une purge l'invalide |
| **rendu** | **balayage de largeurs 60 → 200** : les trois dispositions apparaissent aux bons seuils, la colonne des ressources n'est jamais celle qu'on retire, rien n'est tronqué |

Le test de rendu balaie et n'échantillonne pas. Dix tests de ce projet ont nommé la bonne
propriété sans pouvoir échouer dessus ; le dernier défaut de modale se reproduisait à une
hauteur précise par largeur, et trois tailles échantillonnées l'avaient manqué.

## 8. Hors périmètre

- Pas de redimensionnement des colonnes à la souris ni au clavier.
- Pas de tri par niveau de sûreté — `[s]` garde ses trois clés existantes.
- Pas de jauge d'organisation : les plafonds sont par dépôt.
- Pas de préchargement spéculatif des dépôts voisins.
