use dengjen_tts_core::SynthesisConfig;
use std::collections::HashMap;

pub(crate) const NOISE_SCALE: &str = "noise_scale";
pub(crate) const LENGTH_SCALE: &str = "length_scale";
pub(crate) const NOISE_SCALE_W: &str = "noise_scale_w";

pub struct MeloSynthesisConfig {
    pub speaker: Option<i64>,
    pub noise_scale: f32,
    pub length_scale: f32,
    pub noise_scale_w: f32,
}

impl From<&MeloSynthesisConfig> for SynthesisConfig {
    fn from(config: &MeloSynthesisConfig) -> Self {
        SynthesisConfig {
            speaker: config.speaker,
            parameters: HashMap::from([
                (NOISE_SCALE.to_string(), config.noise_scale),
                (LENGTH_SCALE.to_string(), config.length_scale),
                (NOISE_SCALE_W.to_string(), config.noise_scale_w),
            ]),
        }
    }
}

impl From<&SynthesisConfig> for MeloSynthesisConfig {
    fn from(config: &SynthesisConfig) -> Self {
        MeloSynthesisConfig {
            speaker: config.speaker,
            noise_scale: config
                .parameters
                .get(NOISE_SCALE)
                .copied()
                .unwrap_or_default(),
            length_scale: config
                .parameters
                .get(LENGTH_SCALE)
                .copied()
                .unwrap_or_default(),
            noise_scale_w: config
                .parameters
                .get(NOISE_SCALE_W)
                .copied()
                .unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_generic_synthesis_config_without_losing_fields() {
        let original = MeloSynthesisConfig {
            speaker: Some(3),
            noise_scale: 0.667,
            length_scale: 1.2,
            noise_scale_w: 0.8,
        };
        let generic: SynthesisConfig = (&original).into();
        let round_tripped: MeloSynthesisConfig = (&generic).into();

        assert_eq!(round_tripped.speaker, Some(3));
        assert_eq!(round_tripped.noise_scale, 0.667);
        assert_eq!(round_tripped.length_scale, 1.2);
        assert_eq!(round_tripped.noise_scale_w, 0.8);
    }

    #[test]
    fn preserves_unrelated_parameters_already_present_on_the_generic_config() {
        // A generic SynthesisConfig carrying a parameter this crate doesn't know about
        // (set by another backend, or a future MeloTTS parameter) must survive a
        // MeloSynthesisConfig round-trip unless explicitly overwritten. This documents
        // the current limitation: converting generic -> Melo -> generic drops any
        // parameter key MeloSynthesisConfig has no field for, so any setter built on
        // top of this struct must merge onto the full SynthesisConfig's `parameters`
        // map directly rather than only through this round-trip.
        let mut generic = SynthesisConfig {
            speaker: Some(1),
            parameters: HashMap::new(),
        };
        generic.parameters.insert(NOISE_SCALE.to_string(), 0.5);
        generic
            .parameters
            .insert("some_future_param".to_string(), 42.0);

        let melo: MeloSynthesisConfig = (&generic).into();
        let back: SynthesisConfig = (&melo).into();

        assert_eq!(back.parameters.get(NOISE_SCALE), Some(&0.5));
        assert!(
            !back.parameters.contains_key("some_future_param"),
            "expected this round-trip to drop unknown parameters -- any setter built on \
             MeloSynthesisConfig alone (not merging onto the full SynthesisConfig) would \
             silently lose them, which is exactly the #105 bug class"
        );
    }
}
