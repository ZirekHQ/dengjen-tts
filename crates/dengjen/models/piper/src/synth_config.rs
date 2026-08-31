use dengjen_tts_core::SynthesisConfig;
use std::collections::HashMap;

pub const NOISE_SCALE: &str = "noise_scale";
pub const LENGTH_SCALE: &str = "length_scale";
pub const NOISE_W: &str = "noise_w";

/// Piper's own standard factory defaults, used when a `SynthesisConfig`'s generic
/// `parameters` map omits one of these keys. `0.0` is not a usable length scale (or noise
/// scale/weight), so a missing key must fall back to a real value, not `f32::default()`.
const DEFAULT_NOISE_SCALE: f32 = 0.667;
const DEFAULT_LENGTH_SCALE: f32 = 1.0;
const DEFAULT_NOISE_W: f32 = 0.8;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PiperSynthesisConfig {
    pub speaker: Option<i64>,
    pub length_scale: f32,
    pub noise_scale: f32,
    pub noise_w: f32,
}

impl From<&PiperSynthesisConfig> for SynthesisConfig {
    fn from(config: &PiperSynthesisConfig) -> Self {
        SynthesisConfig {
            speaker: config.speaker,
            parameters: HashMap::from([
                (LENGTH_SCALE.to_string(), config.length_scale),
                (NOISE_SCALE.to_string(), config.noise_scale),
                (NOISE_W.to_string(), config.noise_w),
            ]),
        }
    }
}

impl From<&SynthesisConfig> for PiperSynthesisConfig {
    fn from(config: &SynthesisConfig) -> Self {
        PiperSynthesisConfig {
            speaker: config.speaker,
            length_scale: config
                .parameters
                .get(LENGTH_SCALE)
                .copied()
                .unwrap_or(DEFAULT_LENGTH_SCALE),
            noise_scale: config
                .parameters
                .get(NOISE_SCALE)
                .copied()
                .unwrap_or(DEFAULT_NOISE_SCALE),
            noise_w: config
                .parameters
                .get(NOISE_W)
                .copied()
                .unwrap_or(DEFAULT_NOISE_W),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piper_config_round_trips_through_synthesis_config() {
        let piper = PiperSynthesisConfig {
            speaker: Some(3),
            length_scale: 1.5,
            noise_scale: 0.5,
            noise_w: 0.9,
        };
        let generic = SynthesisConfig::from(&piper);
        assert_eq!(generic.speaker, Some(3));
        assert_eq!(generic.parameters.get(LENGTH_SCALE), Some(&1.5));
        let round_tripped = PiperSynthesisConfig::from(&generic);
        assert_eq!(round_tripped, piper);
    }

    #[test]
    fn missing_parameter_keys_fall_back_to_piper_factory_defaults_not_zero() {
        let generic = SynthesisConfig {
            speaker: None,
            parameters: HashMap::new(),
        };
        let piper = PiperSynthesisConfig::from(&generic);
        assert_eq!(piper.length_scale, DEFAULT_LENGTH_SCALE);
        assert_eq!(piper.noise_scale, DEFAULT_NOISE_SCALE);
        assert_eq!(piper.noise_w, DEFAULT_NOISE_W);
    }
}
