pub fn percent_to_param(value: u8, min: f32, max: f32) -> f32 {
    let normalized = value as f32 / 100.0;
    min + normalized * (max - min)
}

#[allow(dead_code)]
pub fn param_to_percent(value: f32, min: f32, max: f32) -> u8 {
    let range = max - min;
    let normalized = (value - min) / range;
    (normalized * 100.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_to_param_maps_0_to_the_range_minimum() {
        assert_eq!(percent_to_param(0, 0.5, 5.5), 0.5);
    }

    #[test]
    fn percent_to_param_maps_100_to_the_range_maximum() {
        assert_eq!(percent_to_param(100, 0.5, 5.5), 5.5);
    }

    #[test]
    fn percent_to_param_maps_50_to_the_range_midpoint() {
        assert_eq!(percent_to_param(50, 0.0, 1.0), 0.5);
    }

    #[test]
    fn param_to_percent_is_the_inverse_of_percent_to_param_at_the_range_bounds() {
        assert_eq!(param_to_percent(0.5, 0.5, 5.5), 0);
        assert_eq!(param_to_percent(5.5, 0.5, 5.5), 100);
    }

    #[test]
    fn param_to_percent_rounds_to_the_nearest_whole_percent() {
        assert_eq!(param_to_percent(0.503, 0.5, 5.5), 0);
        assert_eq!(param_to_percent(0.53, 0.5, 5.5), 1);
    }
}
