# winget manifests

On every `v*` tag, the `winget` job of the [Release](../../.github/workflows/release.yml)
workflow generates the winget manifests (type *portable*, pointing at the
release's standalone `.exe`) and attaches them to the release as
`winget-manifests.tar.gz`.

## Test a manifest locally

```powershell
# Download and extract winget-manifests.tar.gz from the release, then:
winget install --manifest manifests\systm-d.bondebarras\<version>
```

## Publish to winget (`winget install bondebarras`)

The first publication requires a PR to the community repository
[`microsoft/winget-pkgs`](https://github.com/microsoft/winget-pkgs). The
simplest route is [`wingetcreate`](https://github.com/microsoft/winget-create):

```powershell
winget install wingetcreate
wingetcreate update systm-d.bondebarras `
  --version <version> `
  --urls https://github.com/systm-d/bondebarras/releases/download/v<version>/bondebarras-windows-x86_64.exe `
  --submit
```

Once the package is accepted, subsequent versions can be submitted
automatically (a GitHub token with access to a fork of `winget-pkgs`).
