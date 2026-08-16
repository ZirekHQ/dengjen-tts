# Contributing to Dengjen

Thanks for taking the time to contribute. Two lightweight conventions make reviews easier and
keep the project's history useful — neither is required to get a PR merged.

## Commit signing (recommended)

A signed commit lets anyone verify it actually came from you, not someone spoofing your name and
email. GitHub marks signed commits "Verified," and it's one of the cheapest supply-chain
protections available. See [GitHub's guide to commit signing](https://docs.github.com/en/authentication/managing-commit-signature-verification)
for GPG, SSH, or S/MIME setup — a few minutes, one time.

## Conventional Commit PR titles (recommended)

We squash-merge, so the PR title becomes the commit that lands on `main`. Prefixing it with a
type — `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:` — lets us auto-generate changelogs
and keeps `git log` skimmable. Example: `fix: handle empty phoneme_id_map entries in piper`. See
[conventionalcommits.org](https://www.conventionalcommits.org/) for the full spec.

Not following either convention won't block your PR — a maintainer may just tweak the title or
ask you to sign before merging.

## Getting started

See [README.md](../README.md) for build instructions and [CLAUDE.md](../CLAUDE.md) for the lint
and CI conventions enforced on this repo.
