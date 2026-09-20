+++
[extra]
name = "Bon Débarras"
tagline = "Good riddance."
lede = "See what GitHub accumulates. Clean it up without surprises — bondebarras brings the forgotten resources of every organization a token can see into one terminal interface, flags the safest deletions, and deletes nothing without a confirmation."
cta = "View on GitHub"
cta2 = "Install"
cta3 = "Read the documentation"
status = "v1.0.0-rc.3 — pre-release"
status_url = "https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.3"
status_note = "No stable release yet: feature-complete and safe to run — nothing is ever deleted without a confirmation — but the CLI flags and the JSON schema may still change before v1.0.0, and Homebrew, the AUR and winget are not published."
+++

<section id="demo" class="preview">
<h2>One screen, entirely at the keyboard</h2>
<p class="section-lede">Three columns, always on screen — organizations, this org's repositories, and the selected one's resources — flagged caches stand out at a glance.</p>
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
<figcaption>Left to right: organizations, this org's repositories, and the selected one's resources — ⛑ marks a row safe to delete, • one worth checking, ⚑ a cache pinned to a closed pull request. A wide-terminal sketch: on a narrower one the footer keeps [d] first, then the selection keys.</figcaption>
</figure>
<p class="more"><a href="https://github.com/systm-d/bondebarras/blob/main/docs/tui.md">Every column, every symbol, every key →</a></p>
</section>

<section id="benefits" class="features">
<h2>Three things it does</h2>
<div class="grid">
<div class="card"><h3>See everything in one place</h3><p>Organizations, repositories and resources on a single screen, a repository's own detail fetched only once the cursor rests on it.</p></div>
<div class="card"><h3>Start with the safest</h3><p>Caches pinned to closed pull requests, and every other candidate, are ranked by safety level. A protected item stays out of every bulk selection.</p></div>
<div class="card"><h3>Understand before deleting</h3><p>Size, age, origin and cost, then an explicit confirmation: every mutation stays a human choice, and every outcome is reported one by one.</p></div>
</div>
<p class="note">A Billing tab prices what is left — Actions minutes, storage, budgets and retention, read-only: bondebarras never changes a budget or a retention setting.</p>
</section>

<section id="resources" class="resources">
<h2>Eight resource families</h2>
<p class="section-lede">One pass over every organization a token can see, and everything CI left behind lands in the same list.</p>
<div class="chips"><span class="chip">Actions caches</span><span class="chip">artifacts</span><span class="chip">workflow runs</span><span class="chip">package versions (GHCR)</span><span class="chip">merged branches</span><span class="chip">tags</span><span class="chip">release assets</span><span class="chip">repository archiving</span></div>
<p class="note">bondebarras never deletes a release, and never deletes a repository. A release's assets can go; the release itself stays. A repository is only ever archived, by hand, one row at a time — and archiving is the one operation here that GitHub can undo.</p>
<p class="more"><a href="https://github.com/systm-d/bondebarras/blob/main/docs/resources.md">Every family, size by size and rule by rule →</a></p>
</section>

<section id="safety" class="safety">
<h2>Safety, before anything else</h2>
<p class="section-lede">This tool deletes things. Four rules hold everywhere, TUI and cron alike.</p>
<ul class="pledges">
<li>Nothing is mutated without a confirmation. Headless, that confirmation is <code>--yes</code>: without it, <code>clean</code> prints the plan and deletes nothing.</li>
<li>There is no trash and no undo for anything GitHub lets this tool delete, and the tool never pretends otherwise. Archiving a repository is the one reversible operation, and it is worded as such rather than borrowing a deletion's wording.</li>
<li>A protected resource is never taken in bulk — from a keystroke or from a cron. Individual selection stays available, one row at a time.</li>
<li>A repository is never archived headlessly: there is no <code>--archive</code> flag, and none is planned.</li>
</ul>
<p class="note">Every resource row carries one of three markers: <strong>⛑</strong> safe according to the documented rules, <strong>•</strong> worth checking, unmarked keep. <span class="kbd">A</span> takes every ⛑ row, <span class="kbd">V</span> adds every • row, and neither ever takes a protected one.</p>
<p class="more"><a href="https://github.com/systm-d/bondebarras/blob/main/docs/safety.md">The safety model, in full →</a></p>
</section>

<section id="cli" class="usage">
<h2>Scriptable, too</h2>
<p class="section-lede">Run it bare and it opens the TUI. <code>scan</code> and <code>clean</code> give scripts and cron jobs the same overview and the same cleanup.</p>
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">bash</span></div>
<div class="term-body cmds">
<div class="comment"># Bare: opens the interactive TUI</div>
<div class="line"><span class="prompt">$</span>bondebarras</div>
<div class="comment"># stdout carries JSON and nothing else, so a | jq pipeline always parses</div>
<div class="line"><span class="prompt">$</span>bondebarras scan <span class="flag">--org</span> systm-d <span class="flag">--json</span></div>
<div class="comment"># Without --yes, clean prints its plan on stderr and deletes nothing</div>
<div class="line"><span class="prompt">$</span>bondebarras clean <span class="flag">--org</span> systm-d <span class="flag">--repo</span> josephine <span class="flag">--caches</span> <span class="flag">--stale-pr</span></div>
<div class="out">Plan (2 élément(s) · 522.0 Mo) — relancez avec --yes pour l'appliquer :</div>
<div class="out">  v0-rust-coverage-Linux-x64                 261.0 Mo</div>
<div class="out">  v0-rust-coverage-Linux-x64-PR32            261.0 Mo</div>
</div>
</div>
<p class="note"><code>--yes</code> replaces the confirmation a human would give, and nothing else: it never widens the selection, and the protected filter applies first, headless or not.</p>
<p class="more"><a href="https://github.com/systm-d/bondebarras/blob/main/docs/cli.md">Every flag, exit code and JSON field →</a></p>
</section>

<section id="install" class="install">
<h2>Install</h2>
<p class="section-lede">Reads and writes only the GitHub API — no account of its own, no telemetry, no cloud storage.</p>
<h3>Available now — v1.0.0-rc.3</h3>
<p>Binaries and packages are attached to <a href="https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.3">v1.0.0-rc.3</a>, each with its own <code>.sha256</code> sidecar. Download the one for your platform, then follow the installation guide.</p>
<table class="platforms">
<thead><tr><th>Platform</th><th>Architecture</th><th>Format</th></tr></thead>
<tbody>
<tr><td>Linux</td><td>x86-64</td><td><code>.tar.gz</code></td></tr>
<tr><td>Debian / Ubuntu</td><td>x86-64</td><td><code>.deb</code></td></tr>
<tr><td>Fedora / RHEL</td><td>x86-64</td><td><code>.rpm</code></td></tr>
<tr><td>macOS (Apple Silicon)</td><td>aarch64</td><td><code>.tar.gz</code></td></tr>
<tr><td>Windows</td><td>x86-64</td><td><code>.exe, .zip</code></td></tr>
<tr><td>Arch Linux</td><td>x86-64</td><td><code>PKGBUILD</code></td></tr>
</tbody>
</table>
<h3>From source, on every platform</h3>
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">install</span></div>
<div class="term-body cmds">
<div class="line"><span class="prompt">$</span>cargo install <span class="flag">--git</span> https://github.com/systm-d/bondebarras bondebarras</div>
</div>
</div>
<p class="note">Needs Rust ≥ 1.88 and a C compiler. This is the only channel that works today without downloading a file first.</p>
<h3>Not published yet</h3>
<p>Not one of these channels carries a package today, and each will be listed here only once its package has actually been published.</p>
<ul>
<li><strong>Homebrew</strong> and <strong>winget</strong> — the release workflow skips both on a pre-release tag, on purpose.</li>
<li><strong>AUR</strong> — no AUR job exists and no package was ever submitted; the <code>PKGBUILD</code> attached to each release is the supported path meanwhile.</li>
<li><strong>crates.io</strong> — publication is opt-in per repository variable, and it is off.</li>
</ul>
<p class="more"><a href="https://github.com/systm-d/bondebarras/blob/main/docs/installation.md">Every platform, and how to verify a download →</a></p>
</section>

<section id="docs" class="docs">
<h2>Documentation</h2>
<p class="section-lede">The pages below describe how the tool actually behaves; where a summary on this page is shorter, they are the ones that bind.</p>
<div class="chips"><a class="chip" href="https://github.com/systm-d/bondebarras/blob/main/docs/installation.md">Installation</a><a class="chip" href="https://github.com/systm-d/bondebarras/blob/main/docs/tui.md">Using the TUI</a><a class="chip" href="https://github.com/systm-d/bondebarras/blob/main/docs/cli.md">CLI reference</a><a class="chip" href="https://github.com/systm-d/bondebarras/blob/main/docs/safety.md">Safety model</a><a class="chip" href="https://github.com/systm-d/bondebarras/blob/main/docs/resources.md">Supported resources</a><a class="chip" href="https://github.com/systm-d/bondebarras/blob/main/docs/billing.md">Billing and GitHub limits</a></div>
<p class="more"><a href="https://github.com/systm-d/bondebarras/blob/main/docs/README.md">All the documentation</a></p>
</section>