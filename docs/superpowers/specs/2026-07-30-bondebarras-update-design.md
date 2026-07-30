# bondebarras — sous-commande `update`

- **Date :** 2026-07-30
- **Statut :** Design
- **Auteur :** k@levilainpetit.dev (avec Claude Code)
- **Référence :** `josephine`, dont le modèle est repris — pas celui de `claudine`

## 1. Pourquoi le modèle de josephine et pas celui de claudine

Les deux projets voisins ont une commande de mise à jour. Elles ne font pas la même chose.

**Claudine** télécharge l'asset et remplace le binaire en place, quel que soit le canal
d'installation. C'est faux dès que le binaire appartient à un gestionnaire de paquets :
remplacer un fichier posé par `dpkg` désynchronise sa base, et la prochaine mise à jour
système écrase silencieusement la version installée à la main.

**Josephine** détecte d'abord **comment le binaire a été installé**, puis :

- pour un canal géré (`apt`, `dnf`) elle propose la commande du gestionnaire ;
- pour un canal qu'elle ne doit pas toucher (Homebrew, AUR, Nix, cargo) elle **affiche la
  commande à lancer et n'installe rien** ;
- elle vérifie un sha256 avant d'installer quoi que ce soit.

C'est le comportement correct, et c'est celui que reprend bondebarras.

Le cas est concret : bondebarras vient d'être installé par `cargo install --path`. Un
self-replace y serait doublement faux — cargo réinstallerait par-dessus au build suivant.

## 2. Ce que la commande fait

```
bondebarras update           # met à jour, ou dit comment le faire
bondebarras update --check   # dit seulement si une version plus récente existe
```

Étapes, dans l'ordre :

1. Lire la version courante (`CARGO_PKG_VERSION`).
2. Interroger `GET /repos/systm-d/bondebarras/releases/latest` — **sans authentification**,
   le dépôt est public. Une mise à jour ne doit pas exiger de jeton.
3. Comparer les versions sémantiquement.
4. Détecter le canal d'installation.
5. Selon le canal : proposer la commande, ou télécharger + vérifier + installer.

## 3. Les trois issues de comparaison

| État | Sens |
|---|---|
| `UpToDate` | rien à faire |
| `Available(v)` | une version plus récente est publiée |
| `Ahead` | **le binaire local est plus récent que tout ce qui est publié** |

`Ahead` n'est pas un cas d'école : c'est exactement l'état de la machine aujourd'hui —
0.5.0 installé depuis les sources, aucune release publiée. Sans ce cas, la commande dirait
« vous êtes à jour » alors qu'elle n'a rien trouvé du tout, ou pire, proposerait une
rétrogradation.

## 4. Détection du canal

Deux signaux, dans cet ordre :

1. **Le chemin de l'exécutable** — `/.cargo/` → cargo, `linuxbrew` ou `/Cellar/` → Homebrew,
   `/nix/store/` → Nix.
2. **Le gestionnaire de paquets lui-même**, interrogé sur le chemin réel : `dpkg -S`,
   `rpm -qf`, `pacman -Qo`. Un chemin sous `/usr/bin` ne dit rien par lui-même ; seul le
   gestionnaire sait s'il le possède.

Le second signal prime sur le premier quand les deux répondent.

| Canal | Action |
|---|---|
| Deb | `sudo apt install <paquet>` — proposée, exécutée après confirmation |
| Rpm | `sudo dnf install <paquet>` — idem |
| Pacman | **manuel** : « la mise à jour passe par l'AUR » |
| Homebrew | **manuel** : `brew upgrade bondebarras` |
| Nix | **manuel** : le `/nix/store` est en lecture seule, la mise à jour vient de la config |
| Cargo | **manuel** : `cargo install --git https://github.com/systm-d/bondebarras bondebarras` |
| Tarball / inconnu | téléchargement + vérification du sha256, puis message manuel pointant vers le fichier déjà vérifié |

**Aucun binaire n'est jamais remplacé en place — pas même pour Tarball/inconnu.** La première
version de ce document disait « self-replace » pour ce cas ; ce n'est pas ce qui a été livré,
pour deux raisons qui se recoupent :

- L'unique dépendance ajoutée pour cette commande (§7) est `ureq` + `sha2` (+ `tempfile`,
  ajouté après revue de sécurité — voir §5). Décompresser l'archive `.tar.gz` que publie
  `release.yml` demanderait une crate d'extraction supplémentaire, hors budget.
- `josephine`, le modèle explicitement désigné par ce document, ne remplace pas non plus le
  binaire en place pour ce cas : `install_plan(Tarball | Unknown, …)` y télécharge et vérifie
  l'archive, puis affiche quand même un message manuel renvoyant vers la page de release —
  sans même se servir du fichier déjà téléchargé.

bondebarras fait mieux que josephine sur ce dernier point sans faire plus : le message pointe
vers le fichier **déjà téléchargé et dont la somme est déjà vérifiée** sur disque, plutôt que
de renvoyer l'utilisateur le retélécharger depuis la page GitHub. Mais il n'extrait ni ne
remplace rien lui-même. C'est plus prudent que ce que demandait la version précédente de ce
tableau, pas un raccourci pris dessus.

**Le self-replace, littéral, n'est donc plus dans le périmètre de cette commande.** S'il est
voulu un jour, deux pistes, aucune retenue ici : ajouter une crate d'extraction (`tar` au
minimum), ou publier en plus un binaire brut par plateforme dans `release.yml` — Windows en a
déjà un (`bondebarras-windows-x86_64.exe`, utilisé par winget), ce qui rendrait un vrai
remplacement possible sur cette seule plateforme sans nouvelle dépendance.

## 5. Vérification

Chaque release publie un fichier de sommes (`<asset>.sha256`, format `sha256sum` —
`release.yml` le génère pour chaque artefact, binaires compris, dans le job `release`). La
commande télécharge l'asset **et** sa somme, calcule le sha256 du fichier reçu et compare.

**La vérification échoue fermée, pas ouverte.** Trois états distincts, un seul qui installe :

| État | Cause | Issue |
|---|---|---|
| Somme absente | cette release ne publie pas de `.sha256` pour cet asset | refus |
| Somme injoignable | le `.sha256` est listé mais son téléchargement ou son parsing échoue | refus |
| Somme différente | les deux sommes ont été obtenues et ne concordent pas | refus |
| Somme identique | — | installation |

Une revue de sécurité a trouvé qu'une première version confondait les deux premiers cas dans un
seul état « non vérifié » qui laissait quand même l'installation continuer — exactement le
scénario qu'un attaquant capable d'interférer avec la seule requête du `.sha256` (sans toucher
au JSON de la release lui-même) produirait. Sans les distinguer et refuser sur les trois,
`update` resterait un vecteur d'exécution de code arbitraire déclenché par une seule commande —
pour un outil qui a par ailleurs le droit de supprimer des données. Conséquence assumée :
`update` n'installe rien tant qu'une release ne publie pas de somme qui concorde — c'est le
prix correct pour cette commande, pas un défaut à corriger plus tard.

**Dossier de préparation.** Le fichier téléchargé est écrit dans un répertoire à suffixe
aléatoire (`tempfile`), créé avec des sémantiques `O_EXCL` — jamais un nom prévisible dérivé du
PID sous un répertoire partagé, qui se serait pré-planté ou couru par un utilisateur local
tiers avant même que la vérification ci-dessus ne s'exécute. Ce répertoire (et son contenu)
n'est supprimé qu'après la tentative d'installation, jamais avant.

**Nom de l'asset.** Avant tout usage sur disque ou en ligne de commande, le nom de l'asset —
qui vient tel quel de la réponse de l'API GitHub — est validé : ni séparateur de chemin, ni
`.`/`..`, ni tiret de tête (que `apt`/`dnf` liraient comme une option). Refusé, pas assaini.

## 6. Ce que la commande ne fait pas

- **Elle ne s'exécute pas sans que l'utilisateur l'ait tapée.** Aucune vérification
  automatique au lancement du TUI : un outil de nettoyage n'a pas à parler à un serveur de
  release pendant qu'on lui demande de scanner des orgs.
- **Elle ne rétrograde jamais.** `Ahead` s'affiche et s'arrête.
- **Elle ne touche pas à un binaire possédé par un gestionnaire**, même si elle en a
  techniquement les droits.
- Pas de canal *nightly*, pas de pré-release : `releases/latest` ignore déjà ces dernières.

## 7. Dépendances

`ureq = "3"`, comme josephine — même version, même usage (appels à l'API GitHub et
téléchargement des assets). « Comme josephine » ne porte que sur `ureq` : josephine dépend
aussi de `semver` pour comparer les versions, bondebarras non (voir plus bas). Bondebarras
embarque déjà `rustls` via octocrab, donc le coût réel est la couche `ureq` elle-même, pas une
seconde pile TLS.

L'alternative — réutiliser octocrab pour la métadonnée — échoue au téléchargement : son
`BaseUriLayer` réécrit l'hôte de toute requête, et l'asset vit sur `objects.githubusercontent.com`
après redirection.

`sha2` pour la somme de contrôle.

`tempfile`, ajouté après la revue de sécurité du §5, pour le dossier de préparation à suffixe
aléatoire — la seule dépendance des quatre qui n'était pas prévue au design initial.

**Pas de `semver`.** La comparaison de versions (§3) est un comparateur
`major.minor.patch[-pré-release]` fait main : suffisant pour des tags GitHub (pré-release
correctement classée sous sa version stable, repli sur une comparaison de chaînes si l'un des
deux côtés ne parse pas), et une dépendance de moins à porter pour une comparaison aussi
bornée.

## 8. Tests

| Cible | Vérification |
|---|---|
| comparaison | plus récent, égal, **plus ancien → `Ahead`**, pré-release classée sous sa stable |
| chemin | `/.cargo/`, `linuxbrew`, `/Cellar/`, `/nix/store/`, `/usr/bin` (→ aucun) |
| canal, gestionnaire | un chemin `/usr/bin` ambigu est tranché par la réponse du gestionnaire, pas par le chemin seul |
| plan | chaque canal donne la bonne commande ; les quatre canaux non touchés donnent `Manual` |
| sha256 | vecteur connu, fichier vide, fichier plus grand que le tampon de lecture |
| parsing de somme | `<hex>  <nom>` → le premier champ |
| release | JSON réel de l'API ; une release sans asset pour la plateforme → `None` nommé, pas une panique |
| vérification | trois états distincts (absente / injoignable / différente) → refus dans les trois cas, un seul message par état |
| nom d'asset | séparateur de chemin, `.`/`..`, tiret de tête, nom vide → refus nommé et testable |

Deux tests comptent plus que les autres. **`Ahead`** : c'est l'état de la machine aujourd'hui,
et sans lui la commande mentirait dès son premier usage. **Gestionnaire de paquets** : une
implémentation qui devine `Tarball` depuis un chemin `/usr/bin` sans jamais consulter le
gestionnaire reproduirait, à l'intérieur de la détection de canal, exactement l'erreur que ce
document reproche à claudine ailleurs — toucher un binaire géré comme s'il ne l'était pas.
