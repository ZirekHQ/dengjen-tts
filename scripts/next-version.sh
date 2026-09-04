#!/usr/bin/env bash
# Computes the next semver version from the last vX.Y.Z tag and the
# Conventional Commit subjects merged to main since then (this repo
# squash-merges, so a PR title *is* the commit subject).
#
#   fix!: ... / feat!: ... / "BREAKING CHANGE:" in a commit body -> major
#   feat: ...                                                    -> minor
#   anything else that isn't purely docs/chore/style/refactor/test -> patch
#   only docs/chore/style/refactor/test since the last tag        -> no release
#
# Usage: scripts/next-version.sh
# Prints the next version (e.g. "1.1.1") to stdout and exits 0.
# Exits 1 (nothing printed) if there's nothing release-worthy since the
# last tag -- this is the normal "skip" case a caller should treat as
# not-an-error. Exits 2 for anything that means the computation itself is
# broken (e.g. no vX.Y.Z tag exists at all) -- a caller must NOT treat this
# as skip, or a broken tag state silently suppresses every future release.
set -euo pipefail

# --merged HEAD restricts to tags actually reachable from the current
# branch -- a vX.Y.Z tag pushed on some other branch (a future hotfix, a
# stray release cut elsewhere) must not become "the last release" here just
# because its number sorts higher; only a tag that's actually an ancestor
# of HEAD is a real baseline to diff from.
#
# --list's pattern is a glob, not a regex -- 'v[0-9]*.[0-9]*.[0-9]*' would
# also match a prerelease tag like "v1.1.0-rc1" (the trailing [0-9]* happily
# absorbs "0-rc1"). Grep with an anchored regex afterward so only an exact
# vX.Y.Z tag counts. This also excludes this repo's other tag families
# (go-release-v*, grpc-release-v*, java-v*, publish-*), which don't carry a
# bare vX.Y.Z name.
#
# The `|| true` matters under pipefail: grep exits 1 when no tag matches,
# and without it that failure would abort the script right here (exit 1,
# same code as the deliberate "nothing to release" case below) instead of
# reaching the explicit -z check and its distinct exit 2.
last_tag="$(git tag --list 'v*' --merged HEAD --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -1 || true)"
if [ -z "$last_tag" ]; then
  echo "::error::No vX.Y.Z tag found to compute the next version from" >&2
  exit 2
fi

version="${last_tag#v}"
major="${version%%.*}"
rest="${version#*.}"
minor="${rest%%.*}"
patch="${rest#*.}"

subjects="$(git log "${last_tag}..HEAD" --pretty='%s')"
bodies="$(git log "${last_tag}..HEAD" --pretty='%b')"

if [ -z "$subjects" ]; then
  echo "::notice::No commits since ${last_tag} -- nothing to release" >&2
  exit 1
fi

bump=""
while IFS= read -r subject; do
  [ -z "$subject" ] && continue
  # Conventional Commit header: type(scope)!: description -- '!' before ':' marks a breaking change.
  if echo "$subject" | grep -qE '^[a-zA-Z]+(\([^)]*\))?!:'; then
    bump="major"
    break
  fi
  if echo "$subject" | grep -qE '^feat(\([^)]*\))?:'; then
    [ "$bump" != "major" ] && bump="minor"
    continue
  fi
  # docs/chore/style/refactor/test alone don't warrant a release.
  if echo "$subject" | grep -qE '^(docs|chore|style|refactor|test)(\([^)]*\))?:'; then
    continue
  fi
  [ -z "$bump" ] && bump="patch"
done <<< "$subjects"

if echo "$bodies" | grep -qE '^BREAKING[ -]CHANGE:'; then
  bump="major"
fi

if [ -z "$bump" ]; then
  echo "::notice::Only docs/chore/style/refactor/test commits since ${last_tag} -- nothing to release" >&2
  exit 1
fi

case "$bump" in
  major) echo "$((major + 1)).0.0" ;;
  minor) echo "${major}.$((minor + 1)).0" ;;
  patch) echo "${major}.${minor}.$((patch + 1))" ;;
esac
