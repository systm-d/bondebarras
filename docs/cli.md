# CLI reference

The headless surface: the synopsis as `--help` prints it, `scan` and its JSON
output, how stable that schema is, `clean` and its dry-run-by-default rule,
selecting resource families, the `--stale-pr` and `--older-than` filters, what
`--yes` does, the operations that are deliberately impossible without the
interface, `update` and `update --check`, exit codes, and cron examples with
logging recommendations.

> Not written yet — tracked in #25.

Until then, the [CLI subcommands section of the
README](../README.md#cli-subcommands) carries the flag table and the JSON
field list. The rules about what `clean` refuses to select — protected
resources, and repository archiving entirely — are already documented in the
[safety model](safety.md#the-tui-and-headless-runs-differ-deliberately).
