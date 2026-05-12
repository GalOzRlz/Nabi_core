use crate::config_builder::ENCODER_COUNT;
use crate::{SharedMidiState, SynthFunc};
use fundsp::{
    math::{clamp01, xerp},
    net::Net,
    prelude::AudioUnit,
    prelude64::{adsr_live, envelope2, moog_q},
};
use inventory;
use std::sync::Arc;

pub type SoundBuilder = fn(state: &SharedMidiState) -> Box<dyn AudioUnit>;

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

#[macro_export]
/// Convenience macro to build a `ProgramTable` struct. Given a sequence of tuples of `&str` objects
/// and `SynthFunc` objects, it returns a proper `ProgramTable`.
macro_rules! program_table {
    ($( ($name:expr, $def:expr) ),* $(,)?) => {
        ProgramTable::new(vec![
            $( ($name.to_owned(), $def.into_speaker_def()) ),*
        ])
    };
}

/// Maximum number of entries controllable via MIDI messages in a MIDI program table.
pub const NUM_PROGRAM_SLOTS: usize = 2_usize.pow(7);

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
pub type ProgramTableItem = (String, SpeakerDef, [f32; ENCODER_COUNT]);

/// Struct containing all the entries from which you can choose your synths.
pub struct ProgramTable {
    pub entries: Vec<ProgramTableItem>,
}

impl ProgramTable {
    pub fn new(entries: Vec<ProgramTableItem>) -> Self {
        Self { entries }
    } }

/// Pipes a pitch into `synth`, then modulates the output volume depending on MIDI status.
pub fn simple_sound(state: &SharedMidiState, synth: Box<dyn AudioUnit>) -> Box<dyn AudioUnit> {
    let control = state.control_var();
    state.assemble_unpitched_sound(
        synth,
        Box::new(control >> envelope2(move |_, n| clamp01(n))),
    )
}

#[derive(Copy, Clone, Debug)]
/// Represents ADSR (Attack/Decay/Sustain/Release) settings for the purpose of generating MIDI-ready sounds.
pub struct Adsr {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Adsr {
    /// Returns an ADSR filter in a `Box`.
    pub fn boxed(&self, state: &SharedMidiState) -> Box<dyn AudioUnit> {
        let control = state.control_var();
        Box::new(control >> adsr_live(self.attack, self.decay, self.sustain, self.release))
    }

    /// Returns an ADSR filter in a `Net64`.
    pub fn net64ed(&self, state: &SharedMidiState) -> Net {
        Net::wrap(self.boxed(state))
    }

    /// Stacks pitch with an ADSR and pipes them into `timed_sound`. Useful for any sound needing two
    /// inputs, where the first is a pitch and the second is time-varying information.
    pub fn timed_sound(&self, timed_sound: Box<dyn AudioUnit>, state: &SharedMidiState) -> Net {
        Net::pipe(
            Net::stack(state.bent_pitch(), self.net64ed(state)),
            Net::wrap(timed_sound),
        )
    }

    /// Stacks `source` with an ADSR that is piped into an exponential interpolator.
    /// Thes two stacked inputs are then piped into a Moog filter.
    pub fn timed_moog(&self, source: Box<dyn AudioUnit>, state: &SharedMidiState) -> Net {
        Net::pipe(
            Net::stack(
                Net::wrap(source),
                Net::pipe(
                    self.net64ed(state),
                    Net::wrap(Box::new(envelope2(move |_, n| xerp(1100.0, 11000.0, n)))),
                ),
            ),
            Net::wrap(Box::new(moog_q(0.6))),
        )
    }

    /// Convenience method to create a ready-to-go sound using `timed_sound()` above.
    pub fn assemble_timed(
        &self,
        timed_sound: Box<dyn AudioUnit>,
        state: &SharedMidiState,
    ) -> Box<dyn AudioUnit> {
        state.assemble_pitched_sound(
            Box::new(self.timed_sound(timed_sound, state)),
            self.boxed(state),
        )
    }
}
