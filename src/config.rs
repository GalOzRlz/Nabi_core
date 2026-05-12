/// Determines the voice stealing strategy:
/// LegatoOldest: Keep envelope and steal the oldest voice
/// LegatoLast: either oldest or latest voice
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum VoiceStealingConfig {
    LegatoOldest,
    LegatoLast,
}

/// Determine if voices are freed from current voices queue by instrument ADSR or by being at zero volume.
/// Release on zero is a bit costlier but allows for 0.0 release sounds to play better.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FreeVoiceStrategy {
    FollowADSR,
    ReleaseOnZero,
}

/// Configuration block for extra features
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub voice_stealing: VoiceStealingConfig,
    pub voice_release: FreeVoiceStrategy,
    pub cc_mappings: Vec<usize>,
    pub cc_start_values: Vec<f32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            voice_stealing: VoiceStealingConfig::LegatoOldest,
            voice_release: FreeVoiceStrategy::ReleaseOnZero,
            // todo: make these overrideable by controller.toml
            cc_mappings: vec![74, 71, 76, 77], // use vec so we can hot-swap?
            cc_start_values: vec![0.0, 0.0, 0.0, 1.0],
        }
    }
}
