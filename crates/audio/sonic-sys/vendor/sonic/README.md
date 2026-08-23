# Vendored sonic source

`sonic.c`/`sonic.h` are vendored from
[waywardgeek/sonic](https://github.com/waywardgeek/sonic) at commit
[`8694c59`](https://github.com/waywardgeek/sonic/commit/8694c59) — the exact
commit this project's `deps/sonic` submodule was already pinned to
(`release-0.2.0-96-g8694c59`) before this vendoring change, chosen to
guarantee zero behavior change.

[espeak-ng/sonic](https://github.com/espeak-ng/sonic) was evaluated as an
alternative (it's actively maintained by the espeak-ng project, which this
repo already depends on via `deps/espeak-ng`, and adds input clamping on
speed/pitch/rate/volume/sample-rate/channel-count) but was rejected: its
`SONIC_MIN_VOLUME` floor of `0.01f` means `sonicSetVolume(stream, 0.0)` no
longer produces true digital silence, breaking this project's
`apply_to_raw_samples_with_volume_zero_mutes_the_signal` test — a real
functional regression, not the pure mechanism swap this vendoring change was
scoped to be. Revisit if that's ever intentionally relaxed.

Sonic is Copyright 2010, 2011, Bill Cox, released under the Apache License,
Version 2.0 (see `LICENSE` in this directory).

To update: pull the desired revision's `sonic.c`/`sonic.h`/`LICENSE` from
either upstream, replace the files here, and update this README's pinned
commit.
