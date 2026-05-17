use std::collections::HashMap;
use crate::config_builder::{TomlEffectSection};
use crate::tunings::{TunerBuilder, TunerEntry};
use crate::{SharedMidiState, SynthFunc};
use fundsp::prelude::{multipass, AudioUnit, U2};
use inventory;
use fundsp::net::Net;
use crate::effects_builders::PatchFxChain;

pub type SoundBuilder = fn(state: &SharedMidiState) -> Box<dyn AudioUnit>;

#[derive(Clone)]
pub struct PatchDef {
    pub function: SynthFunc,
    pub name: String,
    pub tuning: TunerBuilder,
    pub sound_config: Option<toml::Table>,
    pub effects: PatchFxChain,
}

/// Globally registered sound entries.
pub struct SoundEntry {
    /// Name used in TOML files (e.g. "fm_bell").
    pub name: &'static str,
    pub builder: SoundBuilder,
}

inventory::collect!(SoundEntry);

/// Place this inside every sound file to register the builder.
#[macro_export]
macro_rules! register_sound {
    ($name:expr, $builder:ident) => {
        inventory::submit! {
            SoundEntry {
                name: $name,
                builder: $builder as fn(&SharedMidiState) -> Box<dyn AudioUnit>,
            }
        }
    };
}

/// Maximum number of entries controllable via MIDI messages in a MIDI program table.
pub const NUM_PATCH_SLOTS: usize = 2_usize.pow(7);

/// Struct containing all the entries from which you can choose your synths.
pub struct PatchTable {
    pub entries: Vec<PatchDef>,
}

impl PatchTable {
    pub fn new(entries: Vec<PatchDef>) -> Self {
        Self { entries }
    } }

