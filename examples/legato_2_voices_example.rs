use std::sync::{Arc, Mutex};

use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use midi_fundsp::config::{Config, FreeVoiceStrategy, VoiceStealingConfig};
use midi_fundsp::io::start_midi_output_thread;
use midi_fundsp::sound_builders::*;
use midi_fundsp::{
    io::{get_first_midi_device, start_midi_input_thread},
    program_table,
    sounds::moog_organ,
};
use midir::MidiInput;
use read_input::{InputBuild, shortcut::input};

fn main() -> anyhow::Result<()> {
    let mut config = Config::default();
    config.voice_stealing = VoiceStealingConfig::LegatoLast;
    config.voice_release = FreeVoiceStrategy::ReleaseOnZero;
    let mut midi_in = MidiInput::new("midir reading input")?;
    let in_port = get_first_midi_device(&mut midi_in)?;
    let midi_msgs = Arc::new(SegQueue::new());
    let quit = Arc::new(AtomicCell::new(false));
    start_midi_input_thread(midi_msgs.clone(), midi_in, in_port, quit.clone());
    start_midi_output_thread::<2>(
        midi_msgs,
        Arc::new(Mutex::new(program_table![("Organ", moog_organ)])),
        Some(config),
    );
    input::<String>().msg("Press Enter to exit\n").get();
    Ok(())
}
