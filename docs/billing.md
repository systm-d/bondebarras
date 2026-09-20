# Billing and GitHub limits

> **GitHub's pricing and limits can change.** This page describes what
> bondebarras *displays* and where it gets it from. It links GitHub's current
> documentation, and **GitHub remains the source of truth for billing.** Where
> a figure below is hardcoded in bondebarras rather than read from a response,
> this page says so, because that is exactly the kind of figure that goes
> stale.

The Billing tab (`b` in the [TUI](tui.md)) is **strictly diagnostic**. No
selection, no deletion, no setting is reachable from it. Minutes cannot be
reclaimed retroactively and neither can GB-hours already counted, so the only
useful thing this view can do is name the repository behind them.

Which role or scope buys each part of the tab, and what it shows when one is
missing, is in [authentication](authentication.md#budgets-and-billing-are-roles-not-scopes).

## What bondebarras reads

Four endpoints, all `GET`, each refused on its own without dropping the
organization:

| What | Endpoint | Refused when |
| --- | --- | --- |
| Usage report | `/organizations/{org}/settings/billing/usage` | You are not an owner of the org (403) |
| Plan name | `/orgs/{org}` → `plan.name` | You are not an owner — GitHub simply omits the field |
| Budgets | `/organizations/{org}/settings/billing/budgets` | You are not an admin or billing manager |
| Retention | `/orgs/{org}/actions/permissions/artifact-and-log-retention` | The token has no `admin:org` |

The usage report is the modern one: GitHub's legacy billing endpoints
(`/settings/billing/actions`, `/packages`, `/shared-storage`) all return
**410 Gone**. The replacement is richer — it reports per repository × per SKU
× per month, which is what makes "which repository burnt the minutes" an
answerable question at all.

## What it never modifies

**Everything on this tab is read-only, permanently.**

- **Budgets are never written.** Changing a budget commits money.
- **Retention is never written.** The `PUT` on the same path exists and is
  deliberately not called. The tab shows the tap; it does not turn it.
- **No plan, no allowance, no threshold is ever changed** — bondebarras has no
  endpoint for any of them.

The only mutations bondebarras performs anywhere are the deletions and the one
archive described in the [safety model](safety.md), none of which are
reachable from this tab.

## Actions minutes

Minutes are shown in **Linux-equivalent minutes**, because that is the unit
the allowance is denominated in. GitHub bills a Windows minute as two and a
macOS minute as ten, so bondebarras multiplies before summing:

| SKU | Multiplier |
| --- | --- |
| `Actions Linux` | ×1 |
| `Actions Windows` | ×2 |
| `Actions macOS …` (any variant) | ×10 |
| Anything else | Counted ×1 **and reported** |

A runner family bondebarras does not know is never silently folded in at ×1:
it still counts, and the tab names it — `⚠ SKU inconnu, compté ×1 : <sku>` —
because a silent multiplier would skew the gauge with no way to notice. Test:
`an_unknown_sku_is_reported_not_swallowed`.

The breakdown under the gauge names the repositories that consumed the
allowance, **heaviest first by equivalent minutes, not by raw minutes**. The
two orders disagree exactly when it matters: a repository burning 6 079
Windows minutes costs more allowance than one burning 8 000 Linux minutes, and
ranking by raw quantity would point you at the wrong repository. Test:
`minute_lines_name_the_repo_and_rank_by_allowance_cost`.

**Only private repositories count.** A public repository's Actions runs are
free and unlimited, so they never draw on the allowance. This is not inferred
from the discount fields — GitHub's usage report discounts a *private*
repository still inside its allowance exactly the way it discounts a public
one, so the repository listing is the only place the distinction survives.
When the gauge reads 0 while the report shows real minutes and a real bill,
the tab says why rather than leaving a contradiction on screen:

```text
(dépôt public : minutes Actions gratuites et illimitées, hors plafond)
```

Tests: `a_private_repo_within_its_allowance_still_consumes_it`,
`a_public_repo_is_excluded_regardless_of_its_discount`,
`a_zero_allowance_beside_a_real_bill_says_the_minutes_are_public`.

### Included minutes per plan

| Plan | Included Linux-equivalent minutes / month |
| --- | --- |
| `free` | 2 000 |
| `team` | 3 000 |
| `enterprise` | 50 000 |
| Anything else, or unreadable | **No figure — and therefore no percentage** |

These three figures are **hardcoded from GitHub's published table**, not read
from any response. GitHub exposes no allowance field.

## Actions storage, in GB-hours

Storage is billed by the hour a gigabyte exists — not by peak, not by month's
end. Deleting artifacts stops the accumulation; it refunds nothing already
counted, and the tab says so in as many words:

```text
Supprimer des artefacts arrête l'accumulation,
  mais ne rend pas les GB-heures déjà comptées.
```

bondebarras counts a usage line as Actions storage only when its SKU is
`Actions storage` *and* its unit is `GigabyteHours` — both halves, so another
product's gigabyte-hours and an oddly-typed Actions line are each kept out.
Test: `storage_gbh_sums_only_actions_storage_gigabyte_hours`.

The allowance is the plan's included storage times **the displayed month's own
hours** (its days × 24), and the gauge always writes that base out:

```text
371.09 / 1 440 GB-h   ██  26 %   base 720 h
```

| Plan | Included storage |
| --- | --- |
| `free` | 0.5 GB |
| `team` | 2 GB |
| `enterprise` | 50 GB |
| Anything else, or unreadable | No figure, no percentage |

> **The hour base is an open measurement.** GitHub's documentation converts
> GB-hours to GB-months "by dividing by the hours in the month (usually 720
> hours for a 30-day month)". bondebarras uses the real month's days × 24 — so
> 720 for September, 744 for July, 672 for a non-leap February. One
> organization's September 2026 report suggested GitHub may have used a
> 744-hour base instead. This is not settled; it is why the gauge states its
> base on every line rather than letting a percentage stand alone. Test:
> `hours_in_month_counts_the_displayed_months_days`.

**Public repositories' storage is counted**, and the tab's own header says so
— `Stockage Actions · GB-heures, dépôts publics compris`. GitHub's
documentation says a public repository's *minutes* are free; it says nothing
of its storage, and the report discounts both kinds alike. Counting it is the
cautious reading.

## Artifacts, Packages and cache are three different things

This is the distinction most easily read wrong, so it is worth stating
plainly:

| Storage | Billed? | Does bondebarras show a size? |
| --- | --- | --- |
| **Actions artifacts and logs** | Yes — this is what "Actions storage" in GB-hours *is* | Yes: per artifact, and per repository in GB-hours |
| **Packages (GHCR)** | Not covered by any SKU in this report | **No — never.** GitHub exposes no size for a package version, under any field name |
| **Actions cache** | Storage past the repository's included threshold is billed | Yes: per cache entry, and per repository |

**Package versions carry no size, ever.** There is no size field in the
versions API, and no billing SKU covers package storage, so bondebarras stores
`0` and displays `—` rather than formatting that zero as `0 o`, which would
read as "empty" — the opposite of the truth. **Never estimate or extrapolate
one.** See [supported resources](resources.md#package-version-ghcr).

Caches do not appear in the usage report as their own line either; their size
comes from the Actions cache endpoints, and is shown per repository in the
tree and per entry in the resources column.

## The 10 GB cache threshold, and the limit nobody can read

Every repository's cache gauge is measured against **10 GB, decimal
(10 000 000 000 bytes)** — matching the units GitHub's own billing UI uses.

**It is a cost threshold, not a ceiling, and the gauge says so on every
repository:**

```text
Cache    ████████████████████  124 %   12.4 Go / 10 Go
  (seuil inclus ; limite réelle non exposée par l'API)
```

Three separate facts sit behind that caveat:

1. **10 GB is the *default included* threshold per repository.** An authorized
   administrator can raise a repository's actual cache limit above it.
2. **Storage past the included threshold is billed** — unconditionally, not as
   one branch of an alternative.
3. **Eviction *to make room* starts only once the repository reaches its
   *configured* limit**, which can therefore sit well above 10 GB. **No
   endpoint bondebarras can reach exposes that configured limit** — the cache
   usage endpoint only ever returns totals — which is why the figure is
   hardcoded and labelled rather than read.

Past 100 %, the gauge keeps the two apart instead of asserting the one it
cannot check:

```text
⚠ dépasse le seuil inclus : le stockage en excès est facturé ;
  l'éviction, elle, attend la limite configurée du dépôt
```

**Independently of any limit, GitHub removes every cache entry that has not
been read in over 7 days.** That rule never waits for a threshold, and it is
why a cache pinned to a long-closed pull request is dead weight rather than a
time bomb.

The gauge is **never clamped at 100 %**: an organization well past its
allowance is exactly what this view exists to surface, and GitHub does not
clamp there either. Tests: `the_cache_gauge_reports_overshoot_rather_than_capping`,
`cache_over_included_is_strictly_above_ten_gigabytes`.

## Unknown plans, and the absence of a percentage

**bondebarras never shows a percentage against a guessed allowance.** A
guessed one made a Team organization read 50 % for 1 004 minutes when 33 % was
true, and an Enterprise one read 901 % for 36 %.

Two different unknowns, kept apart because they are different facts:

| On screen | Means |
| --- | --- |
| `formule inconnue, pas de quota` | The plan could not be read at all — you are not an owner |
| `formule <name>, quota inconnu` | The plan was read, but bondebarras has no included figure for it |

The second exists because reading `formule inconnue` under a header that
already named the plan contradicted itself. Tests:
`gauge_line_names_the_plan_when_only_its_quota_is_unknown`,
`an_unknown_plan_shows_no_percentage_anywhere_in_the_tab`.

Every month the tab pages through is measured against **today's** plan:
`plan.name` is the only plan GitHub reports, so a mid-month plan change is
invisible to it. The month line says as much —
`2026-09 · quota documenté de la formule actuelle`.

## Enterprise organizations share their quota

On `enterprise`, the allowance belongs to the enterprise account and is shared
across its organizations. bondebarras only ever sees the one organization's
usage, so **every percentage is a floor, not a measurement**, and the tab says
so:

```text
Formule enterprise : quota partagé par tout le compte
  entreprise, ces pourcentages sont des minimums.
```

Test: `an_enterprise_org_says_its_quota_is_shared`.

## Actions budgets

A budget is what GitHub does once the allowance runs out. bondebarras reads
the organization's **Actions product budget** — scope `organization`, type
`ProductPricing`, SKU `actions` — and reports one of four states:

| State | Line | Meaning |
| --- | --- | --- |
| Blocking | `Budget Actions : 0.00 $ · bloquant` | GitHub stops Actions usage at the budget |
| Alert only | `Budget Actions : 50.00 $ · alerte seule, sans blocage` | GitHub bills the overage rather than stopping it |
| None | `Budget Actions : aucun, dépassement facturé sans plafond` | Overage is billed with no ceiling (if a payment method is registered) |
| Unreadable | `Budget Actions : illisible` | Nobody knows — you are not an admin or billing manager |

**"No budget" and "unreadable" never share a line**, and never share a JSON
value either. Amounts are whole US dollars, as GitHub's schema has them.

When a gauge reaches **90 %** or more *and* a blocking budget exists, a
warning appears under it saying what will happen. It reads the percentage the
gauge itself displays, so the two can never disagree. It is shown only on the
report's **most recent month** — an older month's outcome is already settled,
and the warning is in the future tense. Tests:
`a_blocking_budget_at_95_percent_warns_under_the_gauge`,
`an_older_month_never_warns_about_a_blocking_budget`,
`no_budget_warning_without_a_blocking_budget`.

Per-SKU budgets (`SkuPricing`) are **named, never interpreted**: bondebarras
has never observed one in the wild, so it reports each one and says plainly
that the warnings do not take it into account.

The budgets listing is read across up to ten pages of 100. A listing still
incomplete at that point, a page that fails, or a single entry missing one of
its five fields all make the whole listing read as **unreadable** rather than
partial — because the entry that went missing could be the Actions budget, and
"no budget: overage billed" would then be said of an organization GitHub
actually blocks. Tests: `budgets_fetch_treats_a_malformed_entry_as_unreadable`,
`budgets_fetch_gives_up_rather_than_truncate`.

> Observed on 2026-09-10: three organizations the account does not own all
> answered **400 `Unable to get budgets.`**, although GitHub's documentation
> announces 403, 404 or 500. Any failure degrades the same way.

## Artifact and log retention

Retention is **the tap**: every artifact a workflow uploads is kept that long
unless the workflow's own `retention-days` asks for less. It sits beside the
storage it governs, and bondebarras never changes it.

It is highlighted — `⚠`, warning colour, and a reason — only when it is **90
days or more** *and* the organization holds at least **36 GB-hours** that
month. Unknown storage never highlights: nobody can say whether it counts.

> The 36 GB-hour threshold is 10 % of the smallest plan's included storage
> (0.5 GB × 720 h = 360 GB-h), taken as a fixed figure rather than recomputed
> per month — it is deliberately independent of the plan, so the highlight
> still works when the plan cannot be read.

Two notes are shown whatever the setting, because both are true of the setting
itself rather than of any figure beside it:

```text
Note : retention-days, dans un workflow, fixe la durée
  de cet artefact, dans la limite de ce réglage.
Note : un changement de rétention ne vaut que pour
  les nouveaux artefacts et journaux.
```

The second was verified on 2026-09-10: an artifact uploaded the day before an
organization moved to 7 days kept its original December expiry.

Without `admin:org` the line reads `Rétention artefacts et journaux :
illisible`, and `scan --json` reports `artifact_retention_days: null`. GitHub's
90-day default is **never assumed** in its place. Test:
`retention_fetch_never_assumes_a_default`.

## Which month, and what the figures are worth

- **The TUI opens on the newest month the usage report carries**, and `←`
  pages back from there. It reads no clock.
- **`scan --json` reports the current UTC month** in `billing_month`. Early in
  a month the two can differ — see the [CLI reference](cli.md#scan---json).
- **Costs come from GitHub's own `netAmount`**, never recomputed as
  `gross − discount`: when the three disagree (rounding, a credit, an
  adjustment), GitHub's figure is the one on the invoice. Test:
  `cost_trusts_githubs_net_rather_than_recomputing_it`.
- **Amounts are in US dollars**, the currency of the usage report. Nothing is
  converted: bondebarras has no exchange rate and does not invent one.

Figures quoted on this page as observations — the 400 on budgets, the
retention behaviour — were measured on the dates given, against the real API.
The allowance and threshold tables are GitHub's published figures as
bondebarras hardcodes them, and those are the ones to re-check against
GitHub's documentation before relying on them.

## GitHub's own documentation

- [Billing for GitHub Actions](https://docs.github.com/en/billing/concepts/product-billing/github-actions)
  — minutes, storage, included allowances, and how overage is billed.
- [Dependency caching — usage limits and eviction policy](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#usage-limits-and-eviction-policy)
  — the 10 GB default included threshold, the configured limit, and the 7-day
  eviction rule.

If either page disagrees with anything above, GitHub is right and this page is
out of date — please [open an issue](https://github.com/systm-d/bondebarras/issues).
