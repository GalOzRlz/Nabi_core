use fundsp::combinator::An;
use fundsp::net::Net;
use fundsp::prelude32::Var;
use fundsp::prelude64::{constant, follow, lowpass_q, moog_q, multipass, pass, product, var, U2};
use crate::effects::{cc_smooth, master_highpass};
use crate::SharedMidiState;

pub fn simple_lowpass(cutoff_val: An<Var>, max_cutoff_hz: f32) -> Net {
    let cutoff_hrz = product(constant(max_cutoff_hz), cutoff_val) >> cc_smooth();
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> lowpass_q(2.0),
    ))
}

pub fn master_lowpass(cc_idx: usize, shared_midi_state: &SharedMidiState, q: f32) -> Net {
    let cutoff_val = var(&shared_midi_state.control_change[cc_idx].clone()) >> cc_smooth();
    let cutoff_hrz = product(constant(20_000.0), cutoff_val) >> cc_smooth();
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> moog_q(q),
    ))
}

pub fn cc_eq_2_mono(cc1: usize, cc2: usize, q: f32, shared_midi_state: &SharedMidiState) -> Net {
    let hp = master_highpass(cc1, shared_midi_state, q);
    let lp = master_lowpass(cc2, shared_midi_state, q);
    pass() >> lp >> hp
}

pub fn cc_eq_2_stereo(cc1: usize, cc2: usize, q: f32, shared_midi_state: &SharedMidiState) -> Net {
    let hp = master_highpass(cc1, shared_midi_state, q);
    let lp = master_lowpass(cc2, shared_midi_state, q);
    multipass::<U2>() >> (lp.clone() | lp) >> (hp.clone() | hp)
}
