## Description

<!-- Describe the changes and their motivation. -->

## Type of change

- [ ] Bug fix (`fix`)
- [ ] New feature (`feat`)
- [ ] Refactoring (`refactor`)
- [ ] Documentation (`docs`)
- [ ] Other (`chore`, `test`, …)

## Checklist

<!-- The gate's commands are deliberately not restated here: this copy had
     already drifted, its clippy line missing `--all-targets`, so every PR
     ticked a lint narrower than the one CI actually runs (#28). -->

- [ ] The whole quality gate passes — every command in
      [CONVENTIONS.md § Quality gate](../CONVENTIONS.md#quality-gate-run-before-every-pr),
      each one on its own
- [ ] New behavior is covered by tests
- [ ] The PR does not change code formatting (no stray `cargo fmt`)
- [ ] The commit message follows Conventional Commits

## Related issue

Closes # <!-- issue number, if any -->
