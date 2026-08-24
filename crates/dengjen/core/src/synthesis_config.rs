use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SynthesisConfig {
    pub speaker: Option<i64>,
    pub parameters: HashMap<String, f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_synthesis_config_has_no_speaker_and_no_parameters() {
        let config = SynthesisConfig::default();
        assert_eq!(config.speaker, None);
        assert!(config.parameters.is_empty());
    }

    #[test]
    fn synthesis_config_stores_a_named_parameter() {
        let mut config = SynthesisConfig::default();
        config.parameters.insert("noise_scale".to_string(), 0.667);
        assert_eq!(config.parameters.get("noise_scale"), Some(&0.667));
    }
}
