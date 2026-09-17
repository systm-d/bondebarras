# Authentication and permissions

bondebarras talks to the GitHub REST API over HTTPS and needs a token to do
it. This page explains where that token comes from, which permissions buy
which features, and — just as importantly — what the tool does when a
permission is missing, family by family.

The short version: a token with `repo`, `read:org`, `read:packages` and
`delete:packages` runs everything bondebarras does except one optional
display.

## How a token is resolved

Two sources, tried in this fixed order:

1. **`gh auth token`** — the GitHub CLI's own session. If the command
   succeeds and prints a non-empty token, that token is used.
2. **`$GITHUB_TOKEN`** — the environment variable, used only if step 1
   produced nothing.

If neither yields a token, bondebarras stops with:

```text
Erreur : aucun jeton GitHub trouvé.
Connectez-vous avec `gh auth login`, ou définissez la variable d'environnement GITHUB_TOKEN.
```

> **The `gh` session wins.** If you are logged in with `gh` *and* you export
> `GITHUB_TOKEN`, the environment variable is never read. This is the most
> common surprise on this page: when a permission seems missing despite a
> freshly minted `GITHUB_TOKEN`, you are almost certainly still running on the
> `gh` session's token. Check with `gh auth status`, and either log out of
> `gh` or grant the missing scope to the `gh` session.

**bondebarras performs no pre-flight permission check.** It resolves a token,
builds a client, and starts working. A missing permission is therefore
discovered where it is used — as one family's listing being refused, or one
panel of the Billing tab reading `illisible` — never as a startup error. That
is deliberate: a token that cannot read packages can still do everything else,
and refusing to start would help nobody.

## Installing and authenticating the GitHub CLI

Using `gh` is the zero-configuration path: if you already have it logged in,
bondebarras needs no setup at all.

Install it from [cli.github.com](https://cli.github.com), then:

```sh
gh auth login          # interactive: choose GitHub.com, HTTPS or SSH, and authenticate
gh auth status         # confirm which account and which scopes are active
```

To add a scope to an existing session without starting over:

```sh
gh auth refresh -h github.com -s read:packages -s delete:packages
```

## The minimal classic scopes

| Scope | What it buys |
| --- | --- |
| `repo` | Reading and deleting Actions caches, artifacts and workflow runs; listing and deleting branches, tags and release assets; listing repositories, including private ones; archiving a repository |
| `read:org` | Listing the organizations the token can see, which is what the left column is built from |
| `read:packages` | Listing container package versions and their tags |
| `delete:packages` | Deleting a container package version |

Branches, tags and release assets need nothing beyond `repo` — it is already in
the list. Repository archiving needs no new scope either: it goes through the
same `repo`-scoped endpoint, gated by your **admin rights on that one
repository** rather than by a scope to grant.

**`delete_repo` is never needed.** Repository deletion is permanently out of
scope for this tool; archiving is reversible and covers the same need more
safely.

## The optional `admin:org` scope

`admin:org` is optional, and buys exactly one thing: **displaying an
organization's artifact and log retention**, which GitHub reveals only to that
scope — or to the fine-grained "Actions policies" permission.

bondebarras never changes the setting. The `PUT` on the same path exists and is
deliberately not called.

Without it, the Billing tab reads `Rétention artefacts et journaux : illisible`
and `scan --json` reports `artifact_retention_days: null`. Everything else
works unchanged.

> Measured live on 2026-09-17: with only the four declared scopes above, all
> three organizations tested answered **403** on the retention endpoint. If
> that line reads `illisible` for you, this is why — it is the expected
> behaviour of the documented scope set, not a fault.

Retention is worth the scope if you are trying to reduce storage: it is the
tap. Every artifact a workflow uploads is kept that long unless the workflow's
own `retention-days` asks for less, and changing it applies only to *new*
artifacts and logs.

## Budgets and billing are roles, not scopes

Two parts of the Billing tab are gated by your **role in the organization**,
which no scope can substitute for.

**The usage report** (Actions minutes and storage) requires being an **owner**
of the organization. Anyone else gets a 403. That is not fatal: the
organization stays fully navigable for caches, artifacts and runs, and only the
billing column is marked unavailable.

**Budgets** are reserved to **organization admins and billing managers**.
GitHub's documentation announces 403, 404 or 500 for anyone else; the observed
answer on three organizations, on 2026-09-10, was a **400 `Unable to get
budgets.`** Any failure degrades the same way.

The **plan name** comes from `GET /orgs/{org}`, which also only tells an owner.
Without it there is no allowance to measure against, so the tab shows the total
and says `formule inconnue` — **never a percentage against a guessed
allowance**.

A crucial distinction the tool preserves everywhere: **"unreadable" is never
collapsed into "zero" or "none".** A readable organization with no Actions
budget and an organization whose budgets could not be read are different facts,
and a cron reading the JSON must be able to tell them apart — otherwise "no
budget: overage billed with no ceiling" would be said of an organization GitHub
actually blocks.

## Feature → permission → behaviour if missing

| Feature | Permission needed | Behaviour if missing |
| --- | --- | --- |
| Listing organizations | `read:org` | The organization does not appear at all |
| Listing repositories | `repo` | The organization's repository list is empty |
| Caches, artifacts, workflow runs (read and delete) | `repo` | That family's rows are absent; the interface names the refused family |
| Branches, tags, release assets (read and delete) | `repo` | That family's rows are absent; the interface names the refused family |
| Package versions (read) | `read:packages` | No package rows; the rest of the drill-down is unaffected |
| Package versions (delete) | `delete:packages` | The deletion fails per item and is reported as a failure |
| Repository archiving | `repo` **plus admin rights on that repository** | The repository is shown but not tickable: `Droits d'admin requis sur ce dépôt : sélection refusée.` |
| Billing usage report (minutes, storage) | Organization **owner** | Billing column unavailable; `billing_readable: false`, `storage_gbh: null` |
| Plan name and allowances | Organization **owner** | `formule inconnue`, no percentage; `plan: null`, `minutes_allowance: null` |
| Actions budgets | Organization **admin** or **billing manager** | `Budget Actions : illisible`; `budgets_readable: false` |
| Artifact and log retention | `admin:org` (optional) | `Rétention artefacts et journaux : illisible`; `artifact_retention_days: null` |
| Repository deletion | *Not applicable* | Permanently out of scope — never offered, never needed |

## What degrades when something cannot be read

The governing rule: **a refusal costs one family's rows, never the whole
view.**

A repository's drill-down joins nine calls. Each is unwrapped independently, so
a token missing one scope — or one endpoint's transient outage — costs only
that family's rows. Caches, artifacts and workflow runs used to fail the whole
drill-down when any one of them was refused, even families that needed no scope
the failed call did; they no longer do.

Family by family:

- **Caches / artifacts / workflow runs / branches / tags / release assets** —
  the refused family's rows are absent, and the interface says which families
  were refused, so an empty list never quietly reads as "this repository holds
  nothing".
- **Package versions** — the same. Note that most repositories publish no image
  at all, so a 404 here is the *normal* case and yields an empty list, not an
  error.
- **The closed-pull-request listing** — not one of the six families, and its
  failure is subtler: the `⚑` flag disappears and every branch reads as
  "not known to be dead". Nothing is hidden and nothing is wrongly offered; the
  view is simply less useful.
- **The default-branch lookup** — degrades to unknown, and the branch
  classification errs toward *protected* rather than live for an unmatched
  branch. The cautious direction.
- **The branch listing, if truncated** — the "cache on a vanished branch"
  rule is withheld entirely: an incomplete listing cannot prove a name is
  absent, so those caches fall to `•` rather than `⛑`.
- **Billing, plan, budgets, retention** — each degrades to its own `illisible`
  or `null` and never drops the organization.

## A diagnostic example

Start by confirming which identity is actually in play:

```sh
gh auth status
```

Then read the scopes GitHub reports for that token — it returns them on every
response, in the `X-OAuth-Scopes` header:

```sh
curl -sI -H "Authorization: Bearer $(gh auth token)" https://api.github.com/user \
  | grep -i '^x-oauth-scopes:'
```

Expect something covering the four scopes above, for example:

```text
x-oauth-scopes: delete:packages, read:org, read:packages, repo
```

An empty or absent header means the token is a fine-grained personal access
token or a GitHub App installation token, whose permissions are not expressed
as classic scopes. Those can work, but the mapping is yours to check against
the table above, permission by permission.

Finally, let bondebarras tell you what it could and could not read. The JSON
output states each answer as an explicit flag rather than making you infer it:

```sh
bondebarras scan --org my-org --json \
  | jq '.[] | {org, billing_readable, budgets_readable, plan, artifact_retention_days}'
```

```json
{
  "org": "my-org",
  "billing_readable": true,
  "budgets_readable": false,
  "plan": "team",
  "artifact_retention_days": null
}
```

Read that as: the usage report came back, budgets did not (you are not an admin
or billing manager), the plan is known, and retention is unreadable (no
`admin:org`). Nothing there is an error — it is a precise description of what
this token can see.

## Handling the token safely

- **Never paste a token into a shared command, a screenshot, a bug report or a
  terminal recording.** A classic token with `repo` can read and delete across
  every repository you can reach; treat it exactly as you would a password.
- Prefer `gh auth login` over exporting `GITHUB_TOKEN`: the session is stored
  by the CLI's own credential handling rather than sitting in your shell
  history, your environment, or a dotfile.
- When you must use `GITHUB_TOKEN` — CI, a container — inject it as a secret at
  runtime. Never commit it, and never bake it into an image.
- Use `$(gh auth token)` inline, as the diagnostic above does, rather than
  copying the literal value anywhere.
- Grant the narrowest scope set that does the job. `admin:org` buys one
  read-only display; add it only if you want that line.
- Revoke a token you have pasted anywhere, immediately, and mint a new one.
  Rotation is cheap; a leaked `repo` token is not.

bondebarras never writes your token anywhere, never sends it to any host other
than GitHub's API, and never logs it.
