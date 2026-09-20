+++
[extra]
name = "Bon Débarras"
tagline = "Good riddance."
lede = "GitHub Actions quietly fills every organization you own with dead caches, expired artifacts, and workflow runs nobody will ever look at again. bondebarras is the terminal tool that shows you exactly how much, and clears it out — safely, one confirmation at a time."
cta = "View on GitHub"
cta2 = "Install"
status = "v1.0.0-rc.2 — pre-release"
status_url = "https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.2"
status_note = "No stable release yet: feature-complete and safe to run — nothing is ever deleted without a confirmation — but the CLI flags and the JSON schema may still change before v1.0.0, and Homebrew, the AUR and winget are not published."
+++

<section class="flow-section">
<h2 class="flow-title">GitHub Actions writes. bondebarras clears it out.</h2>
<p class="section-lede flow-lede">Every CI run leaves something behind: a cache, an artifact, a workflow run. bondebarras scans every organization a token can see, and puts it all in one screen.</p>
<div class="flow" aria-label="GitHub Actions feeds every org with junk, bondebarras scans it and gives you one screen">
<div class="flow-node src"><span class="flow-name">GitHub Actions</span><span class="flow-sub">across every org</span></div>
<div class="flow-arrow" aria-hidden="true">→</div>
<div class="flow-chips">
<span class="chip">caches</span><span class="chip">artifacts</span><span class="chip">workflow runs</span><span class="chip">closed PRs</span>
</div>
<div class="flow-arrow" aria-hidden="true">→</div>
<div class="flow-node hub"><span class="flow-name">bondebarras</span><span class="flow-sub">one terminal screen</span></div>
<div class="flow-arrow" aria-hidden="true">→</div>
<div class="flow-node you"><span class="flow-name">you</span><span class="flow-sub">space back</span></div>
</div>
</section>

<section class="why">
<h2>Why bondebarras exists</h2>
<div class="why-grid">
<div class="why-text">
<p>Fifteen organizations is enough to lose track of what CI is doing. On the author's own account, GitHub Actions caches alone add up to <strong>51.4 GB</strong> — and a single repository was holding <strong>69 caches for 11.1 GB</strong> by itself, almost all of it pinned to pull requests that had been closed for months.</p>
<p>10 GB is the default <em>included</em> cache threshold per repository, not a fixed ceiling: an administrator can raise the real limit, and storage above 10 GB is billed. GitHub evicts <em>to make room</em> only once a repository reaches its configured limit — a figure its API does not expose — and, separately from any limit, removes every cache entry not accessed in over 7 days (<a href="https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#usage-limits-and-eviction-policy">GitHub's usage limits and eviction policy</a>). Either way, dead caches from long-closed PRs sit there, crowding out the caches that still matter — the ones CI actually reuses — and every eviction they cause makes the next build slower.</p>
<p>The alternative is fifteen browser tabs, each on <em>Settings → Actions → Caches</em>, clicking through repositories one at a time with no way to tell which caches are still alive. bondebarras reads the whole picture in one scan and lets you clear it from the keyboard.</p>
</div>
<div class="term-window why-tree">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">15 orgs, before bondebarras</span></div>
<div class="term-body cmds">
<div class="line dim">systm-d              37.2 GB   132 caches</div>
<div class="line dim">SecondBrain-io       13.9 GB    43 caches</div>
<div class="line dim">Tech-Work-events    190 MB      1 cache</div>
<div class="line dim">exec-d               71 MB      2 caches</div>
<div class="line dim">delfour-co           37 MB      3 caches</div>
<div class="line dim">10 more orgs          0 B       0 caches</div>
<div class="line dim">───────────────────────────────────</div>
<div class="line">total                51.4 GB   <span class="cmt"># ← mostly closed-PR caches</span></div>
</div>
</div>
</div>
</section>

<section class="missions">
<h2>Six things it does for you</h2>
<p class="section-lede">Not a rules engine, not a config file — a fast way to see the mess and clear it, on purpose, every time.</p>
<div class="grid">

<div class="mission">
<div class="mission-glyph" aria-hidden="true">⌕</div>
<h3>Scan</h3>
<p class="mission-line">See every org's footprint in seconds.</p>
<ul>
<li>Two-stage scan: org aggregates first, repository detail on demand</li>
<li>Every organization the token can see, one pass</li>
<li>Three columns at once — orgs, repositories, resources — folding from the left as the terminal narrows, so the column you delete from is never the one dropped</li>
<li>A repository loads once the cursor rests on it for 300 ms, then stays for the session: coming back costs no request</li>
<li>Two gauges above the resources — caches against GitHub's default included 10 GB per-repository threshold (the decimal 10 GB it bills on, not 10 GiB), minutes against the plan's allowance — neither ever clamped at 100 %, and the cache gauge says plainly that the repository's real limit is not exposed by the API</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">⚑</div>
<h3>Flag</h3>
<p class="mission-line">Find the safest deletions automatically.</p>
<ul>
<li>Caches cross-checked against closed pull requests</li>
<li>A closed or merged PR's cache can never be read again</li>
<li>Three levels on every row: ⛑ safe, • worth checking, unmarked keep</li>
<li>Package versions checked for missing tags and orphaned attestations</li>
<li>Branches offered dead the moment a pull request merges them — zero extra requests, a PR closed without merging leaves its branch alone</li>
<li>Release assets measured in bytes: <strong>7.3 GB</strong> across four orgs, the release itself never deleted, only its binaries</li>
<li>Stale repositories surfaced by push age — a dozen with no push in 500–775 days across five orgs — but never auto-selected: age alone is never proof a repository is dead</li>
<li><span class="kbd">A</span> takes every ⛑ row, <span class="kbd">V</span> adds every • row — neither ever takes a protected one, and the status line says how many it left; repositories are excluded from both, on purpose</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">☰</div>
<h3>Select</h3>
<p class="mission-line">Sort, filter, and pick — nothing persisted.</p>
<ul>
<li>Sort by size, age, or name</li>
<li>Incremental filter on the resource label</li>
<li>A protected row — a tagged package version, the default or a protected branch, a live unmerged one, any tag — is never taken in bulk, but stays yours to tick one row at a time</li>
<li>No rules engine, no saved config: you decide, every time</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">⛨</div>
<h3>Purge, safely</h3>
<p class="mission-line">Delete with a confirmation and a result.</p>
<ul>
<li>Tiered confirmation: a bare [y/N] for the regenerable caches, artifacts and workflow runs; an itemised recap and an explicit warning for package versions, merged branches, tags and release assets, none of which come back</li>
<li>Runs in the background, TUI stays responsive</li>
<li>A progress row with a real, counted done/total — never an estimate</li>
<li>Spaced out and retried on GitHub's rate limit</li>
<li>A deletion GitHub confirms takes its row off the list and moves the bar on; one it refuses leaves the row where it is and names it on the status line — <code>Erreur : suppression de 9 — 404</code> — and a run that ends with failures says so instead of hiding it, closing on <code>, 2 échec(s).</code></li>
<li>No trash, no undo: nothing GitHub lets us delete comes back, and the tool never pretends otherwise</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">▣</div>
<h3>Archive</h3>
<p class="mission-line">Close the tap instead of mopping forever.</p>
<ul>
<li>A repository is ticked from the repositories column, one row at a time, and archived through the same confirmation flow as a deletion</li>
<li>Archiving frees no bytes, and the tool says so plainly: an archived repository has its Actions disabled, so it stops <em>producing</em> the caches, artifacts and workflow runs everything else here cleans up</li>
<li>The one reversible thing this tool does — un-archiving restores it — so its confirmation says that, in different words from a deletion's</li>
<li>Never preselected, never headless: no <span class="kbd">A</span>, no <code>--archive</code> flag, and none planned — a push date alone is no proof a repository is dead</li>
<li>Already archived, or a repository this token can't administer: shown, never tickable at all</li>
<li>Deleting a repository is permanently out of scope — un-archiving already covers it, more safely</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">◔</div>
<h3>Track billing</h3>
<p class="mission-line">See which repository is burning the allowance.</p>
<ul>
<li>Actions minutes against the documented allowance of the organization's current plan — <strong>free 2,000</strong>, <strong>team 3,000</strong>, <strong>enterprise 50,000</strong> a month — month by month</li>
<li>The plan comes from an owner-only endpoint: when it can't be read, or names a plan with no documented figure, the tab shows the total and says <em>formule inconnue</em> — never a percentage against a guessed plan</li>
<li>Private repositories only: a public repository's Actions minutes are free and unlimited, whatever the volume</li>
<li>On <code>enterprise</code> the allowance belongs to the whole account and is shared across its organizations, so the percentage is a minimum — and the tab says so</li>
<li>Per-repository breakdown, heaviest first: one repository burnt <strong>24,632</strong> private Linux-equivalent minutes in a single month — exactly what the tab exists to surface</li>
<li>Actions storage billed in GB-hours — every hour a gigabyte of artifacts exists — against the plan's included storage (0.5 / 2 / 50 GB) times the displayed month's hours, with the repositories holding it named; public repositories count here, unlike their minutes</li>
<li>The organization's Actions budget, and what it does once the allowance runs out: blocking at 0 $, billed up to a ceiling, or no ceiling at all — read, never changed, because changing one commits money</li>
<li>Artifact and log retention, read-only (GitHub's default is 90 days) — the tap that decides how long every uploaded artifact is kept</li>
<li>Strictly diagnostic — minutes can't be reclaimed after the fact, and deleting artifacts stops the accumulation without refunding hours already counted</li>
</ul>
</div>

</div>
</section>

<section class="preview">
<h2>One screen, entirely at the keyboard</h2>
<p class="section-lede">Three columns, always on screen — organizations, this org's repositories, and the selected one's resources — flagged caches stand out at a glance.</p>
<figure class="shot">
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">bondebarras — systm-d</span></div>
<div class="term-body">
<div class="tui">
<div class="tui-head"><span class="tui-brand">bondebarras</span><span class="tabs"><span class="tab active">Orgs</span><span class="tab">Billing</span></span><span class="tui-home">15 orgs</span></div>
<div class="tui-panels">
<div class="tui-col"><div class="col-title">ORGS</div><div class="row sel">systm-d      37.2 Go</div><div class="row dim">SecondBrain  13.9 Go</div></div>
<div class="tui-col"><div class="col-title">DÉPÔTS</div><div class="row">[ ] another-r… 775 j          9.8 Go</div><div class="row sel">[ ] ci-heavy   2 j         ⚠ 11.1 Go</div><div class="row">    repolens   déjà archivé      0 o</div></div>
<div class="tui-col grow"><div class="col-title">RESSOURCES · 69 éléments · cochés 522.0 Mo</div><div class="row dim">Cache   ████████████   111 %   11.1 Go / 10 Go</div><div class="row dim">  (seuil inclus ; limite réelle non exposée par l'API)</div><div class="row">  ⚠ dépasse le seuil inclus : le stockage en excès est facturé ; l'éviction, elle, attend la limite configurée du dépôt</div><div class="row dim">Minutes ████            33 %   1 004 / 3 000</div><div class="row sel">[x]⛑ cache  coverage-linux-x64               261Mo  <span class="stale">PR#32 ⚑</span></div><div class="row">[x]⛑ cache  coverage-linux-x64               261Mo  <span class="stale">PR#25 ⚑</span></div><div class="row">[ ]  cache  ubuntu-22.04-test                257Mo  12j</div><div class="row">[ ]• artif  build-output                     1.1Mo  45j</div></div>
</div>
<div class="tui-foot"><span class="key">←/→</span> col.<span class="key">↑/↓</span> ligne<span class="key">espace</span> cocher<span class="key">A</span> sûrs<span class="key">V</span> +à vérifier<span class="key">d</span> supprimer<span class="key">f</span> filtrer<span class="key">s</span> trier<span class="key">b</span> billing<span class="key">q</span> quitter</div>
</div>
</div>
</div>
<figcaption>Left to right: organizations, this org's repositories, and the selected one's resources — ⛑ marks a row safe to delete, • one worth checking, ⚑ a cache pinned to a closed pull request. A wide-terminal sketch: on a narrower one the footer keeps [d] first, then the selection keys.</figcaption>
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
<figcaption>Tier-1 confirmation — the only friction a regenerable resource needs. A package version, a merged branch, a tag or a release asset gets an itemised recap and a warning instead; archiving a repository gets wording of its own, since it is the one thing here that can be undone.</figcaption>
</figure>
</section>

<section id="usage" class="usage">
<h2>Scriptable, too</h2>
<p class="section-lede">Run it bare and it opens the TUI. <code>scan</code> and <code>clean</code> give scripts and cron jobs the same overview and the same cleanup — <code>--json</code> for machine-readable output, <code>--yes</code> to confirm without a prompt. Without it, <code>clean</code> only prints the plan.</p>
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">bash</span></div>
<div class="term-body cmds">
<div class="line"><span class="prompt">$</span>bondebarras</div>
<div class="out">→ opens the interactive TUI</div>
<div class="line"><span class="prompt">$</span>bondebarras scan</div>
<div class="out">systm-d                    37.2 Go  (132 caches)</div>
<div class="out">SecondBrain-io             13.9 Go   (43 caches)</div>
<div class="line"><span class="prompt">$</span>bondebarras scan <span class="flag">--org</span> systm-d <span class="flag">--json</span></div>
<div class="out">[ { "org": "systm-d", "cache_bytes": 37200000000, "cache_count": 132, … } ]</div>
<div class="line"><span class="prompt">$</span>bondebarras clean <span class="flag">--org</span> systm-d <span class="flag">--repo</span> josephine <span class="flag">--caches</span> <span class="flag">--stale-pr</span> <span class="flag">--yes</span></div>
<div class="out">Bon débarras ! 261.0 Mo libérés.</div>
<div class="comment"># package versions carry no size — GitHub's API exposes none</div>
<div class="line"><span class="prompt">$</span>bondebarras clean <span class="flag">--org</span> systm-d <span class="flag">--repo</span> repolens <span class="flag">--packages</span> <span class="flag">--yes</span></div>
<div class="out">Bon débarras ! 45 élément(s) supprimé(s) · taille inconnue.</div>
<div class="comment"># the release stays — only its assets go</div>
<div class="line"><span class="prompt">$</span>bondebarras clean <span class="flag">--org</span> exec-d <span class="flag">--repo</span> terminus <span class="flag">--assets</span> <span class="flag">--older-than</span> 180 <span class="flag">--yes</span></div>
<div class="out">Bon débarras ! 1.4 Go libérés.</div>
</div>
</div>
</section>

<section id="install" class="install">
<h2>Install</h2>
<p class="section-lede">Reads and writes only the GitHub API — no account of its own, no telemetry, no cloud storage.</p>
<h3>Available now — v1.0.0-rc.2</h3>
<p>Binaries and packages for Windows x86-64, macOS Apple Silicon, Linux x86-64, Debian/Ubuntu AMD64 and Fedora/RHEL x86-64 are attached to <a href="https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.2">the v1.0.0-rc.2 release</a>. Download the one for your platform first: the package commands below install a file you already have, they do not fetch one.</p>
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">install</span></div>
<div class="term-body cmds">
<div class="comment"># From source — all platforms, Intel Macs included</div>
<div class="line"><span class="prompt">$</span>cargo install <span class="flag">--git</span> https://github.com/systm-d/bondebarras bondebarras</div>
<div class="comment"># Debian / Ubuntu — after downloading the .deb from the release page</div>
<div class="line"><span class="prompt">$</span>sudo dpkg -i bondebarras_*_amd64.deb</div>
<div class="comment"># Fedora / RHEL — after downloading the .rpm from the release page</div>
<div class="line"><span class="prompt">$</span>sudo rpm -i bondebarras-*.x86_64.rpm</div>
</div>
</div>
<h3>After the first stable release</h3>
<p>Homebrew, the AUR and winget are <strong>not published yet</strong>, so none of their commands works today. For Homebrew and winget the release workflow skips the step on a pre-release tag, on purpose; for the AUR there is nothing to skip — no AUR job exists at all, and no package has ever been submitted. Each will be listed here only once its package has actually been published through that channel.</p>
<ul>
<li><strong>Homebrew</strong> (macOS) — not published: the tap serves stable releases only</li>
<li><strong>AUR</strong> (Arch Linux) — not published: no AUR page exists yet, and the <code>PKGBUILD</code> attached to each release is the supported path meanwhile</li>
<li><strong>winget</strong> (Windows) — not published: the manifest has not been accepted into <code>winget-pkgs</code></li>
</ul>
</section>
