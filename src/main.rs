use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};
use std::sync::{MutexGuard};
use rdev::{listen, Button, Event, EventType, Key};
use std::thread::{self, sleep};
use std::time::Duration;
use serde::Deserialize;
use arboard::Clipboard;

#[derive(Debug, Deserialize)]
struct ExpansionFile {
    case_sensitive: HashMap<String, String>,
    case_insensitive: HashMap<String, String>,
}

struct ExpansionData {
    key_buffer: String,
    expansion_table: ExpansionFile,
    cursor_position: i8,
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
        self.key_buffer.push_str(c);
    }

    fn pop_from_buffer(&mut self) {
        self.key_buffer.pop();
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

    fn decrement(&mut self) {
        if self.cursor_position >= 0 {
            self.cursor_position -= 1;
        }
        if self.cursor_position == 0 {
            self.typing_state = TypingState::Empty;
        }
    }

    fn increment(&mut self) {
        self.cursor_position += 1;
    }
    
}


// atomic boolean for listening state
static GLOBAL_LISTENING: AtomicBool = AtomicBool::new(true);

fn main() {
    // load up toml and create hashmap
    let expansion_table = load_expansion_table().unwrap();

    let expansion_data = Arc::new(Mutex::new(ExpansionData::new(expansion_table)));

    // let expansion_clone = expansion_data.clone();

    // set key buffer + clone it for the callback closure
    // let key_buffer = Arc::new(Mutex::new(String::new()));
    // let callback_buffer = key_buffer.clone();
    // if we need to go back to passing the buffer, we can


    let callback = move |event: Event| {
        match event.event_type {
            EventType::KeyPress(key) => {
                // key press: send event (to get name), buffer, and key to handler
                // let mut data = expansion_clone.lock().unwrap();
                handle_key_press(event, expansion_data.clone(), key);

            },
            EventType::ButtonPress(button) => {
                // mouse button: just send the buffer + button to handler
                handle_mouse_press(expansion_data.clone(), button);
            },
            _ => { return; }
        }
    };

    if let Err(error) = listen(callback) {
        println!("Error: {:?}", error)
    }

    loop {
        thread::park();
    }

}

fn handle_key_press(event: Event, expansion_data: Arc<Mutex<ExpansionData>>, key: Key) {

    //let mut expansion_data = expansion_data.lock().unwrap();

    let mut expansion_data = expansion_data.lock().unwrap();

    println!("Handling key press event: {:?}", event);
    // check global listening flag
    if GLOBAL_LISTENING.load(Ordering::SeqCst) == false {
        println!("Global listening disabled, ignoring key press");
        return;
    }

    println!("Key pressed: {:?}", key);

    match key {
        Key::Space | Key::Return => {
            match expansion_data.typing_state {
                
                TypingState::Typing => {
                // check for match; if we don't find one, set primed flag
                if let Some((trigger_length, completion)) = check_for_completion(&mut expansion_data) {
                    println!("Found match: {}", completion);
                    expand_trigger_phrase(trigger_length, completion).unwrap();

                    expansion_data.reset();
                    return;
                }

                //check for special cases here, like ff
                // TODO, build these!
                if expansion_data.key_buffer == "ff" {}
                if expansion_data.key_buffer == "nn" {}
                if expansion_data.key_buffer.starts_with("/days") {}

                // no match, set the typing state to NoMatch/prime it
                // special function if this was a space key
                if let Key::Space = key {
                    expansion_data.push_to_buffer(" ");
                    expansion_data.increment();
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
            expansion_data.decrement();

            println!("{:?}", &expansion_data.key_buffer);
        },

        //cases that adjust cursor position
        Key::LeftArrow => { expansion_data.decrement();}
        Key::RightArrow => {
            // if we're at the end of the buffer, reset
            if expansion_data.key_buffer.len() as i8 == expansion_data.cursor_position {
                expansion_data.reset();
                return;
            }
            else {
                expansion_data.increment();
            }
            // if we're not, just increment
        }

        // Key::Delete => {}

        //cases that instantly clear the buffer and resets
        Key::UpArrow | Key::DownArrow | Key::Escape | Key::Tab => {
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
            expansion_data.set_typing_state(TypingState::Typing);
            if let Some(c) = event.name {
                println!("{:?}", c);
                expansion_data.push_to_buffer(&c);
                println!("{:?}", &expansion_data.key_buffer);
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
            println!("Mouse button pressed, buffer cleared");
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
    
    thread::spawn(move || {
    // expansion_data.global_listening = false; // disable global listening during expansion
    GLOBAL_LISTENING.store(false, Ordering::SeqCst);
    let completion = completion.replace("\n", "\r\n");
    
    delete_characters(length);

    println!("deleted {} characters", length);

    let mut clipboard = Clipboard::new().unwrap();

    // get old clipboard contents
    let old_clipboard = clipboard.get_text().unwrap_or_default();
    clipboard.set_text(completion.to_owned()).unwrap();
    sleep(Duration::from_millis(50)); // wait a bit to ensure clipboard is set

    rdev::simulate(&EventType::KeyPress(Key::ControlLeft)).unwrap();
    rdev::simulate(&EventType::KeyPress(Key::KeyV)).unwrap();
    rdev::simulate(&EventType::KeyRelease(Key::KeyV)).unwrap();
    rdev::simulate(&EventType::KeyRelease(Key::ControlLeft)).unwrap();

    println!("pasted: {}", completion);
    sleep(Duration::from_millis(50)); // wait a bit to ensure paste is done
    // restore old clipboard contents
    clipboard.set_text(old_clipboard).unwrap();

    GLOBAL_LISTENING.store(true, Ordering::SeqCst);

    });

    Ok(())

}

fn delete_characters(count: usize) {
    println!("Deleting {} characters", count);

    for _ in 0..count {

        println!("Simulating backspace");
        if let Err(e) = rdev::simulate(&EventType::KeyPress(Key::Backspace)) {
            println!("Error simulating backspace: {}", e);
        }
        println!("Backspace pressed");
        if let Err(e) = rdev::simulate(&EventType::KeyRelease(Key::Backspace)) {
            println!("Error simulating backspace release: {}", e);
        }
        println!("Backspace released");
    }
    
}