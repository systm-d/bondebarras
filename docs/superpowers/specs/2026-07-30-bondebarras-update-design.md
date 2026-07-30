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
| Tarball / inconnu | self-replace, après vérification du sha256 |

**Le self-replace est le dernier recours, pas le défaut.** C'est l'inverse de claudine.

## 5. Vérification

Chaque release publie un fichier de sommes. La commande télécharge l'asset **et** sa somme,
calcule le sha256 du fichier reçu et refuse d'installer en cas de divergence.

Sans cette vérification, `update` serait un vecteur d'exécution de code arbitraire déclenché
par une seule commande — pour un outil qui a par ailleurs le droit de supprimer des données.

## 6. Ce que la commande ne fait pas

- **Elle ne s'exécute pas sans que l'utilisateur l'ait tapée.** Aucune vérification
  automatique au lancement du TUI : un outil de nettoyage n'a pas à parler à un serveur de
  release pendant qu'on lui demande de scanner des orgs.
- **Elle ne rétrograde jamais.** `Ahead` s'affiche et s'arrête.
- **Elle ne touche pas à un binaire possédé par un gestionnaire**, même si elle en a
  techniquement les droits.
- Pas de canal *nightly*, pas de pré-release : `releases/latest` ignore déjà ces dernières.

## 7. Dépendance

`ureq = "3"`, comme josephine. Bondebarras embarque déjà `rustls` via octocrab, donc le coût
réel est la couche `ureq` elle-même, pas une seconde pile TLS.

L'alternative — réutiliser octocrab pour la métadonnée — échoue au téléchargement : son
`BaseUriLayer` réécrit l'hôte de toute requête, et l'asset vit sur `objects.githubusercontent.com`
après redirection.

`sha2` pour la somme. Rien d'autre.

## 8. Tests

| Cible | Vérification |
|---|---|
| comparaison | plus récent, égal, **plus ancien → `Ahead`**, pré-release ignorée |
| chemin | `/.cargo/`, `linuxbrew`, `/Cellar/`, `/nix/store/`, `/usr/bin` (→ aucun) |
| plan | chaque canal donne la bonne commande ; les quatre canaux gérés donnent `Manual` |
| sha256 | vecteur connu, fichier vide, fichier plus grand que le tampon de lecture |
| parsing de somme | `<hex>  <nom>` → le premier champ |
| release | JSON réel de l'API ; une release sans asset pour la plateforme → erreur nommée |

Le test qui compte est **`Ahead`**. C'est l'état de la machine aujourd'hui, et sans lui la
commande mentirait dès son premier usage.
