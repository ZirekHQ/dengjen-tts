/// Linearly rescales a `0..=100` percentage onto `[min, max]`.
pub fn percent_to_param(value: u8, min: f32, max: f32) -> f32 {
    let fraction = f32::from(value) / 100.0;
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
