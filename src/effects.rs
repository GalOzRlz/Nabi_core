use fundsp::audionode::{Bus, MultiPass, Unop};
use fundsp::combinator::An;
use fundsp::numeric_array::NumericArray;
use fundsp::prelude64::*;
pub fn master_reverb(wet: f32) -> An<Bus<Unop<MultiPass<U2>, FrameMulScalar<U2>>, Unop<impl AudioNode<Inputs=U2, Outputs=U2>, FrameMulScalar<U2>>>> {
    let wet= wet.clamp(0.0, 1.0);
    let dry = 1.0 - wet;
    (multipass() * dry) & (wet * reverb_stereo(10.0, 5.0, 0.5))
}