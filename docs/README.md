# Documentation

This folder holds both the **user documentation** for bondebarras — the pages
below, which describe how the tool actually behaves — and the **design
history**, which records how it came to behave that way.

## User documentation

| Page | What it covers |
| --- | --- |
| [Installation](installation.md) | Installing on each supported platform, verifying a download, building from source, and updating |
| [Authentication and permissions](authentication.md) | How a token is resolved, the scopes and roles each feature needs, and what degrades when one is missing |
| [Using the TUI](tui.md) | The three columns, navigation, selection, confirmation, the Billing tab, and every keyboard shortcut |
| [CLI reference](cli.md) | `scan`, `clean` and `update`: flags, JSON output, exit codes, and cron usage |
| [Safety model](safety.md) | The contractual reference behind every safety promise: levels, protections, confirmations, and the limits of the model |
| [Supported resources](resources.md) | The eight resource families, family by family: size, selection rules, criteria, and reversibility |
| [Billing and GitHub limits](billing.md) | What the Billing tab reads, what it never modifies, and how GitHub's limits are presented |
| [Troubleshooting](troubleshooting.md) | Common symptoms, their causes, and how to resolve them |
| [Releases and versioning](releases.md) | Stable releases versus pre-releases, published formats, checksums, and versioning policy |

Pages not yet written say so explicitly, at the top, and name the issue that
will fill them. A page in this folder never looks finished when it is not.

## Design history

- [`docs/superpowers/`](superpowers/) — the specs and implementation plans
  behind each phase of the product, from v0.1 to the billing work.
- [`docs/audits/`](audits/) — point-in-time audits of the project, such as the
  documentation and site review that produced the structure of this folder.

> **These are historical records, not documentation.** Each file describes
> decisions taken on a given date, against the code as it stood that day. They
> are kept because the reasoning is worth preserving — but they are not
> updated as the product changes, and they may no longer describe current
> behaviour. When a design document and the user documentation disagree, the
> user documentation above is the one that is binding; when the user
> documentation and the code disagree, that is a bug worth reporting.

## Elsewhere in the repository

- [`README.md`](../README.md) — what bondebarras is, and how to get started
- [`CHANGELOG.md`](../CHANGELOG.md) — what changed in each release
- [`CONTRIBUTING.md`](../CONTRIBUTING.md) — how to set up and submit a change
- [`CONVENTIONS.md`](../CONVENTIONS.md) — the shared standards and quality gate
- [`SECURITY.md`](../SECURITY.md) — how to report a vulnerability
