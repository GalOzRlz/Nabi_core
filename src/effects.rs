use crate::effects_builders::{to_stereo, EffectDef};
use crate::modulators::{smooth_noise_constructor, smooth_random_lfo};
use crate::{register_effect, SharedMidiState};
use fundsp::combinator::An;
use fundsp::prelude64::*;
use std::collections::HashMap;

use crate::effects_builders::EffectFunc;
use crate::eqs::master_lowpass;

pub fn to_net<F:AudioNode + 'static>(fx: An<F>) -> Net {
    Net::wrap(Box::new(fx))
}

pub fn master_limiter() -> Net {
    let block = dcblock() >> limiter(0.002, 0.3);
    let master = multipass::<U2>() >> (block.clone() | block);
    to_net(master)
}

pub(crate) fn cc_smooth() -> An<Follow<f64>> {
    follow(0.005)
}

fn sensitive_cc_smooth() -> An<Follow<f64>> {
    follow(0.15)
}

/// Factory for stereo effects with wet/dry control via Net  (suitable for live Midi CC)
fn cc_controlled_wet_dry_fx(wet_amount: Net, effect: Net) -> Net {
    // Duplicate wet to stereo (0 inputs, 2 outputs)
    let effect = to_stereo(effect);
    let wet_amount =  wet_amount >> cc_smooth();
    let wet_stereo = wet_amount.clone() | wet_amount.clone();

    let dry_mono = constant(1.0) - wet_amount;
    let dry_stereo = dry_mono.clone() | dry_mono;

    let pass = Net::wrap(Box::new(multipass::<U2>())); // U2 -> U2 identity
    (pass * dry_stereo) & (effect * wet_stereo)
}

fn cc_controlled_reverb(wet_amount: Net, reverb_time: f32, room_size: f32, damping: f32) -> Net {
    let reverb = to_net(reverb_stereo(room_size, reverb_time, damping));
    cc_controlled_wet_dry_fx(wet_amount, reverb)
}

pub fn prophet_lowpass_filter() -> Net {
    Net::wrap(Box::new(
    !butterpass() >> butterpass()))
}

pub fn master_highpass(cc_idx: usize, shared_midi_state: &SharedMidiState, q: f32) -> Net {
    let cutoff_val = var(&shared_midi_state.control_change[cc_idx].clone()) >> cc_smooth();
    let cutoff_hrz = product(constant(8_000.0), cutoff_val) >> cc_smooth();
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> highpass_q(q),
    ))
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
    let wet_amount = (pass() | total_wow + total_flutter)  >> tap_linear(center-wow_ms_range-flutter_ms_range, center+wow_ms_range+flutter_ms_range);
    Net::wrap(Box::new(wet_amount.clone()|wet_amount))
}

pub fn tape_effect_factory(params: &TapeParams, cc_map: &HashMap<String, usize>) -> EffectFunc {
    let _ = params.blank;
    let depth_val    = *cc_map.get("depth").unwrap_or(&0);
    Box::new(move | state| {
        let depth_net = to_net(state.get_control_change(depth_val));
        tape_wow(depth_net)
    })
}

register_effect!(
    struct: Tape,
    name: "tape_drift",
    factory: tape_effect_factory,
    construction_params: [(blank, 0.0)],
    cc_params: [("depth", 2, 0.4)]
);


fn fundsp_reverb_factory(params: &ReverbParams, cc_map: &HashMap<String, usize>) -> EffectFunc {
    let room_size = params.room_size;   // ← typed, compiler‑checked
    let damping   = params.damping;
    let length    = params.length;
    let mix_cc    = *cc_map.get("wet_amount").unwrap_or(&0);
    Box::new(move | state| {
        let wet_amount = state.get_control_change(mix_cc);
        cc_controlled_reverb(to_net(wet_amount), length as f32, room_size as f32, damping as f32)
    })
}

register_effect!(
    struct: Reverb,
    name: "simple_reverb",
    factory: fundsp_reverb_factory,
    construction_params: [(room_size, 8.8), (damping, 0.5), (length, 2.8)],
    cc_params: [("wet_amount", 1, 0.5)]
);

