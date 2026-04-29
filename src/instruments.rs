use fundsp::prelude64::*;
use std::cmp::max;
/// A comb-filter based plucked string synthesizer with independent pitch and gate control.
///
/// # Inputs
/// - **Input 0**: Frequency (Hz) - can be modulated at audio rate
/// - **Input 1**: Gate signal - rising edge triggers a new pluck
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
}

impl CombPluck {
    /// Create a new plucked string synthesizer.
    ///
    /// # Parameters
    /// - `feedback`: Decay per sample (0.0 to 1.0). Higher = longer sustain.
    /// - `max_delay_seconds`: Maximum delay time for lowest frequency (defines lowest note).
    /// - `excitation_gain`: Volume of initial noise burst (0.0 to 1.0).
    pub fn new(feedback: f32, max_delay_seconds: f32, excitation_gain: f32) -> Self {
        let sample_rate = DEFAULT_SR as f32;
        let max_delay_samples = (max_delay_seconds * sample_rate).ceil() as usize;
        let max_delay_samples = max(max_delay_samples, 2);
        let max_delay_samples_f32 = max_delay_samples as f32;

        Self {
            sample_rate,
            max_delay_samples,
            max_delay_samples_f32,
            inv_max_delay: 1.0 / max_delay_samples_f32,
            feedback: feedback.clamp(0.0, 1.0),
            excitation_gain: excitation_gain.clamp(0.0, 1.0),
            smoothing: 0.05,

            buffer: Vec::with_capacity(max_delay_samples),
            write_pos: 0,
            read_pos_f: 0.0,

            current_freq: 440.0,
            target_freq: 440.0,

            last_gate: 0.0,
        }
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
            let noise = (fastrand::f32() * 2.0 - 1.0) * self.excitation_gain; // todo: provide your own noise source?
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
        self.current_freq = self.current_freq * (1.0 - self.smoothing)
            + self.target_freq * self.smoothing;

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
        let read_frac = self.read_pos_f - read_int ;
        let idx1 = read_int as usize % self.max_delay_samples;
        let idx2 = (read_int as usize + 1) % self.max_delay_samples;

        let delayed = self.buffer[idx1] * (1.0 - read_frac)
            + self.buffer[idx2] * read_frac;

        let output = delayed;
        // todo: add polarity option
        let write_value = delayed * self.feedback + excitation;
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
    type Inputs = typenum::U2;
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
    }

    #[inline]
    fn tick(&mut self, input: &Frame<f32, Self::Inputs>) -> Frame<f32, Self::Outputs> {
        self.target_freq = input[0].max(0.0);
        let gate = input[1];

        // Gate rising edge detection (0→1 transition)
        if self.last_gate <= 0.5 && gate > 0.5 {
            self.pluck();
        }
        self.last_gate = gate;

        if self.target_freq > 0.0 {
            self.update_delay_length();
        }

        let output = self.process_comb(0.0);
        output.into()
    }
}

fn pluck_string_generic(feedback: f32, max_delay_seconds: f32, gain: f32) -> An<CombPluck> {
    let feedback = feedback.clamp(0.0, 1.0);
    let max_delay_seconds = max_delay_seconds.clamp(0.0, 1.3);
    let gain = gain.clamp(0.0, 1.0);
    An(CombPluck::new(feedback, max_delay_seconds, gain))
}

pub fn pluck_comb() -> An<CombPluck>  {
    pluck_string_generic(0.995, 0.1, 0.5)
}
