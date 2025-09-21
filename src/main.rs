use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::usize;
use std::{collections::HashMap, sync::Mutex};
use std::sync::{MutexGuard};
use rdev::{listen, Button, Event, EventType, Key};
use std::thread::{self, sleep};
use std::time::Duration;
use serde::Deserialize;
use arboard::Clipboard;
use chrono::{Local};


/// A macro that functions like `println!`, but only compiles in debug builds.
#[macro_export]
macro_rules! debug_println {
    ($($arg:tt)*) => {
        // This version is used in debug builds
        #[cfg(debug_assertions)]
        {
            print!("[DEBUG] "); // Optional: Add a prefix to easily spot debug prints
            println!($($arg)*);
        }
        // This version is used in release builds and expands to nothing
        #[cfg(not(debug_assertions))]
        {
            // The macro call is replaced with an empty expression,
            // so there is zero performance impact.
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

enum KeyEventMessage {
    KeyPress(rdev::Key, Option<String>),
    MouseClick(rdev::Button),
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
        // Cast the cursor position to usize, as string indexing requires it.
        // We'll also clamp the value to prevent panics if the cursor is out of bounds.
        let index = (self.cursor_position as usize).min(self.key_buffer.len());

        // Insert the string slice 'c' at the calculated index.
        self.key_buffer.insert_str(index, c);

        // After inserting, we must advance the cursor by the length of what was inserted.
        // Note: This might cause issues with your i8 type if 'c' is long! (More on this below)
        self.cursor_position += c.len();
    }
    fn pop_from_buffer(&mut self) {
        // We can only remove a character if the buffer is not empty AND the cursor is not at the start.
        if self.cursor_position > 0 && !self.key_buffer.is_empty() {
            // The cursor is positioned AFTER the character we want to remove.
            // So, we need to calculate the index of the character to remove.
            let remove_index = self.cursor_position - 1;

            // Ensure the calculated index is valid before removing.
            // This check is important if cursor logic and buffer length can get out of sync.
            if self.key_buffer.is_char_boundary(remove_index as usize) {
                self.key_buffer.remove(remove_index as usize);
                
                // After removing the character, move the cursor back.
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


// atomic boolean for listening state
static GLOBAL_LISTENING: AtomicBool = AtomicBool::new(true);

fn main() {
    // load up toml and create hashmap
    let expansion_table = load_expansion_table().unwrap();

    let expansion_data = Arc::new(Mutex::new(ExpansionData::new(expansion_table)));

    let (sender, receiver) = std::sync::mpsc::channel();

    thread::spawn(move || {
        // This thread loops forever, receiving messages.
        for message in receiver {
            // All your complex logic now lives safely on this one thread.
            match message {
                KeyEventMessage::KeyPress(key, event_name) => {
                    handle_key_press(expansion_data.clone(), key, event_name);
                },
                KeyEventMessage::MouseClick(button) => {
                    handle_mouse_press(expansion_data.clone(), button);
                },
            }
        }
    });

    let callback = move |event: Event| {
        let message = match event.event_type {
            EventType::KeyPress(key) => Some(KeyEventMessage::KeyPress(key, event.name)),
            EventType::ButtonPress(button) => Some(KeyEventMessage::MouseClick(button)),
            _ => None,
        };

        if let Some(msg) = message {
            // Send the thread-safe message. This is non-blocking and very fast.
            sender.send(msg).unwrap();
        }
    };

    if let Err(error) = listen(callback) {
        println!("Error: {:?}", error)
    }

    loop {
        thread::park();
    }

}

fn handle_key_press(expansion_data: Arc<Mutex<ExpansionData>>, key: rdev::Key, event_name: Option<String>) {

    
    if GLOBAL_LISTENING.load(Ordering::SeqCst) == false {
        // println!("Global listening disabled, ignoring key press");
        return;
    }

    // acquire lock on expansion data
    let mut expansion_data = expansion_data.lock().unwrap();

    debug_println!("Key pressed: {:?}", key);

    match key {
        Key::Space | Key::Return => {
            match expansion_data.typing_state {
                
                TypingState::Typing => {
                // check for match; if we don't find one, set primed flag
                if let Some((trigger_length, completion)) = check_for_completion(&mut expansion_data) {
                    debug_println!("Found match: {}", completion);
                    thread::spawn( move || {
                        expand_trigger_phrase(trigger_length, completion).unwrap();
                        
                    });

                    expansion_data.reset();
                    return;
                }

                //check for special cases here, like ff
                // TODO, build these!
                if expansion_data.key_buffer == "ff" {
                    delete_characters(3);
                    rdev::simulate(&EventType::KeyPress(Key::ShiftLeft)).unwrap();
                    rdev::simulate(&EventType::KeyPress(Key::ShiftRight)).unwrap();
                    rdev::simulate(&EventType::KeyPress(Key::End)).unwrap();
                    rdev::simulate(&EventType::KeyRelease(Key::End)).unwrap();
                    rdev::simulate(&EventType::KeyPress(Key::Space)).unwrap();
                    rdev::simulate(&EventType::KeyRelease(Key::Space)).unwrap();                  
                    rdev::simulate(&EventType::KeyRelease(Key::ShiftLeft)).unwrap();
                    rdev::simulate(&EventType::KeyRelease(Key::ShiftRight)).unwrap();
                }

                if expansion_data.key_buffer == "nn" {
                    // inputs date and simulates keys to type: "mm/dd/yy:" without leading 0s
                    let now = chrono::Local::now();
                    let date_string = now.format("%-m/%-d/%y").to_string();
                    
                    GLOBAL_LISTENING.store(false, Ordering::SeqCst);

                    sleep(Duration::from_millis(20));
                    delete_characters(2);
                    for c in date_string.chars() {
                        let key_event = match c {
                            '0' => Key::Num0,
                            '1' => Key::Num1,
                            '2' => Key::Num2,
                            '3' => Key::Num3,
                            '4' => Key::Num4,
                            '5' => Key::Num5,
                            '6' => Key::Num6,
                            '7' => Key::Num7,
                            '8' => Key::Num8,
                            '9' => Key::Num9,
                            '/' => Key::Slash,
                            ' ' => Key::Space,
                            _ => continue, // Skip unsupported characters
                        };
                        rdev::simulate(&EventType::KeyPress(key_event)).unwrap();
                        rdev::simulate(&EventType::KeyRelease(key_event)).unwrap();
                        sleep(Duration::from_millis(10)); // slight delay between key presses
                    }
                    rdev::simulate(&EventType::KeyPress(Key::ShiftLeft)).unwrap();
                    sleep(Duration::from_millis(10));
                    rdev::simulate(&EventType::KeyPress(Key::SemiColon)).unwrap();
                    rdev::simulate(&EventType::KeyRelease(Key::SemiColon)).unwrap();
                    rdev::simulate(&EventType::KeyRelease(Key::ShiftLeft)).unwrap();
                    sleep(Duration::from_millis(5));
                    rdev::simulate(&EventType::KeyPress(Key::Space)).unwrap();
                    rdev::simulate(&EventType::KeyRelease(Key::Space)).unwrap();
                    
                    GLOBAL_LISTENING.store(true, Ordering::SeqCst);
                }
                    
                if let Some(date_string) = handle_date_expansion(&expansion_data.key_buffer) {
                    GLOBAL_LISTENING.store(false, Ordering::SeqCst);
                    let trigger_length = expansion_data.key_buffer.len();
                    debug_println!("Date expansion triggered: {}", date_string);
                    
                    // Spawn a thread to do the simulation. Delete the trigger + the space/enter.
                    thread::spawn(move || {
                        expand_trigger_phrase(trigger_length + 1, date_string).unwrap();
                    });

                    expansion_data.reset();
                    return;
                }

                // no match, set the typing state to NoMatch/prime it
                // special function if this was a space key
                if let Key::Space = key {
                    expansion_data.push_to_buffer(" ");
                    //expansion_data.increment();
                    expansion_data.set_typing_state(TypingState::NoMatch);
                }
                else { // enter key
                    expansion_data.reset();
                }
                
                
                }
                
                TypingState::Empty => {}
                
                TypingState::NoMatch => { expansion_data.reset(); }
            }
            
        
        }
        

        Key::Backspace => {
            expansion_data.pop_from_buffer();
            expansion_data.set_typing_state(TypingState::Typing);
            //expansion_data.decrement();

            debug_println!("{:?}", &expansion_data.key_buffer);
        },

        //cases that adjust cursor position
        Key::LeftArrow => { expansion_data.decrement_cursor_position();}
        Key::RightArrow => {
            // if we're at the end of the buffer, reset
            if expansion_data.key_buffer.len() == expansion_data.cursor_position {
                expansion_data.reset();
                return;
            }
            else {
                expansion_data.increment_cursor_position();
            }
            // if we're not, just increment
        }

        // Key::Delete => {}

        //cases that instantly clear the buffer and resets
        Key::UpArrow | Key::DownArrow | Key::Escape | Key::Tab |
        Key::PageDown | Key::PageUp | Key::Home | Key::End => {
            expansion_data.reset();
            return;
        }

        Key::KeyA | Key::KeyB | Key::KeyC | Key::KeyD | Key::KeyE | Key::KeyF |
        Key::KeyG | Key::KeyH | Key::KeyI | Key::KeyJ | Key::KeyK | Key::KeyL | Key::KeyM |
        Key::KeyN | Key::KeyO | Key::KeyP | Key::KeyQ | Key::KeyR | Key::KeyS | Key::KeyT |
        Key::KeyU | Key::KeyV | Key::KeyW | Key::KeyX | Key::KeyY | Key::KeyZ |
        Key::Num0 | Key::Num1 | Key::Num2 | Key::Num3 | Key::Num4 | Key::Num5 |
        Key::Num6 | Key::Num7 | Key::Num8 | Key::Num9 |
        Key::Minus | Key::Equal | Key::LeftBracket | Key::RightBracket |
        Key::Quote | Key::Comma | Key::Dot | Key::Slash => {
            if matches!(expansion_data.typing_state, TypingState::NoMatch) {
                expansion_data.reset();
            }
            expansion_data.set_typing_state(TypingState::Typing);
            if let Some(c) = event_name {
                debug_println!("{:?}", c);
                debug_println!("Char to push: '{}', len: {}, bytes: {:?}", c, c.len(), c.as_bytes());

                expansion_data.push_to_buffer(&c);
                debug_println!("{:?}", &expansion_data.key_buffer);
            }
        },
        _ => {}
    }
}

fn handle_mouse_press(buffer: Arc<Mutex<ExpansionData>>, button: Button) {
    // handle mouse clicks
    match button {
        rdev::Button::Left | rdev::Button::Right | rdev::Button::Middle => {
            { buffer.lock().unwrap().reset(); }
            debug_println!("Mouse button pressed, buffer cleared");
        },
        _ => {}
    }
}

fn load_expansion_table() -> Result<ExpansionFile, Box<dyn std::error::Error> > 
{
    let path = "C:\\Projects\\text_expander\\expansions.toml";
    let contents = std::fs::read_to_string(path)?;
    let expansion_file: ExpansionFile = toml::from_str(&contents)?;    
    
    //for (key, value) in &expansion_file.case_insensitive {
    //    println!("{}: {}", key, value);
    //}
    
    Ok(expansion_file)
}

fn check_for_completion(expansion_data: &mut MutexGuard<ExpansionData>) ->
    Option<(usize, String)> {
    // returns option containing a tuple of length of the trigger and the resulting expansion
    // check the buffer against expansion file
    let buffer = &expansion_data.key_buffer;
    
    if let Some(expansion) = expansion_data.expansion_table.case_sensitive.get(buffer) {
        return Some((buffer.len(), expansion.clone()));
    }
    
    if let Some(expansion) = expansion_data.expansion_table.case_insensitive.get(buffer) {
        return Some((buffer.len(), expansion.clone()));
    }
    // no matches found? return None
    None
}

fn expand_trigger_phrase(length: usize, completion: String) 
    -> Result<(), Box<dyn std::error::Error>> {
    
    // thread::spawn(move || {
    // expansion_data.global_listening = false; // disable global listening during expansion
    GLOBAL_LISTENING.store(false, Ordering::SeqCst);
    let completion = completion.replace("\n", "\r\n");
    
    delete_characters(length);

    debug_println!("deleted {} characters", length);

    let mut clipboard = Clipboard::new().unwrap();

    // get old clipboard contents
    let old_clipboard = clipboard.get_text().unwrap_or_default();
    clipboard.set_text(completion.to_owned()).unwrap();
    sleep(Duration::from_millis(50)); // wait a bit to ensure clipboard is set

    rdev::simulate(&EventType::KeyPress(Key::ControlLeft)).unwrap();
    rdev::simulate(&EventType::KeyPress(Key::KeyV)).unwrap();
    rdev::simulate(&EventType::KeyRelease(Key::KeyV)).unwrap();
    rdev::simulate(&EventType::KeyRelease(Key::ControlLeft)).unwrap();

    // println!("pasted: {}", completion);
    sleep(Duration::from_millis(50)); // wait a bit to ensure paste is done
    // restore old clipboard contents
    clipboard.set_text(old_clipboard).unwrap();

    GLOBAL_LISTENING.store(true, Ordering::SeqCst);

    Ok(())

}

fn delete_characters(count: usize) {
    debug_println!("Deleting {} characters", count);

    for _ in 0..count + 1 {

        // println!("Simulating backspace");
        if let Err(e) = rdev::simulate(&EventType::KeyPress(Key::Backspace)) {
            println!("Error simulating backspace: {}", e);
        }
        thread::sleep(Duration::from_millis(10)); // slight delay to ensure key press is registered
        // println!("Backspace pressed");
        if let Err(e) = rdev::simulate(&EventType::KeyRelease(Key::Backspace)) {
            println!("Error simulating backspace release: {}", e);
        }
        // println!("Backspace released");
        thread::sleep(Duration::from_millis(10));
    }
}
    
/// Checks for date expansion triggers like "/days40" or "/wks8".
/// Returns a formatted date string (e.g., "9/16/25") if a valid trigger is found.
fn handle_date_expansion(buffer: &str) -> Option<String> {
    debug_println!("doing the date expansion thing!");
    
    let (prefix, num_str) = if buffer.starts_with("/days") {
        ("/days", &buffer[5..])
    } else if buffer.starts_with("/wks") {
        ("/wks", &buffer[4..])
    } else {
        return None; // Not a date expansion trigger
    };
    
    debug_println!("made it through 1st if: {prefix}, {num_str}");

    // Try to parse the number part of the trigger
    if let Ok(num) = num_str.parse::<i64>() {
        let current_date = Local::now();
        
        // Calculate the future date safely
        let future_date = if prefix == "/days" {
            current_date.checked_add_signed(chrono::Duration::days(num))
        } else { // "/wks"
            current_date.checked_add_signed(chrono::Duration::weeks(num))
        };

        // Only proceed if we got a valid future date
        if let Some(date) = future_date {
            // Use format with standard specifiers that work everywhere
            // %m = month with zero padding, %d = day with zero padding, %y = 2-digit year
            let formatted_with_padding = date.format("%m/%d/%y").to_string();
            
            // Now remove leading zeros manually
            let parts: Vec<&str> = formatted_with_padding.split('/').collect();
            let formatted = format!("{}/{}/{}",
                parts[0].parse::<u32>().unwrap(),  // Parsing removes leading zeros
                parts[1].parse::<u32>().unwrap(),
                parts[2]  // Year is already 2 digits
            );
            
            debug_println!("formatted date str, returning: {formatted}");
            return Some(formatted);
        }
    }
    
    None
}