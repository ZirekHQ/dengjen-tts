/// Linearly rescales a `0..=100` percentage onto `[min, max]`. A `value` above 100 (reachable
/// from the C API's `uint8_t` range) clamps to 100 rather than extrapolating past `max`.
pub fn percent_to_param(value: u8, min: f32, max: f32) -> f32 {
    let fraction = f32::from(value.min(100)) / 100.0;
    min + fraction * (max - min)
}

/// Inverse of [`percent_to_param`]: recovers the nearest whole percentage
/// that produced `value` within `[min, max]`.
#[allow(dead_code)]
pub fn param_to_percent(value: f32, min: f32, max: f32) -> u8 {
    let span = max - min;
    let fraction = (value - min) / span;
    (fraction * 100.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_zero_percent_to_the_range_floor() {
        assert_eq!(percent_to_param(0, 0.5, 5.5), 0.5);
    }

    #[test]
    fn maps_one_hundred_percent_to_the_range_ceiling() {
        assert_eq!(percent_to_param(100, 0.5, 5.5), 5.5);
    }

    #[test]
    fn clamps_a_value_above_one_hundred_to_the_range_ceiling() {
        assert_eq!(percent_to_param(255, 0.5, 5.5), 5.5);
    }

    #[test]
    fn maps_fifty_percent_to_the_range_midpoint() {
        assert_eq!(percent_to_param(50, 0.0, 1.0), 0.5);
    }

    #[test]
    fn recovers_the_bounding_percentages_from_the_range_endpoints() {
        assert_eq!(param_to_percent(0.5, 0.5, 5.5), 0);
        assert_eq!(param_to_percent(5.5, 0.5, 5.5), 100);
    }

    #[test]
    fn rounds_to_the_nearest_whole_percent() {
        assert_eq!(param_to_percent(0.503, 0.5, 5.5), 0);
        assert_eq!(param_to_percent(0.53, 0.5, 5.5), 1);
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    // min < max strictly: every real PARAM_RANGE constant in this crate (speed/volume/pitch)
    // satisfies this, and param_to_percent divides by (max - min), so a zero span isn't a
    // case worth modeling here.
    fn param_range() -> impl Strategy<Value = (f32, f32)> {
        (-1000.0f32..1000.0, 0.01f32..1000.0).prop_map(|(min, span)| (min, min + span))
    }

    proptest! {
        #[test]
        fn percent_to_param_stays_within_the_range_for_any_u8(
            value: u8,
            (min, max) in param_range(),
        ) {
            let result = percent_to_param(value, min, max);
            prop_assert!(result >= min && result <= max);
        }

        #[test]
        fn percent_to_param_is_monotonic_over_the_unclamped_domain(
            a in 0u8..=100,
            b in 0u8..=100,
            (min, max) in param_range(),
        ) {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            prop_assert!(percent_to_param(lo, min, max) <= percent_to_param(hi, min, max));
        }

        #[test]
        fn param_to_percent_round_trips_percent_to_param_within_rounding_tolerance(
            value in 0u8..=100,
            (min, max) in param_range(),
        ) {
            let param = percent_to_param(value, min, max);
            let recovered = param_to_percent(param, min, max);
            prop_assert!((i32::from(recovered) - i32::from(value)).abs() <= 1);
        }
    }
}
