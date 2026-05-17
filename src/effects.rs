use crate::effects_builders::{EffectDef, EffectInput};
use std::collections::HashMap;
use std::f32::consts::LN_2;
use std::f64::consts::{PI};
use crate::modulators::{smooth_noise_constructor, smooth_random_lfo};
use crate::{register_effect, SharedMidiState};
use fundsp::combinator::An;
use fundsp::prelude64::*;
use toml::Table;
use crate::effects_builders::EffectBuilder;
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

pub fn master_tape_effect(cc: usize, shared_midi_state: &SharedMidiState) -> Net {
    let depth: Net = Net::wrap(Box::new(
        var(&shared_midi_state.control_change[cc].clone()))) >> sensitive_cc_smooth();
    tape_wow(depth)
}

pub fn pitch_shifter(pitch_st: f32, freq_hz: f32, wet_amt: f32) -> Net {
    let max_delay = 0.1; // 100 ms – supports grain rates down to ~10 Hz for octave shifts
    let freq_hz= freq_hz.clamp(20.0, 100.0);
    let ratio = (pitch_st * LN_2 * 1.0).exp();   // 2.0 for +12, 0.5 for -12
    let depth_mag = ((ratio - 1.0).abs() / freq_hz)
        .min(max_delay  * 0.999);
    let min_delay = max_delay - depth_mag;

    let phasor = lfo(move |t: f64| {
        let phase = (t * freq_hz as f64).fract();
        phase
    });

    // Two candidate delay modulations:
    // - up_delay: decreasing delay → pitch up
    // - down_delay: increasing delay → pitch down
    let up_delay = dc(max_delay) - (phasor.clone() * dc(depth_mag ));
    let down_delay = dc(min_delay) + (phasor * dc(depth_mag ));

    // Select which delay to use based on sign of pitch_st
    let control = dc(if pitch_st >= 0.0 { 1.0 } else { 0.0 });
    let mod_sig = up_delay * control.clone() + down_delay * (dc(1.0) - control);

    let amp_env = lfo(move |t: f64| {
        let phase = (t * freq_hz as f64).fract();
        0.5 - 0.5 * (2.0 * PI * phase).cos()
    });

    // Apply the modulated delay line
    let shifted = (pass() | mod_sig) >> tap(min_delay, max_delay);

    let shifted_env = shifted * amp_env;

    // Smooth with short decay delay
    let feedback_line = feedback(delay(0.003) * 0.5);

    // Dry/wet wet_amount with feedback on wet path
    let dry = pass() * dc(1.0 - wet_amt);
    let wet = shifted_env >> feedback_line * dc(wet_amt);
    to_net(dry & wet)
}


pub fn frequency_shifter_factory(input: &EffectInput) -> EffectBuilder {
    let pitch_st = input.param("pitch_st", 12.0);
    let freq_hz   = input.param("freq_hz", 60.0);
    let wet_amount_cc    = input.cc_knob("wet_amount");
    Box::new(move | state| {
        let ps = pitch_shifter(pitch_st as f32, freq_hz as f32, 1.0);
        let wet_amount = state.control_change_net(wet_amount_cc);
        cc_controlled_wet_dry_fx(wet_amount, ps)

    })
}

register_effect!("dirty_pitch_shifter", frequency_shifter_factory,
    construction_params: [ ("pitch_st", 12.0), ("freq_hz", 60.0),],
    cc_params: [ ("wet_amount", 2, 0.6) ]
);


fn fundsp_reverb_factory(input: &EffectInput) -> EffectBuilder {
    let room_size = input.param("room_size", 0.8);
    let damping   = input.param("damping", 0.5);
    let length    = input.param("length", 3.0);
    let wet_amount_cc    = input.cc_knob("wet_amount");
    Box::new(move | state| {
        let wet_amount = state.get_control_change(wet_amount_cc);
        cc_controlled_reverb(to_net(wet_amount), length as f32, room_size as f32, damping as f32)
    })
}

register_effect!("reverb", fundsp_reverb_factory,
    construction_params: [ ("room_size", 0.8), ("damping", 0.5), ("length", 3.0) ],
    cc_params: [ ("wet_amount", 1, 0.5) ]
);
