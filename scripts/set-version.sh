#!/usr/bin/env bash
# Sets the release version everywhere it is recorded: Cargo workspace (and Cargo.lock) and the phone app.
# Usage: scripts/set-version.sh 1.0.0
set -euo pipefail
version="${1:-}"
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]]; then
  echo "usage: $0 <semver, e.g. 1.0.0 or 1.0.0-rc.1>" >&2
  exit 1
fi
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
# Only the version line inside [workspace.package].
sed -i.bak '/^\[workspace.package\]/,/^\[/{s/^version *= *".*"/version = "'"$version"'"/}' Cargo.toml && rm Cargo.toml.bak
cargo update --workspace --quiet
(cd apps/phone && npm version "$version" --no-git-tag-version --allow-same-version >/dev/null)
echo "Version set to $version. Next: add a '## [$version] - $(date +%F)' section to CHANGELOG.md (docs/releasing.md)."
