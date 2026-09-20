# Troubleshooting

Common symptoms, what causes them, and what to do. Each entry follows the same
shape — **Symptom → Cause → Resolution → Safety note** — and the safety note
is there to say what the symptom does *not* put at risk, because most of these
are refusals working as designed.

Most permission-shaped failures are covered family by family in
[authentication](authentication.md#what-degrades-when-something-cannot-be-read).

---

## No organization appears

**Symptom.** bondebarras starts, the organizations column is empty, or an
organization you expect is missing.

**Cause.** The organizations column is built from `GET /user/orgs`, which
needs the **`read:org`** scope. Without it the call returns nothing and no
organization is listed. An organization can also be missing individually: if
either of the two calls that *define* it — cache usage and the repository
listing — fails, that organization is dropped rather than shown half-built.

Two lesser causes worth ruling out: an organization that has enabled SAML SSO
requires the token to be authorized for it separately, and a personal account
is not an organization — bondebarras works on organizations only.

**Resolution.** Check which identity is actually in play, then its scopes:

```sh
gh auth status
curl -sI -H "Authorization: Bearer $(gh auth token)" https://api.github.com/user \
  | grep -i '^x-oauth-scopes:'
```

Add the scope if it is missing:

```sh
gh auth refresh -h github.com -s read:org
```

Remember that **the `gh` session wins over `$GITHUB_TOKEN`** — a freshly
minted environment token is never read while `gh` is logged in.

**Safety note.** Nothing is at risk: bondebarras deletes nothing it cannot
see. A missing organization means less visibility, never a wrong deletion.

---

## `401 Unauthorized`

**Symptom.** Every call fails; bondebarras reports an error and exits.

**Cause.** The token is invalid, expired, or revoked. A 401 is about *who you
are*, not about what you may do.

**Resolution.** Mint a new one:

```sh
gh auth login          # or: gh auth refresh
```

If you use `$GITHUB_TOKEN`, replace its value — and check whether `gh` is also
logged in, since the `gh` session takes priority.

**Safety note.** A 401 stops bondebarras before any mutation is attempted.
Nothing was deleted.

---

## `403 Forbidden`

**Symptom.** One family's rows are missing, one panel of the Billing tab reads
`illisible`, or a single deletion fails with a 403.

**Cause.** A 403 is about *what you may do*, and GitHub uses it for several
different things:

| Where | What it means |
| --- | --- |
| A resource listing | The token lacks that family's scope |
| The usage report | You are not an **owner** of the organization |
| The retention setting | The token lacks the optional `admin:org` |
| Archiving a repository | You lack **admin rights on that repository** |
| A deletion, **with** a `Retry-After` header | GitHub's secondary rate limit — see below |
| A deletion, **without** `Retry-After` | A missing permission |

**bondebarras performs no pre-flight permission check.** It resolves a token
and starts working, so a missing permission surfaces where it is used, never
as a startup error. That is deliberate: a token that cannot read packages can
still do everything else.

**Resolution.** Read the table above to tell which 403 you have, then grant
the corresponding scope or role. The mapping from feature to permission is in
[authentication](authentication.md#feature--permission--behaviour-if-missing).

**Safety note.** A refused *listing* costs only that family's rows — the rest
of the drill-down is unaffected, and the interface names the families it could
not read, so an empty list never quietly reads as "this repository holds
nothing". A refused *deletion* is reported as a failure per item and makes the
headless run exit non-zero; it is never counted as a success.

---

## Billing is unreadable

**Symptom.** The Billing tab shows
`⚠ facturation illisible — vous n'êtes pas propriétaire de cette organisation`,
and `scan --json` reports `"billing_readable": false` with `"storage_gbh":
null`.

**Cause.** The usage report is reserved to organization **owners**. No scope
substitutes for the role.

**Resolution.** Ask an owner to run the scan, or to grant ownership. There is
nothing to configure in bondebarras.

**Safety note.** The organization stays fully navigable — caches, artifacts,
runs, branches, tags and assets are all unaffected. Only the billing figures
are missing, and they are missing *as* `null`, never as zero.

---

## Budgets are unreadable

**Symptom.** `Budget Actions : illisible`, and `"budgets_readable": false` in
the JSON.

**Cause.** GitHub reserves the budgets endpoint to organization **admins and
billing managers**. Its documentation announces 403, 404 or 500 for anyone
else; the answer observed on three organizations on 2026-09-10 was a **400
`Unable to get budgets.`** Any failure degrades the same way.

A readable listing can also be *refused as a whole* for a subtler reason: if a
single budget entry is missing one of its five fields, or the listing is still
incomplete after ten pages, bondebarras reports it as unreadable rather than
partial — the entry that went missing could be the Actions budget itself.

**Resolution.** Have an admin or billing manager run it. Otherwise, read the
budget in GitHub's own billing settings.

**Safety note.** This is the entry where the distinction matters most:
**"unreadable" is never collapsed into "no budget"**. Saying "no budget:
overage billed with no ceiling" about an organization GitHub actually blocks
would be worse than saying nothing. Check `budgets_readable` before acting on
`actions_budget: null`.

---

## Retention is unreadable

**Symptom.** `Rétention artefacts et journaux : illisible`, and
`"artifact_retention_days": null`.

**Cause.** GitHub exposes the setting only to the classic **`admin:org`**
scope or the fine-grained "Actions policies" permission. bondebarras treats
both as optional and requires neither.

> Measured on 2026-09-17: with only the four documented scopes, all three
> organizations tested answered **403** here. If the line reads `illisible` for
> you, this is the expected behaviour of the documented scope set — not a
> fault.

**Resolution.** If you want the line, add the scope:

```sh
gh auth refresh -h github.com -s admin:org
```

It is worth it if you are trying to reduce storage: retention is the tap.
GitHub's 90-day default is **never assumed** in place of a real answer, so a
missing value stays `null`.

**Safety note.** `admin:org` buys exactly one read-only display. bondebarras
never writes the setting — the `PUT` on the same path exists and is
deliberately not called.

---

## A repository is visible but cannot be archived

**Symptom.** Pressing `espace` on a repository row does nothing, and the
status line says one of:

```text
Ce dépôt est déjà archivé : rien à faire.
Droits d'admin requis sur ce dépôt : sélection refusée.
```

**Cause.** Only a genuine candidate is tickable. A repository is classified
`AlreadyArchived` when GitHub reports it archived, and `NoAdminRights` when
`permissions.admin` is not true for your token — including when GitHub did not
report the field at all, which defaults to *no rights* on purpose. GitHub
would answer 403 to the archive request, and offering a tick the API then
refuses would be a lie in front of the user.

**Resolution.** Get admin rights on that repository, or archive it from
GitHub's own settings. Note that `age_days` has nothing to do with it: a
repository is never offered automatically, whatever its `pushed_at`.

**Safety note.** This is a *harder* refusal than a protected tag or a live
branch, which stay individually tickable. Archiving is also the one family
with **no headless path at all** — there is no `--archive` flag and none is
planned. And remember that archiving is the one operation here GitHub can
undo.

---

## A package version shows no size

**Symptom.** Package rows show `—` where the sized families show bytes, and
the column header carries a warning.

**Cause.** **GitHub exposes no size for a package version, under any field
name**, and no billing SKU covers package storage either. bondebarras stores
`0` and displays `—` rather than formatting that zero as `0 o`, which would
read as "empty" — the opposite of the truth.

**Resolution.** None, and none is coming. **Never estimate or extrapolate a
figure here.** Treat this family as a hygiene cleanup measured in versions,
not a volume one. Branches and tags show `—` for the same reason: a ref
carries no size.

**Safety note.** A plan made only of sizeless items reports
`N élément(s) · taille inconnue` rather than `0 o`, so an itemised
confirmation never looks like it would delete nothing. Do not read `—` as
"this is empty, deleting it is free".

---

## Deletions slow down, or a few fail under load

**Symptom.** A large purge takes longer than the item count suggests, or a
handful of items fail while the rest succeed.

**Cause.** GitHub's **secondary** rate limit, which targets bursts rather than
totals. A purge of a hundred-plus caches is exactly such a burst. The primary
limit (5 000 requests/hour) is never the binding constraint at this scale.

bondebarras already spaces deletions **120 ms apart** and retries a throttled
one **up to three times**, waiting whatever GitHub named in `Retry-After` or
five seconds when it named none. What counts as throttling is deliberately
narrow: a **429 always**, and a **403 only when it carries `Retry-After`** —
because GitHub returns 403 for missing permissions far more often than for
throttling, and retrying a permission error would only delay the real message
by three backoffs.

**Resolution.** Let it run. If items still fail, read the per-item errors: a
403 without `Retry-After` is a permissions problem, not a rate-limit one.

**Safety note.** Every item reports its own outcome, carrying its own kind, id
and repository, so two runs in flight can never be attributed to one another.
A run that ends with failures says so instead of hiding it, and exits non-zero
headlessly. A failed item is left in the list, because the resource still
exists.

---

## A repository lists at most 100 caches, artifacts, runs or package versions

**Symptom.** A busy repository shows exactly 100 rows of a family, and you know
it has more.

**Cause.** Those four listings are fetched as a **single page of 100** and are
not paginated. Branches, tags, releases, closed pull requests, repositories and
budgets *are* paginated, up to ten pages of 100.

**Resolution.** Delete what is listed and reload with `Entrée` — the next 100
come back. There is no flag to raise the page count.

**Safety note.** This under-reports, which is the safe direction: you are shown
less than exists, never more. It also means a "this repository is clean" read
from a truncated listing is not proof — reload after a purge before concluding.

---

## `--older-than` selects no branch or tag

**Symptom.** `clean --branches --older-than 90 --yes` reports
`Rien à supprimer.` on a repository full of merged branches.

**Cause.** Neither the branch listing nor the closed-pull-request listing
carries a per-branch timestamp, so bondebarras stores an age of **`0`** for
every branch and every tag. Any `--older-than` of 1 or more therefore filters
all of them out. The filter is inclusive of its boundary, so only
`--older-than 0` would keep them — which selects everything and defeats the
point.

**Resolution.** Drop `--older-than` when you mean to act on branches:
`--branches --yes` already takes *only* branches a merged pull request came
from, which is the narrowing you actually wanted. Caches, artifacts, workflow
runs, package versions and release assets all carry real ages and honour the
flag normally.

**Safety note.** The outcome is an empty selection, which is the harmless
direction. A headless run that selects nothing exits 0 and deletes nothing.

---

## The terminal is too small

**Symptom.** Columns disappear, or a column's title reads
`fenêtre trop basse` and its keys stop responding.

**Cause.** Two independent guards.

*Width.* The layout degrades **from the left**, never dropping the resources
column: three columns from 100 terminal columns, two from 78, one below that.
With two columns the left one is the repositories column unless focus is on
the organizations.

*Height.* When a column has no room to draw even one row, it draws none and
says so — and the keys that would act on those rows (`espace`, `A`, `V`, `d`)
do nothing. They would otherwise tick and delete rows no frame ever showed.

**Resolution.** Make the window bigger. At any width, the header recalls what
a hidden column held, and `←`/`→` still reach every column.

**Safety note.** This is a safety feature, not a glitch. The confirmation
modal is also protected: the prompt and the warning that justifies it are
never truncated to fit — the itemised recap gives way first, all the way down
to nothing. A user who cannot see what is about to be deleted can still
refuse; a user who cannot see the prompt could do neither.

---

## `update` refuses to install because of a checksum

**Symptom.** One of:

```text
⚠ Cette release ne publie aucune empreinte pour ce fichier. Par prudence, rien n'est installé.
⚠ Impossible de vérifier l'empreinte de ce fichier (somme injoignable ou illisible). …
⚠ L'empreinte du fichier téléchargé ne correspond pas à celle publiée. …
```

**Cause.** Verification **fails closed**: nothing is installed unless the
published digest matches. The three messages are three distinct states, kept
apart on purpose — a release that never published a checksum, a sidecar that
could not be fetched or parsed, and a digest that genuinely disagrees.

**Resolution.** The second message usually means a transient network problem:
retry later. The third means the bytes you received are not the bytes that
were published — do not work around it, and do not install the file. Download
from the [release page](https://github.com/systm-d/bondebarras/releases/latest)
and verify manually, as in
[installation](installation.md#verify-what-you-downloaded).

**Safety note.** There is no flag to bypass this, deliberately. Also note that
**`update` exits 0 even when it refused** — read its output rather than its
exit code.

---

## Telling a zero from a missing value in the JSON

**Symptom.** `scan --json` shows `0`, `null` or `[]` and you need to know which
means "nothing" and which means "unknown".

**Cause.** They mean genuinely different things, and bondebarras keeps them
apart everywhere:

| Value | Reading |
| --- | --- |
| `null` | **Unknown.** The API did not say, or refused. Never a default in disguise |
| `0` / `0.0` | **A real, measured zero.** The data was read and the quantity is nothing |
| `[]` | **Read, and genuinely empty** — as opposed to `null`, unreadable |

Worked examples:

- `"minutes_allowance": null` means the plan could not be read. It does **not**
  mean the Free plan's 2 000.
- `"storage_gbh": null` means billing was unreadable; `0.0` means the month was
  read and held no storage.
- `"actions_sku_budgets": []` means a readable organization with no SKU budget;
  `null` means the budgets listing was refused.
- `"actions_budget": null` is ambiguous on its own — it covers both "no Actions
  budget" and "budgets unreadable". **Read `budgets_readable` alongside it.**
- A real `storage_allowance_gbh` beside a `null` `storage_gbh` is correct, not
  a contradiction: the allowance is a fact about the plan, the usage a fact
  about a report that could not be read.

**Resolution.** Always read the `*_readable` booleans before acting on a
`null`. The full key list is in the [CLI reference](cli.md#scan---json).

**Safety note.** This is the distinction that matters most for automation. A
cron that reads `null` as `0` would conclude "no budget, overage billed with no
ceiling" about an organization GitHub actually blocks — and act on it.

---

## Something else

If bondebarras and this documentation disagree, that is a bug worth reporting:
[open an issue](https://github.com/systm-d/bondebarras/issues). For a
vulnerability, follow [SECURITY.md](../SECURITY.md) instead.
