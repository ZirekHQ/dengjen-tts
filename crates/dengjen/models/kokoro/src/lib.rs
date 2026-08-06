mod config;
mod inference;
mod phonemize;
mod voice_style;
mod vocab;

use dengjen_core::{DengjenModel, DengjenResult};
use std::path::Path;
use std::sync::Arc;

pub use config::{load_config, KokoroVoiceConfig};
pub use inference::KokoroModel;
pub use phonemize::text_to_kokoro_phonemes;
pub use voice_style::VoiceStyles;
pub use vocab::Vocab;

pub fn from_config_path(config_path: &Path) -> DengjenResult<Arc<dyn DengjenModel + Send + Sync>> {
    let config = load_config(config_path)?;
    let model = KokoroModel::from_config(config)?;
    Ok(Arc::new(model))
}
