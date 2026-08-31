#!/usr/bin/env bash
set -euo pipefail

name="$1"
version="$2"
url="https://crates.io/api/v1/crates/${name}/${version}"
max_attempts=60
sleep_seconds=5

for attempt in $(seq 1 "$max_attempts"); do
  status=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "User-Agent: dengjen-tts-publish-ci (https://github.com/ZirekHQ/dengjen-tts)" \
    "$url")
  if [ "$status" = "200" ]; then
    echo "${name} ${version} is live on crates.io"
    exit 0
  fi
  echo "Waiting for ${name} ${version} to index on crates.io (attempt ${attempt}/${max_attempts}, status ${status})..."
  sleep "$sleep_seconds"
done

echo "Timed out waiting for ${name} ${version} to appear on crates.io" >&2
exit 1
