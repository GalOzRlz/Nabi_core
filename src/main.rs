use midi_fundsp::config_builder::{build_patch_table, load_all_programs};

fn main() {
    let all_programs = load_all_programs(&[
        "config/builtin.toml",
        "config/community.toml",
    ]);

    let table = build_patch_table(&all_programs);
    println!("Loaded {} programs:", table.entries.len());
    for (i, (name, _, _)) in table.entries.iter().enumerate() {
        println!("  {i}: {name}");
    }
}