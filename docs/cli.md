# CLI reference

The headless surface: `scan`, `clean` and `update`. Every flag below is taken
from the binary's own `--help` at `1.0.0-rc.2`, and every behaviour is
verified against the code, with the named tests that enforce it cited inline.

The interactive surface is the [TUI](tui.md). What a headless run is allowed
to select — and the whole families it is not — is the
[safety model](safety.md#the-tui-and-headless-runs-differ-deliberately).

## Synopsis

```text
Audit et nettoyage des orgs GitHub

Usage: bondebarras [COMMAND]

Commands:
  scan    Affiche l'état des organisations sans rien supprimer
  clean   Supprime des ressources sans interface. Sans `--yes`, affiche le plan sans rien toucher
  update  Vérifie s'il existe une version plus récente, et propose ou applique la mise à jour
          selon la manière dont bondebarras a été installé. Ne requiert aucun jeton GitHub :
          le dépôt est public
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

**With no subcommand, bondebarras opens the [TUI](tui.md).**

`scan` and `clean` resolve a token before doing anything
([authentication](authentication.md)). `update` deliberately does not: the
repository is public, and a version check must never require authentication.

## `scan`

```text
Usage: bondebarras scan [OPTIONS]

Options:
      --org <ORG>  Limite le scan à une organisation
      --json       Sortie JSON sur stdout, pour un pipeline machine
  -h, --help       Print help
```

`scan` reads and never writes. It runs the stage-1 overview only — org-level
aggregates — and never drills into a repository. That is six reads per
organization (cache usage, the repository list, the usage report, the plan,
budgets, retention), plus one extra page for each listing that runs past 100
items — not one request per repository.

Without `--org`, it scans every organization the token can see. With `--org`,
it scans that one and never enumerates the others, so a narrowly-scoped cron
token that cannot list organizations still works.

Plain output is one line per organization, biggest cache first:

```text
exec-d                      37.2 Go  (412 caches)
systm-d                     11.1 Go  (69 caches)
```

### `scan --json`

**stdout carries JSON and nothing else.** Every progress and diagnostic line
goes to stderr, so a `| jq` pipeline always parses.

The document is an array with one object per organization:

```json
[
  {
    "org": "exec-d",
    "cache_bytes": 37166609585,
    "cache_count": 412,
    "billing_readable": true,
    "plan": "team",
    "minutes_allowance": 3000,
    "billing_month": "2026-09",
    "storage_gbh": 371.09,
    "storage_allowance_gbh": 1440.0,
    "budgets_readable": true,
    "actions_budget": { "amount": 0, "blocking": true },
    "actions_sku_budgets": [],
    "artifact_retention_days": 7,
    "repos": [
      { "name": "disconnected", "cache_bytes": 12360000000, "cache_count": 69, "storage_gbh": 359.88 }
    ]
  }
]
```

Key by key:

| Key | Type | Meaning |
| --- | --- | --- |
| `org` | string | The organization's login |
| `cache_bytes` | integer | Total active Actions cache, in bytes |
| `cache_count` | integer | Number of active cache entries |
| `billing_readable` | boolean | Whether the usage report could be read at all |
| `plan` | string \| null | `free`, `team`, `enterprise` — `null` unless you own the org |
| `minutes_allowance` | integer \| null | Included Linux-equivalent minutes for `plan`; `null` for an unknown or unread plan |
| `billing_month` | string | `YYYY-MM`, the **current UTC month** |
| `storage_gbh` | number \| null | Actions storage that month, in GB-hours; `null` when billing is unreadable |
| `storage_allowance_gbh` | number \| null | Included storage for `plan` × that month's hours |
| `budgets_readable` | boolean | Whether the budgets listing could be read |
| `actions_budget` | object \| null | `{amount, blocking}` for the org's Actions budget; `null` when there is none **or** when budgets are unreadable — read `budgets_readable` to tell them apart |
| `actions_sku_budgets` | array \| null | Per-SKU Actions budgets; `[]` for a readable org with none, `null` when unreadable |
| `artifact_retention_days` | integer \| null | Artifact and log retention; `null` without `admin:org` |
| `repos` | array | Per repository: `name`, `cache_bytes`, `cache_count`, `storage_gbh` |

Three properties of this document are deliberate and worth relying on:

**A figure the API did not give is `null`, never a default.**
`minutes_allowance` is `null` for an unreadable plan, not the Free plan's
2 000; `storage_gbh` is `null` when billing is unreadable, not `0`.

**"Unreadable" is never collapsed into "none".** `actions_sku_budgets` is `[]`
for an organization that genuinely has no SKU budget and `null` when the
listing was refused. A cron reading this must be able to tell them apart —
otherwise "no budget: overage billed with no ceiling" would be said of an
organization GitHub actually blocks. Test:
`scan_json_tells_no_budget_from_unreadable_budgets`.

**A real allowance can sit beside a `null` usage, and that is correct.**
`storage_allowance_gbh` degrades on `plan` alone, `storage_gbh` on whether the
report was read: a known plan with an unreadable report gives a real allowance
next to a `null` usage. The allowance is a fact about the plan; the usage is a
fact about a report that could not be read. Test:
`a_known_plan_with_unreadable_billing_gives_an_allowance_but_no_usage`.

One subtlety when correlating months: **`billing_month` is the current UTC
month**, taken from the clock at run time — not the newest month the usage
report carries. The TUI's Billing tab reads the report's newest month instead.
Early in a month, before GitHub has reported anything for it, a `scan --json`
can therefore show a real `storage_allowance_gbh` beside a `storage_gbh` of
`0` for a month with no usage yet.

### Schema stability

**The `scan --json` schema is not stable before `1.0.0`.** The current release
is `1.0.0-rc.2`, a pre-release; its changelog states that the CLI flags and
this schema may still change before `v1.0.0`. Pin a version if you parse it,
and read [releases and versioning](releases.md) for what the project does and
does not promise.

## `clean`

```text
Usage: bondebarras clean [OPTIONS] --org <ORG> --repo <REPO>

Options:
      --org <ORG>                Organisation ciblée
      --repo <REPO>              Dépôt ciblé
      --caches                   Inclut les caches Actions
      --artifacts                Inclut les artifacts
      --runs                     Inclut les workflow runs
      --packages                 Inclut les versions de packages (conteneurs)
      --branches                 Inclut les branches mergées
      --tags                     Inclut les tags
      --assets                   Inclut les assets de releases
      --stale-pr                 Restreint aux ressources rattachées à une PR fermée
      --older-than <OLDER_THAN>  Restreint aux ressources d'au moins N jours
      --yes                      Confirme sans interaction. Sans lui, rien n'est supprimé
  -h, --help                     Print help
```

`--org` and `--repo` are required: `clean` works on exactly one repository.

### The dry run is the default

**Without `--yes`, `clean` prints the plan and deletes nothing.** The plan goes
to **stderr**:

```text
Plan (12 élément(s) · 4.1 Go) — relancez avec --yes pour l'appliquer :
  Linux-cargo-a1b2c3d4e5f6                     467.2 Mo
  ghcr-layer-digest                                   —
```

A family GitHub exposes no size for shows `—`, never `0 o`, on this screen
exactly as in the TUI — the two go through one function and cannot drift.

With nothing selected, it says `Rien à supprimer.` and exits 0.

### Selecting families

**Naming no family selects nothing.** A `clean` that quietly meant
"everything" would be the worst possible default for an irreversible operation
running unattended. Family flags are cumulative, and each selects only its own
kind. Tests: `no_family_flag_selects_nothing`, `families_are_cumulative`,
`every_kind_is_selected_by_its_own_flag_and_no_other`.

| Flag | Family | What it still cannot take |
| --- | --- | --- |
| `--caches` | Actions caches | — |
| `--artifacts` | Artifacts | — |
| `--runs` | Workflow runs | — |
| `--packages` | Container package versions | A tagged version — it is protected |
| `--branches` | Branches | Any branch but one a merged PR came from |
| `--tags` | Tags | **Everything** — every tag is protected, so this flag by itself takes nothing |
| `--assets` | Release assets | — |

**A `protected` resource is filtered out before any other rule.** The guard
lives on the resource, not in the caller's discipline, because a cron has no
human to notice a broken deployment. Test:
`a_protected_resource_is_never_taken_in_bulk`.

### `--stale-pr` and `--older-than`

Both narrow the selection *within* the families already named; neither selects
anything on its own.

**`--stale-pr`** keeps only resources attached to a closed or merged pull
request. In practice this means **caches**: the flag reads a
`refs/pull/<n>/…` ref, and a cache is the only family that carries one — an
artifact and a package version carry no ref at all, a workflow run carries
`refs/heads/…`, and branches, tags and assets are never flagged. Test:
`stale_pr_narrows_within_the_chosen_families`.

**`--older-than <days>`** keeps resources at least that many days old, and is
**inclusive of its boundary**: `--older-than 30` keeps a 30-day-old resource.
Test: `older_than_is_inclusive_of_the_boundary`.

> **`--older-than` silently excludes every branch and tag.** Neither the branch
> listing nor the closed-PR listing carries a per-branch timestamp, so
> bondebarras stores an age of `0` for every branch and every tag. Any
> `--older-than` of 1 or more therefore filters all of them out. Combining
> `--branches --older-than 90` selects nothing at all, and says
> `Rien à supprimer.` — which is honest, but not what the flags look like they
> ask for. Caches, artifacts, workflow runs, package versions and release
> assets all carry real ages.

### What `--yes` does

`--yes` replaces the confirmation a human would give — and nothing else. It
does **not** widen the selection: every rule above still applies, and the
protected filter applies first, headless or not.

With `--yes`, deletions run spaced 120 ms apart, each reporting its own
outcome. A throttled deletion is retried up to three times; a 403 *without*
`Retry-After` is a permission error and is not retried. The recap and every
failure go to stderr:

```text
Erreur : suppression de 9 — DELETE … a échoué : 404 Not Found
Bon débarras ! 4.1 Go libérés.
```

### What is deliberately impossible headlessly

| Operation | Why |
| --- | --- |
| **Archiving a repository** | No `--archive` flag exists, and none is planned. `clean` refuses `ResourceKind::Repository` unconditionally, whatever flags are set. Test: `headless_select_never_returns_a_repository` |
| **Deleting a protected resource** | A tagged package version, a live branch, every tag. Individual selection in the TUI is the only path left to them |
| **Deleting a release** | There is no such operation anywhere in bondebarras — only its assets, via `--assets` |
| **Deleting a repository** | Permanently out of scope. `delete_repo` is never needed |
| **Anything at risk tier 3** | Refused outright with no flag to bypass it. Nothing reaches that tier today; the rule is set now, while the CLI surface is small |

## `update`

```text
Usage: bondebarras update [OPTIONS]

Options:
      --check  N'affiche que la disponibilité d'une nouvelle version ; n'installe et ne propose rien
  -h, --help   Print help
```

`update` queries GitHub Releases **on demand only** — never when the TUI
starts — and needs no token. It compares the running version against the
latest published release and reports one of three outcomes: already up to
date, a newer version available, or a local build *ahead* of anything
published (a build from source is not offered a downgrade).

`--check` reports availability and installs nothing. Without it, bondebarras
downloads the asset matching the install channel it detected, verifies it
against the release's published `.sha256`, and then either runs the package
manager's own command or prints what to do:

| Detected channel | What `update` does |
| --- | --- |
| `.deb` | `sudo apt install <file>` |
| `.rpm` | `sudo dnf install <file>` |
| Homebrew, AUR/pacman, Nix, `cargo install` | Prints the right instruction and touches nothing — overwriting a managed file would desynchronize that manager's database |
| A manually installed binary | Downloads and verifies the archive, then tells you where it is |

**Verification fails closed.** Nothing is installed unless the digest matches:
a release that published no checksum, a checksum that could not be fetched or
parsed, and a checksum that disagrees are three distinct refusals, each with
its own message. Tests: `verify_outcome_proceeds_only_when_verified`,
`verify_outcome_refuses_to_install_when_no_checksum_was_published`,
`the_three_refusal_messages_are_distinct`.

> **`update` exits 0 even when it gives up.** A version check that could not
> reach GitHub, a refused checksum, and an install that did not complete are
> all reported on stdout and still exit successfully. Other failures do exit
> `1` — a staging directory that could not be created, a download cut short or
> refused, a `sudo` that could not be launched. Either way, do not use
> `update`'s exit code as a signal in a script; read its output instead.

Details of each channel, and the one case where the archive offered may not
match your platform, are in [installation](installation.md#updating).

## Exit codes

| Code | When |
| --- | --- |
| `0` | Success — including a `scan`, a dry-run `clean`, a `clean --yes` in which every deletion succeeded, `Rien à supprimer.`, and an `update` that gave up for one of the three reasons noted above |
| `1` | Any error: no token found, a failed scan, a tier-3 refusal, a `clean --yes` in which **at least one** deletion failed, or an `update` whose download or installation could not be carried out |

A `clean --yes` that fails partway exits `1` and has still deleted whatever
succeeded before the failure. There is no rollback — there is nothing to roll
back to.

## Cron examples

Two things shape a good cron line here: **stdout is JSON and stderr is
everything else**, and `clean` is a dry run until you add `--yes`.

Audit every organization nightly and keep the JSON:

```sh
0 3 * * *  bondebarras scan --json > /var/log/bondebarras/scan-$(date +\%F).json 2>> /var/log/bondebarras/scan.err
```

Purge the caches of closed pull requests weekly — the highest-volume,
lowest-risk cleanup there is, and entirely regenerable:

```sh
0 4 * * 1  bondebarras clean --org my-org --repo my-repo --caches --stale-pr --yes >> /var/log/bondebarras/clean.log 2>&1
```

Monthly, drop workflow runs and artifacts older than 90 days:

```sh
0 5 1 * *  bondebarras clean --org my-org --repo my-repo --runs --artifacts --older-than 90 --yes >> /var/log/bondebarras/clean.log 2>&1
```

Recommendations, in order of how much they matter:

- **Run it once without `--yes` first, and read the plan.** That is what the
  dry run is for, and it costs one scan.
- **Capture stderr.** The plan, every per-item failure and the final recap all
  go there. A cron line that only redirects stdout keeps the silence and
  throws away the diagnosis.
- **Check the exit code.** `1` means at least one deletion failed; the reasons
  are on stderr, one line each.
- **One repository per line.** `clean` takes exactly one `--repo`; several
  repositories means several lines, which also keeps a failure on one from
  hiding the others.
- **Do not put `--tags` in a cron.** It is accepted and it selects nothing,
  every time. If you meant to delete a specific tag, that is an individual
  selection in the [TUI](tui.md).
- **Archiving is not available here at all**, by design. It is a decision for
  a human looking at the repository.
