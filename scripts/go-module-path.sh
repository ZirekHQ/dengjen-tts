#!/usr/bin/env bash
set -euo pipefail

base="github.com/ZirekHQ/dengjen-tts-go"
version="${1:?usage: scripts/go-module-path.sh <vX.Y.Z>}"

if [[ ! "$version" =~ ^v(0|[1-9][0-9]*)\.[0-9]+\.[0-9]+$ ]]; then
  echo "::error::'${version}' is not a vX.Y.Z version" >&2
  exit 1
fi

major="${BASH_REMATCH[1]}"
if [[ "$major" -ge 2 ]]; then
  echo "${base}/v${major}"
else
  echo "$base"
fi
