#[derive(Debug, Clone, Default)]
pub struct PiperSynthesisConfig {
    pub speaker: Option<i64>,
    pub noise_scale: f32,
    pub length_scale: f32,
    pub noise_w: f32,
}

#[derive(Debug, Clone)]
pub enum SynthesisConfig {
    Piper(PiperSynthesisConfig),
    None,
}
