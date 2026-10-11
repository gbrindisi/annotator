#!/usr/bin/env bash
# Cut a release: bump versions, roll the changelog, verify, commit, tag, push.
# Usage: scripts/release.sh X.Y.Z   (RELEASE_PUSH=0 to skip pushing)
set -euo pipefail

die() { echo "release: $*" >&2; exit 1; }

v="${1:-}"
[[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "usage: $0 X.Y.Z"
tag="v$v"
cd "$(git rev-parse --show-toplevel)"

# Preconditions: on main, clean, in sync with origin, tag unused, changelog has content.
[[ "$(git branch --show-current)" == main ]] || die "not on main"
[[ -z "$(git status --porcelain)" ]] || die "working tree not clean"
git fetch -q origin
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] || die "main differs from origin/main (pull or push first)"
git rev-parse -q --verify "refs/tags/$tag" >/dev/null && die "tag $tag already exists"
unreleased="$(awk '/^## \[Unreleased\]/{f=1;next} /^## \[/{f=0} f' CHANGELOG.md | grep -v '^[[:space:]]*$' || true)"
[[ -n "$unreleased" ]] || die "CHANGELOG.md has nothing under ## [Unreleased]"

# Bump versions (Cargo.toml [package], Cargo.lock, web/package.json).
perl -0pi -e "s/^version = \"[^\"]*\"/version = \"$v\"/m" Cargo.toml   # first match = [package]
(cd web && npm version "$v" --no-git-tag-version --allow-same-version >/dev/null)

# Roll the changelog: [Unreleased] -> [X.Y.Z] - date, fresh empty [Unreleased] on top.
today="$(date +%Y-%m-%d)"
perl -0pi -e "s/^## \[Unreleased\]\n/## [Unreleased]\n\n## [$v] - $today\n/m" CHANGELOG.md

# Verify: lint, tests (incl. web), release build with embedded UI. Updates Cargo.lock.
make lint
make build
make test
built="$(target/release/annotator --version)"
[[ "$built" == "annotator $v" ]] || die "built binary reports '$built', expected 'annotator $v'"

git add Cargo.toml Cargo.lock web/package.json web/package-lock.json CHANGELOG.md
git commit -q -m "chore(release): $tag"
git tag -a "$tag" -m "annotator $tag"
echo "release: committed and tagged $tag"

if [[ "${RELEASE_PUSH:-1}" == 1 ]]; then
  git push -q origin main "$tag"
  echo "release: pushed main and $tag"
else
  echo "release: not pushed; run: git push origin main $tag"
fi
echo "release: install locally with: make install"
