use fundsp::combinator::An;
use fundsp::prelude64::*;

pub fn master_reverb(wet: Net) -> Net {
    // Duplicate wet to stereo (0 inputs, 2 outputs)
    let wet_stereo = wet.clone() | wet.clone();

    // Stereo dry signal: 1.0 - wet on each channel (0 inputs, 2 outputs)
    let dry_mono = constant(1.0) - wet;
    let dry_stereo = dry_mono.clone() | dry_mono;

    // Build the effect: (input * dry) + (reverb(input) * wet)
    let pass = Net::wrap(Box::new(multipass::<U2>()));          // U2 -> U2 identity
    let reverb = Net::wrap(Box::new(reverb_stereo(5.0, 2.5, 0.5))); // U2 -> U2

    (pass * dry_stereo) & (reverb * wet_stereo)
}
pub fn simple_lowpass(cutoff_val: An<Var>, max_cutoff_hz: f32) -> An<Pipe<Pipe<Pipe<Stack<Pass, Pipe<Binop<FrameMul<fundsp::typenum::U1>, Constant<typenum::U1>, Var>, Follow<f64>>>, Pipe<Stack<MultiPass<U2>, Constant<U1>>, Svf<f64, LowpassMode<f64>>>>, DCBlock<f64>>, Shaper<Clip>>> {
    let cutoff_hrz = product(constant(max_cutoff_hz / 127.0), cutoff_val);
    (pass() | cutoff_hrz >> follow(0.05_f32)) >> lowpass_q(2.0) >> dcblock() >> clip()
}