use anyhow::Result;
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::thread;
use std::time::Duration;

use crate::state::OutputMethod;

pub fn output_text(text: &str, method: &OutputMethod) -> Result<()> {
    // Short delay so focus returns to the target app after hotkey release.
    thread::sleep(Duration::from_millis(80));

    match method {
        OutputMethod::Paste => paste_text(text),
        OutputMethod::Type => type_text(text),
    }
}

fn paste_text(text: &str) -> Result<()> {
    // Write to clipboard
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text.to_owned())?;

    // Give the clipboard time to sync
    thread::sleep(Duration::from_millis(50));

    // Simulate Ctrl+V
    let mut enigo = Enigo::new(&Settings::default())?;
    enigo.key(Key::Control, Direction::Press)?;
    enigo.key(Key::Unicode('v'), Direction::Click)?;
    enigo.key(Key::Control, Direction::Release)?;

    Ok(())
}

fn type_text(text: &str) -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default())?;
    enigo.text(text)?;
    Ok(())
}
