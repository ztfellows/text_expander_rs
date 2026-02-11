#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::env;
use std::collections::HashMap;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use serde::Deserialize;
use arboard::Clipboard;
use chrono::Local;

mod windows_input;
mod keyboard_hook;

use keyboard_hook::{KeyId, MouseButton, HookMessage};


/// A macro that functions like `println!`, but only compiles in debug builds.
#[macro_export]
macro_rules! debug_println {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            print!("[DEBUG] ");
            println!($($arg)*);
        }
        #[cfg(not(debug_assertions))]
        {
        }
    };
}


#[derive(Debug, Deserialize)]
struct ExpansionFile {
    case_sensitive: HashMap<String, String>,
    case_insensitive: HashMap<String, String>,
}

struct ExpansionData {
    key_buffer: String,
    expansion_table: ExpansionFile,
    cursor_position: usize,
    typing_state: TypingState,
    global_listening: bool,
}

enum TypingState {
    Typing,
    Empty,
    NoMatch,
}

impl ExpansionData {
    fn new(expansion_table: ExpansionFile) -> Self {
        ExpansionData {
            key_buffer: String::new(),
            expansion_table,
            cursor_position: 0,
            typing_state: TypingState::Empty,
            global_listening: true,
        }
    }

    fn clear_buffer(&mut self) {
        self.key_buffer.clear();
    }

    fn push_to_buffer(&mut self, c: &str) {
        let index = (self.cursor_position as usize).min(self.key_buffer.len());
        self.key_buffer.insert_str(index, c);
        self.cursor_position += c.len();
    }

    fn pop_from_buffer(&mut self) {
        if self.cursor_position > 0 && !self.key_buffer.is_empty() {
            let remove_index = self.cursor_position - 1;
            if self.key_buffer.is_char_boundary(remove_index as usize) {
                self.key_buffer.remove(remove_index as usize);
                self.cursor_position -= 1;
            }
        }
    }

    fn set_typing_state(&mut self, state: TypingState) {
        self.typing_state = state;
    }

    fn reset(&mut self) {
        self.clear_buffer();
        self.typing_state = TypingState::Empty;
        self.cursor_position = 0;
        self.global_listening = true;
    }

    fn decrement_cursor_position(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
        if self.cursor_position == 0 {
            self.reset();
        }
    }

    fn increment_cursor_position(&mut self) {
        self.cursor_position += 1;
    }
}


// Atomic boolean for listening state
static GLOBAL_LISTENING: AtomicBool = AtomicBool::new(true);

fn main() {
    let expansion_table = load_expansion_table().unwrap();
    let expansion_data = Arc::new(Mutex::new(ExpansionData::new(expansion_table)));

    let (sender, receiver) = std::sync::mpsc::channel();

    // Processing thread — uses explicit loop so we can pass &receiver for draining
    thread::spawn(move || {
        loop {
            let message = match receiver.recv() {
                Ok(msg) => msg,
                Err(_) => break, // sender dropped
            };

            match message {
                HookMessage::KeyDown { key, vk_code, scan_code } => {
                    let event_name = keyboard_hook::resolve_character(vk_code, scan_code);
                    handle_key_press(expansion_data.clone(), key, event_name, &receiver);
                }
                HookMessage::MouseDown(button) => {
                    handle_mouse_press(expansion_data.clone(), button);
                }
            }
        }
    });

    // Install hooks and run message pump (blocks main thread)
    if let Err(error) = keyboard_hook::install_hooks_and_run(sender) {
        println!("Error: {:?}", error);
    }
}

fn handle_key_press(
    expansion_data_arc: Arc<Mutex<ExpansionData>>,
    key: KeyId,
    event_name: Option<String>,
    receiver: &Receiver<HookMessage>,
) {
    if !GLOBAL_LISTENING.load(Ordering::SeqCst) {
        return;
    }

    let mut expansion_data = expansion_data_arc.lock().unwrap();

    debug_println!("Key pressed: {:?}", key);

    match key {
        KeyId::Space | KeyId::Return => {
            // Space/Enter are swallowed by the hook to prevent WM_CHAR
            // ordering issues. We must re-inject them if no expansion fires.
            let (reinject_vk, reinject_scan) = match key {
                KeyId::Space => (0x20u16, 0x39u16),
                _ => (0x0Du16, 0x1Cu16), // VK_RETURN, scan 0x1C
            };
            let separator = if key == KeyId::Space { " " } else { "\n" };

            match expansion_data.typing_state {
                TypingState::Typing => {
                    // Check for expansion match
                    if let Some((trigger_length, completion)) = check_for_completion(&expansion_data) {
                        debug_println!("Found match: {}", completion);
                        expansion_data.reset();
                        drop(expansion_data);
                        expand_trigger_phrase(trigger_length, completion, separator, receiver)
                            .expect("Error in expand_trigger_phrase");
                        return;
                    }

                    // Check for ff trigger
                    if expansion_data.key_buffer == "ff" {
                        expansion_data.reset();
                        drop(expansion_data);

                        disable_keyboard_listening();

                        // Delete "ff" only (separator was swallowed)
                        windows_input::send_backspaces_fast(2)
                            .expect("Error sending backspaces for ff");
                        thread::sleep(Duration::from_millis(30));

                        // Select to end of line and delete
                        windows_input::send_shift_end()
                            .expect("Error sending Shift+End for ff");
                        thread::sleep(Duration::from_millis(30));
                        windows_input::send_delete_key()
                            .expect("Error sending Delete for ff");

                        replay_buffered_keystrokes(receiver);
                        enable_keyboard_listening();
                        return;
                    }

                    // Check for nn (date) trigger
                    if expansion_data.key_buffer == "nn" {
                        let now = Local::now();
                        let date_string = now.format("%-m/%-d/%y:").to_string();
                        expansion_data.reset();
                        drop(expansion_data);
                        expand_trigger_phrase(2, date_string, separator, receiver)
                            .expect("Error in expanding date phrase");
                        return;
                    }

                    // Check for /wksN and /daysN triggers
                    if let Some(date_string) = handle_date_expansion(&expansion_data.key_buffer) {
                        let trigger_length = expansion_data.key_buffer.len();
                        debug_println!("Date expansion triggered: {}", date_string);
                        expansion_data.reset();
                        drop(expansion_data);
                        expand_trigger_phrase(trigger_length, date_string, separator, receiver)
                            .expect("Error in date expansion");
                        return;
                    }

                    // No match — re-inject the swallowed key and transition
                    drop(expansion_data);
                    let _ = windows_input::send_key_tap(reinject_vk, reinject_scan);
                    let mut expansion_data = expansion_data_arc.lock().unwrap();
                    if let KeyId::Space = key {
                        expansion_data.push_to_buffer(" ");
                        expansion_data.set_typing_state(TypingState::NoMatch);
                    } else {
                        expansion_data.reset();
                    }
                }

                TypingState::Empty | TypingState::NoMatch => {
                    if matches!(expansion_data.typing_state, TypingState::NoMatch) {
                        expansion_data.reset();
                    }
                    // Re-inject the swallowed key
                    drop(expansion_data);
                    let _ = windows_input::send_key_tap(reinject_vk, reinject_scan);
                }
            }
        }

        KeyId::Backspace => {
            expansion_data.pop_from_buffer();
            expansion_data.set_typing_state(TypingState::Typing);
            debug_println!("{:?}", &expansion_data.key_buffer);
        }

        // Cursor movement
        KeyId::LeftArrow => {
            expansion_data.decrement_cursor_position();
        }
        KeyId::RightArrow => {
            if expansion_data.key_buffer.len() == expansion_data.cursor_position {
                expansion_data.reset();
                return;
            } else {
                expansion_data.increment_cursor_position();
            }
        }

        // Navigation keys — reset
        KeyId::UpArrow | KeyId::DownArrow | KeyId::Escape | KeyId::Tab
        | KeyId::PageDown | KeyId::PageUp | KeyId::Home | KeyId::End => {
            expansion_data.reset();
            return;
        }

        // Printable characters
        KeyId::KeyA | KeyId::KeyB | KeyId::KeyC | KeyId::KeyD | KeyId::KeyE | KeyId::KeyF
        | KeyId::KeyG | KeyId::KeyH | KeyId::KeyI | KeyId::KeyJ | KeyId::KeyK | KeyId::KeyL | KeyId::KeyM
        | KeyId::KeyN | KeyId::KeyO | KeyId::KeyP | KeyId::KeyQ | KeyId::KeyR | KeyId::KeyS | KeyId::KeyT
        | KeyId::KeyU | KeyId::KeyV | KeyId::KeyW | KeyId::KeyX | KeyId::KeyY | KeyId::KeyZ
        | KeyId::Num0 | KeyId::Num1 | KeyId::Num2 | KeyId::Num3 | KeyId::Num4 | KeyId::Num5
        | KeyId::Num6 | KeyId::Num7 | KeyId::Num8 | KeyId::Num9
        | KeyId::Minus | KeyId::Equal | KeyId::LeftBracket | KeyId::RightBracket
        | KeyId::Quote | KeyId::Comma | KeyId::Dot | KeyId::Slash
        | KeyId::SemiColon | KeyId::BackSlash | KeyId::BackQuote => {
            if matches!(expansion_data.typing_state, TypingState::NoMatch) {
                expansion_data.reset();
            }
            expansion_data.set_typing_state(TypingState::Typing);
            if let Some(c) = event_name {
                debug_println!("{:?}", c);
                debug_println!(
                    "Char to push: '{}', len: {}, bytes: {:?}",
                    c,
                    c.len(),
                    c.as_bytes()
                );
                expansion_data.push_to_buffer(&c);
                debug_println!("{:?}", &expansion_data.key_buffer);
            }
        }

        _ => {}
    }
}

fn handle_mouse_press(buffer: Arc<Mutex<ExpansionData>>, button: MouseButton) {
    match button {
        MouseButton::Left | MouseButton::Right | MouseButton::Middle => {
            buffer.lock().unwrap().reset();
            debug_println!("Mouse button pressed, buffer cleared");
        }
    }
}

fn load_expansion_table() -> Result<ExpansionFile, Box<dyn std::error::Error>> {
    let path = env::current_exe()?
        .parent()
        .ok_or("Failed to get executable directory")?
        .join("expansions.toml");

    println!("Loading expansions from: {:?}", path);

    let contents = std::fs::read_to_string(&path)?;
    let expansion_file: ExpansionFile = toml::from_str(&contents)?;

    Ok(expansion_file)
}

fn check_for_completion(expansion_data: &ExpansionData) -> Option<(usize, String)> {
    let buffer = &expansion_data.key_buffer;

    // Case-sensitive lookup first
    if let Some(expansion) = expansion_data.expansion_table.case_sensitive.get(buffer) {
        return Some((buffer.len(), expansion.clone()));
    }

    // Case-insensitive section — also matched by exact case (all triggers stored lowercase)
    if let Some(expansion) = expansion_data.expansion_table.case_insensitive.get(buffer) {
        return Some((buffer.len(), expansion.clone()));
    }

    None
}

/// Backspaces first, then clipboard set, then paste, then restore.
/// Receives &Receiver to drain synthetic events before re-enabling listening.
fn expand_trigger_phrase(
    length: usize,
    completion: String,
    separator: &str,
    receiver: &Receiver<HookMessage>,
) -> Result<(), Box<dyn std::error::Error>> {
    disable_keyboard_listening();

    let completion = format!("{}{}", completion, separator);
    let completion = completion.replace("\n", "\r\n");

    // Step 1: Send backspaces (separator was swallowed by hook, so just trigger length)
    windows_input::send_backspaces_fast(length)?;
    debug_println!("deleted {} characters", length);

    // Step 2: Wait for target app to finish processing backspaces
    thread::sleep(Duration::from_millis(30));

    // Step 3: Save old clipboard, set expansion text
    let mut clipboard = Clipboard::new()?;
    let old_clipboard = clipboard.get_text().unwrap_or_default();
    clipboard.set_text(completion.to_owned())?;

    // Step 4: Let clipboard settle
    thread::sleep(Duration::from_millis(10));

    // Step 5: Paste
    windows_input::send_ctrl_v()?;

    // Step 6: Wait for paste to complete — target app must process Ctrl+V
    // from its message queue and read clipboard before we restore it.
    // 50ms was too short for some apps; 100ms gives comfortable margin.
    thread::sleep(Duration::from_millis(100));

    // Step 7: Restore old clipboard
    clipboard.set_text(old_clipboard)?;

    // Step 8: Replay any keystrokes the user typed during expansion
    replay_buffered_keystrokes(receiver);

    // Step 9: Re-enable listening
    enable_keyboard_listening();

    Ok(())
}

/// Replay keystrokes that were buffered during expansion.
/// Re-injects them as synthetic key taps so the hook passes them to the target
/// app without re-sending them to the channel. Mouse events are discarded.
fn replay_buffered_keystrokes(receiver: &Receiver<HookMessage>) {
    while let Ok(msg) = receiver.try_recv() {
        if let HookMessage::KeyDown { vk_code, scan_code, .. } = msg {
            let _ = windows_input::send_key_tap(vk_code as u16, scan_code as u16);
        }
    }
}

/// Checks for date expansion triggers like "/days40", "/wks8", or "/mo3".
fn handle_date_expansion(buffer: &str) -> Option<String> {
    debug_println!("doing the date expansion thing!");

    let (prefix, num_str) = if buffer.starts_with("/days") {
        ("/days", &buffer[5..])
    } else if buffer.starts_with("/wks") {
        ("/wks", &buffer[4..])
    } else if buffer.starts_with("/mo") {
        ("/mo", &buffer[3..])
    } else {
        return None;
    };

    debug_println!("made it through 1st if: {prefix}, {num_str}");

    if let Ok(num) = num_str.parse::<i64>() {
        let current_date = Local::now();

        let future_date = if prefix == "/mo" {
            if num >= 0 {
                current_date.checked_add_months(chrono::Months::new(num as u32))
            } else {
                current_date.checked_sub_months(chrono::Months::new((-num) as u32))
            }
        } else if prefix == "/days" {
            current_date.checked_add_signed(chrono::Duration::days(num))
        } else {
            current_date.checked_add_signed(chrono::Duration::weeks(num))
        };

        if let Some(date) = future_date {
            let formatted_with_padding = date.format("%m/%d/%y").to_string();
            let parts: Vec<&str> = formatted_with_padding.split('/').collect();
            let formatted = format!(
                "{}/{}/{}",
                parts[0].parse::<u32>().unwrap(),
                parts[1].parse::<u32>().unwrap(),
                parts[2]
            );

            debug_println!("formatted date str, returning: {formatted}");
            return Some(formatted);
        }
    }

    None
}

fn disable_keyboard_listening() {
    GLOBAL_LISTENING.store(false, Ordering::SeqCst);
}
fn enable_keyboard_listening() {
    GLOBAL_LISTENING.store(true, Ordering::SeqCst);
}
