use fundsp::combinator::An;
use fundsp::prelude64::*;

pub fn master_reverb(wet_amount: Net) -> Net {
    // todo: 0 wet = multiplass() early
    
    // Duplicate wet to stereo (0 inputs, 2 outputs)
    let wet_stereo = wet_amount.clone() | wet_amount.clone();
    
    let dry_mono = constant(1.0) - wet_amount;
    let dry_stereo = dry_mono.clone() | dry_mono;

    let pass = Net::wrap(Box::new(multipass::<U2>()));          // U2 -> U2 identity
    let reverb = Net::wrap(Box::new(reverb_stereo(5.0, 2.5, 0.5))); // U2 -> U2

    (pass * dry_stereo) & (reverb * wet_stereo)
}

pub fn simple_lowpass(cutoff_val: An<Var>, max_cutoff_hz: f32) -> Net {
    let cutoff_hrz = product(constant(max_cutoff_hz), cutoff_val);
    Net::wrap(Box::new((pass() | cutoff_hrz >> follow(0.05_f32)) >> lowpass_q(2.0) >> dcblock() >> clip()))
}