use crate::modulators::{smooth_noise_constructor, smooth_random_lfo};
use crate::SharedMidiState;
use fundsp::combinator::An;
use fundsp::prelude64::*;
use std::f32::consts::{LN_2};
use std::f64::consts::PI;

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

fn cc_controlled_reverb(wet_amount: Net, reverb_time: f32) -> Net {
    let reverb = to_net(reverb_stereo(10.0, reverb_time, 0.4));
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

pub fn master_reverb(global_fx_cc_idx_1: usize, shared_midi_state: &SharedMidiState) -> Net {
    let reverb_amount: Net = Net::wrap(Box::new(
        var(&shared_midi_state.control_change[global_fx_cc_idx_1].clone())
    ));
    cc_controlled_reverb(reverb_amount, 3.0)
}

pub fn tape_wow(depth: Net) -> Net {
    let wow_ms_range = 0.025;
    let flutter_ms_range = 0.0022;
    let center = 0.030;
    let wow_mod = smooth_random_lfo(0.6);
    let flutter_mod = smooth_noise_constructor(smooth3, 9.0);
    let total_wow = (wow_mod * depth.clone() + 2.0) * wow_ms_range;
    let total_flutter = (flutter_mod * depth + 2.0) * flutter_ms_range;
    let min_delay = center-wow_ms_range-flutter_ms_range;
    let max_delay = center+wow_ms_range+flutter_ms_range;
    let mix = (pass() | total_wow + total_flutter)  >> tap_linear(min_delay, max_delay);
    Net::wrap(Box::new(mix.clone()|mix))
}

pub fn master_tape_effect(cc: usize, shared_midi_state: &SharedMidiState) -> Net {
    let depth: Net = Net::wrap(Box::new(
        var(&shared_midi_state.control_change[cc].clone()))) >> sensitive_cc_smooth();
    tape_wow(depth)
}

pub fn pitch_shifter() -> Net {
    let max_delay = 0.1;          // 30 ms grain buffer
    let pitch_st = 12.0;            // +12 semitones = one octave up
    let freq_hz = 40.0;              // grain rate in Hz (independent of pitch)     // 0..1
    let wet_amt = 0.5;              // dry/wet mix

    let ratio = (pitch_st.abs() * LN_2 / 12.0).exp();
    let depth = (ratio - 1.0) / freq_hz;
    let depth = depth.min(max_delay * 0.99).max(0.0); // ensure non‑negative
    let depth_clamped = depth.min(max_delay * 0.99);
    let min_delay = max_delay - depth_clamped;

    let amp_env = lfo(move |t: f64| {
        let phase = (t * freq_hz as f64).fract();
        let win = 0.5 - 0.5 * (2.0 * PI * phase).cos();
        win
    });

    let phasor = lfo(move |t: f64| {
         (t * freq_hz as f64).fract()
    });

    let feedback = feedback(delay(0.003) * 0.6);
    let mod_sig = dc(max_delay) - (phasor * depth);
    let shifted = (pass() | mod_sig) >> tap(min_delay, max_delay);
    let shifted_env = shifted * amp_env;

    let output = (pass() * dc(1.0 - wet_amt)) & (shifted_env >> feedback * dc(wet_amt));
    to_net(output)
}

/// Pitch shifter using a modulated delay line (Doppler effect).
///
/// # Arguments
/// * `pitch_st` - Pitch shift in semitones (positive = up, negative = down)
/// * `freq_hz`  - Grain rate (Hz). Lower values sound more granular/gritty.
/// * `wet_amt`  - Wet/dry mix (0.0 = dry only, 1.0 = wet only)
///
/// # Note
/// For a full octave shift (±12 semitones) at low grain rates (e.g., 20 Hz),
/// you may need to increase `max_delay` (see constant below).
pub fn frequency_shifter(freq_hz: f32, wet_amt: f32, shared_midi_state: SharedMidiState) -> Net {
    let pitch_st =  1.0
    let max_delay = 0.1; // 100 ms – supports grain rates down to ~10 Hz for octave shifts

    let ratio = (pitch_st * LN_2 * 1.0).exp();   // 2.0 for +12, 0.5 for -12
    let depth_mag = ((ratio - 1.0).abs() / freq_hz)
        .min(max_delay  * 0.999);
    let min_delay = max_delay - depth_mag;

    // Rising sawtooth LFO (0 → 1) – same for both directions
    let phasor = lfo(move |t: f64| {
        let phase = (t * freq_hz as f64).fract();
        phase
    });

    // Two candidate delay modulations:
    // - up_delay: decreasing delay → pitch up
    // - down_delay: increasing delay → pitch down
    let up_delay = dc(max_delay) - (phasor.clone() * dc(depth_mag as f32));
    let down_delay = dc(min_delay) + (phasor * dc(depth_mag as f32));

    // Select which delay to use based on sign of pitch_st
    let control = dc(if pitch_st >= 0.0 { 1.0 } else { 0.0 });
    let mod_sig = up_delay * control.clone() + down_delay * (dc(1.0) - control);

    // Amplitude envelope (Hann window) to smooth grain transitions
    let amp_env = lfo(move |t: f64| {
        let phase = (t * freq_hz as f64).fract();
        0.5 - 0.5 * (2.0 * PI * phase).cos()
    });

    // Apply the modulated delay line
    let shifted = (pass() | mod_sig) >> tap(min_delay, max_delay);
    let shifted_env = shifted * amp_env;

    // Optional feedback echo (you can remove or adjust)
    let feedback_line = feedback(delay(0.003) * 0.5);

    // Dry/wet mix with feedback on wet path
    let dry = pass() * dc(1.0 - wet_amt);
    let wet = shifted_env >> feedback_line * dc(wet_amt);
    to_net(dry & wet)
}
