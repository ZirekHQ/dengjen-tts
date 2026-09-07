#!/usr/bin/env bash
















set -euo pipefail


















if [ -n "${PREVIOUS_TAG:-}" ]; then
  last_tag="$PREVIOUS_TAG"
  if ! echo "$last_tag" | grep -qE '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'; then
    echo "::error::PREVIOUS_TAG '${last_tag}' doesn't look like a vX.Y.Z tag" >&2
    exit 2
  fi
  if ! git rev-parse -q --verify "refs/tags/${last_tag}" >/dev/null; then
    echo "::error::PREVIOUS_TAG '${last_tag}' does not exist in this repo" >&2
    exit 2
  fi
  if ! git merge-base --is-ancestor "$last_tag" HEAD; then
    echo "::error::PREVIOUS_TAG '${last_tag}' is not an ancestor of HEAD -- the ${last_tag}..HEAD range wouldn't reflect commits since that release" >&2
    exit 2
  fi
else
  last_tag="$(git tag --list 'v*' --merged HEAD --sort=-v:refname | grep -E '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' | head -1 || true)"
  if [ -z "$last_tag" ]; then
    echo "::error::No vX.Y.Z tag found to compute the next version from" >&2
    exit 2
  fi
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
  
  if echo "$subject" | grep -qE '^[a-zA-Z]+(\([^)]*\))?!:'; then
    bump="major"
    break
  fi
  if echo "$subject" | grep -qE '^feat(\([^)]*\))?:'; then
    [ "$bump" != "major" ] && bump="minor"
    continue
  fi
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
