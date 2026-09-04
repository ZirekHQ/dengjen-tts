#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonic_stream_create_and_destroy_round_trip_returns_non_null_and_does_not_crash() {
        // SAFETY: `sonicCreateStream` has no aliasing/lifetime preconditions beyond returning a
        // pointer that must be null-checked before use, which the code below does immediately.
        let handle = unsafe { sonicCreateStream(22050, 1) };

        if handle.is_null() {
            panic!("sonicCreateStream(22050, 1) unexpectedly returned a null stream pointer");
        }

        // SAFETY: `handle` was just confirmed non-null above and is not used again after this
        // call, satisfying `sonicDestroyStream`'s valid-handle, no-double-free contract.
        unsafe { sonicDestroyStream(handle) };
    }
}
