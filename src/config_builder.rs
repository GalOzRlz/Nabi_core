use crate::patch_builder::{IntoSpeakerDef, PatchEntry, PatchTable, PatchTableItem, SoundBuilder};
use serde::{de::{self, Visitor}, Deserialize, Deserializer};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use fastrand::usize;
use fundsp::math::midi_hz;
use crate::tunings::{TunerBuilder, TunerEntry};

pub const ENCODER_COUNT: usize = 4;
// todo: refactor sound control vs. effects control
pub const DEFAULT_CC_VALS: CcValuesArray = [0.0, 0.0, 0.0, 1.0];
pub const DEFAULT_CC_MAPPING: CcMapping = [74, 71, 76, 77];
pub type CcValuesArray = [f32; ENCODER_COUNT];
pub type CcMapping = [usize; ENCODER_COUNT];

/// Determines the voice stealing strategy:
/// LegatoOldest: Keep envelope and steal the oldest voice
/// LegatoLast: either oldest or latest voice
#[derive(Debug, Copy, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum VoiceStealingConfig {
    LegatoOldest,
    LegatoLast,
}

/// Determine if voices are freed from current voices queue by instrument ADSR or by being at zero volume.
/// Release on zero is a bit costlier but allows for 0.0 release sounds to play better.
#[derive(Debug, Copy, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum FreeVoiceStrategy {
    FollowADSR,
    ReleaseOnZero,
}

#[derive(Deserialize)]
struct GlobalConfigToml {
    #[serde(default)]
    global: GlobalSection,
}

impl Default for GlobalSection {
    fn default() -> Self {
        Self {
            cc_mappings: None,
            voice_stealing: None,
            voice_release: None,
        }
    }
}

#[derive(Deserialize)]
struct GlobalSection {
    #[serde(default)]                             // None if missing
    cc_mappings: Option<CcMapping>,

    #[serde(default)]
    voice_stealing: Option<VoiceStealingConfig>,

    #[serde(default)]
    voice_release: Option<FreeVoiceStrategy>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlobalConfig {
    pub voice_stealing: VoiceStealingConfig,
    pub voice_release: FreeVoiceStrategy,
    pub cc_mappings: CcMapping,          // your type that wraps [u8; 4]
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            voice_stealing: VoiceStealingConfig::LegatoLast,
            voice_release: FreeVoiceStrategy::ReleaseOnZero,
            cc_mappings: DEFAULT_CC_MAPPING,
        }
    }
}


/// Custom deserializer for [u8; 4] from a TOML array of integers.
#[derive(Debug, Clone, Copy)]
pub struct TomlCcArray(pub CcValuesArray);

impl Default for TomlCcArray {
    fn default() -> Self { TomlCcArray(DEFAULT_CC_VALS)}
}

impl<'de> Deserialize<'de> for TomlCcArray {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de>
    {
        struct CcVisitor;
        impl<'de> Visitor<'de> for CcVisitor {
            type Value = TomlCcArray;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                let fmt = format!("an array of {} integers (0–255)", ENCODER_COUNT);
                f.write_str(fmt.as_str())
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where A: de::SeqAccess<'de>
            {
                let mut arr = DEFAULT_CC_VALS.clone();
                for (i, slot) in arr.iter_mut().enumerate() {
                    *slot = seq.next_element()?.ok_or_else(|| {
                        de::Error::invalid_length(i, &self)
                    })?;
                }
                Ok(TomlCcArray(arr))
            }
        }
        deserializer.deserialize_seq(CcVisitor)
    }
}


#[derive(Deserialize)]
pub struct TomlPatch {
    function: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    cc: TomlCcArray,
    #[serde(default)]
    tuning: Option<String>,
}

#[derive(Deserialize)]
pub struct PatchFile {
    program: Vec<TomlPatch>,
}

#[derive(Debug, serde::Deserialize)]
struct TomlOrderConfig {
    patch_order: Vec<String>,
}

// loading and building functions:
pub fn load_global_config() -> Option<GlobalConfig> {
    let path = "config/midi.toml";
    let default_config = GlobalConfig::default();

    match std::fs::read_to_string(path) {
        Ok(text) => {
            match toml::from_str::<GlobalConfigToml>(&text) {
                Ok(cfg) => Some(GlobalConfig {
                    cc_mappings: cfg.global.cc_mappings
                        .unwrap_or(default_config.cc_mappings),
                    voice_stealing: cfg.global.voice_stealing.unwrap_or(default_config.voice_stealing),
                    voice_release: cfg.global.voice_release.unwrap_or(default_config.voice_release),
                }),
                Err(e) => {
                    eprintln!("Warning: failed to parse midi.toml: {}. Using defaults.", e);
                    Some(default_config)
                }
            }
        }
        Err(_) => {
            eprintln!("midi.toml not found, using default config.");
            Some(default_config)
        }
    }
}

fn load_patch_file(path: &str) -> Result<Vec<TomlPatch>, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let file: PatchFile = toml::from_str(&text)?;
    Ok(file.program)
}

/// Load multiple TOML files, merge duplicates (last definition wins for CC and name).
pub fn load_all_programs(paths: &[&str]) -> Vec<TomlPatch> {
    let mut all_programs = Vec::new();
    let mut used_names: HashSet<String> = HashSet::new();

    for path in paths {
        let programs = match load_patch_file(path) {
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

            all_programs.push(TomlPatch {
                function: prog.function,
                name: Some(display_name),
                cc: prog.cc,
                tuning: prog.tuning,
            });
        }
    }
    all_programs
}

pub fn build_patch_table(programs: &[TomlPatch]) -> PatchTable {
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
                    "Unknown function '{}' for program '{}', skipping {:?}",
                    prog.function,
                    prog.name.as_deref().unwrap_or(&prog.function),
                    prog.tuning,
                );
                continue;
            }
        };

        let tuner_map: HashMap<&str, TunerBuilder> = inventory::iter::<TunerEntry>()
            .map(|e| (e.name, e.tuner))
            .collect();
        let tuner = if let Some(ref tuning_name) = prog.tuning {
            match tuner_map.get(tuning_name.as_str()) {
                Some(&t) => t,
                None => {
                    eprintln!("Unknown tuning '{}', using default", tuning_name);
                    midi_hz
                }
            }
        } else {
            midi_hz
        };
        let cc = prog.cc.0; // [u8; 4]
        let name = prog.name.clone().unwrap_or_else(|| prog.function.clone());
        let def = (name, builder.into_speaker_def(), cc, tuner);
        entries.push(def);
    }
    PatchTable::new(entries)
}

fn get_patch_table_from_toml(paths: &[&str]) -> PatchTable {
    let all_programs = load_all_programs(paths);

    let table = build_patch_table(&all_programs);
    println!("Loaded {} programs:", &table.entries.len());
    for (i, (name, _, _, _)) in table.entries.iter().enumerate() {
        println!("  {}: {name}", i + 1);
    }
    table
}

pub fn reorder_by_names(entries: &mut Vec<PatchTableItem>, order: &[String]) {
    // Attach original index to each entry, then drain.
    let indexed: Vec<(usize, PatchTableItem)> = entries
        .drain(..)
        .enumerate()
        .collect();

    // Build lookup from name to the actual entry.
    let mut name_to_entry: HashMap<String, (usize, PatchTableItem)> = HashMap::new();
    for item in indexed {
        name_to_entry.insert(item.1.0.clone(), item);
    }

    let mut new_entries = Vec::with_capacity(name_to_entry.len());
    let mut used_indices = HashSet::new();

    // Pick entries in the given order.
    for name in order {
        if let Some((idx, entry)) = name_to_entry.remove(name) {
            new_entries.push(entry);
            used_indices.insert(idx);
        }
    }

    // Collect the remaining entries, sorted by original index.
    let mut remaining: Vec<(usize, PatchTableItem)> = name_to_entry
        .into_values()
        .collect();
    remaining.sort_by_key(|(idx, _)| *idx);
    for (_, entry) in remaining {
        new_entries.push(entry);
    }

    *entries = new_entries;
}

pub fn create_ordered_patch_table(patch_paths: &[&str], order_path: &str) -> PatchTable {
    let mut patch_table = get_patch_table_from_toml(patch_paths);
    if let Ok(text) = std::fs::read_to_string(order_path) {
        if let Ok(ord_config) = toml::from_str::<TomlOrderConfig>(&text) {
            eprintln!("Loaded ordered patch table:{:?}", ord_config.patch_order);
            reorder_by_names(&mut patch_table.entries, &ord_config.patch_order);
        } else {
            eprintln!("Failed to parse order.toml inside toml, using default order");
        }
    }
    else {
        eprintln!("Failed to parse order.toml in read_to_string, using default order");
    }
    patch_table
}