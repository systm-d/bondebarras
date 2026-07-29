+++
[extra]
tagline = "Good riddance."
lede = "GitHub Actions quietly fills every organization you own with dead caches, expired artifacts, and workflow runs nobody will ever look at again. bondebarras is the terminal tool that shows you exactly how much, and clears it out — safely, one confirmation at a time."
cta = "View on GitHub"
cta2 = "Install"
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
<p>GitHub only evicts a repository's caches once it crosses the 10 GB ceiling, or after seven days without a read. Until then, dead caches from long-closed PRs sit there, crowding out the caches that still matter — the ones CI actually reuses — and every eviction they cause makes the next build slower.</p>
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
<h2>Five things it does for you</h2>
<p class="section-lede">Not a rules engine, not a config file — a fast way to see the mess and clear it, on purpose, every time.</p>
<div class="grid">

<div class="mission">
<div class="mission-glyph" aria-hidden="true">⌕</div>
<h3>Scan</h3>
<p class="mission-line">See every org's footprint in seconds.</p>
<ul>
<li>Two-stage scan: org aggregates first, repo detail on demand</li>
<li>Every organization the token can see, one pass</li>
<li>Repository detail loads only when you drill in</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">⚑</div>
<h3>Flag</h3>
<p class="mission-line">Find the safest deletions automatically.</p>
<ul>
<li>Caches cross-checked against closed pull requests</li>
<li>A closed or merged PR's cache can never be read again</li>
<li>Package versions checked for missing tags and orphaned attestations</li>
<li>Branches offered dead the moment a pull request merges them — zero extra requests, a PR closed without merging leaves its branch alone</li>
<li>Release assets measured in bytes: <strong>7.3 GB</strong> across four orgs, the release itself never deleted, only its binaries</li>
<li>Stale repositories surfaced by push age — a dozen with no push in 500–775 days across five orgs — but never auto-selected: age alone is never proof a repository is dead</li>
<li>One keystroke selects every flagged row, repositories excluded on purpose</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">☰</div>
<h3>Select</h3>
<p class="mission-line">Sort, filter, and pick — nothing persisted.</p>
<ul>
<li>Sort by size, age, or name</li>
<li>Incremental filter on the resource label</li>
<li>No rules engine, no saved config: you decide, every time</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">⛨</div>
<h3>Purge, safely</h3>
<p class="mission-line">Delete with a confirmation and a result.</p>
<ul>
<li>Tiered confirmation: a bare [y/N] for regenerable caches, artifacts and workflow runs; an itemised recap for package versions, which don't come back</li>
<li>Runs in the background, TUI stays responsive</li>
<li>Spaced out and retried on GitHub's rate limit</li>
<li>Every item reports <span class="kbd">✓</span> or <span class="kbd">✗</span> when it's done</li>
<li>Repository archiving is the one exception to "never comes back" — reversible, frees no bytes, just turns off the Actions that keep refilling everything above</li>
</ul>
</div>

<div class="mission">
<div class="mission-glyph" aria-hidden="true">◔</div>
<h3>Track billing</h3>
<p class="mission-line">See which repository is burning the allowance.</p>
<ul>
<li>Actions-minutes usage against the free allowance, month by month</li>
<li>Per-repository breakdown, heaviest allowance consumer first</li>
<li>Strictly diagnostic — minutes can't be reclaimed after the fact</li>
</ul>
</div>

</div>
</section>

<section class="preview">
<h2>One screen, entirely at the keyboard</h2>
<p class="section-lede">The org tree on the left, the selected repository's resources on the right — flagged caches stand out at a glance.</p>
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
<figcaption>Left: organizations and their repos, biggest cache footprint first. Right: this repository's resources, flagged caches in blue.</figcaption>
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
<figcaption>Tier-1 confirmation — the only friction v0.1's regenerable resources need.</figcaption>
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
<div class="out">Bon débarras ! 0 o libérés.</div>
<div class="comment"># the release stays — only its assets go</div>
<div class="line"><span class="prompt">$</span>bondebarras clean <span class="flag">--org</span> exec-d <span class="flag">--repo</span> terminus <span class="flag">--assets</span> <span class="flag">--older-than</span> 180 <span class="flag">--yes</span></div>
<div class="out">Bon débarras ! 1.4 Go libérés.</div>
</div>
</div>
</section>

<section id="install" class="install">
<h2>Install</h2>
<p class="section-lede">Reads and writes only the GitHub API — no account of its own, no telemetry, no cloud storage.</p>
<div class="term-window">
<div class="term-bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="term-title">install</span></div>
<div class="term-body cmds">
<div class="comment"># From source — all platforms</div>
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
