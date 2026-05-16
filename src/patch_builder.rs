use crate::config_builder::CcValuesArray;
use crate::tunings::TunerBuilder;
use crate::{SharedMidiState, SynthFunc};
use fundsp::prelude::{multipass, AudioUnit, U2};
use inventory;
use std::sync::Arc;
use fundsp::net::Net;

pub type SoundBuilder = fn(state: &SharedMidiState) -> Box<dyn AudioUnit>;

/// Globally registered sound entries.
pub struct PatchEntry {
    /// Name used in TOML files (e.g. "fm_bell").
    pub name: &'static str,
    pub builder: SoundBuilder,
}

inventory::collect!(PatchEntry);

/// Place this inside every sound file to register the builder.
#[macro_export]
macro_rules! register_sound {
    ($name:expr, $builder:ident) => {
        inventory::submit! {
            PatchEntry {
                name: $name,
                builder: $builder as fn(&SharedMidiState) -> Box<dyn AudioUnit>,
            }
        }
    };
}

/// Maximum number of entries controllable via MIDI messages in a MIDI program table.
pub const NUM_PATCH_SLOTS: usize = 2_usize.pow(7);

/// A Speaker Definition enum to handle either separate L/R output or true stereo instruments (i.e., with U2 outputs).
#[derive(Clone)]
pub enum SpeakerDef {
    Stereo(SynthFunc),
    LR { left: SynthFunc, right: SynthFunc },
}

/// A Trait to turn an AudioUnit or a tuple of AudioUnit into a SpeakerDef containing SynthFunc(s).
pub trait IntoSpeakerDef {
    fn into_speaker_def(self) -> SpeakerDef;
}

/// Into a SpeakerDef::Stereo for a single AudioUnit
impl<F> IntoSpeakerDef for F
where
    F: Fn(&SharedMidiState) -> Box<dyn AudioUnit> + Send + Sync + 'static,
{
    fn into_speaker_def(self) -> SpeakerDef {
        SpeakerDef::Stereo(Arc::new(self))
    }
}

/// Return an owned SpeakerDef::LR and for a tuple of AudioUnits
impl<L, R> IntoSpeakerDef for (L, R)
where
    L: Fn(&SharedMidiState) -> Box<dyn AudioUnit> + Send + Sync + 'static,
    R: Fn(&SharedMidiState) -> Box<dyn AudioUnit> + Send + Sync + 'static,
{
    fn into_speaker_def(self) -> SpeakerDef {
        SpeakerDef::LR {
            left: Arc::new(self.0),
            right: Arc::new(self.1),
        }
    }
}

/// convenience type for a Program Table item with name and SpeakerDef.
pub type PatchTableItem = (String, SpeakerDef, CcValuesArray, TunerBuilder);

/// Struct containing all the entries from which you can choose your synths.
pub struct PatchTable {
    pub entries: Vec<PatchTableItem>,
}

impl PatchTable {
    pub fn new(entries: Vec<PatchTableItem>) -> Self {
        Self { entries }
    } }


pub fn connect_node_vec(node_vec: &Vec<Net>, mut starting_net: Option<Net>) -> Net {
    let mut net = starting_net.unwrap_or_else(|| Net::wrap(Box::new(multipass::<U2>())));
    for node in node_vec.to_owned().into_iter() {
        net = net >> node;
    }
    net
}
