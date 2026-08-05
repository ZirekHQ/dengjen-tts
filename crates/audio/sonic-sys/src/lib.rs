#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonic_stream_create_and_destroy_round_trip_returns_non_null_and_does_not_crash() {
        unsafe {
            let stream = sonicCreateStream(22050, 1);
            assert!(!stream.is_null());
            sonicDestroyStream(stream);
        }
    }
}
