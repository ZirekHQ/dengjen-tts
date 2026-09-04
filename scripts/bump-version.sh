#!/usr/bin/env bash
# Bumps every hand-synced copy of the workspace version to the given value.
#
# [workspace.package].version in the root Cargo.toml is the single source of
# truth; every member crate's own [package].version reads it via
# `version.workspace = true`, so those never need touching. Internal
# path-dependencies mostly read the same value via `workspace = true` on the
# [workspace.dependencies] entries this script updates -- the one exception
# is crates/frontends/python/Cargo.toml's `dengjen-tts-piper` dependency,
# which can't use workspace inheritance (it narrows `default-features` to
# false, which Cargo silently ignores on an inherited dependency) and so
# pins its own version string.
#
# The Java bindings (bindings/java) and the Python wheel
# (crates/frontends/python/pyproject.toml) are NOT touched here: Java's
# version comes from its own `java-v*` git tags via the git-versioning
# Gradle plugin, and maturin reads the Python wheel version straight off
# the underlying Rust crate's Cargo.toml -- both already track this
# workspace version (or their own release cadence) automatically.
#
# Usage: scripts/bump-version.sh 1.1.1
set -euo pipefail

new_version="${1:?usage: scripts/bump-version.sh <new-version>}"
if ! echo "$new_version" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
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

# [workspace.package].version, plus every `version = "<old>"` inside
# [workspace.dependencies] (each of those entries pins the same workspace
# version for publish resolution -- see the comment above that table).
# Scoped to lines naming an internal path so this can't collide with an
# unrelated third-party dependency that happens to pin the same version.
sed -i.bak "0,/^version = \"${old_version}\"\$/s//version = \"${new_version}\"/" Cargo.toml
sed -i.bak "/path = \"crates\//s/version = \"${old_version}\"/version = \"${new_version}\"/" Cargo.toml
rm -f Cargo.toml.bak

# The one internal path-dependency that can't use workspace inheritance (see
# comment above).
sed -i.bak "s/version = \"${old_version}\", default-features = false }\$/version = \"${new_version}\", default-features = false }/" crates/frontends/python/Cargo.toml
rm -f crates/frontends/python/Cargo.toml.bak

# Cargo.lock pins each workspace member's own version (matched via --locked
# in several CI steps, e.g. cargo publish), so it goes stale the moment
# Cargo.toml's version changes. cargo check only re-resolves entries that
# are actually inconsistent with Cargo.toml -- since nothing here changed
# any external dependency's version constraint, this touches only the
# workspace-local package entries, not third-party deps. Not --offline: a
# fresh CI runner (e.g. prepare-release.yml) has no pre-warmed registry
# index, and --offline would fail outright rather than just fetch it.
cargo check --workspace --quiet

echo "Bumped ${old_version} -> ${new_version}:"
git diff --stat -- Cargo.toml Cargo.lock crates/frontends/python/Cargo.toml
