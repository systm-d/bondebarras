+++
[extra]
tagline = "Bon débarras."
lede = "GitHub Actions remplit sans bruit chaque organisation que tu possèdes de caches morts, d'artifacts expirés et de workflow runs que personne ne regardera jamais plus. bondebarras est l'outil terminal qui te montre exactement combien, et qui fait le ménage — en sûreté, une confirmation à la fois."
cta = "Voir sur GitHub"
cta2 = "Installer"
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
<p>GitHub n'évince les caches d'un dépôt qu'au-delà de 10 Go, ou après sept jours sans lecture. En attendant, les caches morts de PR fermées depuis longtemps restent là, prennent la place de ceux qui comptent encore — ceux que la CI réutilise vraiment — et chaque éviction qu'ils provoquent ralentit le build suivant.</p>
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
<h2>Cinq choses qu'il fait pour toi</h2>
<p class="section-lede">Pas un moteur de règles, pas un fichier de config — un moyen rapide de voir le bazar et de le nettoyer, exprès, à chaque fois.</p>
<div class="grid">

<div class="mission">
<div class="mission-glyph" aria-hidden="true">⌕</div>
<h3>Scanner</h3>
<p class="mission-line">Voir l'empreinte de chaque org en quelques secondes.</p>
<ul>
<li>Scan en deux étages : agrégats d'org d'abord, détail du repo à la demande</li>
<li>Toutes les organisations accessibles au jeton, en une passe</li>
<li>Le détail du dépôt ne se charge qu'au drill-down</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">⚑</div>
<h3>Repérer</h3>
<p class="mission-line">Trouver automatiquement les suppressions les plus sûres.</p>
<ul>
<li>Caches croisés avec les pull requests fermées</li>
<li>Le cache d'une PR fermée ou mergée ne sera plus jamais relu</li>
<li>Versions de packages vérifiées : sans tag, ou attestation orpheline</li>
<li>Branches proposées mortes dès qu'une pull request les merge — zéro requête en plus, une PR fermée sans merge laisse sa branche tranquille</li>
<li>Assets de releases mesurés en octets : <strong>7,3 Go</strong> sur quatre orgs, la release elle-même jamais supprimée, seulement ses binaires</li>
<li>Une touche sélectionne toutes les lignes marquées</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">☰</div>
<h3>Sélectionner</h3>
<p class="mission-line">Trier, filtrer, choisir — rien n'est persisté.</p>
<ul>
<li>Tri par taille, âge ou nom</li>
<li>Filtre incrémental sur le libellé de la ressource</li>
<li>Pas de moteur de règles, pas de config enregistrée : tu décides, à chaque fois</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">⛨</div>
<h3>Nettoyer, en sûreté</h3>
<p class="mission-line">Supprimer avec une confirmation et un résultat.</p>
<ul>
<li>Confirmation par palier : un simple [y/N] pour les caches, artifacts et workflow runs régénérables ; un récapitulatif chiffré pour les versions de packages, qui ne reviennent pas</li>
<li>S'exécute en tâche de fond, le TUI reste réactif</li>
<li>Espacé et retenté face à la limite de débit de GitHub</li>
<li>Chaque élément annonce <span class="kbd">✓</span> ou <span class="kbd">✗</span> une fois terminé</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">◔</div>
<h3>Suivre la facturation</h3>
<p class="mission-line">Voir quel dépôt brûle l'allocation.</p>
<ul>
<li>Usage des minutes Actions face à l'allocation gratuite, mois par mois</li>
<li>Répartition par dépôt, le plus gros consommateur d'allocation d'abord</li>
<li>Strictement diagnostique — les minutes ne se récupèrent pas après coup</li>
</ul>
</div>

</div>
</section>

<section class="preview">
<h2>Un seul écran, entièrement au clavier</h2>
<p class="section-lede">L'arbre des orgs à gauche, les ressources du dépôt sélectionné à droite — les caches marqués sautent aux yeux.</p>
<figure class="shot">
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">bondebarras — systm-d</span></div>
<div class="term-body">
<div class="tui">
<div class="tui-head"><span class="tui-brand">bondebarras</span><span class="tui-home">15 orgs</span></div>
<div class="tui-panels">
<div class="tui-col"><div class="col-title">Orgs</div><div class="row sel">▾ systm-d      37.2 Go</div><div class="row">   ci-heavy    11.1 Go</div><div class="row">   another-repo 9.8 Go</div><div class="row dim">▸ SecondBrain 13.9 Go</div></div>
<div class="tui-col grow"><div class="col-title">69 éléments · 11.1 Go</div><div class="row sel">[x] cache  coverage-linux-x64            261.0 Mo  <span class="stale">PR#32 ⚑</span></div><div class="row">[x] cache  coverage-linux-x64            261.0 Mo  <span class="stale">PR#25 ⚑</span></div><div class="row">[ ] cache  ubuntu-22.04-test              257.0 Mo  12j</div><div class="row">[x] artif  build-output                    1.1 Mo  <span class="stale">PR#32 ⚑</span></div></div>
</div>
<div class="tui-foot"><span class="key">espace</span> cocher<span class="key">s</span> trier<span class="key">f</span> filtrer<span class="key">A</span> tout ⚑<span class="key">d</span> supprimer</div>
</div>
</div>
</div>
<figcaption>À gauche : les organisations et leurs dépôts, la plus grosse empreinte cache d'abord. À droite : les ressources de ce dépôt, les caches marqués en bleu.</figcaption>
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
<figcaption>Confirmation palier 1 — la seule friction dont les ressources régénérables de la v0.1 ont besoin.</figcaption>
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
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">install</span></div>
<div class="term-body cmds">
<div class="comment"># Depuis les sources — toutes plateformes</div>
<div class="line"><span class="prompt">$</span>cargo install <span class="flag">--git</span> https://github.com/systm-d/bondebarras bondebarras</div>
<div class="comment"># Debian / Ubuntu</div>
<div class="line"><span class="prompt">$</span>sudo dpkg -i bondebarras_*_amd64.deb</div>
<div class="comment"># Fedora / RHEL</div>
<div class="line"><span class="prompt">$</span>sudo rpm -i bondebarras-*.rpm</div>
<div class="comment"># Arch — AUR</div>
<div class="line"><span class="prompt">$</span>yay -S bondebarras</div>
<div class="comment"># Homebrew</div>
<div class="line"><span class="prompt">$</span>brew tap systm-d/bondebarras https://github.com/systm-d/bondebarras</div>
<div class="line"><span class="prompt">$</span>brew install bondebarras</div>
</div>
</div>
</section>
