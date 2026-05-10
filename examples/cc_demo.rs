use std::sync::{Arc, Mutex};

use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use midi_fundsp::{
    io::{get_first_midi_device, start_midi_input_thread, start_midi_output_thread_alt_tuning},
    program_table,
    sounds::music_box,
    tunings::just_intonation,
};
use midi_fundsp::sound_builders::*;
use midi_msg::MidiMsg;
use midir::MidiInput;
use read_input::{InputBuild, shortcut::input};

fn main() -> anyhow::Result<()> {
    let mut midi_in = MidiInput::new("midir reading input")?;
    let in_port = get_first_midi_device(&mut midi_in)?;
    let incoming_msgs = Arc::new(SegQueue::new());
    let outgoing_msgs = Arc::new(SegQueue::new());
    let quit = Arc::new(AtomicCell::new(false));
    start_midi_input_thread(incoming_msgs.clone(), midi_in, in_port, quit.clone());
    run_midi_show_thread(incoming_msgs, outgoing_msgs.clone());
    start_midi_output_thread_alt_tuning::<10>(
        outgoing_msgs,
        Arc::new(Mutex::new(program_table![("Music Box", music_box::<7>)])),
        just_intonation,
        None,
    );
    input::<String>().msg("Press Enter to exit\n").get();
    Ok(())
}

fn run_midi_show_thread(
    incoming_msgs: Arc<SegQueue<MidiMsg>>,
    outgoing_msgs: Arc<SegQueue<MidiMsg>>,
) {
    std::thread::spawn(move || {
        loop {
            if let Some(msg) = incoming_msgs.pop() {
                println!("{msg:?}");
                outgoing_msgs.push(msg);
            }
        }
    });
}
