#!/bin/sh
# Prepares a release: scripts/release.sh X.Y.Z
# Writes the version in every place that has it, and dates the "Unreleased" section of the changelog.
# It changes nothing else: review the diff, commit it, merge it and push the tag vX.Y.Z.
set -eu

version=${1:-}
case "$version" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "usage: scripts/release.sh X.Y.Z" >&2; exit 1 ;;
esac

cd "$(dirname "$0")/.."

grep -q '^## Unreleased$' CHANGELOG.md || { echo "CHANGELOG.md has no \"## Unreleased\" section to date" >&2; exit 1; }

# each pattern touches the first line that matches, which is the one of the package
replace() { perl -0pi -e "$2" "$1"; }

replace Cargo.toml "s/^version = \".*\"\$/version = \"$version\"/m"
replace deploy/kubernetes/deployment.yaml "s/^(\\s*image: altrdev\\/torchio:).*\$/\${1}$version/m"
replace deploy/helm/torchio/Chart.yaml "s/^version: .*\$/version: $version/m"
replace deploy/helm/torchio/Chart.yaml "s/^appVersion: .*\$/appVersion: $version/m"
replace CHANGELOG.md "s/^## Unreleased\$/## $version - $(date +%Y-%m-%d)/m"

# the lock file has the version of the package too
cargo update --workspace --offline

git --no-pager diff --stat
