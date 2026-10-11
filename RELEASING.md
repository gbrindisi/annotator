# Releasing

Versions follow SemVer and live in `Cargo.toml` (source of truth, shown by `annotator --version`)
and `web/package.json` (kept in sync). Tags are `vX.Y.Z` on `main`.

## Day to day

Add a line under `## [Unreleased]` in `CHANGELOG.md` with every user-visible change
(`Added` / `Changed` / `Fixed` / `Removed`).

## Cutting a release

```bash
git switch main && git pull
make release VERSION=X.Y.Z      # RELEASE_PUSH=0 to stop before pushing
make install                    # update your local ~/bin/annotator
```

`scripts/release.sh` refuses to run unless you are on a clean `main` in sync with `origin`, the
tag is new and the changelog has unreleased entries. It then:

1. bumps `Cargo.toml`, `Cargo.lock` and `web/package.json` to `X.Y.Z`;
2. moves `[Unreleased]` entries into `## [X.Y.Z] - <date>`;
3. runs `make lint`, `make build` (web UI + release binary) and `make test`, and checks that
   the built binary reports `annotator X.Y.Z`;
4. commits `chore(release): vX.Y.Z`, creates the annotated tag `vX.Y.Z`;
5. pushes `main` and the tag.

If a step fails nothing is committed: fix, `git checkout -- .` to drop the partial bump, rerun.

## Choosing the number

- patch (`0.1.1`): fixes only;
- minor (`0.2.0`): new features, or any change to the comment format, CLI flags or exit codes
  while < 1.0;
- major: after 1.0, breaking changes to the CLI contract agents rely on (`open` output, exit
  codes) or the on-disk comment format.

## Installing

`make install-app` (macOS) rebuilds `~/Applications/Annotator.app` with the current version; the
app itself always runs `~/bin/annotator`, so `make install` is what actually updates behaviour.

`make install` builds and copies the binary to `~/bin` (override with `BINDIR=...`) and stops
a running daemon so the next `annotator open` starts the new version.
