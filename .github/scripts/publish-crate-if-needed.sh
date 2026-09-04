#!/usr/bin/env bash
# Publishes crate $1 at version $2 unless crates.io already has it -- lets
# publish.yml be re-run from scratch to retry a partial failure (e.g. one
# crate publishes, the next 403s on a missing token scope) without `cargo
# publish` hard-failing on a version crates.io already has.
#
# crates.io's API 403s any request without a descriptive User-Agent (the
# bare "curl/x.y.z" default doesn't qualify) -- set one explicitly or every
# lookup here fails before it gets to check anything.
#
# curl -sf can't tell "confirmed not published" (404) apart from a transient
# lookup failure (5xx, network blip) -- both are just "nonzero exit" to -f,
# and treating them the same would attempt a real publish on the transient-
# failure path, which cargo then rejects hard if the version *is* actually
# already there. Capture the status code and only branch on a value we've
# actually seen.
set -euo pipefail

crate="${1:?usage: publish-crate-if-needed.sh <crate-name> <version>}"
version="${2:?usage: publish-crate-if-needed.sh <crate-name> <version>}"

status="$(curl -s -o /dev/null -w '%{http_code}' \
  -H "User-Agent: dengjen-tts-publish-ci (https://github.com/ZirekHQ/dengjen-tts)" \
  "https://crates.io/api/v1/crates/${crate}/${version}")"
case "$status" in
  200) echo "${crate} ${version} is already published -- skipping" ;;
  404) cargo publish -p "$crate" --locked --no-verify ;;
  *)
    echo "::error::Unexpected status ${status} checking crates.io for ${crate} ${version}" >&2
    exit 1
    ;;
esac
