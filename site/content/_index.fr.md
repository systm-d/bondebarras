+++
[extra]
name = "Bon Débarras"
tagline = "Good riddance."
lede = "Voyez ce que GitHub accumule. Nettoyez sans mauvaise surprise — bondebarras réunit les ressources oubliées de toutes les organisations accessibles au jeton dans une seule interface terminal, repère les suppressions les plus sûres, et ne supprime rien sans confirmation."
cta = "Voir sur GitHub"
cta2 = "Installer"
cta3 = "Lire la documentation"
status = "v1.0.0-rc.3 — préversion"
status_url = "https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.3"
status_note = "Pas encore de version stable : complet et sûr à l'usage — rien n'est jamais supprimé sans confirmation — mais les drapeaux de la CLI et le schéma JSON peuvent encore changer avant la v1.0.0, et Homebrew, l'AUR et winget ne sont pas publiés."
+++

<section id="demo" class="preview">
<h2>Un seul écran, entièrement au clavier</h2>
<p class="section-lede">Trois colonnes, toujours à l'écran — les organisations, les dépôts de l'org courante et les ressources du dépôt sélectionné — les caches marqués sautent aux yeux.</p>
<!-- #30: a real TUI screenshot belongs here, ahead of the sketch
     below. Deliberately left empty until that capture exists: no
     stand-in, no redrawn sketch, no description of an image nobody
     has taken yet. The figure below is the schematic it replaces,
     unchanged and still labelled as a sketch in its caption. -->
<figure class="shot">
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">bondebarras — systm-d</span></div>
<div class="term-body">
<div class="tui">
<div class="tui-head"><span class="tui-brand">bondebarras</span><span class="tui-sep">·</span><span class="tui-count">15 orgs</span><span class="tui-sep">·</span><span class="tab active">Orgs</span></div>
<div class="tui-panels">
<div class="tui-col"><div class="col-title">ORGS</div><div class="row sel">systm-d      37.2 Go</div><div class="row dim">SecondBrain  13.9 Go</div></div>
<div class="tui-col"><div class="col-title">DÉPÔTS</div><div class="row">[ ] another-r… 775 j          9.8 Go</div><div class="row sel">[ ] ci-heavy   2 j         ⚠ 11.1 Go</div><div class="row">    repolens   déjà archivé      0 o</div></div>
<div class="tui-col grow"><div class="col-title">RESSOURCES · 69 éléments · cochés 522.0 Mo</div><div class="row dim">Cache   ████████████████████  111 %   11.1 Go / 10 Go</div><div class="row dim">  (seuil inclus ; limite réelle non exposée par l'API)</div><div class="row">  ⚠ dépasse le seuil inclus : le stockage en excès est facturé ;</div><div class="row">  l'éviction pour faire de la place, elle, attend la limite</div><div class="row">  configurée du dépôt ; et, indépendamment de toute limite,</div><div class="row">  toute entrée non lue depuis plus de 7 jours est supprimée</div><div class="row dim">Minutes ██████   33 %   1 004 / 3 000</div><div class="row sel">[x]⛑ cache coverage-linux-x64                   261Mo <span class="stale">PR#32 ⚑</span></div><div class="row">[x]⛑ cache coverage-linux-x64                   261Mo <span class="stale">PR#25 ⚑</span></div><div class="row">[ ]  cache ubuntu-22.04-test                    257Mo 12j</div><div class="row">[ ]• artif build-output                         1.1Mo 45j</div></div>
</div>
<div class="tui-foot"><span class="key">←/→</span> col.<span class="key">↑/↓</span> ligne<span class="key">espace</span> cocher<span class="key">A</span> sûrs<span class="key">V</span> +à vérifier<span class="key">d</span> supprimer<span class="key">f</span> filtrer<span class="key">s</span> trier<span class="key">b</span> billing<span class="key">q</span> quitter</div>
</div>
</div>
</div>
<figcaption>De gauche à droite : les organisations, les dépôts de l'org courante, et les ressources du dépôt sélectionné — ⛑ marque une ligne sûre à supprimer, • une ligne à vérifier, ⚑ un cache épinglé à une pull request fermée. Croquis d'un terminal large : plus étroit, le pied garde d'abord [d], puis les touches de sélection.</figcaption>
</figure>
<p class="more"><a href="https://github.com/systm-d/bondebarras/blob/main/docs/tui.md">Chaque colonne, chaque symbole, chaque touche →</a></p>
</section>

<section id="benefits" class="features">
<h2>Trois choses qu'il fait</h2>
<div class="grid">
<div class="card"><h3>Tout voir au même endroit</h3><p>Organisations, dépôts et ressources sur un seul écran, le détail d'un dépôt n'étant chargé que lorsque le curseur s'y pose.</p></div>
<div class="card"><h3>Commencer par le plus sûr</h3><p>Les caches épinglés à des pull requests fermées, et tous les autres candidats, sont classés par niveau de sûreté. Un élément protégé reste exclu de toute sélection en masse.</p></div>
<div class="card"><h3>Comprendre avant de supprimer</h3><p>Taille, âge, origine et coût, puis une confirmation explicite : chaque mutation reste un choix humain, et chaque résultat est rapporté un par un.</p></div>
</div>
<p class="note">Un onglet Billing chiffre ce qui reste — minutes Actions, stockage, budgets et rétention, en lecture seule : bondebarras ne modifie jamais un budget ni une rétention.</p>
</section>

<section id="resources" class="resources">
<h2>Huit familles de ressources</h2>
<p class="section-lede">Une passe sur toutes les organisations accessibles au jeton, et tout ce que la CI a laissé derrière elle arrive dans la même liste.</p>
<div class="chips"><span class="chip">caches Actions</span><span class="chip">artifacts</span><span class="chip">workflow runs</span><span class="chip">versions de packages (GHCR)</span><span class="chip">branches mergées</span><span class="chip">tags</span><span class="chip">assets de releases</span><span class="chip">archivage de dépôts</span></div>
<p class="note">bondebarras ne supprime jamais une release, ni un dépôt. Les assets d'une release peuvent partir ; la release, elle, reste. Un dépôt ne peut qu'être archivé, à la main, une ligne à la fois — et l'archivage est la seule opération ici que GitHub sait défaire.</p>
<p class="more"><a href="https://github.com/systm-d/bondebarras/blob/main/docs/resources.md">Chaque famille, taille par taille et règle par règle →</a></p>
</section>

<section id="safety" class="safety">
<h2>La sûreté, avant tout le reste</h2>
<p class="section-lede">Cet outil supprime des choses. Quatre règles tiennent partout, TUI comme cron.</p>
<ul class="pledges">
<li>Rien n'est modifié sans confirmation. En headless, cette confirmation s'appelle <code>--yes</code> : sans lui, <code>clean</code> affiche le plan et ne supprime rien.</li>
<li>Pas de corbeille, pas d'annulation pour ce que GitHub laisse supprimer à cet outil, et l'outil ne fait jamais semblant du contraire. L'archivage d'un dépôt est la seule opération réversible, et il est formulé comme telle au lieu d'emprunter les mots d'une suppression.</li>
<li>Une ressource protégée n'est jamais prise en masse — ni par une touche, ni par un cron. La sélection individuelle reste disponible, une ligne à la fois.</li>
<li>Un dépôt n'est jamais archivé en headless : il n'existe pas de drapeau <code>--archive</code>, et aucun n'est prévu.</li>
</ul>
<p class="note">Chaque ligne de ressource porte l'un de trois marqueurs : <strong>⛑</strong> sûr selon les règles documentées, <strong>•</strong> à vérifier, non marqué on garde. <span class="kbd">A</span> coche toutes les lignes ⛑, <span class="kbd">V</span> y ajoute les •, et ni l'une ni l'autre ne prend une ligne protégée.</p>
<p class="more"><a href="https://github.com/systm-d/bondebarras/blob/main/docs/safety.md">Le modèle de sûreté, en entier →</a></p>
</section>

<section id="cli" class="usage">
<h2>Scriptable, aussi</h2>
<p class="section-lede">L'invocation nue ouvre le TUI. <code>scan</code> et <code>clean</code> donnent le même aperçu et le même nettoyage aux scripts et aux tâches cron.</p>
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">bash</span></div>
<div class="term-body cmds">
<div class="comment"># Nu : ouvre l'interface TUI interactive</div>
<div class="line"><span class="prompt">$</span>bondebarras</div>
<div class="comment"># stdout ne porte que du JSON, donc un pipeline | jq parse toujours</div>
<div class="line"><span class="prompt">$</span>bondebarras scan <span class="flag">--org</span> systm-d <span class="flag">--json</span></div>
<div class="comment"># Sans --yes, clean affiche son plan sur stderr et ne supprime rien</div>
<div class="line"><span class="prompt">$</span>bondebarras clean <span class="flag">--org</span> systm-d <span class="flag">--repo</span> josephine <span class="flag">--caches</span> <span class="flag">--stale-pr</span></div>
<div class="out">Plan (2 élément(s) · 522.0 Mo) — relancez avec --yes pour l'appliquer :</div>
<div class="out">  v0-rust-coverage-Linux-x64                 261.0 Mo</div>
<div class="out">  v0-rust-coverage-Linux-x64-PR32            261.0 Mo</div>
</div>
</div>
<p class="note"><code>--yes</code> remplace la confirmation qu'un humain donnerait, et rien d'autre : il n'élargit jamais la sélection, et le filtre des ressources protégées s'applique d'abord, headless ou non.</p>
<p class="more"><a href="https://github.com/systm-d/bondebarras/blob/main/docs/cli.md">Chaque drapeau, chaque code de sortie, chaque champ JSON →</a></p>
</section>

<section id="install" class="install">
<h2>Installation</h2>
<p class="section-lede">Ne lit et n'écrit que l'API GitHub — pas de compte propre, pas de télémétrie, pas de stockage cloud.</p>
<h3>Disponible maintenant — v1.0.0-rc.3</h3>
<p>Les binaires et paquets sont joints à <a href="https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.3">v1.0.0-rc.3</a>, chacun avec son fichier <code>.sha256</code>. Télécharge celui de ta plateforme, puis suis le guide d'installation.</p>
<table class="platforms">
<thead><tr><th>Plateforme</th><th>Architecture</th><th>Format</th></tr></thead>
<tbody>
<tr><td>Linux</td><td>x86-64</td><td><code>.tar.gz</code></td></tr>
<tr><td>Debian / Ubuntu</td><td>x86-64</td><td><code>.deb</code></td></tr>
<tr><td>Fedora / RHEL</td><td>x86-64</td><td><code>.rpm</code></td></tr>
<tr><td>macOS (Apple Silicon)</td><td>aarch64</td><td><code>.tar.gz</code></td></tr>
<tr><td>Windows</td><td>x86-64</td><td><code>.exe, .zip</code></td></tr>
<tr><td>Arch Linux</td><td>x86-64</td><td><code>PKGBUILD</code></td></tr>
</tbody>
</table>
<h3>Depuis les sources, sur toutes les plateformes</h3>
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">install</span></div>
<div class="term-body cmds">
<div class="line"><span class="prompt">$</span>cargo install <span class="flag">--git</span> https://github.com/systm-d/bondebarras bondebarras</div>
</div>
</div>
<p class="note">Demande Rust ≥ 1.88 et un compilateur C. C'est le seul canal qui fonctionne aujourd'hui sans télécharger un fichier d'abord.</p>
<h3>Pas encore publiés</h3>
<p>Aucun de ces canaux ne porte de paquet aujourd'hui, et chacun ne sera annoncé ici qu'une fois son paquet réellement publié.</p>
<ul>
<li><strong>Homebrew</strong> et <strong>winget</strong> — le workflow de release écarte les deux sur un tag de préversion, volontairement.</li>
<li><strong>AUR</strong> — aucun job AUR n'existe et aucun paquet n'a jamais été soumis ; le <code>PKGBUILD</code> joint à chaque release est le chemin pris en charge en attendant.</li>
<li><strong>crates.io</strong> — la publication s'active par variable de dépôt, et elle est désactivée.</li>
</ul>
<p class="more"><a href="https://github.com/systm-d/bondebarras/blob/main/docs/installation.md">Chaque plateforme, et comment vérifier un téléchargement →</a></p>
</section>

<section id="docs" class="docs">
<h2>Documentation</h2>
<p class="section-lede">Les pages ci-dessous décrivent le comportement réel de l'outil ; là où un résumé de cette page est plus court, ce sont elles qui font foi.</p>
<div class="chips"><a class="chip" href="https://github.com/systm-d/bondebarras/blob/main/docs/installation.md">Installation</a><a class="chip" href="https://github.com/systm-d/bondebarras/blob/main/docs/tui.md">Utiliser le TUI</a><a class="chip" href="https://github.com/systm-d/bondebarras/blob/main/docs/cli.md">Référence CLI</a><a class="chip" href="https://github.com/systm-d/bondebarras/blob/main/docs/safety.md">Modèle de sûreté</a><a class="chip" href="https://github.com/systm-d/bondebarras/blob/main/docs/resources.md">Ressources prises en charge</a><a class="chip" href="https://github.com/systm-d/bondebarras/blob/main/docs/billing.md">Facturation et limites GitHub</a></div>
<p class="more"><a href="https://github.com/systm-d/bondebarras/blob/main/docs/README.md">Toute la documentation</a></p>
</section>