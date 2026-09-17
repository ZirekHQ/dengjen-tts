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

## Fuzzing

Each phonemizer/model crate with a `fuzz/` directory has one or more `cargo-fuzz` targets. CI
only builds them (`fuzz-build` job) so they don't bit-rot — it doesn't run them, since actual
fuzzing is too slow for every PR. This is deliberate, not a gap: run one yourself with
`cargo +nightly fuzz run <target>` from the crate that owns it whenever you touch the logic that
target exercises.

## Benchmarks

`crates/audio/ops` and `crates/dengjen/synth` each have `divan` benches. CI's `benchmarks` job
runs audio/ops's bench for real and posts the numbers to the job summary — it's self-contained, no
fixtures needed. Synth's benches need real Piper voice fixtures that aren't committed to this
repo, so CI only compile-checks them (via the `clippy` job's `--benches` flag); running them
yourself needs fixtures at `crates/dengjen/synth/models/{std,rt}/` — see
[#220](https://github.com/ZirekHQ/dengjen-tts/issues/220) for provisioning those in CI.

## Workspace dependencies

`crates/workspace-hack` unifies feature flags for shared dependencies across the workspace's 14
crates (via [`cargo-hakari`](https://docs.rs/cargo-hakari)), so CI jobs that check the same crate
under different feature combinations (e.g. `coverage`) don't force a rebuild of shared deps on
every switch. After adding, removing, or changing the features of a dependency, run
`cargo hakari generate` and commit the result — CI's `hakari` job fails if it's stale.

## Getting started

See [README.md](../README.md) for build instructions and [CLAUDE.md](../CLAUDE.md) for the lint
and CI conventions enforced on this repo.
