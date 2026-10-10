# Releasing Microcosm

## What players get

A zip per system, each with the game and a short README:

| System  | In the zip                      |
|---------|---------------------------------|
| Windows | `Microcosm.exe`                 |
| macOS   | `Microcosm.app` (Apple silicon) |
| Linux   | `microcosm`                     |

The default settings and the built-in maps (`configs/default.toml`, `configs/maps/*.map`) are
built into the program, so it runs from any folder. Saves and maps made in the editor go in the
player's data folder (`crates/evo_app/src/paths.rs`):

- Windows: `%APPDATA%\Microcosm`
- macOS: `~/Library/Application Support/Microcosm`
- Linux: `~/.local/share/microcosm`

To change what ships, edit `configs/default.toml` or a map in `configs/maps/` (a map saved in the
editor lands in your data folder: copy it into `configs/maps/` and add it to `BUILTIN_MAPS` in
`paths.rs`), then rebuild.

## Building a zip on this machine

```
./scripts/package.sh
```

builds with the `dist` profile (no debug info, stripped, thin LTO: a few minutes) and writes
`dist/Microcosm-<version>-<system>.zip` for the system you're on. On Windows, run it in Git Bash.

## Building all three with GitHub Actions

`.github/workflows/release.yml` builds Windows, macOS and Linux on GitHub's machines:

1. Set the version in the root `Cargo.toml` (`[workspace.package] version`).
2. Commit, then tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
3. The zips appear on the GitHub release for the tag (and under the run in the Actions tab).

It can also be run by hand from the Actions tab (Run workflow); the zips are then only in the
run's artifacts.

## itch.io

By hand: create the project on itch.io (kind: Downloadable), upload each zip and tick its system.

Automatically, with butler: in the GitHub repository's Settings → Secrets and variables → Actions,

- add the secret `BUTLER_API_KEY` (from https://itch.io/user/settings/api-keys), and
- add the variable `ITCH_GAME`, your itch.io user and game, e.g. `codapop/microcosm`.

Each tagged release then pushes the zips to the `windows`, `mac` and `linux` channels.

## Notes

- **macOS** builds are signed ad hoc (`codesign --sign -` in `package.sh`), not with an Apple
  Developer ID, and aren't notarized (that needs Apple's paid developer membership). The ad-hoc
  signature is required: without it macOS calls the downloaded app "damaged" and only offers the
  Trash. With it, the first launch is blocked as "can't be verified", and players allow it in
  System Settings → Privacy & Security → Open Anyway (on macOS 14 and older, right-click → Open
  also works). The macOS build is Apple silicon only.
- **Windows** builds run without a console window, so command-line messages aren't shown.
- The Linux build is made on Ubuntu 22.04 so it runs on that and newer distributions.
