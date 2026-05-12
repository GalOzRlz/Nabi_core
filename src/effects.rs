use crate::SharedMidiState;
use fundsp::combinator::An;
use fundsp::prelude64::*;

pub fn master_limiter() -> Net {
    let block = dcblock() >> limiter(0.002, 0.3);
    let master = multipass::<U2>() >> (block.clone() | block);
    Net::wrap(Box::new(master))
}

fn common_follow() -> An<Follow<f64>> {
    follow(0.05)
}

fn cc_controlled_reverb(wet_amount: Net) -> Net {

    // Duplicate wet to stereo (0 inputs, 2 outputs)
    let wet_stereo = wet_amount.clone() | wet_amount.clone();

    let dry_mono = constant(1.0) - wet_amount;
    let dry_stereo = dry_mono.clone() | dry_mono;

    let pass = Net::wrap(Box::new(multipass::<U2>())); // U2 -> U2 identity
    let reverb = Net::wrap(Box::new(reverb_stereo(5.0, 2.5, 0.5))); // U2 -> U2

    (pass * dry_stereo) & (reverb * wet_stereo)
}

pub fn simple_lowpass(cutoff_val: An<Var>, max_cutoff_hz: f32) -> Net {
    let cutoff_hrz = product(constant(max_cutoff_hz), cutoff_val) >> common_follow();
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> lowpass_q(2.0),
    ))
}

pub fn master_lowpass(cc_idx: usize, shared_midi_state: &SharedMidiState, q: f32) -> Net {
    let cutoff_val = var(&shared_midi_state.control_change[cc_idx].clone()) >> common_follow();
    let cutoff_hrz = product(constant(20_000.0), cutoff_val) >> common_follow();
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> lowpass_q(q),
    ))
}

pub fn master_highpass(cc_idx: usize, shared_midi_state: &SharedMidiState, q: f32) -> Net {
    let cutoff_val = var(&shared_midi_state.control_change[cc_idx].clone()) >> common_follow();
    let cutoff_hrz = product(constant(20_000.0), cutoff_val) >> common_follow();
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> highpass_q(q),
    ))
}

pub fn master_reverb(global_fx_cc_idx_1: usize, shared_midi_state: &SharedMidiState) -> Net {
    let reverb_amount: Net = Net::wrap(Box::new(
        var(&shared_midi_state.control_change[global_fx_cc_idx_1].clone()) >> common_follow(),
    ));
    cc_controlled_reverb(reverb_amount)
}

pub fn eq_2_mono(cc1: usize, cc2: usize, q: f32, shared_midi_state: &SharedMidiState) -> Net {
    let hp = master_highpass(cc1, shared_midi_state, q);
    let lp = master_lowpass(cc2, shared_midi_state, q);
    pass() >> lp >> hp
}

pub fn eq_2_stereo(cc1: usize, cc2: usize, q: f32, shared_midi_state: &SharedMidiState) -> Net {
    let hp = master_highpass(cc1, shared_midi_state, q);
    let lp = master_lowpass(cc2, shared_midi_state, q);
    multipass::<U2>() >> (lp.clone() | lp) >> (hp.clone() | hp)
}
