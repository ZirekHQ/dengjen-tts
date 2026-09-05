#!/usr/bin/env bash




















set -euo pipefail

new_version="${1:?usage: scripts/bump-version.sh <new-version>}"
if ! echo "$new_version" | grep -qE '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'; then
  echo "::error::'${new_version}' doesn't look like a semver version (X.Y.Z)" >&2
  exit 1
fi

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

old_version="$(awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f && /^version = /{gsub(/version = "|"/,""); print; exit}' Cargo.toml)"
if [ -z "$old_version" ]; then
  echo "::error::Couldn't find [workspace.package].version in Cargo.toml" >&2
  exit 1
fi






sed -i.bak "0,/^version = \"${old_version}\"\$/s//version = \"${new_version}\"/" Cargo.toml
sed -i.bak "/path = \"crates\//s/version = \"${old_version}\"/version = \"${new_version}\"/" Cargo.toml
rm -f Cargo.toml.bak



sed -i.bak "s/version = \"${old_version}\", default-features = false }\$/version = \"${new_version}\", default-features = false }/" crates/frontends/python/Cargo.toml
rm -f crates/frontends/python/Cargo.toml.bak









cargo check --workspace --quiet

echo "Bumped ${old_version} -> ${new_version}:"
git diff --stat -- Cargo.toml Cargo.lock crates/frontends/python/Cargo.toml
