const WELL_C_MINUS_1: f32 = 8.203_544;

/// C standard in hertz on equal temperament with A as 440 hertz.
/// Rf: https://inspiredacoustics.com/en/MIDI_note_numbers_and_center_frequencies
const STANDARD_C: f32 = 8.18;

/// Terry Riley's "Harp of New Albion" tuning which is a modified Malcolm tuning (Limit-5).
/// Adapted from the Scala library: https://www.huygens-fokker.org/scala/
pub fn just_intonation(midi_pitch: f32) -> f32 {
    let midi_pitch = midi_pitch as u8;
    let octave = (midi_pitch / 12) as f32;
    2.0_f32.powf(octave)
        * STANDARD_C
        * match midi_pitch % 12 {
            0 => 1.0,
            1 => 1.066_666_7,  // 16/15
            2 => 1.125,        // 9/8
            3 => 1.2,          // 6.5
            4 => 1.28,         // 5.4
            5 => 1.333_333_4,  // 4/3
            6 => 1.422_222_3,  // 64/45
            7 => 1.5,          // 3/2
            8 => 1.6,          // 8/5
            9 => 1.666_666_6,  // 5/3
            10 => 1.777_777_8, // 16/9
            11 => 1.875,       // 15/8
            _ => panic!("Unreachable"),
        }
}

/// Derived from: https://www.historicaltuning.com/Chapter8.pdf, Table 8.3
/// This is believed by the author to be Bach's well-temperament.
pub fn well_temperament(midi_pitch: f32) -> f32 {
    let midi_pitch = midi_pitch as u8;
    let octave = (midi_pitch / 12) as f32;
    2.0_f32.powf(octave)
        * WELL_C_MINUS_1
        * match midi_pitch % 12 {
            0 => 1.0,
            1 => 1.058_267_4,
            2 => 1.119_929_8,
            3 => 1.187_864_9,
            4 => 1.254_242_8,
            5 => 1.336_348,
            6 => 1.411_023_1,
            7 => 1.496_616,
            8 => 1.585_609_4,
            9 => 1.676_104_9,
            10 => 1.779_786_5,
            11 => 1.881_364_2,
            _ => panic!("Unreachable"),
        }
}

#[cfg(test)]
mod tests {
    use crate::tunings::{STANDARD_C, just_intonation, well_temperament};
    use float_eq::assert_float_eq;

    #[test]
    fn test_well() {
        // Corresponds to Table 8.3 in https://www.historicaltuning.com/Chapter8.pdf
        for (midi, hz) in [
            (53.0, 175.404633854),
            (54.0, 185.206238152),
            (55.0, 196.440880223),
            (56.0, 208.121862788),
            (57.0, 220.000000000),
            (58.0, 233.608892472),
            (59.0, 246.941650914),
            (60.0, 262.513392643),
            (61.0, 277.809357360),
            (62.0, 293.996156549),
            (63.0, 311.830459864),
            (64.0, 329.255534464),
        ] {
            assert_float_eq!(well_temperament(midi), hz, abs <= 1e-3);
        }
    }
    #[test]
    fn test_just_intonation_multipliers_relative_to_c4() {
        let expected_ratios = [
            1.0,         // C
            16.0 / 15.0, // C#
            9.0 / 8.0,   // D
            6.0 / 5.0,   // D#
            5.0 / 4.0,   // E
            4.0 / 3.0,   // F
            64.0 / 45.0, // F#
            3.0 / 2.0,   // G
            8.0 / 5.0,   // G#
            5.0 / 3.0,   // A
            16.0 / 9.0,  // A#
            15.0 / 8.0,  // B
        ];

        let c4_freq = just_intonation(60.0);
        assert_float_eq!(c4_freq, 2.0_f32.powf(5.0) * STANDARD_C, abs <= 1e-3);
        for (pitch_class, expected_ratio) in expected_ratios.iter().enumerate() {
            let midi = 60.0 + pitch_class as f32;
            let freq = just_intonation(midi);
            let actual_multiplier = freq / c4_freq;

            assert_float_eq!(
                actual_multiplier,
                *expected_ratio,
                abs <= 0.035,
                "Multiplier for note {} (MIDI {}) does not match expected",
                pitch_class,
                midi
            );
        }
    }
}
