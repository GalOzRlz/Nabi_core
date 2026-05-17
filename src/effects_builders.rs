use std::collections::HashMap;
use std::sync::Arc;
use fundsp::prelude64::{AudioUnit, Net};
use fundsp::prelude::{multipass, U2};
use toml::Table;
use crate::config_builder::{CcValuesArray, TomlEffectSection, DEFAULT_CC_VALS, ENCODER_COUNT};
use crate::SharedMidiState;

#[macro_export]
macro_rules! register_effect {
    (
        struct: $struct_name:ident,
        name: $name:expr,
        factory: $factory_fn:ident,
        construction_params: [ $( ($c_name:ident, $c_default:expr) ),* $(,)? ],
        cc_params: [ $( ($cc_name:expr, $cc_default_knob:expr, $cc_default_val:expr) ),* $(,)? ]
    ) => {
        paste::paste! {
            // Params struct (PascalCase)
            pub struct [<$struct_name Params>] {
                $( pub $c_name: f64, )*
            }

            impl [<$struct_name Params>] {
                fn from_table(table: &toml::Table) -> Self {
                    Self {
                        $(
                            $c_name: table.get(stringify!($c_name))
                                .and_then(|v| v.as_float())
                                .unwrap_or($c_default),
                        )*
                    }
                }
            }

            // Wrapper – now completely snake_case
            fn [<__effect_wrapper_ $struct_name:snake>] (
                construction: &toml::Table,
                cc_map: &std::collections::HashMap<String, usize>,
            ) -> EffectBuilder {
                let params = [<$struct_name Params>]::from_table(construction);
                $factory_fn(&params, cc_map)
            }

            inventory::submit! {
                EffectDef {
                    name: $name,
                    factory: [<__effect_wrapper_ $struct_name:snake>] as fn(
                        &toml::Table,
                        &std::collections::HashMap<String, usize>,
                    ) -> EffectBuilder,
                    construction_defaults: &[ $( (stringify!($c_name), $c_default) ),* ],
                    cc_params: &[ $( ($cc_name, $cc_default_knob, $cc_default_val) ),* ],
                }
            }
        }
    };
}

pub type EffectBuilder = Box<
    dyn Fn(&SharedMidiState) -> Net
    + Send
    + Sync
    + 'static,
>;

pub type EffectFactory = fn(
    construction: &Table,
    knob_map: &HashMap<String, usize>,
) -> EffectBuilder;

pub struct EffectDef {
    pub name: &'static str,
    pub factory: EffectFactory,
    pub construction_defaults: &'static [(&'static str, f64)],
    /// (param_name, default_knob, default_value)
    pub cc_params: &'static [(&'static str, usize, f64)],
}

inventory::collect!(EffectDef);

#[derive(Clone)]
pub struct PatchFxChain {
    pub chain: Arc<Vec<EffectBuilder>>,
    pub initial_cc: CcValuesArray,
    /// (knob_index 1‑based, label)
    pub knob_labels: Vec<(usize, String)>,
}

// todo: set assembled_chain field with refreshing
impl PatchFxChain {
    pub fn assemble_net(&mut self, shared_midi_state: &SharedMidiState) -> Net {
        let arc_vec: Arc<Vec<Net>> = Arc::new(
            self.chain
                .iter()
                .map(|fx| fx(shared_midi_state))
                .collect()
        );
        connect_node_vec(arc_vec, None)
        }
    pub fn new(
        effects: Option<&TomlEffectSection>,
        registry: &HashMap<&str, &EffectDef>,
    ) -> Self {
        let mut chain = Vec::new();
        let mut initial_cc = DEFAULT_CC_VALS.clone();
        let mut knob_labels = Vec::new();

        if let Some(effects) = effects {
            for eff_name in &effects.chain {
                let def = registry.get(eff_name.as_str())
                    .unwrap_or_else(|| panic!("Unknown effect: {}", eff_name));

                // ---- Construction values ----
                let mut construction = Table::new();
                // defaults from the effect definition
                for (k, v) in def.construction_defaults.iter() {
                    construction.insert(k.to_string(), toml::Value::from(*v));
                }
                // overrides from TOML (excluding the "mapping" key)
                if let Some(eff_cfg) = effects.extras.get(eff_name.as_str())
                    .and_then(|v| v.as_table())
                {
                    for (k, v) in eff_cfg {
                        if k != "mapping" {
                            construction.insert(k.clone(), v.clone());
                        }
                    }
                }

                // ---- CC parameter mappings ----
                let mut knob_map = HashMap::new();
                let user_mappings: Option<&toml::Table> = effects.extras.get(eff_name.as_str())
                    .and_then(|v| v.get("mapping"))
                    .and_then(|v| v.as_table());

                for (param_name, default_knob, default_val) in def.cc_params.iter() {
                    let mut knob = *default_knob;
                    // user override?
                    if let Some(m) = user_mappings {
                        if let Some(val) = m.get(*param_name).and_then(|v| v.as_integer()) {
                            knob = val as usize;
                        }
                    }
                    // clamp knob index
                    if knob < 1 { knob = 1; }
                    if knob > ENCODER_COUNT { knob = ENCODER_COUNT; }

                    knob_map.insert(param_name.to_string(), knob);

                    // initial value: TOML config override > default
                    let init_val = effects.extras.get(eff_name.as_str())
                        .and_then(|v| v.as_table())
                        .and_then(|t| t.get(*param_name))
                        .and_then(|v| v.as_float())
                        .unwrap_or(*default_val);

                    initial_cc[knob - 1] = init_val as f32;
                    knob_labels.push((knob, format!("{}: {}", eff_name, param_name)));
                }

                // Build the effect closure
                let closure = (def.factory)(&construction, &knob_map);
                chain.push(closure);
            }
        }
        let chain = Arc::new(chain);
        PatchFxChain { chain, initial_cc, knob_labels }
    }
}


pub fn to_stereo(net: Net) -> Net {
    match net.inputs() {
        1 => (net.clone() | net),
        2 => net,
        _ => panic!("only 1 and 2 inputs are supported!")
    }
}

fn connect_node_vec(node_vec: Arc<Vec<Net>>, starting_net: Option<Net>) -> Net {
    let nodes = (*node_vec).clone();
    let mut net = starting_net.unwrap_or_else(|| Net::wrap(Box::new(multipass::<U2>())));
    for node in nodes {
        net = to_stereo(net) >> node;
    }
    net
}
