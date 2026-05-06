/// Determines the voice stealing strategy:
/// LegatoOldest: Keep envelope and steal the oldest voice
/// LegatoLast: either oldest or latest voice
/// LastRetrigger: steal last voice and re-trigger current
/// OldestRetrigger: steal latest voice and re-trigger current
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum VoiceStealingConfig {
    LegatoOldest,
    LegatoLast,
    OldestRetrigger,
    LastRetrigger,
    // todo: quietest?
}

/// determine if voices are freed by instrument ADSR or by being at zero volume
/// release on zero is a bit costlier but allows for 0.0 release sounds to play more naturally
 #[derive(Debug, Copy, Clone, PartialEq)]
pub enum FreeVoiceStrategy {
    FollowADSR,
    ReleaseOnZero
}

/// Configuration block for extra features
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub voice_stealing: VoiceStealingConfig,
    pub voice_release: FreeVoiceStrategy,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            voice_stealing: VoiceStealingConfig::LegatoOldest,
            voice_release: FreeVoiceStrategy::FollowADSR,
        }
    }
}
