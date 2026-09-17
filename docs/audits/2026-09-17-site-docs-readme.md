> Documentation audit produced on 2026-09-17 against commit `f65b69f` and the
> published release `v1.0.0-rc.1`; issues #19 to #35 track it. Copied verbatim
> below — this header is the only addition.

# Bon Débarras — plan détaillé de refonte du site, de la documentation et du README

> Document de cadrage prêt à être transformé en tickets ou en plan d'implémentation.
>
> Audit vérifié le 17 septembre 2026 sur la branche `main`, commit `f65b69f82046`,
> et sur la release publique `v1.0.0-rc.1`.

## 1. Résultat recherché

Le projet est techniquement mûr, mais trois supports remplissent aujourd'hui des rôles qui se
recouvrent :

- le **site** cherche à convaincre, démontrer, documenter et expliquer les cas limites ;
- le **README** sert à la fois de page d'accueil GitHub, de manuel utilisateur, de référence CLI,
  de documentation de sécurité et de documentation de facturation ;
- le dossier **`docs/`** contient surtout l'historique de conception interne, mais pas encore de
  documentation utilisateur structurée.

La refonte doit donner un rôle clair à chaque support :

| Support | Question à laquelle il répond | Public principal | Niveau de détail |
| --- | --- | --- | --- |
| Site | « Pourquoi utiliser Bon Débarras et à quoi ressemble-t-il ? » | Découverte, partage, référencement | Court, visuel, orienté bénéfices |
| README | « Comment comprendre, installer et essayer le projet en cinq minutes ? » | Visiteur GitHub, futur utilisateur, contributeur | Synthétique, actionnable |
| Documentation | « Comment fonctionne précisément telle commande ou telle règle de sécurité ? » | Utilisateur actif, intégrateur, mainteneur | Exhaustif et durable |
| `docs/superpowers/` | « Pourquoi le produit a-t-il été conçu ainsi ? » | Mainteneur du projet | Historique, non contractuel |

### Objectifs mesurables

1. Un visiteur doit comprendre la promesse en moins de dix secondes.
2. Une méthode d'installation réellement utilisable doit être visible sans faire défiler une longue
   description fonctionnelle.
3. Aucun canal d'installation ne doit être présenté comme disponible avant sa publication effective.
4. Les affirmations concernant GitHub doivent refléter le fonctionnement actuel de GitHub Actions.
5. Une information détaillée ne doit avoir qu'une seule source de vérité.
6. Le README doit permettre un premier lancement sans servir de manuel complet.
7. Le site français et le site anglais doivent rester structurellement synchronisés.

---

## 2. État actuel vérifié

### 2.1 Publication et installation

La release candidate [`v1.0.0-rc.1`](https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.1)
est publiée. Elle contient notamment :

- un exécutable et une archive Windows x86-64 ;
- une archive Linux x86-64 ;
- une archive macOS Apple Silicon ;
- un paquet Debian/Ubuntu AMD64 ;
- un paquet RPM x86-64 ;
- une formule Homebrew générée comme **asset de release** ;
- un `PKGBUILD` généré comme **asset de release**.

En revanche :

- GitHub ne considère pas cette préversion comme une « latest release » stable ;
- la formule `Formula/bondebarras.rb` du dépôt contient encore un SHA-256 nul et n'est donc pas
  installable telle quelle ;
- le workflow exclut volontairement les préversions de la mise à jour du tap Homebrew ;
- la présence d'un `PKGBUILD` dans les assets ne prouve pas qu'un paquet AUR est effectivement
  publié ;
- le workflow exclut également les préversions de la génération/soumission winget stable.

Conséquence : le site peut annoncer la disponibilité de la RC et de ses binaires directs, mais ne
doit pas présenter `brew install bondebarras`, `yay -S bondebarras` ou
`winget install bondebarras` comme des chemins validés tant que les paquets correspondants ne sont
pas réellement publiés.

### 2.2 Limite de cache GitHub Actions

Le projet décrit encore 10 Go comme un plafond fixe par dépôt. La documentation GitHub actuelle
indique plutôt :

- 10 Go est la **limite par défaut** par dépôt ;
- cette limite peut être augmentée par un administrateur autorisé ;
- l'utilisation au-delà de 10 Go est facturable ;
- l'éviction intervient lorsque le dépôt atteint sa **limite configurée**, qui peut donc être
  supérieure à 10 Go ;
- la limite configurée n'est pas exposée par l'API utilisée par Bon Débarras.

Source officielle :
[Dependency caching reference — Usage limits and eviction policy](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#usage-limits-and-eviction-policy).

Le produit peut conserver une jauge par rapport à 10 Go, mais il doit la présenter comme le
**seuil inclus par défaut**, pas comme la limite réelle connue du dépôt.

### 2.3 Métadonnées et identité visuelle

Le site possède déjà :

- plusieurs favicons PNG et ICO ;
- une Apple Touch Icon ;
- une balise `og:image` ;
- `twitter:card=summary_large_image` ;
- une couleur de thème ;
- une version française et une version anglaise.

Le travail restant est donc un enrichissement : canonical, `hreflang`, titres et descriptions
localisés, balises OpenGraph complètes, image sociale au bon ratio, manifeste web et intégration du
nouveau logo carré.

### 2.4 Volumétrie éditoriale

- `README.md` : environ 547 lignes ;
- landing française : environ 223 lignes de contenu ;
- landing anglaise : environ 223 lignes de contenu ;
- documentation utilisateur dédiée : inexistante ;
- documentation de conception : abondante dans `docs/superpowers/`.

Le problème n'est pas le manque de contenu, mais son emplacement et sa hiérarchie.

---

## 3. Architecture documentaire cible

### 3.1 Arborescence proposée

```text
README.md
docs/
├── README.md
├── installation.md
├── authentication.md
├── tui.md
├── cli.md
├── safety.md
├── resources.md
├── billing.md
├── troubleshooting.md
├── releases.md
└── superpowers/
    ├── plans/
    └── specs/
site/
├── content/
│   ├── _index.md
│   └── _index.fr.md
├── templates/
│   ├── base.html
│   └── index.html
└── static/
    ├── brand/
    ├── screenshots/
    ├── favicon.ico
    ├── icon.svg
    ├── icon-*.png
    ├── og-bondebarras-1200x630.png
    ├── site.webmanifest
    └── robots.txt
```

Il n'est pas nécessaire de déplacer immédiatement `docs/superpowers/`. Ajouter une page
`docs/README.md` suffit à expliquer qu'il s'agit d'un historique de conception et non de la
documentation contractuelle du produit.

### 3.2 Source de vérité par information

| Information | Source de vérité | Reprises autorisées |
| --- | --- | --- |
| Positionnement produit | `README.md`, version courte | Résumé encore plus court sur le site |
| Installation détaillée | `docs/installation.md` | Quick start dans le README ; tableau de disponibilité sur le site |
| Canaux disponibles | Release GitHub + workflow de release | Affichage synthétique, sans promesse anticipée |
| Scopes et authentification | `docs/authentication.md` | Liste minimale dans le README |
| Navigation TUI | `docs/tui.md` | Capture + cinq touches essentielles dans le README |
| Référence CLI | `docs/cli.md` | Trois exemples dans le README et sur le site |
| Ressources prises en charge | `docs/resources.md` | Liste courte dans le README |
| Sécurité et réversibilité | `docs/safety.md` | Encadré synthétique dans le README et le site |
| Facturation | `docs/billing.md` | Une phrase et un lien ailleurs |
| Contribution | `CONTRIBUTING.md` | Simple lien dans le README |
| Conventions techniques | `CONVENTIONS.md` | Ne pas dupliquer dans `CONTRIBUTING.md` |
| Historique des choix | `docs/superpowers/` | Aucun copier-coller vers la documentation utilisateur |

### 3.3 Règle éditoriale centrale

Une information détaillée peut être résumée ailleurs, mais elle ne doit pas être recopiée dans
plusieurs variantes longues. Chaque résumé doit renvoyer vers sa source de vérité.

Exemple : la landing peut dire « les suppressions sont classées par niveau de risque et toujours
confirmées ». La définition exacte des niveaux, des familles protégées et des exceptions reste dans
`docs/safety.md`.

---

## 4. Modifications prioritaires communes à tous les supports

## 4.1 P0 — Corriger la présentation de la limite de cache

### Fichiers concernés

- `README.md`
- `site/content/_index.md`
- `site/content/_index.fr.md`
- en cohérence ultérieure avec le produit :
  `crates/bondebarras-core/src/tui/views/gauges.rs`

### Formulations à supprimer

- « plafond de 10 Gio par dépôt » ;
- « GitHub évince dès que le dépôt dépasse 10 Go » ;
- « 10 GiB per-repository ceiling » ;
- toute affirmation impliquant que le pourcentage affiché représente la limite configurée réelle.

### Formulation française proposée

> Bon Débarras compare les caches au seuil inclus par défaut de 10 Go par dépôt. GitHub permet
> d'augmenter la limite réelle : au-delà de 10 Go, le stockage supplémentaire peut être facturé et
> l'éviction ne commence qu'une fois la limite configurée atteinte. Cette limite n'étant pas exposée
> par l'API, la jauge affiche un repère de coût, pas la capacité exacte du dépôt.

### Formulation anglaise proposée

> Bon Débarras compares cache usage with GitHub's default included threshold of 10 GB per
> repository. The actual limit can be increased, in which case usage above 10 GB may be billed and
> eviction starts only when the configured limit is reached. GitHub does not expose that configured
> limit through the API, so the gauge is a cost threshold, not the repository's known capacity.

### Libellés courts proposés

- jauge FR : `Cache · seuil inclus 10 Go` ;
- aide FR : `limite réelle non exposée par l'API` ;
- jauge EN dans la documentation : `Cache · 10 GB included threshold` ;
- avertissement au-dessus de 10 Go : `⚠ dépasse le seuil inclus ; facturation ou éviction selon la limite configurée`.

### Critères d'acceptation

- aucune occurrence publique ne qualifie plus 10 Go de plafond fixe ;
- le site et le README distinguent seuil inclus, limite configurée, facturation et éviction ;
- un lecteur ne peut pas conclure qu'un dépôt à 11 Go est nécessairement déjà en train d'évincer ;
- la documentation officielle GitHub est liée depuis `docs/billing.md`.

## 4.2 P0 — Rendre l'installation exacte et dépendante de l'état de publication

### Fichiers concernés

- `README.md`
- `site/content/_index.md`
- `site/content/_index.fr.md`
- `Formula/bondebarras.rb`
- `docs/installation.md`
- éventuellement `.github/workflows/release.yml` pour automatiser les mises à jour futures.

### Changement immédiat

Présenter deux niveaux distincts :

1. **Disponible maintenant — v1.0.0-rc.1** : binaires et paquets attachés à la release, plus
   installation depuis les sources ;
2. **À partir de la première version stable** : tap Homebrew, AUR et winget, uniquement après
   vérification de leur publication réelle.

### Bloc français proposé pour le site

```text
Essayer la release candidate

v1.0.0-rc.1 est disponible pour Windows x86-64, Linux x86-64,
Debian/Ubuntu AMD64, Fedora/RHEL x86-64 et macOS Apple Silicon.

[Télécharger la RC] [Installer depuis les sources]

cargo install --git https://github.com/systm-d/bondebarras bondebarras

Homebrew, AUR et winget seront annoncés ici après la première publication
stable de chaque paquet.
```

### Bloc anglais proposé

```text
Try the release candidate

v1.0.0-rc.1 is available for Windows x86-64, Linux x86-64,
Debian/Ubuntu AMD64, Fedora/RHEL x86-64, and Apple Silicon Macs.

[Download the RC] [Install from source]

cargo install --git https://github.com/systm-d/bondebarras bondebarras

Homebrew, AUR, and winget will be listed here after the first stable package
has actually been published through each channel.
```

### Règles d'affichage

- Ne pas afficher `brew install` tant que `Formula/bondebarras.rb` contient un checksum nul ou
  pointe vers une préversion non servie par le tap.
- Ne pas afficher `yay -S` tant que la page AUR existe réellement et que l'installation a été
  testée sur une machine propre.
- Ne pas afficher `winget install` tant que le manifeste a été accepté dans `winget-pkgs`.
- Pour `.deb` et `.rpm`, montrer d'abord où télécharger le fichier ; une commande `dpkg -i` seule
  n'explique pas comment l'obtenir.
- Marquer clairement la RC comme une préversion.
- Une fois `v1.0.0` publiée, faire du canal stable le chemin principal et garder la compilation
  depuis les sources comme solution secondaire.

### Traitement de `Formula/bondebarras.rb`

Deux options acceptables :

1. remettre le fichier à l'état de gabarit clairement non publiable et ne jamais le présenter comme
   un tap utilisable avant `v1.0.0` ;
2. supprimer `Formula/bondebarras.rb` avant la première stable et laisser le workflow le créer avec
   une URL et un checksum valides.

La seconde option réduit le risque qu'un utilisateur exécute une formule invalide trouvée dans le
dépôt.

### Critères d'acceptation

- chaque commande visible a été exécutée avec succès sur le canal qu'elle prétend utiliser ;
- les plateformes et architectures sont explicites ;
- la RC n'est jamais confondue avec une version stable ;
- le site et le README pointent vers la page de release, pas vers un nom de fichier supposé ;
- aucun checksum nul n'est présent dans un fichier présenté comme installable.

---

## 5. Refonte détaillée du README

## 5.1 Rôle cible

Le README doit fonctionner comme une page d'accueil GitHub et un quick start. Il ne doit plus
contenir toute la spécification fonctionnelle.

Longueur cible :

- environ 180 à 250 lignes au total ;
- les informations essentielles avant la première longue table ;
- moins de 60 secondes entre l'arrivée sur la page et la première commande exécutable.

## 5.2 Ordre cible des sections

```text
1. Logo + phrase de positionnement
2. Badges utiles
3. Capture réelle du TUI
4. Pourquoi / problème résolu
5. Quick start
6. Ce que l'outil sait nettoyer
7. Sécurité
8. Exemples CLI
9. Documentation
10. Contribuer
11. Licence
```

## 5.3 En-tête proposé

```markdown
<p align="center">
  <img src="resources/logo-h.png" alt="Bon Débarras" width="520">
</p>

<p align="center">
  Audit and safely clean up the GitHub resources your CI leaves behind.
</p>

Bon Débarras is a Rust TUI and CLI for seeing what accumulates across the
GitHub organizations you can access, identifying resources that are safe to
remove, and cleaning them up with an explicit confirmation.
```

Le sous-titre doit parler du résultat utilisateur avant d'énumérer les familles de ressources.

## 5.4 Badges

Conserver :

- CI ;
- dernière release ou prérelease, avec un libellé explicite ;
- licence ;
- plateformes si ce badge est automatiquement fiable.

Retirer ou reléguer :

- le badge Pages si son information n'aide pas un utilisateur à décider d'installer l'outil ;
- tout badge purement décoratif ou non maintenu automatiquement.

## 5.5 Capture produit

Remplacer le croquis texte présenté comme tel par une vraie capture dans le premier écran du README.

Fichier recommandé : `resources/screenshots/tui-overview.png`.

La capture doit :

- utiliser des organisations et dépôts fictifs ou anonymisés ;
- montrer les trois colonnes ;
- montrer au moins un cache de PR fermée marqué `⚑` et `⛑` ;
- montrer le seuil cache avec la nouvelle terminologie ;
- rester lisible à environ 1000 px de large ;
- ne contenir aucun jeton, email, nom de client ou donnée confidentielle.

Texte alternatif proposé :

> Bon Débarras TUI showing organizations, repositories, cache usage and resources classified by
> cleanup safety.

## 5.6 Section « Why »

Réduire les paragraphes actuels à deux idées :

1. GitHub disperse les ressources entre de nombreux dépôts et écrans ;
2. Bon Débarras consolide l'inventaire et détecte les suppressions les plus sûres, notamment les
   caches attachés aux références de pull requests fermées.

Les chiffres personnels `51.4 GB`, `69 caches`, `11.1 GB` peuvent être conservés comme exemple,
mais dans un bloc « real-world example » clairement daté. Ils ne doivent pas ressembler à une mesure
universelle ni à un benchmark reproductible.

## 5.7 Quick start

Le quick start doit arriver avant la description exhaustive des fonctionnalités.

Pour la RC actuelle :

````markdown
## Quick start

Bon Débarras currently ships as a release candidate. Download the package for
your platform from [v1.0.0-rc.1][release], or install the current source:

```sh
cargo install --git https://github.com/systm-d/bondebarras bondebarras
bondebarras
```

Bon Débarras uses the token returned by `gh auth token`, then falls back to
`GITHUB_TOKEN`. See [Authentication](docs/authentication.md) for permissions.
````

Après la stable, remplacer la première option par le canal de paquets réellement recommandé.

## 5.8 Fonctionnalités

Remplacer l'actuelle liste très longue par six points courts :

- inventaire multi-organisation et chargement à la demande ;
- caches, artifacts et workflow runs ;
- versions GHCR, branches mergées, tags et assets de releases ;
- archivage manuel et réversible des dépôts ;
- classification de sécurité et protections contre les suppressions de masse dangereuses ;
- visibilité sur les minutes, le stockage, les budgets et la rétention GitHub Actions.

Chaque point doit renvoyer vers `docs/resources.md`, `docs/safety.md` ou `docs/billing.md` au lieu
d'expliquer tous les cas limites dans le README.

## 5.9 Sécurité

Conserver un tableau court :

| Marqueur | Sens | Sélection en masse |
| --- | --- | --- |
| `⛑` | Suppression considérée sûre par les règles du produit | Oui avec `A` |
| `•` | Élément à vérifier | Oui avec `V` |
| aucun | Élément à conserver par défaut | Non |

Ajouter immédiatement les quatre garanties essentielles :

- confirmation avant toute mutation ;
- aucune corbeille pour les suppressions GitHub ;
- ressources protégées exclues des sélections de masse ;
- archivage d'un dépôt uniquement depuis le TUI et jamais en mode headless.

Puis renvoyer vers `docs/safety.md` pour les exceptions détaillées.

Éviter la formulation absolue « safe — nothing live references it » si GitHub ne permet pas de
prouver toutes les références possibles. Préférer :

> Safe according to Bon Débarras' documented rules; review the plan before confirming.

## 5.10 Usage TUI et CLI

Le README doit conserver :

- `bondebarras` ;
- un exemple `scan --json` ;
- un exemple `clean` sans `--yes` pour montrer le dry run ;
- un exemple avec `--yes` accompagné d'un avertissement explicite.

Déplacer vers `docs/tui.md` :

- l'intégralité de la table des raccourcis ;
- les seuils de repli responsive ;
- le délai de 300 ms ;
- les détails du footer à chaque largeur.

Déplacer vers `docs/cli.md` :

- la table complète des options ;
- le schéma JSON ;
- les règles exactes de chaque famille headless ;
- les détails de `update` et de la détection du canal d'installation.

## 5.11 Facturation

Remplacer le bloc actuel très long par un paragraphe :

> The Billing tab reads GitHub Actions minutes, storage, budgets, and retention settings to show
> where usage comes from. It is diagnostic and read-only: Bon Débarras never changes a budget or a
> retention policy. See [Billing and GitHub limits](docs/billing.md).

Déplacer les plans, quotas, GB-heures, règles public/privé, budgets bloquants, Enterprise partagé et
cas d'API illisible dans `docs/billing.md`.

## 5.12 Documentation et contribution

Ajouter un index explicite :

```markdown
## Documentation

- [Installation](docs/installation.md)
- [Authentication and permissions](docs/authentication.md)
- [Using the TUI](docs/tui.md)
- [CLI reference](docs/cli.md)
- [Safety model](docs/safety.md)
- [Supported resources](docs/resources.md)
- [Billing and GitHub limits](docs/billing.md)
- [Troubleshooting](docs/troubleshooting.md)
```

La section développement ne doit plus recopier toute la quality gate. Elle peut dire :

```markdown
See [CONTRIBUTING.md](CONTRIBUTING.md) for setup and pull requests, and
[CONVENTIONS.md](CONVENTIONS.md) for the project's quality gate.
```

## 5.13 Critères d'acceptation du README

- la capture et le quick start sont visibles avant les détails ;
- l'installation proposée fonctionne réellement ;
- aucune section ne dépasse environ deux écrans sans renvoyer vers la documentation ;
- la référence complète des touches et options a quitté le README ;
- tous les liens relatifs fonctionnent sur GitHub ;
- le README reste en anglais, conformément à `CONVENTIONS.md` ;
- les exemples sont cohérents avec `bondebarras --help` ;
- le README ne contient plus d'affirmation obsolète sur la limite de cache.

---

## 6. Création de la documentation utilisateur

## 6.1 `docs/README.md` — index

Contenu :

- une phrase expliquant que ce dossier contient la documentation utilisateur et l'historique de
  conception ;
- deux entrées visuelles ou deux listes distinctes : « User documentation » et « Design history » ;
- les liens vers toutes les pages utilisateur ;
- un avertissement indiquant que `docs/superpowers/` décrit des décisions prises à une date donnée
  et peut ne plus représenter le comportement actuel.

## 6.2 `docs/installation.md`

Sections :

1. statut actuel : release candidate ou stable ;
2. tableau plateforme / architecture / format / état ;
3. vérification SHA-256 ;
4. Linux générique ;
5. Debian/Ubuntu ;
6. Fedora/RHEL ;
7. macOS Apple Silicon ;
8. Windows x86-64 ;
9. compilation depuis les sources ;
10. canaux pas encore disponibles ;
11. désinstallation ;
12. mise à jour avec `bondebarras update`.

Le tableau doit utiliser des états explicites : `Available`, `Pre-release only`, `Planned`,
`Not published`. Ne pas utiliser une coche pour un canal non testé.

## 6.3 `docs/authentication.md`

Sections :

1. ordre de résolution : `gh auth token`, puis `GITHUB_TOKEN` ;
2. installation et authentification de GitHub CLI ;
3. scopes minimaux d'un token classique ;
4. rôle de chaque scope ;
5. permission optionnelle `admin:org` ;
6. rôles nécessaires pour les budgets ;
7. comportement lorsque certaines données sont illisibles ;
8. exemple de diagnostic ;
9. bonnes pratiques : ne jamais copier le token dans une commande partagée ou une capture.

Ajouter un tableau `Feature → permission → behavior if missing`.

## 6.4 `docs/tui.md`

Sections :

1. lancement ;
2. anatomie des trois colonnes ;
3. chargement différé d'un dépôt ;
4. navigation ;
5. tri et filtre ;
6. sélection ;
7. confirmation ;
8. progression et erreurs ;
9. onglet Billing ;
10. terminaux étroits ;
11. raccourcis complets.

Utiliser une vraie capture annotée. Le texte doit expliquer les symboles `⛑`, `•`, `⚑`, `⚠`,
`✓` et `✗`.

## 6.5 `docs/cli.md`

Sections :

1. synopsis généré ou recopié depuis `--help` ;
2. `scan` ;
3. sortie JSON et stabilité du schéma ;
4. `clean` ;
5. dry run par défaut ;
6. sélection des familles ;
7. filtres `--stale-pr` et `--older-than` ;
8. comportement de `--yes` ;
9. opérations volontairement impossibles en headless ;
10. `update` et `update --check` ;
11. codes de sortie ;
12. exemples cron avec recommandations de journalisation.

La documentation doit annoncer si le schéma JSON est stable, expérimental ou susceptible de
changer avant `1.0.0`.

## 6.6 `docs/safety.md`

Sections :

1. philosophie générale ;
2. définition des niveaux de sécurité ;
3. tableau complet `ResourceKind → classification → protection → réversibilité` ;
4. différence entre sélection individuelle et sélection de masse ;
5. différences TUI / headless ;
6. confirmations Tier 1 / Tier 2 ;
7. suppression irréversible ;
8. archivage réversible ;
9. rate limit, retries et résultat par élément ;
10. limites du modèle : « sûr selon les règles documentées » n'est pas une garantie absolue.

Cette page devient la référence contractuelle de toutes les promesses de sécurité faites sur le site
et dans le README.

## 6.7 `docs/resources.md`

Créer un tableau par famille :

| Ressource | Taille connue | Suppression unitaire | Sélection de masse | Critère principal | Réversible |
| --- | --- | --- | --- | --- | --- |
| Cache Actions | Oui | Oui | Oui | PR fermée/mergée, âge | Non, mais régénérable |
| Artifact | Oui | Oui | Oui | Expiration, âge | Non, mais régénérable |
| Workflow run | Partielle selon l'API | Oui | Oui | Âge | Non, mais relançable |
| Version GHCR | Non | Oui | Uniquement non taguée/orpheline | Tags et attestations | Non |
| Branche | Non | Oui | Seulement branche issue d'une PR mergée | Merge + protections | Non |
| Tag | Non | Oui | Non | Toujours protégé en masse | Non |
| Asset de release | Oui | Oui | Oui selon règles | Âge/release | Non |
| Dépôt | Aucun gain direct | Archivage uniquement | Jamais | Choix humain | Oui |

Vérifier chaque cellule avec le code avant publication. Le tableau ci-dessus décrit la structure
attendue, pas un substitut à ce contrôle.

## 6.8 `docs/billing.md`

Sections :

1. ce que Bon Débarras lit ;
2. ce qu'il ne modifie jamais ;
3. minutes GitHub Actions ;
4. stockage et GB-heures ;
5. distinction artifacts, Packages et cache ;
6. seuil de cache inclus par défaut de 10 Go ;
7. limite configurée non exposée par l'API ;
8. plans inconnus et absence de pourcentage ;
9. organisations Enterprise et quotas partagés ;
10. budgets Actions ;
11. rétention ;
12. limites et dates de validité des chiffres.

Ajouter en tête :

> GitHub pricing and limits can change. This page describes what Bon Débarras displays and links to
> GitHub's current documentation; GitHub remains the source of truth for billing.

Lier au minimum :

- [GitHub Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions) ;
- [Dependency caching reference](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#usage-limits-and-eviction-policy).

## 6.9 `docs/troubleshooting.md`

Entrées recommandées :

- aucune organisation visible ;
- erreur 401/403 ;
- billing illisible ;
- budgets illisibles ;
- rétention illisible ;
- dépôt visible mais non archivable ;
- package sans taille ;
- limites GitHub secondaires et retries ;
- terminal trop petit ;
- mise à jour refusée à cause du checksum ;
- différence entre zéro et donnée indisponible dans le JSON.

Chaque entrée doit suivre `Symptom → Cause → Resolution → Safety note`.

## 6.10 `docs/releases.md`

Sections :

- différence stable / préversion ;
- formats produits par le workflow ;
- architectures prises en charge ;
- checksums ;
- politique Homebrew/AUR/winget ;
- lien vers `CHANGELOG.md` ;
- politique SemVer et éventuelles garanties du JSON avant/après 1.0.

## 6.11 Critères d'acceptation de la documentation

- toutes les pages sont reliées depuis `docs/README.md` et le README racine ;
- aucun lien cassé ;
- aucun chapitre utilisateur n'est caché dans `docs/superpowers/` ;
- les commandes ont été validées sur la RC ou explicitement marquées comme futures ;
- les pages distinguent le comportement du produit des règles susceptibles de changer chez GitHub ;
- toute affirmation de facturation renvoie à une source officielle ;
- les titres et ancres restent stables ;
- les exemples n'exposent aucune donnée réelle sensible.

---

## 7. Refonte détaillée du site

## 7.1 Positionnement

Conserver l'identité actuelle : fond sombre, orange terre cuite, typographie terminal et légèreté
technique. La refonte porte sur la hiérarchie, la densité et la preuve visuelle, pas sur un changement
complet de direction artistique.

## 7.2 Nouvelle séquence de la landing

```text
1. Hero : promesse + statut RC + deux CTA
2. Vraie capture du TUI
3. Trois bénéfices
4. Ressources prises en charge
5. Sécurité
6. CLI en trois commandes
7. Installation et disponibilité par plateforme
8. Liens vers la documentation et GitHub
9. Footer
```

Cette structure remplace :

- le diagramme très textuel du flux ;
- les six cartes contenant chacune un mini-chapitre ;
- les deux faux terminaux détaillés ;
- le long bloc de facturation ;
- le bloc d'installation qui mélange canaux disponibles et futurs.

Objectif de réduction : environ 40 à 50 % du texte visible.

## 7.3 Hero

### Contenu français proposé

**Titre :**

> Voyez ce que GitHub accumule. Nettoyez sans mauvaise surprise.

**Sous-titre :**

> Bon Débarras réunit les ressources oubliées de vos organisations GitHub dans une interface
> terminal, repère les suppressions les plus sûres et vous laisse confirmer chaque action.

**Statut :**

> Release candidate `v1.0.0-rc.1`

**CTA principal :** `Télécharger la RC`

**CTA secondaire :** `Voir sur GitHub`

**Lien tertiaire :** `Lire la documentation`

### Contenu anglais proposé

**Title:**

> See what GitHub accumulates. Clean it up without surprises.

**Subtitle:**

> Bon Débarras brings forgotten resources from your GitHub organizations into one terminal UI,
> flags the safest cleanup candidates, and lets you confirm every action.

**Status:** `Release candidate v1.0.0-rc.1`

**Primary CTA:** `Download the RC`

**Secondary CTA:** `View on GitHub`

**Tertiary link:** `Read the documentation`

Éviter « every organization you own », car le produit travaille sur les organisations visibles par
le jeton, pas seulement celles juridiquement ou techniquement possédées par l'utilisateur.

## 7.4 Démonstration réelle

Ajouter immédiatement après le hero :

- une capture réelle du TUI en PNG ou WebP ;
- éventuellement un enregistrement de 8 à 12 secondes, sans lecture automatique sonore ;
- une légende en trois points : `scan → inspect → confirm` ;
- un lien vers `docs/tui.md`.

Garder le site sans JavaScript est possible : une capture WebP suffit pour la première version. Si
une animation est ajoutée, fournir une image fixe de remplacement et respecter
`prefers-reduced-motion`.

## 7.5 Trois bénéfices au lieu de six chapitres

### 1. Tout voir au même endroit

> Organisations, dépôts et ressources sur un seul écran, avec les détails chargés seulement quand
> vous en avez besoin.

### 2. Commencer par le plus sûr

> Les caches de PR fermées et les autres candidats sont classés par niveau de sécurité. Les éléments
> protégés restent exclus des sélections de masse.

### 3. Comprendre avant de supprimer

> Taille, âge, origine, coût et confirmation explicite : chaque mutation reste un choix humain et
> chaque résultat est rapporté individuellement.

La facturation devient un bénéfice secondaire dans la troisième carte ou une ligne distincte, pas
une carte contenant une douzaine de règles.

## 7.6 Ressources prises en charge

Afficher une grille compacte :

- caches Actions ;
- artifacts ;
- workflow runs ;
- versions GHCR ;
- branches mergées ;
- tags ;
- assets de releases ;
- archivage de dépôts.

Ajouter une note :

> Bon Débarras ne supprime jamais une release ni un dépôt. Un dépôt peut uniquement être archivé
> manuellement, et l'archivage est réversible.

## 7.7 Bloc sécurité

Conserver un bloc très visible avec quatre engagements :

- aucune mutation sans confirmation ;
- aucune sélection massive d'une ressource protégée ;
- aucune promesse d'annulation pour une suppression GitHub ;
- aucun archivage de dépôt en mode headless.

CTA : `Comprendre le modèle de sécurité` vers `docs/safety.md`.

## 7.8 Bloc CLI

Limiter à trois commandes :

```sh
bondebarras
bondebarras scan --org systm-d --json
bondebarras clean --org systm-d --repo josephine --caches --stale-pr
```

La troisième commande est volontairement sans `--yes` : le site doit montrer le comportement sûr
par défaut. Une courte note peut préciser que `--yes` est réservé à l'automatisation assumée.

## 7.9 Bloc installation

Afficher :

- le statut de version ;
- un bouton vers la release ;
- un tableau plateforme / format ;
- une seule commande source universelle ;
- un lien vers `docs/installation.md` ;
- les canaux futurs dans une zone explicitement intitulée « pas encore publiés ».

Ne pas afficher cinq commandes sur le même plan si seules deux sont immédiatement exécutables.

## 7.10 Intégration du logo

Fichiers recommandés :

```text
resources/brand/bondebarras-horizontal.png
resources/brand/bondebarras-horizontal.svg
resources/brand/bondebarras-square.png
resources/brand/bondebarras-square.svg
site/static/brand/bondebarras-horizontal.svg
site/static/brand/bondebarras-square.svg
```

Utilisation :

- logo horizontal dans le hero et le README ;
- logo carré pour favicon, Apple Touch Icon et avatar ;
- version carrée centrée dans l'image sociale ;
- fond réellement transparent pour les PNG et SVG ;
- aucun texte minuscule dans le logo carré.

Conserver les exports raster nécessaires, mais garder le SVG comme source principale pour le site.

## 7.11 SEO et partage social

### Fichier concerné

`site/templates/base.html`

### Éléments à ajouter ou compléter

```html
<link rel="canonical" href="{{ current_url | safe }}">
<link rel="alternate" hreflang="en" href="...">
<link rel="alternate" hreflang="fr" href="...">
<link rel="alternate" hreflang="x-default" href="...">

<meta property="og:type" content="website">
<meta property="og:site_name" content="Bon Débarras">
<meta property="og:title" content="...">
<meta property="og:description" content="...">
<meta property="og:url" content="...">
<meta property="og:locale" content="...">
<meta property="og:image" content=".../og-bondebarras-1200x630.png">
<meta property="og:image:width" content="1200">
<meta property="og:image:height" content="630">
<meta property="og:image:alt" content="...">

<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="...">
<meta name="twitter:description" content="...">
<meta name="twitter:image" content=".../og-bondebarras-1200x630.png">
```

Les titres doivent être localisés :

- FR : `Bon Débarras — Nettoyer les ressources GitHub oubliées` ;
- EN : `Bon Débarras — Clean up forgotten GitHub resources`.

L'image actuelle `logo-h@2x.png` mesure 1520 × 442 : elle est trop panoramique pour une carte
sociale large. Créer un visuel dédié 1200 × 630 avec logo, promesse courte et aperçu du TUI.

## 7.12 Favicon et manifeste

Ajouter :

- `icon.svg` à partir du logo carré ;
- `site.webmanifest` avec nom, nom court, couleurs et icônes 192/512 ;
- `mask-icon` si un SVG monochrome convaincant existe ;
- `robots.txt` pointant vers le sitemap Zola ;
- une vérification que les icônes ont un vrai alpha et restent lisibles à 16 × 16.

## 7.13 Accessibilité

### Contraste

La variable `--dim: #766b62` est trop faible pour de petits textes sur plusieurs fonds sombres.
Remplacer par une valeur proche de `#9b8f85`, puis vérifier le contraste sur :

- `--bg` ;
- `--panel` ;
- `--panel-2` ;
- `--bar`.

L'objectif est au minimum 4,5:1 pour le texte normal.

### Terminal et lecteurs d'écran

- une capture illustrative doit utiliser un `alt` synthétique et ne pas faire lire chaque caractère ;
- un faux terminal HTML purement décoratif doit être `aria-hidden="true"` et accompagné d'un résumé ;
- une vraie commande utile doit rester dans `<pre><code>` et être accessible ;
- ne pas transmettre une information seulement par la couleur ;
- conserver un focus clavier visible sur les liens et boutons ;
- ajouter un lien d'évitement `Aller au contenu` / `Skip to content` ;
- tester le sélecteur de langue avec un lecteur d'écran ;
- conserver `prefers-reduced-motion`.

## 7.14 Performance

La base actuelle est excellente : pas de framework frontend, pas de JavaScript et polices système.
Préserver ces choix.

Pour les nouveaux médias :

- WebP ou PNG optimisé ;
- dimensions explicites pour éviter les déplacements de mise en page ;
- `loading="lazy"` sous la ligne de flottaison ;
- image du hero préchargée seulement si son poids le justifie ;
- objectif indicatif : moins de 500 Ko pour la page initiale hors vidéo ;
- aucune bibliothèque ajoutée uniquement pour copier une commande ou animer une carte.

## 7.15 Critères d'acceptation du site

- la promesse, une capture et une installation valide sont visibles dans les deux premiers écrans ;
- le texte visible est réduit d'au moins 40 % sans perdre les liens vers le détail ;
- les versions FR et EN ont exactement les mêmes sections ;
- les deux CTA principaux mènent à des destinations réelles ;
- aucune commande Homebrew/AUR/winget non publiée n'est présentée comme disponible ;
- canonical, `hreflang`, OpenGraph et Twitter Card sont complets ;
- l'image sociale est en 1200 × 630 ;
- le contraste du texte normal atteint 4,5:1 ;
- le site reste utilisable sans JavaScript ;
- `zola check` et `zola build` réussissent ;
- la navigation clavier et le rendu mobile sont vérifiés.

---

## 8. Cohérence entre français et anglais

Le projet conserve une landing bilingue, mais la documentation technique peut rester en anglais dans
un premier temps, conformément aux conventions du dépôt.

Pour éviter les divergences :

1. garder la même séquence de sections dans `_index.md` et `_index.fr.md` ;
2. utiliser les mêmes identifiants de sections ;
3. centraliser dans `config.toml` les données non linguistiques : URL du dépôt, URL de release,
   version courante, couleur de marque, chemins d'assets ;
4. ne pas traduire manuellement les nombres de version ou noms de fichiers ;
5. ajouter une vérification CI légère comparant les titres/identifiants structurels des deux pages ;
6. ouvrir un ticket de traduction pour chaque modification éditoriale, dans la même PR.

---

## 9. Automatisation et garde-fous

## 9.1 Contrôles CI recommandés

Ajouter un job documentation/site exécutant :

```sh
zola check --root site
zola build --root site
markdownlint README.md docs/*.md CONTRIBUTING.md CONVENTIONS.md
lychee README.md docs/*.md site/content/*.md
```

Adapter la syntaxe exacte aux versions retenues dans le workflow.

Ajouter ensuite, si l'effort est acceptable :

- validation HTML ;
- audit axe-core sur la page construite ;
- vérification que les fichiers déclarés dans les métadonnées existent ;
- test empêchant la publication d'une formule avec un SHA-256 entièrement nul ;
- test empêchant l'affichage d'un canal « available » sans URL de publication associée ;
- contrôle que `CONTRIBUTING.md` utilise exactement la quality gate de `CONVENTIONS.md`.

## 9.2 Éviter les versions codées en dur

La landing ne doit pas rester bloquée sur `v1.0.0-rc.1` après la stable.

Solutions possibles, par ordre de simplicité :

1. mettre la version dans `site/config.toml` et la modifier dans la PR de release ;
2. générer un petit fichier de données Zola depuis le workflow de release ;
3. interroger GitHub côté build, jamais côté navigateur.

Ne pas ajouter un appel JavaScript à l'API GitHub au chargement de la page : cela rendrait la landing
dépendante d'une API externe, introduirait une limite de débit et dégraderait une page actuellement
statique.

## 9.3 Documentation CLI

À terme, générer ou vérifier une partie de `docs/cli.md` depuis `bondebarras --help` pour réduire la
dérive. Une approche simple consiste à enregistrer la sortie attendue dans un bloc délimité et à la
mettre à jour en CI ou via une tâche de maintenance explicite.

---

## 10. Backlog proposé

## Lot 1 — Exactitude avant communication

### Ticket 1 — Corriger la sémantique du seuil cache

- priorité : P0 ;
- fichiers : README, landing FR/EN, documentation Billing ;
- dépendance recommandée : aligner ensuite le libellé TUI ;
- résultat : 10 Go est présenté comme seuil inclus par défaut, limite réelle inconnue.

### Ticket 2 — Corriger les canaux d'installation

- priorité : P0 ;
- fichiers : README, landing FR/EN, `Formula/bondebarras.rb` ;
- résultat : seule la RC réellement publiée et l'installation source sont promues ;
- validation : test sur une machine ou un conteneur propre par plateforme disponible.

### Ticket 3 — Clarifier le statut de préversion

- priorité : P0 ;
- résultat : badge, hero et quick start affichent tous `v1.0.0-rc.1 — pre-release` ;
- sortie du ticket : chemin clair pour remplacer ce statut lors de `v1.0.0`.

## Lot 2 — Nouvelle documentation

### Ticket 4 — Créer l'index et le squelette des pages

- priorité : P1 ;
- fichiers : `docs/README.md` et huit pages utilisateur ;
- résultat : navigation complète, même avant rédaction exhaustive.

### Ticket 5 — Extraire sécurité, ressources et authentification du README

- priorité : P1 ;
- résultat : pages de référence complètes et README raccourci.

### Ticket 6 — Extraire TUI, CLI et facturation

- priorité : P1 ;
- résultat : manuel utilisateur structuré, détails supprimés du README.

### Ticket 7 — Ajouter installation, releases et dépannage

- priorité : P1 ;
- résultat : guide opérationnel complet pour la RC puis la stable.

## Lot 3 — Refonte du README

### Ticket 8 — Réécrire le README en quick start

- priorité : P1 ;
- dépendance : tickets 4 à 7 ;
- résultat : 180 à 250 lignes, capture réelle, installation avant les détails.

### Ticket 9 — Aligner contribution et conventions

- priorité : P1 ;
- changement immédiat : ajouter `--all-targets` à la commande clippy de la checklist PR dans
  `CONTRIBUTING.md`, ou ne plus la recopier et renvoyer vers `CONVENTIONS.md` ;
- résultat : une seule quality gate.

## Lot 4 — Refonte de la landing

### Ticket 10 — Remplacer la structure éditoriale

- priorité : P1 ;
- résultat : hero, capture, trois bénéfices, sécurité, CLI, installation, documentation.

### Ticket 11 — Ajouter une vraie démonstration du TUI

- priorité : P1 ;
- résultat : capture anonymisée et optimisée, utilisée sur le site et dans le README.

### Ticket 12 — Intégrer la nouvelle identité

- priorité : P1 ;
- résultat : logo horizontal + carré, exports propres, favicons, avatar et hero cohérents.

### Ticket 13 — Compléter SEO, social et accessibilité

- priorité : P1 ;
- résultat : metadata complètes, OG 1200 × 630, contraste corrigé, lien d'évitement.

## Lot 5 — Pérennisation

### Ticket 14 — Ajouter les contrôles CI documentation/site

- priorité : P2 ;
- résultat : build Zola, liens, Markdown et checksum placeholder vérifiés automatiquement.

### Ticket 15 — Centraliser les données de release du site

- priorité : P2 ;
- résultat : version, statut et URL déclarés une seule fois.

### Ticket 16 — Clarifier la licence pour GitHub

- priorité : P2 ;
- vérifier la détection actuelle de la licence par GitHub ;
- si nécessaire, ajouter un fichier `LICENSE` court expliquant le dual licensing et renvoyant vers
  `LICENSE-MIT` et `LICENSE-APACHE` ;
- ne pas remplacer les deux textes de licence complets.

---

## 11. Ordre d'implémentation recommandé

### Étape 1 — Exactitude

1. corriger le cache 10 Go ;
2. corriger les canaux d'installation ;
3. réparer ou retirer la formule racine invalide ;
4. clarifier partout le statut RC.

Cette étape doit être publiée avant toute campagne de communication.

### Étape 2 — Documentation

1. créer le squelette ;
2. déplacer la sécurité et l'authentification ;
3. déplacer la référence TUI/CLI ;
4. déplacer la facturation ;
5. ajouter installation, releases et dépannage.

### Étape 3 — README

1. intégrer le logo ;
2. ajouter la vraie capture ;
3. réécrire le quick start ;
4. remplacer les blocs longs par des résumés et liens ;
5. vérifier tous les exemples.

### Étape 4 — Site

1. intégrer le nouveau hero ;
2. ajouter la capture ;
3. réduire les fonctionnalités à trois bénéfices ;
4. refaire le bloc installation ;
5. intégrer les logos et l'image sociale ;
6. compléter metadata et accessibilité ;
7. synchroniser FR/EN.

### Étape 5 — Automatisation

1. ajouter les contrôles Zola/Markdown/liens ;
2. centraliser la version de release ;
3. ajouter les garde-fous sur les formules/checksums ;
4. documenter le processus de mise à jour lors d'une release stable.

---

## 12. Définition de terminé globale

La refonte est terminée lorsque :

- le site, le README et la documentation n'annoncent que des canaux réellement disponibles ;
- le comportement des caches est décrit conformément à la documentation GitHub actuelle ;
- la landing montre le vrai produit et non seulement une reconstitution ;
- le README permet un premier lancement en moins de cinq minutes ;
- la documentation exhaustive existe hors du README ;
- les pages FR et EN ont la même structure et les mêmes faits ;
- le nouveau logo horizontal et le logo carré sont utilisés aux bons emplacements ;
- l'image sociale est dédiée, lisible et au format 1200 × 630 ;
- le contraste, la navigation clavier et les alternatives textuelles ont été vérifiés ;
- les builds et les liens sont contrôlés en CI ;
- aucune information détaillée n'est maintenue manuellement dans trois endroits différents.

---

## 13. Résumé décisionnel

La bonne direction n'est pas une refonte graphique radicale. L'identité terminal sombre fonctionne
déjà très bien. Le chantier prioritaire consiste à :

1. **corriger les faits** — cache et canaux d'installation ;
2. **séparer les rôles** — site pour convaincre, README pour démarrer, docs pour approfondir ;
3. **montrer le vrai produit** — capture réelle du TUI ;
4. **réduire la densité** — environ moitié moins de texte sur la landing ;
5. **éviter la dérive** — sources de vérité et contrôles CI.

Une fois ces changements réalisés, Bon Débarras donnera la même impression à l'extérieur que dans
son code : un outil précis, prudent, léger et déjà très abouti.
