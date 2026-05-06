/// Determines whether to steal the oldest or latest notes
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum VoiceStealingConfig {
    Oldest,
    Latest,
}

/// Configuration block for extra features
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub voice_stealing: VoiceStealingConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            voice_stealing: VoiceStealingConfig::Oldest,
        }
    }
}
