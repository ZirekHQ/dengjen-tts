# dengjen-espeak-ng-sys

Rust bindings to [espeak-ng](https://github.com/espeak-ng/espeak-ng), vendoring and building it
from source via `bindgen` + `cmake`.

This is a fork of [`thewh1teagle/piper-rs`](https://github.com/thewh1teagle/piper-rs)'s
`espeak-rs-sys` crate, republished under a new name because the upstream repository has not cut a
release containing the `espeak-ng` submodule bump needed to build against current `espeak-ng`
(missing `espeak_TextToPhonemesWithTerminator`, see
[thewh1teagle/piper-rs#29](https://github.com/thewh1teagle/piper-rs/issues/29) /
[#30](https://github.com/thewh1teagle/piper-rs/pull/30)). It carries that fix.

Consumed by [`dengjen-tts`](https://github.com/ZirekHQ/dengjen-tts), which is where this crate's
publish workflow and source snapshot live — see
`.github/workflows/publish-espeak-ng-sys.yml` and `.github/publish-templates/dengjen-espeak-ng-sys/`.

The `espeak-ng` sources bundled in this crate are GPL-3.0-or-later; see `espeak-ng/COPYING`. The
binding glue code itself is MIT, matching the upstream crate it forks.
