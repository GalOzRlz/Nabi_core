use crate::patch_builder::{SoundEntry, PatchTable, SoundBuilder, PatchDef};
use serde::{de::{Visitor}, Deserialize, Deserializer};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use fundsp::math::midi_hz;
use fundsp::prelude64::AudioUnit;
use crate::effects_builders::{EffectDef, PatchFxChain};
use crate::{SharedMidiState, SynthFunc};
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

#[derive(Deserialize)]
pub struct TomlPatchDef {
    pub function: String,
    pub name: String,
    pub tuning: Option<String>,
    pub config: Option<toml::Table>,
    pub effects: Option<TomlEffectSection>,
}


#[derive(Deserialize)]
pub struct TomlEffectSection {
    pub chain: Vec<String>,
    #[serde(flatten)]
    pub extras: HashMap<String, toml::Value>, // captures tables like `eq`, `reverb`, …
}

/// One program entry in the TOML file.
#[derive(Deserialize)]
pub struct TomlProgram {
    pub function: String,                 // voice builder name
    #[serde(default)]
    pub name: Option<String>,             // optional display name
    #[serde(default)]
    pub tuning: Option<String>,           // optional tuning name
    #[serde(default)]
    pub config: toml::Table,              // per‑voice static config
    #[serde(default)]
    pub effects: Option<TomlEffectSection>,// optional effects section
}
#[derive(Deserialize)]
pub struct ProgramsFile {
    pub program: Vec<TomlProgram>,        // or rename to `PatchFile`
}

#[derive(Debug, serde::Deserialize)]
struct TomlOrderConfig {
    patch_order: Vec<String>,
}

// loading and building functions:
pub fn load_global_config() -> Option<GlobalConfig> {
    let path = "config/global.toml";
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
                    eprintln!("Warning: failed to parse global.toml: {}. Using defaults.", e);
                    Some(default_config)
                }
            }
        }
        Err(_) => {
            eprintln!("global.toml not found, using default config.");
            Some(default_config)
        }
    }
}

fn load_patch_file(path: &str) -> Result<Vec<TomlProgram>, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let file: ProgramsFile = toml::from_str(&text)?;
    Ok(file.program)
}

/// Load multiple TOML files, merge duplicates (last definition wins for CC and name).
pub fn load_all_programs(paths: &[&str]) -> Vec<TomlPatchDef> {
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

            // Preserve the original config and effects from the TOML
            all_programs.push(TomlPatchDef {
                function: prog.function,
                name: display_name,
                tuning: prog.tuning,
                config: Some(prog.config),
                effects: prog.effects,
            });
        }
    }
    all_programs
}

pub fn build_patch_table(programs: &[TomlPatchDef]) -> PatchTable {
    // Build lookup maps from the static registries
    let sound_map: HashMap<&str, SoundBuilder> = inventory::iter::<SoundEntry>()
        .map(|e| (e.name, e.builder))
        .collect();
    let effect_map: HashMap<&str, &EffectDef> = inventory::iter::<EffectDef>()
        .map(|e| (e.name, e))
        .collect();
    let tuner_map: HashMap<&str, TunerBuilder> = inventory::iter::<TunerEntry>()
        .map(|e| (e.name, e.tuner))
        .collect();

    let default_tuner = midi_hz;   // your built‑in equal temperament

    let mut patch_defs = Vec::new();

    for prog in programs {
        // Resolve voice builder
        let voice_builder = match sound_map.get(prog.function.as_str()) {
            Some(&b) => b,
            None => {
                eprintln!("Unknown function '{}' for program '{}', skipping",
                          prog.function, prog.name);
                continue;
            }
        };

        // Resolve tuner
        let tuning = if let Some(ref tuning_name) = prog.tuning {
            tuner_map.get(tuning_name.as_str()).copied().unwrap_or_else(|| {
                eprintln!("Unknown tuning '{}', using default", tuning_name);
                default_tuner
            })
        } else {
            default_tuner
        };

        // Build effect chain
        let fx_chain = PatchFxChain::new(prog.effects.as_ref(), &effect_map);

        // Prepare voice config (empty table if missing)
        //let voice_config = prog.config.clone().unwrap_or_else(toml::Table::new);

        // Wrap everything into an Arc closure (SynthFunc)
        let synth_func: SynthFunc = Arc::new(move |state: &SharedMidiState| -> Box<dyn AudioUnit> {
            voice_builder(state)
        });

        // Assemble PatchDef
        let patch_def = PatchDef {
            function: synth_func,
            name: prog.name.clone(),
            tuning,
            sound_config: None,   // or keep the original clone if needed
            effects: fx_chain,                  // moved into closure, but we still have it here? Wait – `fx_chain` was moved into the closure above.
        };

        patch_defs.push(patch_def);
    }

    PatchTable::new(patch_defs)
}

fn get_patch_table_from_toml(paths: &[&str]) -> PatchTable {
    let all_programs = load_all_programs(paths);
    let table = build_patch_table(&all_programs);
    table
}

/// Rearranges the entries of a PatchTable so that programs whose names appear
/// in `order` come first, in that order. Unmentioned programs are appended
/// at the end, preserving their original relative order.
fn reorder_by_names(entries: &mut Vec<PatchDef>, order: &[String]) {
    // Remove all entries temporarily, replacing with an empty vector.
    let old_entries = std::mem::take(entries);

    // Attach original index to each entry.
    let mut indexed: Vec<(usize, PatchDef)> = old_entries
        .into_iter()
        .enumerate()
        .collect();

    // Build a map from program name -> (index, entry)
    let mut name_to_entry: HashMap<String, (usize, PatchDef)> = HashMap::new();
    for (idx, entry) in indexed {
        name_to_entry.insert(entry.name.clone(), (idx, entry));
    }

    let mut new_entries = Vec::with_capacity(name_to_entry.len());
    let mut used_indices = HashSet::new();

    // Place entries that appear in the order list.
    for name in order {
        if let Some((idx, entry)) = name_to_entry.remove(name) {
            new_entries.push(entry);
            used_indices.insert(idx);
        }
    }

    // Append the remaining entries, sorted by their original index.
    let mut remaining: Vec<(usize, PatchDef)> = name_to_entry.into_values().collect();
    remaining.sort_by_key(|(idx, _)| *idx);
    for (_, entry) in remaining {
        new_entries.push(entry);
    }

    // Replace the original vector with the reordered one.
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