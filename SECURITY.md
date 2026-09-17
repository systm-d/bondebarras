# Security policy

## Supported versions

bondebarras is pre-1.0. Only the most recent release receives fixes; there are
no maintenance branches for older versions.

| Version | Supported |
| ------- | --------- |
| 1.0.0-rc.x | yes |
| 0.x | no — superseded, please upgrade |

## Reporting a vulnerability

**Do not open a public issue for a security problem.**

Use GitHub's private reporting — *Security* tab, then *Report a vulnerability* —
which opens a discussion visible only to you and the maintainer. If that is
unavailable to you, write to **contact@delfour.co** with `bondebarras` in the
subject line.

Please include what you did, what happened, and what you expected, with the
version (`bondebarras --version`) and your platform. A proof of concept helps,
but a clear description is enough to start.

You can expect an acknowledgement within 72 hours and an assessment within a
week. If a fix is warranted, you will be credited in the release notes unless
you ask otherwise.

## What is in scope

bondebarras reads a GitHub token and deletes resources on your behalf, so the
interesting questions are:

- anything that could make it delete a resource the user did not confirm, or
  that its own rules mark as protected;
- anything that could leak the token — it is read from the environment or the
  `gh` CLI, kept in memory, sent only to `api.github.com` over HTTPS, and never
  written to disk, logs, or the `scan --json` output;
- anything that could misreport what is about to be deleted, since every
  deletion decision rests on what the interface says.

Out of scope: GitHub's own API behaviour and rate limits, and the consequences
of a deletion the user confirmed knowingly.
