use crate::sound_builders::PatchEntry;
use fundsp::audiounit::AudioUnit;
use fundsp::prelude64::{saw, square};
use fundsp::prelude::lowpass_hz;
use crate::{register_sound, SharedMidiState};
use crate::sound_builders::Adsr;

fn basic_pluck() -> Box<dyn AudioUnit> {
    Box::new((square() & saw()) >> lowpass_hz::<f32>(3000.0, 0.5))
}
pub fn clavichord_soft(state: &SharedMidiState) -> Box<dyn AudioUnit> {
    let adsr = Adsr {
        attack: 0.01,
        decay: 0.2,
        sustain: 0.1,
        release: 0.5,
    };
    state.assemble_unpitched_sound(basic_pluck(), adsr.boxed(state))
}

register_sound!("clavichord_soft", clavichord_soft);