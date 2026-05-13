use std::sync::{Arc, Mutex};

use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use midi_fundsp::{
    io::{
        Speaker, SynthMsg, choose_midi_device, console_choice_from, start_input_thread,
        start_output_thread,
    },
    sound_builders::ProgramTable,
};
use midir::MidiInput;
use midi_fundsp::config_builder::{build_patch_table, get_patch_table_from_toml, load_all_programs};

fn main() -> anyhow::Result<()> {
    let reset = Arc::new(AtomicCell::new(false));
    let mut quit = false;
    while !quit {
        let patch_table = get_patch_table_from_toml();
        let mut midi_in = MidiInput::new("midir reading input")?;
        let in_port = choose_midi_device(&mut midi_in)?;
        let midi_msgs = Arc::new(SegQueue::new());
        while reset.load() {}
        start_input_thread(midi_msgs.clone(), midi_in, in_port, reset.clone());
        let patch_table = Arc::new(Mutex::new(patch_table));
        start_output_thread::<10>(midi_msgs.clone(), patch_table.clone(), None);
        run_chooser(midi_msgs, patch_table.clone(), reset.clone(), &mut quit);
    }
    Ok(())
}

fn run_chooser(
    midi_msgs: Arc<SegQueue<SynthMsg>>,
    patch_table: Arc<Mutex<ProgramTable>>,
    reset: Arc<AtomicCell<bool>>,
    quit: &mut bool,
) {
    let main_menu = vec!["Pick New Synthesizer Sound", "Pick New MIDI Device", "Quit"];
    while !*quit && !reset.load() {
        println!("Play notes at will. When ready for a change, select one of the following:");
        match console_choice_from("Choice", &main_menu, |s| *s) {
            0 => {
                let program = {
                    let patch_table = patch_table.lock().unwrap();
                    console_choice_from("Change synth to", &patch_table.entries, |opt| {
                        opt.0.as_str()
                    })
                };
                midi_msgs.push(SynthMsg::program_change(program as u8, Speaker::Both));
            }
            1 => reset.store(true),
            2 => *quit = true,
            _ => panic!("This should never happen."),
        }
    }
}
