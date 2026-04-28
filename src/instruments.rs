//! Karplus-Strong plucked string synthesis using a variable-frequency comb filter.
//!
//! This implementation features:
//! - Real-time pitch changes (pitch bend, vibrato, glissando)
//! - Gate-triggered excitation (no re-plucking on frequency changes)
//! - Smooth frequency interpolation to avoid clicks
//! - No dynamic allocations after initialization

use fundsp::prelude64::*;

/// A plucked string synthesizer with independent pitch and gate control.
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
    sample_rate: f64,
    max_delay_samples: usize,   // in samples, integer
    feedback: f64,
    excitation_gain: f64,
    smoothing: f64,

    // Delay line state
    buffer: Vec<f32>,           // audio samples are f64
    write_pos: usize,
    read_pos_f: f64,            // fractional read position

    // Pitch tracking
    current_freq: f64,
    target_freq: f64,

    // Gate detection
    last_gate: f32,
}

impl CombPluck {
    /// Create a new plucked string synthesizer.
    ///
    /// # Parameters
    /// - `feedback`: Decay per sample (0.0 to 1.0). Higher = longer sustain.
    /// - `max_delay_seconds`: Maximum delay time for lowest frequency.
    /// - `excitation_gain`: Volume of initial noise burst (0.0 to 1.0).
    pub fn new(feedback: f64, max_delay_seconds: f64, excitation_gain: f64) -> Self {
        let sample_rate = 44100.0;
        let max_delay_samples = (max_delay_seconds * sample_rate).ceil() as usize;
        let max_delay_samples = Num::max(max_delay_samples, 2);

        Self {
            sample_rate,
            max_delay_samples,
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
    pub fn set_smoothing(&mut self, smoothing: f64) {
        self.smoothing = smoothing.clamp(0.0, 1.0);
    }

    /// Trigger a new pluck (fill delay line with noise).
    pub fn pluck(&mut self) {
        if self.buffer.is_empty() {
            self.buffer.resize(self.max_delay_samples, 0.0);
        }

        for sample in &mut self.buffer {
            let noise = (fastrand::f32() * 2.0 - 1.0) * self.excitation_gain as f32;
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
        let delay_samples = delay_samples.min(self.max_delay_samples as f64);

        // Read position = write position - delay length (circular)
        let raw_read = self.write_pos as f64 - delay_samples;
        self.read_pos_f = raw_read.rem_euclid(self.max_delay_samples as f64);
    }

    /// Process one sample through the comb filter.
    fn process_comb(&mut self, excitation: f64) -> Frame<f32, typenum::U1> {
        if self.buffer.is_empty() {
            return [0.0].into();
        }

        // Linear interpolation between two buffer positions
        let read_int = self.read_pos_f.floor();
        let read_frac = self.read_pos_f - read_int ;
        let idx1 = read_int as usize % self.max_delay_samples;
        let idx2 = (read_int as usize + 1) % self.max_delay_samples;

        let delayed = self.buffer[idx1] as f64 * (1.0 - read_frac)
            + self.buffer[idx2] as f64 * read_frac;

        let output = delayed;
        let write_value = delayed * self.feedback + excitation;
        self.buffer[self.write_pos] = write_value as f32;

        // Advance pointers
        self.write_pos = (self.write_pos + 1) % self.max_delay_samples;
        self.read_pos_f += 1.0;
        if self.read_pos_f >= self.max_delay_samples as f64 {
            self.read_pos_f -= self.max_delay_samples as f64;
        }
        [output as f32].into()
    }
}
unsafe impl Send for CombPluck {}
unsafe impl Sync for CombPluck {}

impl AudioNode for CombPluck {
    const ID: u64 = 67;
    type Inputs = typenum::U1;
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
        self.sample_rate = sample_rate;
        // Recalculate max delay samples for new sample rate, preserving the same time duration
        let duration_secs = self.max_delay_samples as f64 / 44100.0; // original was based on 44.1k
        self.max_delay_samples = (duration_secs * sample_rate).ceil() as usize;
        self.max_delay_samples = Num::max(self.max_delay_samples, 2);
        self.reset();
    }

    #[inline]
    fn tick(&mut self, input: &Frame<f32, Self::Inputs>) -> Frame<f32, Self::Outputs> {
        self.target_freq = input[0].max(0.0) as f64;
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
    fn set(&mut self, setting: Setting) {}
    fn set_hash(&mut self, hash: u64) {}
    fn route(&mut self, input: &SignalFrame, _frequency: f64) -> SignalFrame {
        SignalFrame::new(self.outputs())
    }
}


/// Factory function: medium‑sustain pluck (guitar-like).
pub fn pluck_string() -> An<CombPluck> {
    An(CombPluck::new(0.995, 0.1, 0.5))
}

/// Factory function: bass pluck (longer sustain, lower range).
pub fn pluck_bass() -> CombPluck {
    CombPluck::new(0.998, 0.2, 0.6)
}

/// Factory function: short percussive pluck.
pub fn pluck_percussion() -> CombPluck {
    CombPluck::new(0.98, 0.05, 0.8)
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn test_comb_pluck_creates_output() {
//         let mut pluck = CombPluck::new(0.99, 0.1, 0.5);
//         pluck.set_sample_rate(48000.0);
//         let input = Frame::from([440.0, 1.0]);
//         let output = pluck.tick(&input);
//         assert!(output[0] != 0.0);
//     }
//
//     #[test]
//     fn test_gate_detection() {
//         let mut pluck = CombPluck::new(0.99, 0.1, 0.5);
//         pluck.set_sample_rate(48000.0);
//         let input1 = Frame::from([440.0, 0.0]);
//         let _ = pluck.tick(&input1);
//         let input2 = Frame::from([440.0, 1.0]);
//         let output2 = pluck.tick(&input2);
//         assert!(output2[0] != 0.0);
//         let input3 = Frame::from([440.0, 1.0]);
//         let output3 = pluck.tick(&input3);
//         assert!(output3[0].abs() <= output2[0].abs());
//     }
//
//     #[test]
//     fn test_frequency_change_without_triggers() {
//         let mut pluck = CombPluck::new(0.99, 0.1, 0.5);
//         pluck.set_sample_rate(48000.0);
//         let _ = pluck.tick(&Frame::from([440.0, 1.0]));
//         let output_440 = pluck.tick(&Frame::from([440.0, 1.0]));
//         let output_880 = pluck.tick(&Frame::from([880.0, 1.0]));
//         assert!(output_440[0] != 0.0 || output_880[0] != 0.0);
//     }
// }