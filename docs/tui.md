# Using the TUI

The interactive interface: how it starts, what the three columns hold, how a
repository's resources are loaded, and every key that does anything.

Everything below is verified against the code, and the rules that matter are
enforced by named tests, cited inline. For *why* a row is offered or refused —
the safety levels, the confirmations, the limits of the model — see the
[safety model](safety.md); for what each family is measured in, see
[supported resources](resources.md).

## Launching

```sh
bondebarras
```

With no subcommand, bondebarras opens the TUI. It needs a token
([authentication](authentication.md)); it resolves one, lists every
organization the token can see, and runs the stage-1 scan — cache totals, the
repository list, and the degradable reads (plan, billing, budgets, retention)
— before the first frame is drawn. Nothing is fetched per repository yet.

The terminal is restored on every exit path, including a panic: raw mode off,
alternate screen left, cursor shown (`tui::TerminalGuard`). You should never
need `reset` after bondebarras.

## Anatomy

Five rows, top to bottom:

| Row | What it holds |
| --- | --- |
| Header | ` bondebarras · N orgs · Orgs ` — the tab, and what a hidden column held |
| Body | The three columns, or the Billing panel |
| Status line | The filter being typed, or the last message |
| Progress row | Only while a purge, an archive or a load runs |
| Footer | The keys that act in the focused column |

The progress row is a row of its own and takes its line from the body, never
from the status line or the footer: a bar drawn over the status line would
hide the very failure it was reporting. On a frame shorter than five rows it
gets no line at all (`views::ROWS_WITH_PROGRESS`). Tests:
`the_progress_row_never_costs_the_footer_its_keys`,
`the_progress_row_never_hides_the_status_lines_error`.

### Column 1 — `ORGS`

Every organization the token can see, **biggest cache footprint first**: the
login, then that org's total Actions cache. 22 cells wide whenever it shares
the screen.

### Column 2 — `DÉPÔTS`

The repositories of the org under the cursor, biggest cache first. 38 cells
wide. Five fields:

```text
[ ] lokiprint  685 j       ⚠  12.4 Go
    archived…  déjà archivé    1.2 Go
```

- **A checkbox, only when the repository is a genuine archiving candidate.**
  An already-archived repository, or one this token cannot administer, carries
  no checkbox at all — not an empty one. A box that can never be ticked would
  be a lie of its own. Tests: `an_archivable_repo_carries_a_checkbox`,
  `a_non_archivable_repo_carries_no_checkbox_at_all`.
- **The name**, elided with `…` past 10 characters.
- **The age, or the class that replaces it.** `Archivable` shows `685 j`;
  `AlreadyArchived` shows `déjà archivé`; `NoAdminRights` shows `sans
  droits`. The age is never painted as urgent and never drives any
  preselection — `pushed_at` alone is not proof of abandonment.
- **`⚠`** when the repository's caches are past the included 10 GB.
- **Its own cache total.**

A repository that held Actions storage in the newest month of its org's usage
report gets a detail line under its row — `  ↳ 359.9 GB-h, 2026-09` — so the
repository holding the storage is found without opening every one. That line
is drawn only when the column has at least two inner lines.

### Column 3 — `RESSOURCES`

The resources of the loaded repository, biggest first by default. At least 40
cells wide whenever it shares the screen: this is the column deletion happens
in, so it is never the one a narrow terminal squeezes or drops.

Its title counts what is listed and what is ticked:

```text
 RESSOURCES · 69 éléments · cochés 11.1 Go
```

`cochés` — the size of the **ticked** rows, what `d` would free — not the
size of the listing. Above the list sit, in order, the repository's name, its
cache gauge, its minutes gauge, and the sizeless warning when the title
cannot hold it. Each part is shown whole or dropped entirely; when the height
runs short they yield lowest-value first — the minutes gauge, then the cache
gauge, then the size explanation, then the repository's name
(`repo::head_within`).

#### The two gauges

`tui::views::gauges` draws the loaded repository's Actions cache against
GitHub's **default included** 10 GB threshold, and its Actions minutes
against the allowance of the organization's plan. Neither is ever clamped at
100 %, and the minutes gauge gives a total with no percentage at all when the
plan — and so the allowance — could not be read. What the figures are worth,
and where each comes from, is in
[billing and GitHub limits](billing.md#the-10-gb-cache-threshold-and-the-limit-nobody-can-read).

**GitHub deletes every cache entry it has not read in over 7 days, whatever
limit the repository is configured with.** That rule waits for no threshold:
a repository sitting at 2 GB is billed nothing for its caches and loses them
all the same. It is the rule that explains a cache entry "disappearing on its
own", and the reason a cache pinned to a long-closed pull request is dead
weight rather than a time bomb.

**The interface itself states it in exactly one place: the over-threshold
warning.** The sentence rides on `gauges::OVER_INCLUDED`, which is drawn only
past 100 %; the caveat under a gauge at rest (`gauges::CACHE_CAVEAT`) is
silent about it, deliberately and by test —
`the_quiet_cache_gauge_stays_silent_about_the_seven_day_rule`. The reason is
height, and it is measured rather than asserted:
`the_cache_banner_stays_within_its_line_budget` pins the quiet banner at 2
rows (3 in the narrowest column the resources column is drawn at), and moving
the sentence up onto the caveat would cost a row on **every** repository the
tool ever draws, not only the ones over the threshold. So the gauge you are
most likely to be looking at — one below the threshold — will not tell you
this, which is why this page does.

Past the threshold, the warning keeps three facts apart instead of blending
them into one: the excess storage **is** billed, unconditionally; eviction
*to make room* waits for the repository's configured limit, which no endpoint
bondebarras can reach exposes; and the 7-day sweep waits for neither. Test:
`the_cache_gauge_reports_overshoot_rather_than_capping`, which asserts both
that the rule is stated and that it is stated as independent of any limit.

> **On a short terminal the warning can be absent rather than shortened.** A
> head part is shown whole or dropped entirely, so past a point the cache
> gauge goes with its warning rather than being trimmed. Measured when the
> seven-day sentence was added: at 100 columns (the narrowest the column is
> drawn at, three columns on screen) the warned gauge now needs a terminal 20
> rows high where it needed 17, so heights 17 to 19 no longer show it at all;
> at 60 columns the same shift lost heights 13 and 14. Recorded rather than
> reshaped — giving that sentence its own rank in the head is a design
> question, not a wording one. The band itself is recorded in the
> documentation of `the_cache_banner_stays_within_its_line_budget`, which
> asserts the row counts on either side of it and not the band; what
> `the_column_head_keeps_each_part_whole_or_drops_it_across_swept_heights`
> guarantees is the other half — that a part is drawn whole or dropped, never
> clipped.

A row:

```text
[ ]⛑ cache Linux-cargo-a1b2c3…   467Mo PR#42 ⚑
[x]  branc feat/old-login             — mergée ⚑
[ ]  tag   v1.2.0                     — protégé
```

Checkbox, safety marker, kind, label, size, then a trailing classification.
The kinds are `cache`, `artif`, `run`, `pkg`, `branc`, `tag`, `asset`. The
trailing field is the branch's class (`mergée ⚑`, `par défaut`, `protégée`,
`vivante`), `protégé` for a tag, `PR#42 ⚑` for a resource pinned to a closed
pull request, or the age in days otherwise.

The label is the only part of a row that gives way on a narrow column: the
size and the flag are never clipped. A label ending in a parenthesised suffix
— a release tag, a package version's class — is elided from the *head* so the
suffix survives, since the suffix is what tells such rows apart. Test:
`a_shortened_label_keeps_its_parenthesised_suffix`.

## The symbols

| Symbol | Where | Meaning |
| --- | --- | --- |
| `⛑` | Resource row | Safe **according to the rules documented in [safety.md](safety.md)**. `[A]` takes these |
| `•` | Resource row | Plausibly dead, but a person should look. `[V]` adds these |
| *(blank)* | Resource row | Keep by default — live, protected, or unproven |
| `⚑` | Resource row | Dead weight proven by a pull request: a cache or run pinned to a closed PR (`PR#42 ⚑`), or a branch a merged PR came from (`mergée ⚑`) |
| `⚠` | Repository row, gauges, Billing tab, column head | A fact worth noticing: caches past the included 10 GB, a sizeless family in the list, a blocking budget near its quota, a long retention |
| `[x]` / `[ ]` | Row start | Ticked / not ticked |
| `█` | Gauges, progress row | The filled part of a bar |
| `↳` | Repository detail line | This repository's Actions storage, and the month it is for |
| `…` | Anywhere | Text elided to fit |
| `—` | Size column | GitHub exposes **no size** for this family — never "zero bytes" |

`⛑` and `⚑` are painted in the same colour: they are the safest things on
screen, never an error. `•` takes the warning colour. Test:
`the_safety_markers_are_painted_safe_and_warning_never_error`.

> **The TUI shows no `✓` or `✗`.** Neither glyph appears anywhere in the
> interface — ticked rows are `[x]`, and a purge's per-item outcomes are
> reported as status-line messages and a counted `done/total` bar, not as
> marks beside rows. The only `✓` bondebarras prints is `Intégrité vérifiée ✓`,
> from `bondebarras update` on the command line.

## A repository's resources load on their own

Resources are fetched per repository, and only for the repository the
**repositories-column cursor** rests on — not the focused column's.

- **Rest the cursor for 300 ms** (`app::LOAD_PAUSE`) and the load starts by
  itself. Walking the cursor down a 33-repository org would otherwise spend
  297 requests on rows you only passed over.
- **A load is nine calls** (`scan::TOTAL_CALLS`): caches, artifacts, workflow
  runs, package versions, closed pull requests, branches, tags, release
  assets, and the default branch. The progress row counts them as they land —
  a real count, never an animation.
- **`Entrée` loads now**, skipping both the pause and the cache. It is the one
  way to refresh a repository within a session.
- **A listing is kept for the rest of the session.** Coming back to a
  repository shows it at once, with no request — and says again which families
  had been refused, so kept rows never read as if a refused family were empty.
  Test: `a_second_visit_to_a_listing_with_a_refused_family_requests_nothing_and_warns_again`.
- **While a listing is on its way, the column says `(chargement…)`** — never an
  empty list, which would read as "this repository holds nothing". A failed
  load says `(échec du chargement)` and is retried only by `Entrée`.
- **No load starts on its own for a repository a purge is still working on.**
  A listing read mid-deletion would be read mid-deletion; it waits for the
  purge to end. Test:
  `no_load_starts_on_its_own_for_a_repository_while_a_purge_concerning_it_runs`.

Leaving a repository drops its listing, selection, row cursor and filter, and
supersedes any load still in flight — a listing that lands after the cursor
left is discarded rather than shown under another repository's name. Test:
`a_listing_that_lands_after_the_cursor_left_is_dropped`.

## Navigation

`←`, `→` and `Tab` move between columns and wrap; `→` and `Tab` are the same
key. They work in every layout: with a single column on screen they change
*which* column that is. `↑` and `↓` move the cursor inside the focused column.

Moving to a different organization resets what is scoped beneath it — the
repository cursor, the displayed month, and any repository ticked for
archiving. A `↓` on the last organization moves nothing and therefore resets
nothing. Test: `an_org_key_that_moves_no_org_keeps_the_repository_tick`.

## Sorting and filtering

Both act on the resources column only.

- **`s`** cycles the sort: **size** (biggest first, the default — size is why
  you are here), then **age** (oldest first), then **name**.
- **`f`** opens the filter. It is a mode, deliberately: without one, the
  shortcut keys would shadow every character they use, and a cache key
  containing `s` or `d` would be untypeable. `Entrée` or `Esc` leaves the
  mode; `Esc` again clears the filter.

The filter is a case-insensitive substring match on the label. **It hides rows
from the selection keys as well as from the eye**: `[A]` and `[V]` only ever
tick rows currently visible. Test:
`select_safe_does_not_select_rows_hidden_by_the_filter`.

While you are typing, the status line shows the filter and its cursor, even if
a message is pending — the field you are looking at is always the one you are
typing into. Test: `active_typing_outranks_a_pending_status`.

## Selection

**`espace`** ticks the row under the cursor — in the resources column, the
resource; in the repositories column, the repository, for archiving. Whatever
its safety level: a tag, a tagged package version and a live unmerged branch
are all individually tickable. There is exactly one individual refusal among
resources, a branch GitHub itself would refuse to delete:

```text
Cette branche est protégée par GitHub : sélection refusée.
```

A repository is refused harder — neither of these is tickable at all:

```text
Ce dépôt est déjà archivé : rien à faire.
Droits d'admin requis sur ce dépôt : sélection refusée.
```

Ticking a different repository replaces the previous one: there is no
multi-repository archive, because the endpoint takes one at a time.

**`[A]`** ticks every `⛑` row. **`[V]`** ticks every `⛑` row *and* every `•`
row. Neither ever ticks an unmarked row, a `protected` row, or a repository,
and both go through one shared code path so their guards cannot drift apart.
When a press leaves visible rows unticked because they are protected, the
status line says how many:

```text
3 branches vivantes protégées non cochées.
```

Tests: `select_safe_takes_only_the_safe_rows`,
`select_safe_and_check_adds_check_and_nothing_else`,
`a_protected_row_is_never_taken_in_bulk_even_if_marked_safe`,
`bulk_selection_never_takes_a_repository`.

**None of these keys acts from a column whose rows the last frame could not
draw.** A terminal a dozen lines high can leave the resources column's head
taking every line; `[A]` then ticked rows nobody saw, and `d` deleted them.
The column says `(fenêtre trop basse)` and the keys do nothing instead. Tests:
`list_keys_act_only_while_a_row_of_the_resource_list_is_on_screen`,
`repo_keys_act_only_while_a_repository_row_is_on_screen`.

## Confirmation

**`d`** opens the confirmation modal for the **focused column's** plan: the
ticked resources from the resources column, the ticked repository from the
repositories column, and **nothing at all from the organizations column** — a
plan built from resources ticked earlier would reach a confirmation that does
not list them. Test:
`d_takes_no_plan_from_the_orgs_column_even_with_resources_ticked_off_screen`.

The modal swallows every key while it is up. `y` or `Y` confirms; anything
else cancels, and the status line says `Annulé.`

Tier 1 — caches, artifacts, workflow runs — gets a bare confirmation. Tier 2
lists what will go, up to eight items past which the rest collapses into `…
et N autre(s)`, and says plainly that it will not come back. Archiving gets
its own wording, because it is reversible. The prompt and the warning that
justifies it are **never** truncated to fit a small terminal; the itemised
recap gives way first, all the way down to nothing. A user who cannot see what
is about to be deleted can still refuse; a user who cannot see the prompt can
do neither. Test: `the_prompt_survives_every_height_the_footer_fits_in`.

The exact wording of each modal is in the [safety model](safety.md#confirmations-tier-1-and-tier-2).

## Progress and errors

While work runs, the progress row shows a label, a bar and a counted
`done/total`:

```text
 suppression  ████████████        8/15
```

The label is `suppression`, `archivage`, `suppression et archivage` when one
batch holds both, or `chargement` for a repository load. Both denominators are
counted, never estimated — a purge knows its item count from its own plan, a
load knows it makes nine calls. Several purges running at once share one bar,
and a purge that finishes keeps its items in the count until the last one
lands, so the bar never moves backwards. A refused item counts as processed,
or a purge refused throughout would never reach its end. Tests:
`purge_bar_counts_every_purge_in_flight_and_leaves_with_the_last`,
`purge_bar_counts_a_refused_item_as_processed`.

A purge does not reload the repository: each deletion takes its own row out of
the list, a refused one leaves it (the resource still exists), and only the
purge's end drops the kept listing. Tests:
`a_purge_updates_its_repositorys_listing_row_by_row_without_a_single_load`,
`a_failed_item_stays_listed_and_kept`.

Failures reach the status line one by one, and the recap says how many:

```text
Erreur : suppression de 9 — DELETE … a échoué : 404 Not Found
Bon débarras ! 11.1 Go libérés, 2 échec(s).
```

An archive never borrows a deletion's wording — `Dépôt lokiprint archivé.`,
or `Erreur : archivage de lokiprint refusé.` Tests:
`purge_finished_status_names_the_repo_when_archiving_succeeds`,
`purge_finished_status_falls_back_to_the_ordinary_recap_when_nothing_was_archived`.

**Quitting mid-purge warns first.** A purge runs on its own task, so `q` would
otherwise silently drop whatever is still queued:

```text
Purge en cours — [q] à nouveau pour quitter sans l'achever.
```

The guard clears only once *every* purge in flight has settled. Tests:
`request_quit_arms_the_guard_while_a_purge_is_in_flight_then_quits_on_a_second_press`,
`purge_finished_disarms_the_guard_only_once_every_purge_has_settled`.

## The Billing tab

**`b`** switches to the Billing tab and back. It is **strictly diagnostic**:
no selection, no `d`, nothing destructive is reachable while it is on screen —
only `←`/`→` to page months, `b` to go back, and `q` to quit. Its footer
advertises exactly those.

`←` pages to an older month, `→` back toward the newest; the tab opens on the
newest month the usage report carries. What every line means is in
[billing and GitHub limits](billing.md).

## Narrow and short terminals

The layout degrades **from the left**, never dropping the resources column:

| Terminal width | Columns shown |
| --- | --- |
| 100 or more | Organizations, repositories, resources |
| 78 – 99 | Two: the resources column, plus the focused one on its left |
| below 78 | One: the focused column alone |

These thresholds are derived from the columns' own widths (22 + 38 + 40), not
set by hand. With two columns, the left one is the repositories column unless
focus is on the organizations — pinning it to the repositories would leave the
organizations unreachable at 80 columns. With one column, `←`/`→` change which
column that is.

When a column a visible one depends on is hidden, the header recalls what it
held: the organization whose repositories those are, or the repository the
resources belong to. Test: `the_header_recalls_what_the_hidden_columns_held`.

The footer keeps its movement keys and `[q] quitter` at every width, and gives
the remaining room to the column's actions by rank — `[d]` first wherever it
acts, then the selection keys, then the rest. `[d]` is announced at every
width from 60 columns. Tests:
`every_column_footer_keeps_its_movement_keys_and_quitter_at_every_width`,
`d_is_announced_at_every_width_in_each_column_where_it_acts`.

Height matters too: a column with no room for a row draws none, says
`fenêtre trop basse` in its title, and refuses the keys that would act on rows
no frame showed. The Billing tab drops content from the bottom one whole
passage at a time — never inside a sentence.

## Every key

Movement and the global keys work from any column:

| Key | Action |
| --- | --- |
| `←` / `→` | Previous / next column, wrapping. On the Billing tab: older / newer month |
| `Tab` | Same as `→` |
| `↑` / `↓` | Move the cursor within the focused column |
| `Entrée` | Load the repository under the repositories-column cursor now, skipping the 300 ms pause and any kept listing |
| `b` | Billing tab, and back |
| `Esc` | Clear an active filter; with no filter, quit |
| `q` | Quit — twice while a purge is running |
| `y` / any other key | Confirm / cancel, while a modal is up |

`espace`, `A`, `V`, `s` and `f` act **only in the column that has focus**:

| Key | Organizations | Repositories | Resources |
| --- | --- | --- | --- |
| `espace` | — | Tick that repository for archiving (one at a time) | Tick the row under the cursor |
| `A` | — | — | Tick every `⛑` row |
| `V` | — | — | Tick every `⛑` **and** every `•` row |
| `s` | — | — | Cycle sort: size → age → name |
| `f` | — | — | Enter filter mode |
| `d` | — | Archive the ticked repository | Delete the ticked resources |

Tests: `list_keys_act_on_the_resource_list_in_the_resources_column`,
`list_keys_leave_the_resource_list_alone_in_the_repos_column`,
`list_keys_do_nothing_in_the_orgs_column`,
`footer_announces_d_only_in_the_columns_owning_its_plan`.

## What the TUI can do that the CLI cannot

Individual selection is the TUI's own: a tag, a tagged package version, a live
branch and a repository archive are all reachable here, one row at a time, and
none of them is reachable headlessly. The reasoning — and the four rules that
govern an unattended run — is in the
[safety model](safety.md#the-tui-and-headless-runs-differ-deliberately). The
headless surface itself is the [CLI reference](cli.md).
