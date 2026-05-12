use crate::SharedMidiState;
use fundsp::combinator::An;
use fundsp::prelude64::*;

pub fn to_net<F:AudioNode + 'static>(fx: An<F>) -> Net {
    Net::wrap(Box::new(fx))
}

pub fn master_limiter() -> Net {
    let block = dcblock() >> limiter(0.002, 0.3);
    let master = multipass::<U2>() >> (block.clone() | block);
    to_net(master)
}

fn common_follow() -> An<Follow<f64>> {
    follow(0.05)
}

fn cc_controlled_wet_dry_fx(wet_amount: Net, effect: Net) -> Net {
    // Duplicate wet to stereo (0 inputs, 2 outputs)
    let wet_amount =  wet_amount >> common_follow();
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
    let cutoff_hrz = product(constant(max_cutoff_hz), cutoff_val) >> common_follow();
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> lowpass_q(2.0),
    ))
}

pub fn master_lowpass(cc_idx: usize, shared_midi_state: &SharedMidiState, q: f32) -> Net {
    let cutoff_val = var(&shared_midi_state.control_change[cc_idx].clone()) >> common_follow();
    let cutoff_hrz = product(constant(20_000.0), cutoff_val) >> common_follow();
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> moog_q(q),
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

pub fn pitch_drift_mono() -> Net {
    // A very slow, shallow chorus. Only one voice, 100% wet.
    // delay: base delay (seconds), depth: modulation depth (seconds),
    // freq: LFO rate (Hz), mix: 1.0 = all wet (vibrato).
    Net::wrap(Box::new(pure_chorus(
        1,       // voices
        0.009,   // base delay (15 ms)
        0.003,   // depth (3 ms) – small = few cents of detune
        2.45,    // LFO frequency (0.15 Hz) – very slow
    )))
}

pub fn master_drift(cc: usize, shared_midi_state: &SharedMidiState) -> Net {
    let wet_amount: Net = Net::wrap(Box::new(
        var(&shared_midi_state.control_change[cc].clone())));
    let drift = pitch_drift_mono();
    let stereo = (drift.clone() | drift);
    cc_controlled_wet_dry_fx(wet_amount, stereo)
}

pub fn pure_chorus(
    seed: u64,
    separation: f32,
    variation: f32,
    mod_frequency: f32,
) -> An<impl AudioNode<Inputs =prelude::U1, Outputs =prelude::U1>> {
    (pass()
        | prelude::lfo(move |t| {
        (
            lerp11(
                separation,
                separation + variation,
                fractal_noise(seed, 8, 0.45, t * mod_frequency),
            ),
            lerp11(
                separation * 2.0,
                separation * 2.0 + variation,
                fractal_noise(hash1(seed), 8, 0.45, t * (mod_frequency + 0.02)),
            ),
            lerp11(
                separation * 3.0,
                separation * 3.0 + variation,
                fractal_noise(hash2(seed), 8, 0.45, t * (mod_frequency + 0.04)),
            ),
            lerp11(
                separation * 4.0,
                separation * 4.0 + variation,
                fractal_noise(hash1(seed ^ 0xfedcba), 8, 0.45, t * (mod_frequency + 0.06)),
            ),
        )
    })
        .interval(0.01))
        >> fundsp::prelude::multitap::<fundsp::prelude::U4>(separation, separation * 4.0 + variation)
        * fundsp::prelude::dc(0.2)
}