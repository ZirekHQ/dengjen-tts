# Relicense to GPL-3.0-or-later Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Relicense dengjen workspace-wide from MIT to GPL-3.0-or-later, while preserving the MIT
attribution the original Sonata fork's code still requires.

**Architecture:** Two mechanical, metadata-only tasks — no source code changes, no behavior
changes. Task 1 declares the new license at the top level (root `LICENSE`, a new `NOTICE` file
retaining the original MIT text, `README.md`, and a new `[workspace.package]` table in the root
`Cargo.toml`). Task 2 propagates it to every workspace crate's `Cargo.toml` via Cargo's workspace
field inheritance (`license.workspace = true`), replacing the one existing hardcoded `license =
"MIT"` and adding the field to the nine crates that don't declare one today.

**Tech Stack:** Cargo workspace inheritance (`[workspace.package]` / `license.workspace = true`,
stable since Cargo 1.64).

## Global Constraints

- Target license: **GPL-3.0-or-later**, applied to the whole workspace (every crate), per the
  amended Licensing section of `docs/superpowers/specs/2026-08-06-core-engine-rewrite-design.md`.
- `deps/` submodules (`libtashkeel`, `tqsm`, `espeak-ng`, `sonic`, `nanosnap`) are third-party code
  dengjen doesn't own — their own licenses are untouched by this plan.
- MIT's copyright/license-text retention requirement for code still substantially unmodified from
  the original Sonata fork must be satisfied via a `NOTICE` file, not silently dropped.
- No source code, dependency versions, or CI configuration changes — this plan touches only
  license-declaring files (`LICENSE`, `NOTICE`, `README.md`, `Cargo.toml` files).
- Every crate ends up with the same license value — no per-crate exceptions (the "mixed" licensing
  option was explicitly considered and rejected during brainstorming).

---

### Task 1: Root license declaration

**Files:**
- Modify: `LICENSE` (currently MIT text)
- Create: `NOTICE`
- Modify: `README.md:90-92` (License section)
- Modify: `Cargo.toml` (add `[workspace.package]`)

**Interfaces:**
- Produces: the root `Cargo.toml`'s `[workspace.package]` table with `license =
  "GPL-3.0-or-later"` — Task 2's `license.workspace = true` references depend on this table
  existing.

- [ ] **Step 1: Replace the root `LICENSE` file with the canonical GPL-3.0 text**

The current `LICENSE` file is the MIT license (verify: `head -1 LICENSE` currently shows `MIT
License`). Replace it with the FSF's canonical GPLv3 text — don't hand-transcribe a legal document;
fetch the authoritative source:

```bash
curl -fsSL https://www.gnu.org/licenses/gpl-3.0.txt -o LICENSE
```

- [ ] **Step 2: Verify the fetched LICENSE file**

Run: `head -3 LICENSE`
Expected output starts with:
```
                    GNU GENERAL PUBLIC LICENSE
                       Version 3, 29 June 2007
```

Run: `wc -l LICENSE`
Expected: roughly 675 lines (the canonical GPLv3 text). If the command failed (e.g. no network
access) or the output doesn't match, stop and report — do not substitute a paraphrased or partial
version of the license text.

- [ ] **Step 3: Create the `NOTICE` file**

This preserves the original MIT copyright/license text that dengjen's Sonata-derived code was
under, satisfying MIT's retention requirement now that the combined work is distributed under
GPL-3.0-or-later (GPL permits incorporating MIT-licensed code; MIT still requires the notice to
survive). Create `NOTICE` at the repo root with exactly this content:

```
This project (dengjen) is licensed under the GNU General Public License v3.0 or
later (GPL-3.0-or-later) — see LICENSE.

dengjen began as a fork of Sonata (https://github.com/mush42/sonata) by Musharraf
Omer, originally distributed under the MIT License below. Per the MIT License's
terms, its copyright and permission notice is retained here for any code in this
repository that remains substantially unmodified from that origin. Incorporating
this MIT-licensed code into the GPL-3.0-or-later-licensed work as a whole is
permitted by the GPL; this notice satisfies the MIT License's own retention
requirement for the portions it covers.

---

MIT License

Copyright (c) 2023 Musharraf Omer

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 4: Update `README.md`'s License section**

Currently (`README.md:90-92`):

```markdown
# License

Copyright (c) 2023 Musharraf Omer. This code is licensed under the  MIT license.
```

Replace with:

```markdown
# License

Licensed under the GNU General Public License v3.0 or later (GPL-3.0-or-later) — see
[LICENSE](LICENSE). dengjen began as a fork of [Sonata](https://github.com/mush42/sonata) by
Musharraf Omer, originally MIT-licensed; see [NOTICE](NOTICE) for retained attribution.
```

- [ ] **Step 5: Add `[workspace.package]` to the root `Cargo.toml`**

Current root `Cargo.toml`:

```toml
[workspace]
resolver = "2"

members = [
    "crates/dengjen/core",
    "crates/dengjen/synth",
    "crates/dengjen/models/piper",
    "crates/frontends/grpc",
    "crates/frontends/python",
    "crates/frontends/capi",
    "crates/frontends/cli",
    "crates/text/espeak-phonemizer",
    "crates/audio/sonic-sys",
    "crates/audio/ops",
]

[patch.crates-io]
libtashkeel_core = { path = "./deps/libtashkeel/crates/core" }

[profile.release]
opt-level = 3
lto = true
strip = true
codegen-units = 1
```

Add a `[workspace.package]` table after `members` and before `[patch.crates-io]`:

```toml
[workspace]
resolver = "2"

members = [
    "crates/dengjen/core",
    "crates/dengjen/synth",
    "crates/dengjen/models/piper",
    "crates/frontends/grpc",
    "crates/frontends/python",
    "crates/frontends/capi",
    "crates/frontends/cli",
    "crates/text/espeak-phonemizer",
    "crates/audio/sonic-sys",
    "crates/audio/ops",
]

[workspace.package]
license = "GPL-3.0-or-later"

[patch.crates-io]
libtashkeel_core = { path = "./deps/libtashkeel/crates/core" }

[profile.release]
opt-level = 3
lto = true
strip = true
codegen-units = 1
```

- [ ] **Step 6: Verify the workspace still parses**

Run: `cargo metadata --format-version=1 --no-deps > /dev/null`
Expected: exits 0 (a `[workspace.package]` table with no members referencing it yet via
`license.workspace = true` is valid TOML and a valid, unused Cargo table — this just confirms the
edit didn't break `Cargo.toml` syntax; Task 2 is what actually makes each crate use it).

- [ ] **Step 7: Commit**

```bash
git add LICENSE NOTICE README.md Cargo.toml
git commit -m "Relicense to GPL-3.0-or-later: LICENSE, NOTICE, README, workspace.package"
```

---

### Task 2: Propagate the license to every workspace crate

**Files:**
- Modify: `crates/dengjen/core/Cargo.toml`
- Modify: `crates/dengjen/synth/Cargo.toml`
- Modify: `crates/dengjen/models/piper/Cargo.toml`
- Modify: `crates/frontends/grpc/Cargo.toml`
- Modify: `crates/frontends/python/Cargo.toml`
- Modify: `crates/frontends/capi/Cargo.toml`
- Modify: `crates/frontends/cli/Cargo.toml`
- Modify: `crates/text/espeak-phonemizer/Cargo.toml`
- Modify: `crates/audio/sonic-sys/Cargo.toml`
- Modify: `crates/audio/ops/Cargo.toml`

**Interfaces:**
- Consumes: the `[workspace.package]` table with `license = "GPL-3.0-or-later"` from Task 1.

All ten edits are the same mechanical pattern: add `license.workspace = true` to each crate's
`[package]` table, immediately after the `edition` line (matching where a `license` field
conventionally sits). One crate (`sonic-sys`) already has an explicit `license = "MIT"` line to
replace instead of add.

- [ ] **Step 1: Confirm current state**

Run: `grep -rn "^license" --include=Cargo.toml crates/`
Expected: exactly one match — `crates/audio/sonic-sys/Cargo.toml:7:license = "MIT"`. The other nine
crates have no `license` line at all yet.

- [ ] **Step 2: `crates/dengjen/core/Cargo.toml`**

Current:
```toml
[package]
name = "dengjen-core"
version = "0.2.0"
edition = "2021"
```

Change to:
```toml
[package]
name = "dengjen-core"
version = "0.2.0"
edition = "2021"
license.workspace = true
```

- [ ] **Step 3: `crates/dengjen/synth/Cargo.toml`**

Current:
```toml
[package]
name = "dengjen-synth"
version = "0.2.0"
edition = "2021"
```

Change to:
```toml
[package]
name = "dengjen-synth"
version = "0.2.0"
edition = "2021"
license.workspace = true
```

- [ ] **Step 4: `crates/dengjen/models/piper/Cargo.toml`**

Current:
```toml
[package]
name = "dengjen-piper"
version = "0.2.0"
edition = "2021"
```

Change to:
```toml
[package]
name = "dengjen-piper"
version = "0.2.0"
edition = "2021"
license.workspace = true
```

- [ ] **Step 5: `crates/frontends/grpc/Cargo.toml`**

Current:
```toml
[package]
name = "dengjen-grpc"
version = "0.2.0"
edition = "2021"
```

Change to:
```toml
[package]
name = "dengjen-grpc"
version = "0.2.0"
edition = "2021"
license.workspace = true
```

- [ ] **Step 6: `crates/frontends/python/Cargo.toml`**

Current:
```toml
[package]
name = "dengjen-python"
version = "0.2.0"
edition = "2021"
```

Change to:
```toml
[package]
name = "dengjen-python"
version = "0.2.0"
edition = "2021"
license.workspace = true
```

- [ ] **Step 7: `crates/frontends/capi/Cargo.toml`**

Current:
```toml
[package]
name = "libdengjen"
version = "0.1.0"
edition = "2021"
```

Change to:
```toml
[package]
name = "libdengjen"
version = "0.1.0"
edition = "2021"
license.workspace = true
```

- [ ] **Step 8: `crates/frontends/cli/Cargo.toml`**

Current:
```toml
[package]
name = "dengjen-cli"
version = "0.2.0"
edition = "2021"
```

Change to:
```toml
[package]
name = "dengjen-cli"
version = "0.2.0"
edition = "2021"
license.workspace = true
```

- [ ] **Step 9: `crates/text/espeak-phonemizer/Cargo.toml`**

Current:
```toml
[package]
name = "espeak-phonemizer"
version = "1.0.0"
edition = "2021"
```

Change to:
```toml
[package]
name = "espeak-phonemizer"
version = "1.0.0"
edition = "2021"
license.workspace = true
```

- [ ] **Step 10: `crates/audio/sonic-sys/Cargo.toml`** (the one existing explicit license field)

Current:
```toml
[package]
name = "sonic-sys"
version = "1.0.0"
edition = "2021"
authors = ["Musharraf Omer <ibnomer2011@hotmail.com>"]
description = "Raw FFI bindings to sonic rate boost library"
license = "MIT"
keywords = ["FFI", "sonic", "audio", "tts", "speech"]
categories = ["external-ffi-bindings", "api-bindings"]
repository = "https://github.com/mush42/sonic-sys"
homepage = "https://github.com/mush42/sonic-sys"
readme = "README.md"
```

Replace only the `license = "MIT"` line — leave `authors`/`repository`/`homepage` untouched (they
accurately describe this FFI-bindings code's origin; the relicense doesn't need to erase that):

```toml
[package]
name = "sonic-sys"
version = "1.0.0"
edition = "2021"
authors = ["Musharraf Omer <ibnomer2011@hotmail.com>"]
description = "Raw FFI bindings to sonic rate boost library"
license.workspace = true
keywords = ["FFI", "sonic", "audio", "tts", "speech"]
categories = ["external-ffi-bindings", "api-bindings"]
repository = "https://github.com/mush42/sonic-sys"
homepage = "https://github.com/mush42/sonic-sys"
readme = "README.md"
```

- [ ] **Step 11: `crates/audio/ops/Cargo.toml`**

Current:
```toml
[package]
name = "audio-ops"
version = "1.0.0"
edition = "2021"
```

Change to:
```toml
[package]
name = "audio-ops"
version = "1.0.0"
edition = "2021"
license.workspace = true
```

- [ ] **Step 12: Verify every crate now reports the new license**

Run:
```bash
cargo metadata --format-version=1 --no-deps | python3 -c "
import json, sys
data = json.load(sys.stdin)
for pkg in data['packages']:
    print(f\"{pkg['name']}: {pkg['license']}\")
"
```

Expected: all 10 packages (`dengjen-core`, `dengjen-synth`, `dengjen-piper`, `dengjen-grpc`,
`dengjen-python`, `libdengjen`, `dengjen-cli`, `espeak-phonemizer`, `sonic-sys`, `audio-ops`) print
`GPL-3.0-or-later`.

- [ ] **Step 13: Verify the workspace still builds**

Run: `cargo build --workspace`
Expected: clean build, no errors (license metadata changes don't affect compilation — this
confirms no TOML syntax was broken across the 10 edits).

- [ ] **Step 14: Commit**

```bash
git add crates/dengjen/core/Cargo.toml crates/dengjen/synth/Cargo.toml crates/dengjen/models/piper/Cargo.toml crates/frontends/grpc/Cargo.toml crates/frontends/python/Cargo.toml crates/frontends/capi/Cargo.toml crates/frontends/cli/Cargo.toml crates/text/espeak-phonemizer/Cargo.toml crates/audio/sonic-sys/Cargo.toml crates/audio/ops/Cargo.toml
git commit -m "Relicense every workspace crate to GPL-3.0-or-later via workspace inheritance"
```

---

## Final check

- [ ] Run `cargo build --workspace` — clean build.
- [ ] Run `cargo metadata --format-version=1 --no-deps` and confirm all 10 packages report
  `GPL-3.0-or-later` (Task 2, Step 12).
- [ ] Confirm `LICENSE` is the canonical GPLv3 text, `NOTICE` exists with the original MIT text,
  and `README.md`'s License section references both.

## Out of scope

- Ripping out the `espeak`/`tashkeel` Cargo feature-gating machinery in `dengjen-piper` and its
  consumers (`cli`, `grpc`, `python`, `capi`) — no longer load-bearing for license reasons, but
  removing it is optional cleanup, not required by this plan (per the spec amendment).
- Adopting piper1-gpl's `libpiper` runtime — now license-unblocked, still a separate future
  decision, not part of this plan.
- Removing `NOTICE` (tracked by issue #18) — not a near-term action.
- Any change to `deps/` submodule licenses — third-party code, not dengjen's to relicense.
- Kokoro model backend work — paused mid-brainstorm to resolve this licensing question first;
  resumes as its own spec/plan once this lands.
