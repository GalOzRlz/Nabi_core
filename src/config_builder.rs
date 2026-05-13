use std::collections::HashMap;
use std::collections::HashSet;
use crate::sound_builders::{IntoSpeakerDef, ProgramTable, SoundBuilder, PatchEntry};
use serde::{Deserialize, Deserializer, de::{self, Visitor}};
use std::fmt;

pub const ENCODER_COUNT: usize = 4;
pub const DEFAULT_CC_ARRAY: CcValuesArray = [0.0, 0.0, 0.0, 1.0];

pub type CcValuesArray = [f32; ENCODER_COUNT];
pub type CcMapping = [usize; ENCODER_COUNT];

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
pub struct GlobalConfig {
    pub voice_stealing: VoiceStealingConfig,
    pub voice_release: FreeVoiceStrategy,
    pub cc_mappings: CcMapping,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            voice_stealing: VoiceStealingConfig::LegatoLast,
            voice_release: FreeVoiceStrategy::FollowADSR,
            cc_mappings: [74, 71, 76, 77], // todo: move to midi.toml, read from there!
        }
    }
}

/// Custom deserializer for [u8; 4] from a TOML array of integers.
#[derive(Debug, Clone, Copy)]
pub struct CCArray(pub CcValuesArray);

impl Default for CCArray {
    fn default() -> Self { CCArray(DEFAULT_CC_ARRAY)}
}

impl<'de> Deserialize<'de> for CCArray {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de>
    {
        struct CcVisitor;
        impl<'de> Visitor<'de> for CcVisitor {
            type Value = CCArray;
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
                Ok(CCArray(arr))
            }
        }
        deserializer.deserialize_seq(CcVisitor)
    }
}


#[derive(Deserialize)]
pub struct TomlProgram {
    function: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    cc: CCArray,
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
    let mut all_programs = Vec::new();
    let mut used_names: HashSet<String> = HashSet::new();

    for path in paths {
        let programs = match load_program_file(path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping {}: {}", path, e);
                continue;
            }
        };
        for prog in programs {
            // The display name, defaulting to the function name if not given.
            let display_name = prog.name.clone().unwrap_or_else(|| prog.function.clone());

            if used_names.contains(&display_name) {
                panic!(
                    "Duplicate program name '{}' found in file {}. \
                     Each program must have a unique display name.",
                    display_name, path
                );
            }
            used_names.insert(display_name.clone());

            all_programs.push(TomlProgram {
                function: prog.function,
                name: Some(display_name),
                cc: prog.cc,
            });
        }
    }
    all_programs
}

pub fn build_patch_table(programs: &[TomlProgram]) -> ProgramTable {
    // Lookup from name to raw builder pointer
    let builder_map: HashMap<&str, SoundBuilder> = inventory::iter::<PatchEntry>()
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

pub fn get_patch_table_from_toml() -> ProgramTable {
    let all_programs = load_all_programs(&[
        "config/builtin.toml",
        "config/community.toml",
    ]);

    let table = build_patch_table(&all_programs);
    println!("Loaded {} programs:", &table.entries.len());
    for (i, (name, _, _)) in table.entries.iter().enumerate() {
        println!("  {i}: {name}")
    }
    table
}