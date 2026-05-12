pub const ENCODER_COUNT: usize = 4;

/// Determines the voice stealing strategy:
/// LegatoOldest: Keep envelope and steal the oldest voice
/// LegatoLast: either oldest or latest voice
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum VoiceStealingConfig {
    LegatoOldest,
    LegatoLast,
}

/// Determine if voices are freed from current voices queue by instrument ADSR or by being at zero volume.
/// Release on zero is a bit costlier but allows for 0.0 release sounds to play better.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FreeVoiceStrategy {
    FollowADSR,
    ReleaseOnZero,
}

/// Configuration block for extra features
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub voice_stealing: VoiceStealingConfig,
    pub voice_release: FreeVoiceStrategy,
    pub cc_mappings: Vec<usize>,
    pub cc_default_values: Vec<f32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            voice_stealing: VoiceStealingConfig::LegatoOldest,
            voice_release: FreeVoiceStrategy::ReleaseOnZero,
            // todo: make these overrideable by controller.toml
            cc_mappings: vec![74, 71, 76, 77],
            cc_default_values: vec![0.0, 0.0, 0.0, 1.0],
        }
    }
}

// to do: implement toml overrides for cc mapping and then per-sound defaults

use std::collections::HashMap;
use serde::Deserialize;
use crate::config_builder::cc_array::CcArray;
use crate::sound_builders::{IntoSpeakerDef, ProgramTable, SoundBuilder, SoundEntry};

/// Custom deserializer for [u8; 4] from a TOML array of integers.
pub(crate) mod cc_array {
    use serde::{Deserialize, Deserializer, de::{self, Visitor}};
    use std::fmt;
    use crate::config_builder::ENCODER_COUNT;

    #[derive(Debug, Clone, Copy)]
    pub struct CcArray(pub [f32; ENCODER_COUNT]);

    impl Default for CcArray {
        fn default() -> Self { CcArray([0.0; 4]) }
    }

    impl<'de> Deserialize<'de> for CcArray {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
        {
            struct CcVisitor;
            impl<'de> Visitor<'de> for CcVisitor {
                type Value = CcArray;
                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("an array of 4 integers (0–255)")
                }
                fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                where A: de::SeqAccess<'de>
                {
                    let mut arr = [0.0; 4];
                    for (i, slot) in arr.iter_mut().enumerate() {
                        *slot = seq.next_element()?.ok_or_else(|| {
                            de::Error::invalid_length(i, &self)
                        })?;
                    }
                    Ok(CcArray(arr))
                }
            }
            deserializer.deserialize_seq(CcVisitor)
        }
    }
}

#[derive(Deserialize)]
pub struct TomlProgram {
    function: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    cc: CcArray,
}

#[derive(Deserialize)]
pub struct ProgramFile {
    program: Vec<TomlProgram>,
}

// ---------------------------------------------------------------------------
// Loading & merging
// ---------------------------------------------------------------------------

fn load_program_file(path: &str) -> Result<Vec<TomlProgram>, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let file: ProgramFile = toml::from_str(&text)?;
    Ok(file.program)
}

/// Load multiple TOML files, merge duplicates (last definition wins for CC and name).
pub fn load_all_programs(paths: &[&str]) -> Vec<TomlProgram> {
    let mut merged: Vec<TomlProgram> = Vec::new();
    let mut index_map: HashMap<String, usize> = HashMap::new(); // function name -> index

    for path in paths {
        let programs = match load_program_file(path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping {}: {}", path, e);
                continue;
            }
        };
        for mut prog in programs {
            if let Some(&idx) = index_map.get(&prog.function) {
                // Override CCs and name (if given)
                merged[idx].cc = prog.cc;
                if prog.name.is_some() {
                    merged[idx].name = prog.name;
                }
            } else {
                if prog.name.is_none() {
                    prog.name = Some(prog.function.clone());
                }
                index_map.insert(prog.function.clone(), merged.len());
                merged.push(prog);
            }
        }
    }
    merged
}

pub fn build_program_table(programs: &[TomlProgram]) -> ProgramTable {
    // Lookup from name to raw builder pointer
    let builder_map: HashMap<&str, SoundBuilder> = inventory::iter::<SoundEntry>()
        .map(|e| (e.name, e.builder))
        .collect();

    let mut entries = Vec::new();
    for prog in programs {
        let builder = match builder_map.get(prog.function.as_str()) {
            Some(&b) => b,
            None => {
                eprintln!(
                    "Unknown function '{}' for program '{}', skipping",
                    prog.function,
                    prog.name.as_deref().unwrap_or(&prog.function)
                );
                continue;
            }
        };
        let cc = prog.cc.0; // [u8; 4]
        let name = prog.name.clone().unwrap_or_else(|| prog.function.clone());
        let def = (name, builder.into_speaker_def(), cc);
        entries.push(def);
    }
    ProgramTable::new(entries)
}
