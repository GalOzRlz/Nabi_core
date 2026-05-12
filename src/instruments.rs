use fundsp::prelude64::*;
use std::cmp::max;

pub enum Polarity {
    Positive,
    Negative,
}

impl Polarity {
    fn to_float(&self) -> f32 {
        match self {
            Polarity::Positive => 1.0,
            Polarity::Negative => -1.0,
        }
    }
}
/// A comb-filter based plucked string synthesizer with independent pitch and gate control.
///
/// # Inputs
/// - **Input 0**: Frequency (Hz) - can be modulated at audio rate
/// - **Input 1**: Gate signal - rising edge triggers a new pluck
///- **Input 3**: Extra excitation signal
///
/// # Outputs
/// - **Output 0**: Audio signal from the vibrating string
#[derive(Clone)]
pub struct CombPluck {
    // Constants
    sample_rate: f32,
    max_delay_samples: usize,
    max_delay_samples_f32: f32,
    inv_max_delay: f32,
    feedback: f32,
    excitation_gain: f32,
    smoothing: f32,
    buffer: Vec<f32>,
    write_pos: usize,
    read_pos_f: f32,
    current_freq: f32,
    target_freq: f32,
    last_gate: f32,
    damping_active: bool,
    minimum_damping_frequency: f32,
    maximum_damping_frequency: f32,
    damping: f32,
    damping_freq: f32,
    g: f32,
    filter_state: f32,
}

impl CombPluck {
    /// Create a new plucked string synthesizer based on damping low pass filter and resonant comb filter.
    ///Inputs 
    /// # Parameters
    /// - `feedback`: Decay per sample (0.0 to 1.0). Higher = longer sustain.
    /// - `max_delay_seconds`: Maximum delay time for lowest frequency (defines lowest note).
    /// - `excitation_gain`: Volume of initial noise burst (0.0 to 1.0).
    /// - `damping`: intensity of low pass filter on the feedback line (0.0 to 1.1)
    /// - `minimum_damping_frequency`: the lowest frequency of the damping filter in hrz
    /// - `polarity`: The comb filters' polarity
    pub fn new(
        feedback: f32,
        max_delay_seconds: f32,
        excitation_gain: f32,
        damping: f32,
        minimum_damping_frequency: f32,
        polarity: Polarity,
    ) -> Self {
        let sample_rate = DEFAULT_SR as f32;
        let max_delay_samples = (max_delay_seconds * sample_rate).ceil() as usize;
        let max_delay_samples = max(max_delay_samples, 2);
        let max_delay_samples_f32 = max_delay_samples as f32;

        let mut s = Self {
            sample_rate,
            max_delay_samples,
            max_delay_samples_f32,
            minimum_damping_frequency,
            maximum_damping_frequency: 20_000.0,
            inv_max_delay: 1.0 / max_delay_samples_f32,
            feedback: feedback.clamp(0.0, 1.0) * polarity.to_float(),
            excitation_gain: excitation_gain.clamp(0.0, 1.0),
            smoothing: 0.05,
            buffer: Vec::with_capacity(max_delay_samples),
            write_pos: 0,
            read_pos_f: 0.0,
            current_freq: 440.0,
            target_freq: 440.0,
            last_gate: 0.0,
            damping_active: false,
            damping: 0.0,
            damping_freq: 0.0,
            g: 0.0,
            filter_state: 0.0,
        };
        s.set_damping(damping);
        s
    }
    pub fn set_damping(&mut self, damping: f32) {
        self.damping_active = damping > 0.0;
        self.damping = damping.clamp(0.0, 1.0);
        self.update_damping();
    }
    fn update_damping(&mut self) {
        self.damping_freq = self.maximum_damping_frequency * (1.0 - self.damping)
            + self.minimum_damping_frequency * self.damping;
        self.g = (-std::f32::consts::TAU * self.damping_freq / self.sample_rate).exp();
    }
    /// Set the smoothing coefficient for frequency changes.
    pub fn set_smoothing(&mut self, smoothing: f32) {
        self.smoothing = smoothing.clamp(0.0, 1.0);
    }

    /// Trigger a new pluck (fill delay line with noise).
    pub fn pluck(&mut self) {
        if self.buffer.is_empty() {
            self.buffer.resize(self.max_delay_samples, 0.0);
        }

        for sample in &mut self.buffer {
            let noise = (fastrand::f32() * 2.0 - 1.0) * self.excitation_gain; // todo: provide your own noise source to mix with the initial noise, similar to pluck() in fundsp
            *sample = noise;
        }

        self.write_pos = 0;
        self.read_pos_f = 0.0;
    }

    /// Update the fractional read position based on current frequency.
    fn update_delay_length(&mut self) {
        if self.buffer.is_empty() {
            return;
        }

        // Smooth frequency transition
        self.current_freq =
            self.current_freq * (1.0 - self.smoothing) + self.target_freq * self.smoothing;

        if self.current_freq <= 0.0 {
            return;
        }

        // Desired delay length in samples (fractional)
        let delay_samples = self.sample_rate / self.current_freq;
        let delay_samples = delay_samples.min(self.max_delay_samples_f32);

        // Read position = write position - delay length (circular)
        let raw_read = self.write_pos as f32 - delay_samples;
        // euclidean modulo
        let max = self.max_delay_samples_f32;
        self.read_pos_f = raw_read - max * (raw_read * self.inv_max_delay).floor();
        self.read_pos_f = raw_read - max * (raw_read * self.inv_max_delay).floor();
    }

    /// Process one sample through the comb filter.
    fn process_comb(&mut self, excitation: f32) -> Frame<f32, typenum::U1> {
        if self.buffer.is_empty() {
            return [0.0].into();
        }

        // Linear interpolation between two buffer positions
        let read_int = self.read_pos_f.floor();
        let read_frac = self.read_pos_f - read_int;
        let idx1 = read_int as usize % self.max_delay_samples;
        let idx2 = (read_int as usize + 1) % self.max_delay_samples;

        let delayed = self.buffer[idx1] * (1.0 - read_frac) + self.buffer[idx2] * read_frac;

        let output = delayed;
        let filtered = if self.damping <= 0.0 {
            delayed
        } else {
            // apply low pass to dampen
            let filtered = delayed * (1.0 - self.g) + self.filter_state * self.g;
            self.filter_state = filtered;
            filtered
        };

        // Apply feedback after filtering
        let write_value = filtered * self.feedback + excitation * self.excitation_gain;
        self.buffer[self.write_pos] = write_value;

        // Advance pointers
        self.write_pos = (self.write_pos + 1) % self.max_delay_samples;
        self.read_pos_f += 1.0;
        if self.read_pos_f >= self.max_delay_samples as f32 {
            self.read_pos_f -= self.max_delay_samples as f32;
        }
        [output as f32].into()
    }
}

impl AudioNode for CombPluck {
    const ID: u64 = 67;
    type Inputs = typenum::U3;
    type Outputs = typenum::U1;

    fn reset(&mut self) {
        self.buffer.clear();
        self.buffer.resize(self.max_delay_samples, 0.0);
        self.write_pos = 0;
        self.read_pos_f = 0.0;
        self.current_freq = 440.0;
        self.target_freq = 440.0;
        self.last_gate = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate as f32;
        let duration_secs = self.max_delay_samples_f32 / self.sample_rate;
        self.max_delay_samples = (duration_secs * self.sample_rate).ceil() as usize;
        self.max_delay_samples = Num::max(self.max_delay_samples, 2);
        self.reset();
        self.update_damping();
    }

    #[inline]
    fn tick(&mut self, input: &Frame<f32, Self::Inputs>) -> Frame<f32, Self::Outputs> {
        self.target_freq = input[0].max(0.0);
        let gate = input[1];
        let excitation = input[2].clamp(-1.0, 1.0);

        // Gate rising edge detection (0→1 transition)
        if self.last_gate <= 0.5 && gate > 0.5 {
            self.pluck();
        }
        self.last_gate = gate;

        if self.target_freq > 0.0 {
            self.update_delay_length();
        }

        let output = self.process_comb(excitation);
        output.into()
    }
}

fn pluck_generic(
    feedback: f32,
    max_delay_seconds: f32,
    gain: f32,
    damping: f32,
    polarity: Polarity,
) -> An<CombPluck> {
    let max_delay_seconds = max_delay_seconds.clamp(0.0, 1.3);
    An(CombPluck::new(
        feedback,
        max_delay_seconds,
        gain,
        damping,
        200.0,
        polarity,
    ))
}

pub fn pluck_comb_string() -> An<CombPluck> {
    pluck_generic(0.995, 0.1, 0.5, 0.01, Polarity::Positive)
}

pub fn hit_comb_pipe() -> An<CombPluck> {
    pluck_generic(0.995, 0.1, 0.7, 0.75, Polarity::Negative)
}

pub fn dirty_guitar() -> fn(Net, An<Var>) -> Net {
    let mix = move |pitch: Net, gate: An<Var>|
        (pitch | gate.clone() | constant(0.0)) >> pluck_comb_string() >> lowpass_hz(9000.0, 0.5);
    mix
}

