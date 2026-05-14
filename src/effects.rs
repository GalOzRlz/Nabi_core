use crate::modulators::{smooth_noise_constructor, smooth_random_lfo};
use crate::SharedMidiState;
use fundsp::combinator::An;
use fundsp::prelude64::*;

pub fn to_net<F:AudioNode + 'static>(fx: An<F>) -> Net {
    Net::wrap(Box::new(fx))
}

pub fn mono_to_stereo(net: Net) -> Net {
    net.clone() | net
}

pub fn master_limiter() -> Net {
    let block = dcblock() >> limiter(0.002, 0.3);
    let master = multipass::<U2>() >> (block.clone() | block);
    to_net(master)
}

fn cc_smooth() -> An<Follow<f64>> {
    follow(0.005)
}

fn sensitive_cc_smooth() -> An<Follow<f64>> {
    follow(0.15)
}

/// Factory for stereo effects with wet/dry control via Net  (suitable for live Midi CC)
fn cc_controlled_wet_dry_fx(wet_amount: Net, effect: Net) -> Net {
    // Duplicate wet to stereo (0 inputs, 2 outputs)
    let wet_amount =  wet_amount >> cc_smooth();
    let wet_stereo = wet_amount.clone() | wet_amount.clone();

    let dry_mono = constant(1.0) - wet_amount;
    let dry_stereo = dry_mono.clone() | dry_mono;

    let pass = Net::wrap(Box::new(multipass::<U2>())); // U2 -> U2 identity
    (pass * dry_stereo) & (effect * wet_stereo)
}

fn cc_controlled_reverb(wet_amount: Net, reverb_time: f32) -> Net {
    let reverb = to_net(reverb_stereo(10.0, reverb_time, 0.4));
    cc_controlled_wet_dry_fx(wet_amount, reverb)
}

pub fn prophet_lowpass_filter() -> Net {
    Net::wrap(Box::new(
    !butterpass() >> butterpass()))
}

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

pub fn master_highpass(cc_idx: usize, shared_midi_state: &SharedMidiState, q: f32) -> Net {
    let cutoff_val = var(&shared_midi_state.control_change[cc_idx].clone()) >> cc_smooth();
    let cutoff_hrz = product(constant(8_000.0), cutoff_val) >> cc_smooth();
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> highpass_q(q),
    ))
}

pub fn master_reverb(global_fx_cc_idx_1: usize, shared_midi_state: &SharedMidiState) -> Net {
    let reverb_amount: Net = Net::wrap(Box::new(
        var(&shared_midi_state.control_change[global_fx_cc_idx_1].clone())
    ));
    cc_controlled_reverb(reverb_amount, 3.0)
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

pub fn tape_wow(depth: Net) -> Net {
    let wow_ms_range = 0.025;
    let flutter_ms_range = 0.0022;
    let center = 0.030;
    let wow_mod = smooth_random_lfo(0.6);
    let flutter_mod = smooth_noise_constructor(smooth3, 9.0);
    let total_wow = (wow_mod * depth.clone() + 2.0) * wow_ms_range;
    let total_flutter = (flutter_mod * depth + 2.0) * flutter_ms_range;
    let mix = (pass() | total_wow + total_flutter)  >> tap_linear(center-wow_ms_range-flutter_ms_range, center+wow_ms_range+flutter_ms_range);
    Net::wrap(Box::new(mix.clone()|mix))
}

pub fn master_tape_effect(cc: usize, shared_midi_state: &SharedMidiState) -> Net {
    let depth: Net = Net::wrap(Box::new(
        var(&shared_midi_state.control_change[cc].clone()))) >> sensitive_cc_smooth();
    tape_wow(depth)
}