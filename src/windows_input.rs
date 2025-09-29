// src/windows_input.rs
use winapi::um::winuser::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, 
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VK_BACK, VK_CONTROL, VK_SHIFT,
    VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN, VK_END, VK_SPACE, VK_RETURN
};
use winapi::shared::minwindef::WORD;
use std::thread;
use std::time::Duration;

// Add these imports to your main.rs
use std::ptr::null_mut;

// Add this Windows clipboard helper module to windows_input.rs:
// (Add these additional imports to windows_input.rs)
use winapi::um::winuser::{
    OpenClipboard, CloseClipboard, EmptyClipboard, SetClipboardData,
    GetClipboardSequenceNumber, CF_UNICODETEXT
};
use winapi::ctypes::c_void;


pub fn send_backspaces_fast(count: usize) -> Result<(), Box<dyn std::error::Error>> {
    use std::mem;
    
    // Create an array of INPUT structures for all backspaces
    // We need 2 events per backspace (press + release)
    let mut inputs: Vec<INPUT> = Vec::with_capacity(count * 2);
    
    for _ in 0..count {
        // Create key down event
        let mut key_down: INPUT = unsafe { mem::zeroed() };
        unsafe {
            key_down.type_ = INPUT_KEYBOARD;
            key_down.u.ki_mut().wVk = VK_BACK as WORD;
            key_down.u.ki_mut().dwFlags = 0;
        }
        inputs.push(key_down);
        
        // Create key up event
        let mut key_up: INPUT = unsafe { mem::zeroed() };
        unsafe {
            key_up.type_ = INPUT_KEYBOARD;
            key_up.u.ki_mut().wVk = VK_BACK as WORD;
            key_up.u.ki_mut().dwFlags = KEYEVENTF_KEYUP;
        }
        inputs.push(key_up);
    }
    
    // Send all inputs in one call
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            mem::size_of::<INPUT>() as i32
        )
    };
    
    if sent != inputs.len() as u32 {
        return Err(format!("Failed to send all inputs. Sent: {}/{}", sent, inputs.len()).into());
    }
    
    Ok(())
}

pub fn send_text_via_unicode(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::mem;
    
    // Convert text to UTF-16
    let utf16_text: Vec<u16> = text.encode_utf16().collect();
    
    // Create inputs for each character (press + release)
    let mut inputs: Vec<INPUT> = Vec::with_capacity(utf16_text.len() * 2);
    
    for ch in utf16_text {
        // Skip carriage return if it's part of \r\n (we'll handle line breaks separately)
        if ch == 0x0D {
            continue;
        }
        
        // Convert newline to Enter key
        if ch == 0x0A {
            // Enter key down
            let mut key_down: INPUT = unsafe { mem::zeroed() };
            unsafe {
                key_down.type_ = INPUT_KEYBOARD;
                key_down.u.ki_mut().wVk = VK_RETURN as WORD;
                key_down.u.ki_mut().dwFlags = 0;
            }
            inputs.push(key_down);
            
            // Enter key up
            let mut key_up: INPUT = unsafe { mem::zeroed() };
            unsafe {
                key_up.type_ = INPUT_KEYBOARD;
                key_up.u.ki_mut().wVk = VK_RETURN as WORD;
                key_up.u.ki_mut().dwFlags = KEYEVENTF_KEYUP;
            }
            inputs.push(key_up);
        } else {
            // Unicode character down
            let mut char_down: INPUT = unsafe { mem::zeroed() };
            unsafe {
                char_down.type_ = INPUT_KEYBOARD;
                char_down.u.ki_mut().wScan = ch;
                char_down.u.ki_mut().dwFlags = KEYEVENTF_UNICODE;
            }
            inputs.push(char_down);
            
            // Unicode character up
            let mut char_up: INPUT = unsafe { mem::zeroed() };
            unsafe {
                char_up.type_ = INPUT_KEYBOARD;
                char_up.u.ki_mut().wScan = ch;
                char_up.u.ki_mut().dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;
            }
            inputs.push(char_up);
        }
    }
    
    if inputs.is_empty() {
        return Ok(());
    }
    
    // Send all inputs at once
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            mem::size_of::<INPUT>() as i32
        )
    };
    
    if sent != inputs.len() as u32 {
        return Err(format!("Failed to send all text. Sent: {}/{}", sent, inputs.len()).into());
    }
    
    Ok(())
}

pub fn send_ctrl_v() -> Result<(), Box<dyn std::error::Error>> {
    use std::mem;
    
    let mut inputs: Vec<INPUT> = Vec::with_capacity(4);
    
    // Ctrl down
    let mut ctrl_down: INPUT = unsafe { mem::zeroed() };
    unsafe {
        ctrl_down.type_ = INPUT_KEYBOARD;
        ctrl_down.u.ki_mut().wVk = VK_CONTROL as WORD;
        ctrl_down.u.ki_mut().dwFlags = 0;
    }
    inputs.push(ctrl_down);
    
    // V down
    let mut v_down: INPUT = unsafe { mem::zeroed() };
    unsafe {
        v_down.type_ = INPUT_KEYBOARD;
        v_down.u.ki_mut().wVk = 'V' as WORD;
        v_down.u.ki_mut().dwFlags = 0;
    }
    inputs.push(v_down);
    
    // V up
    let mut v_up: INPUT = unsafe { mem::zeroed() };
    unsafe {
        v_up.type_ = INPUT_KEYBOARD;
        v_up.u.ki_mut().wVk = 'V' as WORD;
        v_up.u.ki_mut().dwFlags = KEYEVENTF_KEYUP;
    }
    inputs.push(v_up);
    
    // Ctrl up
    let mut ctrl_up: INPUT = unsafe { mem::zeroed() };
    unsafe {
        ctrl_up.type_ = INPUT_KEYBOARD;
        ctrl_up.u.ki_mut().wVk = VK_CONTROL as WORD;
        ctrl_up.u.ki_mut().dwFlags = KEYEVENTF_KEYUP;
    }
    inputs.push(ctrl_up);
    
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            mem::size_of::<INPUT>() as i32
        )
    };
    
    if sent != inputs.len() as u32 {
        return Err(format!("Failed to send Ctrl+V. Sent: {}/{}", sent, inputs.len()).into());
    }
    
    Ok(())
}

// Alternative: Direct text injection without clipboard
pub fn expand_text_directly(trigger_len: usize, text: String) -> Result<(), Box<dyn std::error::Error>> {
    // Delete the trigger phrase + space/enter
    send_backspaces_fast(trigger_len + 1)?;
    
    // Small delay to ensure deletion is processed
    thread::sleep(Duration::from_millis(10));
    
    // Send the replacement text directly
    send_text_via_unicode(&text)?;
    
    Ok(())
}

pub fn force_clipboard_update() {
    unsafe {
        // Open and immediately close clipboard to force update
        if OpenClipboard(null_mut()) != 0 {
            CloseClipboard();
        }
    }
}