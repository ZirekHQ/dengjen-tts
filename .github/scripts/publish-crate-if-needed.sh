#!/usr/bin/env bash
# curl captures the HTTP status explicitly rather than using -sf's exit code, since a 404
# (not yet published) and a 5xx/network blip are otherwise indistinguishable and would
# wrongly retry-as-publish either way. Needs an explicit User-Agent or crates.io 403s.
set -euo pipefail

crate="${1:?usage: publish-crate-if-needed.sh <crate-name> <version>}"
version="${2:?usage: publish-crate-if-needed.sh <crate-name> <version>}"

status="$(curl -s -o /dev/null -w '%{http_code}' \
  -H "User-Agent: dengjen-tts-publish-ci (https://github.com/ZirekHQ/dengjen-tts)" \
  "https://crates.io/api/v1/crates/${crate}/${version}")"
case "$status" in
  200) echo "${crate} ${version} is already published -- skipping" ;;
  404)
    if [ "${DRY_RUN:-}" = "true" ]; then
      # No --no-verify here (unlike the real publish below) -- a dry run's
      # whole point is validating the pipeline, and --no-verify would let it
      # pass without ever building the packaged crate.
      cargo publish -p "$crate" --locked --dry-run
    else
      cargo publish -p "$crate" --locked --no-verify
    fi
    ;;
  *)
    echo "::error::Unexpected status ${status} checking crates.io for ${crate} ${version}" >&2
    exit 1
    ;;
esac
