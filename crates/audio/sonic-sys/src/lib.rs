// bindgen's generated bindings mirror the C naming conventions of deps/sonic/sonic.h, which
// these lints don't recognize as idiomatic Rust.
#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]

// Pulls in the bindgen output that build.rs generates from deps/sonic/sonic.h at build time.
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonic_stream_create_and_destroy_round_trip_returns_non_null_and_does_not_crash() {
        let handle = unsafe { sonicCreateStream(22050, 1) };

        if handle.is_null() {
            panic!("sonicCreateStream(22050, 1) unexpectedly returned a null stream pointer");
        }

        unsafe { sonicDestroyStream(handle) };
    }
}
