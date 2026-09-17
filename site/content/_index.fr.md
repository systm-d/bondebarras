+++
[extra]
name = "Bon Débarras"
tagline = "Good riddance."
lede = "GitHub Actions remplit sans bruit chaque organisation que tu possèdes de caches morts, d'artifacts expirés et de workflow runs que personne ne regardera jamais plus. bondebarras est l'outil terminal qui te montre exactement combien, et qui fait le ménage — en sûreté, une confirmation à la fois."
cta = "Voir sur GitHub"
cta2 = "Installer"
status = "v1.0.0-rc.2 — préversion"
status_url = "https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.2"
status_note = "Pas encore de version stable : complet et sûr à l'usage — rien n'est jamais supprimé sans confirmation — mais les drapeaux de la CLI et le schéma JSON peuvent encore changer avant la v1.0.0, et Homebrew, l'AUR et winget ne sont pas publiés."
+++

<section class="flow-section">
<h2 class="flow-title">GitHub Actions écrit. bondebarras fait le ménage.</h2>
<p class="section-lede flow-lede">Chaque run de CI laisse quelque chose derrière lui : un cache, un artifact, un workflow run. bondebarras scanne toutes les organisations accessibles au jeton, et met tout ça sur un seul écran.</p>
<div class="flow" aria-label="GitHub Actions alimente chaque org en déchets, bondebarras scanne et te donne un seul écran">
<div class="flow-node src"><span class="flow-name">GitHub Actions</span><span class="flow-sub">sur chaque org</span></div>
<div class="flow-arrow" aria-hidden="true">→</div>
<div class="flow-chips">
<span class="chip">caches</span><span class="chip">artifacts</span><span class="chip">workflow runs</span><span class="chip">PR fermées</span>
</div>
<div class="flow-arrow" aria-hidden="true">→</div>
<div class="flow-node hub"><span class="flow-name">bondebarras</span><span class="flow-sub">un seul écran terminal</span></div>
<div class="flow-arrow" aria-hidden="true">→</div>
<div class="flow-node you"><span class="flow-name">toi</span><span class="flow-sub">de l'espace en plus</span></div>
</div>
</section>

<section class="why">
<h2>Pourquoi bondebarras existe</h2>
<div class="why-grid">
<div class="why-text">
<p>Quinze organisations, c'est suffisant pour perdre le fil de ce que fait la CI. Sur le compte de l'auteur, les caches Actions à eux seuls atteignent <strong>51,4 Go</strong> — et un seul dépôt en détenait <strong>69 caches pour 11,1 Go</strong> à lui tout seul, presque tous rattachés à des pull requests fermées depuis des mois.</p>
<p>10 Go est le seuil <em>inclus</em> par défaut par dépôt, pas un plafond fixe : un administrateur peut relever la limite réelle, et le stockage au-delà de 10 Go est facturé. GitHub n'évince <em>pour faire de la place</em> qu'une fois la limite configurée du dépôt atteinte — un chiffre que son API n'expose pas — et, indépendamment de cela, toute entrée non lue depuis plus de 7 jours est supprimée (<a href="https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#usage-limits-and-eviction-policy">limites d'usage et politique d'éviction de GitHub</a>). Dans tous les cas, les caches morts de PR fermées depuis longtemps restent là, prennent la place de ceux qui comptent encore — ceux que la CI réutilise vraiment — et chaque éviction qu'ils provoquent ralentit le build suivant.</p>
<p>L'alternative, c'est quinze onglets de navigateur, chacun sur <em>Settings → Actions → Caches</em>, à cliquer dépôt par dépôt sans aucun moyen de savoir lesquels sont encore utiles. bondebarras lit tout ça en un scan et laisse nettoyer au clavier.</p>
</div>
<div class="term-window why-tree">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">15 orgs, avant bondebarras</span></div>
<div class="term-body cmds">
<div class="line dim">systm-d              37,2 Go   132 caches</div>
<div class="line dim">SecondBrain-io       13,9 Go    43 caches</div>
<div class="line dim">Tech-Work-events    190 Mo      1 cache</div>
<div class="line dim">exec-d               71 Mo      2 caches</div>
<div class="line dim">delfour-co           37 Mo      3 caches</div>
<div class="line dim">10 autres orgs        0 o       0 cache</div>
<div class="line dim">───────────────────────────────────</div>
<div class="line">total                51,4 Go   <span class="cmt"># ← surtout des caches de PR fermées</span></div>
</div>
</div>
</div>
</section>

<section class="missions">
<h2>Six choses qu'il fait pour toi</h2>
<p class="section-lede">Pas un moteur de règles, pas un fichier de config — un moyen rapide de voir le bazar et de le nettoyer, exprès, à chaque fois.</p>
<div class="grid">

<div class="mission">
<div class="mission-glyph" aria-hidden="true">⌕</div>
<h3>Scanner</h3>
<p class="mission-line">Voir l'empreinte de chaque org en quelques secondes.</p>
<ul>
<li>Scan en deux étages : agrégats d'org d'abord, détail du dépôt à la demande</li>
<li>Toutes les organisations accessibles au jeton, en une passe</li>
<li>Trois colonnes à la fois — orgs, dépôts, ressources — qui se replient par la gauche quand le terminal rétrécit : celle où l'on supprime n'est jamais la première sacrifiée</li>
<li>Un dépôt se charge quand le curseur s'y pose 300 ms, puis reste en mémoire pour la session : y revenir ne coûte aucune requête</li>
<li>Deux jauges au-dessus des ressources — les caches face au seuil inclus par défaut de 10 Go par dépôt (les 10 Go décimaux sur lesquels GitHub facture, pas 10 Gio), les minutes face au quota de la formule — jamais bornées à 100 %, et la jauge de cache dit sans détour que la limite réelle du dépôt n'est pas exposée par l'API</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">⚑</div>
<h3>Repérer</h3>
<p class="mission-line">Trouver automatiquement les suppressions les plus sûres.</p>
<ul>
<li>Caches croisés avec les pull requests fermées</li>
<li>Le cache d'une PR fermée ou mergée ne sera plus jamais relu</li>
<li>Trois niveaux sur chaque ligne : ⛑ sûr, • à vérifier, non marqué on garde</li>
<li>Versions de packages vérifiées : sans tag, ou attestation orpheline</li>
<li>Branches proposées mortes dès qu'une pull request les merge — zéro requête en plus, une PR fermée sans merge laisse sa branche tranquille</li>
<li>Assets de releases mesurés en octets : <strong>7,3 Go</strong> sur quatre orgs, la release elle-même jamais supprimée, seulement ses binaires</li>
<li>Dépôts dormants repérés par l'âge du dernier push — une douzaine sans push depuis 500 à 775 jours sur cinq orgs — mais jamais présélectionnés : l'âge seul ne prouve jamais qu'un dépôt est mort</li>
<li><span class="kbd">A</span> coche toutes les lignes ⛑, <span class="kbd">V</span> y ajoute les • — ni l'une ni l'autre ne prend une ligne protégée, et la ligne d'état dit combien elle en a laissées ; les dépôts sont exclus des deux, exprès</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">☰</div>
<h3>Sélectionner</h3>
<p class="mission-line">Trier, filtrer, choisir — rien n'est persisté.</p>
<ul>
<li>Tri par taille, âge ou nom</li>
<li>Filtre incrémental sur le libellé de la ressource</li>
<li>Une ligne protégée — version de package taguée, branche par défaut ou protégée, branche vivante sans merge, n'importe quel tag — n'est jamais prise en masse, mais reste cochable à la main, une ligne à la fois</li>
<li>Pas de moteur de règles, pas de config enregistrée : tu décides, à chaque fois</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">⛨</div>
<h3>Nettoyer, en sûreté</h3>
<p class="mission-line">Supprimer avec une confirmation et un résultat.</p>
<ul>
<li>Confirmation par palier : un simple [y/N] pour les caches, artifacts et workflow runs régénérables ; un récapitulatif chiffré et un avertissement explicite pour les versions de packages, les branches mergées, les tags et les assets de releases, dont aucun ne revient</li>
<li>S'exécute en tâche de fond, le TUI reste réactif</li>
<li>Une ligne de progression au compte réel, fait/total — jamais une estimation</li>
<li>Espacé et retenté face à la limite de débit de GitHub</li>
<li>Chaque élément annonce <span class="kbd">✓</span> ou <span class="kbd">✗</span> une fois terminé, et une passe qui finit sur des échecs le dit au lieu de le cacher</li>
<li>Pas de corbeille, pas d'annulation : rien de ce que GitHub laisse supprimer ne revient, et l'outil ne fait jamais semblant du contraire</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">▣</div>
<h3>Archiver</h3>
<p class="mission-line">Couper le robinet plutôt qu'éponger sans fin.</p>
<ul>
<li>Un dépôt se coche depuis la colonne des dépôts, une ligne à la fois, et s'archive par la même confirmation qu'une suppression</li>
<li>L'archivage ne libère aucun octet, et l'outil le dit sans détour : un dépôt archivé a ses Actions désactivées, donc il cesse de <em>produire</em> les caches, artifacts et workflow runs que tout le reste ici nettoie</li>
<li>C'est la seule chose réversible que fait cet outil — désarchiver le restaure — alors sa confirmation le dit, avec d'autres mots que ceux d'une suppression</li>
<li>Jamais présélectionné, jamais en headless : pas de <span class="kbd">A</span>, pas de drapeau <code>--archive</code>, et aucun de prévu — une date de push ne prouve pas qu'un dépôt est mort</li>
<li>Déjà archivé, ou dépôt que ce jeton n'administre pas : affiché, jamais cochable</li>
<li>Supprimer un dépôt est hors périmètre pour de bon — désarchiver couvre déjà le besoin, plus sûrement</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">◔</div>
<h3>Suivre la facturation</h3>
<p class="mission-line">Voir quel dépôt brûle l'allocation.</p>
<ul>
<li>Minutes Actions face au quota documenté de la formule actuelle de l'organisation — <strong>free 2 000</strong>, <strong>team 3 000</strong>, <strong>enterprise 50 000</strong> par mois — mois par mois</li>
<li>La formule vient d'un endpoint réservé aux propriétaires : illisible, ou sans quota documenté, l'onglet affiche le total et dit <em>formule inconnue</em> — jamais un pourcentage contre une formule devinée</li>
<li>Dépôts privés uniquement : les minutes Actions d'un dépôt public sont gratuites et illimitées, quel qu'en soit le volume</li>
<li>En <code>enterprise</code>, le quota appartient au compte entreprise et se partage entre ses organisations : le pourcentage est un minimum, et l'onglet le dit</li>
<li>Répartition par dépôt, le plus gourmand d'abord : un seul dépôt a brûlé <strong>24 632</strong> minutes privées équivalent-Linux en un mois — exactement ce que cet onglet existe pour montrer</li>
<li>Stockage Actions facturé en GB-heures — chaque heure passée par un gigaoctet d'artefacts — face au stockage inclus de la formule (0,5 / 2 / 50 Go) multiplié par les heures du mois affiché, avec les dépôts qui le portent, nommés ; les dépôts publics comptent ici, contrairement à leurs minutes</li>
<li>Le budget Actions de l'organisation, et ce qu'il fait une fois le quota atteint : bloquant à 0 $, facturé jusqu'à un plafond, ou sans plafond du tout — lu, jamais modifié : changer un budget engage de l'argent</li>
<li>Rétention des artefacts et des journaux, en lecture seule (90 jours par défaut chez GitHub) — le robinet qui décide combien de temps chaque artefact uploadé reste là</li>
<li>Strictement diagnostique — les minutes ne se récupèrent pas après coup, et supprimer des artefacts arrête l'accumulation sans rendre les heures déjà comptées</li>
</ul>
</div>

</div>
</section>

<section class="preview">
<h2>Un seul écran, entièrement au clavier</h2>
<p class="section-lede">Trois colonnes, toujours à l'écran — les organisations, les dépôts de l'org courante et les ressources du dépôt sélectionné — les caches marqués sautent aux yeux.</p>
<figure class="shot">
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">bondebarras — systm-d</span></div>
<div class="term-body">
<div class="tui">
<div class="tui-head"><span class="tui-brand">bondebarras</span><span class="tui-home">15 orgs</span></div>
<div class="tui-panels">
<div class="tui-col"><div class="col-title">Orgs</div><div class="row sel">systm-d      37.2 Go</div><div class="row dim">SecondBrain  13.9 Go</div></div>
<div class="tui-col"><div class="col-title">Dépôts</div><div class="row">another-repo  9.8 Go</div><div class="row sel">ci-heavy    ⚠11.1 Go</div></div>
<div class="tui-col grow"><div class="col-title">Ressources · 69 éléments · cochés 522.0 Mo</div><div class="row dim">Cache   ████████████   111 %   11.1 Go / 10 Go</div><div class="row dim">  (seuil inclus ; limite réelle non exposée par l'API)</div><div class="row">  ⚠ dépasse le seuil inclus : le stockage en excès est facturé ; l'éviction, elle, attend la limite configurée du dépôt</div><div class="row dim">Minutes ████            33 %   1 004 / 3 000</div><div class="row sel">[x]⛑ cache  coverage-linux-x64               261Mo  <span class="stale">PR#32 ⚑</span></div><div class="row">[x]⛑ cache  coverage-linux-x64               261Mo  <span class="stale">PR#25 ⚑</span></div><div class="row">[ ]  cache  ubuntu-22.04-test                257Mo  12j</div><div class="row">[ ]• artif  build-output                     1.1Mo  45j</div></div>
</div>
<div class="tui-foot"><span class="key">←/→</span> col.<span class="key">↑/↓</span> ligne<span class="key">espace</span> cocher<span class="key">A</span> sûrs<span class="key">V</span> +à vérifier<span class="key">d</span> supprimer<span class="key">f</span> filtrer<span class="key">s</span> trier<span class="key">b</span> billing<span class="key">q</span> quitter</div>
</div>
</div>
</div>
<figcaption>De gauche à droite : les organisations, les dépôts de l'org courante, et les ressources du dépôt sélectionné — ⛑ marque une ligne sûre à supprimer, • une ligne à vérifier, ⚑ un cache épinglé à une pull request fermée. Croquis d'un terminal large : plus étroit, le pied garde d'abord [d], puis les touches de sélection.</figcaption>
</figure>
<figure class="shot">
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">bondebarras — confirmation</span></div>
<div class="term-body cmds">
<div class="line">3 élément(s) · 783.0 Mo</div>
<div class="line dim">systm-d/ci-heavy</div>
<div class="line"></div>
<div class="line dim">Ces éléments sont régénérables par un re-run.</div>
<div class="line">Supprimer ?   [y/N]</div>
</div>
</div>
<figcaption>Confirmation palier 1 — la seule friction dont une ressource régénérable a besoin. Une version de package, une branche mergée, un tag ou un asset de release ont droit à un récapitulatif chiffré et à un avertissement ; l'archivage d'un dépôt, lui, a ses propres mots : c'est la seule chose ici qui se défait.</figcaption>
</figure>
</section>

<section id="usage" class="usage">
<h2>Scriptable, aussi</h2>
<p class="section-lede">L'invocation nue ouvre le TUI. <code>scan</code> et <code>clean</code> donnent le même aperçu et le même nettoyage à tes scripts et tes tâches cron — <code>--json</code> pour une sortie machine, <code>--yes</code> pour confirmer sans prompt. Sans lui, <code>clean</code> affiche seulement le plan.</p>
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">bash</span></div>
<div class="term-body cmds">
<div class="line"><span class="prompt">$</span>bondebarras</div>
<div class="out">→ ouvre l'interface TUI interactive</div>
<div class="line"><span class="prompt">$</span>bondebarras scan</div>
<div class="out">systm-d                    37.2 Go  (132 caches)</div>
<div class="out">SecondBrain-io             13.9 Go   (43 caches)</div>
<div class="line"><span class="prompt">$</span>bondebarras scan <span class="flag">--org</span> systm-d <span class="flag">--json</span></div>
<div class="out">[ { "org": "systm-d", "cache_bytes": 37200000000, "cache_count": 132, … } ]</div>
<div class="line"><span class="prompt">$</span>bondebarras clean <span class="flag">--org</span> systm-d <span class="flag">--repo</span> josephine <span class="flag">--caches</span> <span class="flag">--stale-pr</span> <span class="flag">--yes</span></div>
<div class="out">Bon débarras ! 261.0 Mo libérés.</div>
<div class="comment"># les versions de packages n'ont pas de taille — l'API GitHub n'en expose aucune</div>
<div class="line"><span class="prompt">$</span>bondebarras clean <span class="flag">--org</span> systm-d <span class="flag">--repo</span> repolens <span class="flag">--packages</span> <span class="flag">--yes</span></div>
<div class="out">Bon débarras ! 0 o libérés.</div>
<div class="comment"># la release reste — seuls ses assets partent</div>
<div class="line"><span class="prompt">$</span>bondebarras clean <span class="flag">--org</span> exec-d <span class="flag">--repo</span> terminus <span class="flag">--assets</span> <span class="flag">--older-than</span> 180 <span class="flag">--yes</span></div>
<div class="out">Bon débarras ! 1.4 Go libérés.</div>
</div>
</div>
</section>

<section id="install" class="install">
<h2>Installation</h2>
<p class="section-lede">Ne lit et n'écrit que l'API GitHub — pas de compte propre, pas de télémétrie, pas de stockage cloud.</p>
<h3>Disponible maintenant — v1.0.0-rc.2</h3>
<p>Les binaires et paquets pour Windows x86-64, macOS Apple Silicon, Linux x86-64, Debian/Ubuntu AMD64 et Fedora/RHEL x86-64 sont joints à <a href="https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.2">la release v1.0.0-rc.2</a>. Télécharge d'abord celui de ta plateforme : les commandes de paquet ci-dessous installent un fichier que tu as déjà, elles ne le récupèrent pas.</p>
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">install</span></div>
<div class="term-body cmds">
<div class="comment"># Depuis les sources — toutes plateformes, Mac Intel compris</div>
<div class="line"><span class="prompt">$</span>cargo install <span class="flag">--git</span> https://github.com/systm-d/bondebarras bondebarras</div>
<div class="comment"># Debian / Ubuntu — après avoir téléchargé le .deb depuis la page de release</div>
<div class="line"><span class="prompt">$</span>sudo dpkg -i bondebarras_*_amd64.deb</div>
<div class="comment"># Fedora / RHEL — après avoir téléchargé le .rpm depuis la page de release</div>
<div class="line"><span class="prompt">$</span>sudo rpm -i bondebarras-*.x86_64.rpm</div>
</div>
</div>
<h3>À partir de la première version stable</h3>
<p>Homebrew, l'AUR et winget ne sont <strong>pas encore publiés</strong> : aucune de leurs commandes ne fonctionne aujourd'hui. Pour Homebrew et winget, le workflow de release écarte volontairement l'étape sur un tag de préversion ; pour l'AUR, il n'y a rien à écarter — aucun job AUR n'existe, et aucun paquet n'a jamais été soumis. Chacun ne sera annoncé ici qu'une fois son paquet réellement publié sur son canal.</p>
<ul>
<li><strong>Homebrew</strong> (macOS) — pas publié : le tap ne sert que les versions stables</li>
<li><strong>AUR</strong> (Arch Linux) — pas publié : aucune page AUR n'existe encore, et le <code>PKGBUILD</code> joint à chaque release est le chemin pris en charge en attendant</li>
<li><strong>winget</strong> (Windows) — pas publié : le manifeste n'a pas encore été accepté dans <code>winget-pkgs</code></li>
</ul>
</section>
