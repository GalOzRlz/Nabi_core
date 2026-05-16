use std::sync::{Arc, Mutex};

use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use midir::MidiInput;
use nabi_core::config_builder::{create_ordered_patch_table, FreeVoiceStrategy, GlobalConfig, VoiceStealingConfig};
use nabi_core::io::start_midi_output_thread;
use nabi_core::io::{get_first_midi_device, start_midi_input_thread};
use read_input::{shortcut::input, InputBuild};

fn main() -> anyhow::Result<()> {
    let mut config = GlobalConfig::default();
    config.voice_stealing = VoiceStealingConfig::LegatoLast;
    config.voice_release = FreeVoiceStrategy::ReleaseOnZero;
    let mut midi_in = MidiInput::new("midir reading input")?;
    let in_port = get_first_midi_device(&mut midi_in)?;
    let midi_msgs = Arc::new(SegQueue::new());
    let quit = Arc::new(AtomicCell::new(false));
    let patch_table = Arc::new(Mutex::new(
        create_ordered_patch_table(
            &["patches/patches.toml"],
            &"order.toml",
        )));
    start_midi_input_thread(midi_msgs.clone(), midi_in, in_port, quit.clone());
    start_midi_output_thread::<2>(
        midi_msgs,
        patch_table,
        Some(config),
    );
    input::<String>().msg("Press Enter to exit\n").get();
    Ok(())
}
