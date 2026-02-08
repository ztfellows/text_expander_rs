// src/windows_input.rs
use winapi::um::winuser::{
    SendInput, INPUT, INPUT_KEYBOARD,
    KEYEVENTF_KEYUP, VK_BACK, VK_CONTROL, VK_SHIFT,
    VK_END, VK_DELETE,
    OpenClipboard, CloseClipboard,
};
use winapi::shared::minwindef::WORD;
use std::mem;
use std::thread;
use std::time::Duration;
use std::ptr::null_mut;

/// Delay in milliseconds between each backspace key down+up pair.
/// Increase if target apps (e.g. EHR software) drop keystrokes.
pub const BACKSPACE_DELAY_MS: u64 = 5;

/// Tag placed in dwExtraInfo to identify our synthetic events.
/// Allows the rdev hook to distinguish self-generated input.
pub const SYNTHETIC_INPUT_TAG: usize = 0x5445_5854; // "TEXT" in hex

/// Send `count` backspaces as individual key down+up pairs with delays.
/// Each event includes the hardware scan code (0x0E) and dwExtraInfo tag.
pub fn send_backspaces_fast(count: usize) -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..count {
        // Key down
        let mut key_down: INPUT = unsafe { mem::zeroed() };
        unsafe {
            key_down.type_ = INPUT_KEYBOARD;
            let ki = key_down.u.ki_mut();
            ki.wVk = VK_BACK as WORD;
            ki.wScan = 0x0E; // hardware scan code for Backspace
            ki.dwFlags = 0;
            ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;
        }

        let sent = unsafe { SendInput(1, &mut key_down, mem::size_of::<INPUT>() as i32) };
        if sent != 1 {
            return Err("Failed to send backspace key down".into());
        }

        // Key up
        let mut key_up: INPUT = unsafe { mem::zeroed() };
        unsafe {
            key_up.type_ = INPUT_KEYBOARD;
            let ki = key_up.u.ki_mut();
            ki.wVk = VK_BACK as WORD;
            ki.wScan = 0x0E;
            ki.dwFlags = KEYEVENTF_KEYUP;
            ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;
        }

        let sent = unsafe { SendInput(1, &mut key_up, mem::size_of::<INPUT>() as i32) };
        if sent != 1 {
            return Err("Failed to send backspace key up".into());
        }

        thread::sleep(Duration::from_millis(BACKSPACE_DELAY_MS));
    }

    Ok(())
}

/// Send Ctrl+V as a single batched SendInput call (atomic modifier chord).
/// Includes hardware scan codes and dwExtraInfo tag.
pub fn send_ctrl_v() -> Result<(), Box<dyn std::error::Error>> {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(4);

    // Ctrl down
    let mut ctrl_down: INPUT = unsafe { mem::zeroed() };
    unsafe {
        ctrl_down.type_ = INPUT_KEYBOARD;
        let ki = ctrl_down.u.ki_mut();
        ki.wVk = VK_CONTROL as WORD;
        ki.wScan = 0x1D; // scan code for Ctrl
        ki.dwFlags = 0;
        ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;
    }
    inputs.push(ctrl_down);

    // V down
    let mut v_down: INPUT = unsafe { mem::zeroed() };
    unsafe {
        v_down.type_ = INPUT_KEYBOARD;
        let ki = v_down.u.ki_mut();
        ki.wVk = 'V' as WORD;
        ki.wScan = 0x2F; // scan code for V
        ki.dwFlags = 0;
        ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;
    }
    inputs.push(v_down);

    // V up
    let mut v_up: INPUT = unsafe { mem::zeroed() };
    unsafe {
        v_up.type_ = INPUT_KEYBOARD;
        let ki = v_up.u.ki_mut();
        ki.wVk = 'V' as WORD;
        ki.wScan = 0x2F;
        ki.dwFlags = KEYEVENTF_KEYUP;
        ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;
    }
    inputs.push(v_up);

    // Ctrl up
    let mut ctrl_up: INPUT = unsafe { mem::zeroed() };
    unsafe {
        ctrl_up.type_ = INPUT_KEYBOARD;
        let ki = ctrl_up.u.ki_mut();
        ki.wVk = VK_CONTROL as WORD;
        ki.wScan = 0x1D;
        ki.dwFlags = KEYEVENTF_KEYUP;
        ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;
    }
    inputs.push(ctrl_up);

    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            mem::size_of::<INPUT>() as i32,
        )
    };

    if sent != inputs.len() as u32 {
        return Err(format!("Failed to send Ctrl+V. Sent: {}/{}", sent, inputs.len()).into());
    }

    Ok(())
}

/// Send Shift+End to select from cursor to end of line.
pub fn send_shift_end() -> Result<(), Box<dyn std::error::Error>> {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(4);

    // Shift down
    let mut shift_down: INPUT = unsafe { mem::zeroed() };
    unsafe {
        shift_down.type_ = INPUT_KEYBOARD;
        let ki = shift_down.u.ki_mut();
        ki.wVk = VK_SHIFT as WORD;
        ki.wScan = 0x2A; // scan code for Left Shift
        ki.dwFlags = 0;
        ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;
    }
    inputs.push(shift_down);

    // End down
    let mut end_down: INPUT = unsafe { mem::zeroed() };
    unsafe {
        end_down.type_ = INPUT_KEYBOARD;
        let ki = end_down.u.ki_mut();
        ki.wVk = VK_END as WORD;
        ki.wScan = 0x4F; // scan code for End
        ki.dwFlags = 0;
        ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;
    }
    inputs.push(end_down);

    // End up
    let mut end_up: INPUT = unsafe { mem::zeroed() };
    unsafe {
        end_up.type_ = INPUT_KEYBOARD;
        let ki = end_up.u.ki_mut();
        ki.wVk = VK_END as WORD;
        ki.wScan = 0x4F;
        ki.dwFlags = KEYEVENTF_KEYUP;
        ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;
    }
    inputs.push(end_up);

    // Shift up
    let mut shift_up: INPUT = unsafe { mem::zeroed() };
    unsafe {
        shift_up.type_ = INPUT_KEYBOARD;
        let ki = shift_up.u.ki_mut();
        ki.wVk = VK_SHIFT as WORD;
        ki.wScan = 0x2A;
        ki.dwFlags = KEYEVENTF_KEYUP;
        ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;
    }
    inputs.push(shift_up);

    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            mem::size_of::<INPUT>() as i32,
        )
    };

    if sent != inputs.len() as u32 {
        return Err(format!("Failed to send Shift+End. Sent: {}/{}", sent, inputs.len()).into());
    }

    Ok(())
}

/// Send a single Delete key press+release.
pub fn send_delete_key() -> Result<(), Box<dyn std::error::Error>> {
    // Key down
    let mut key_down: INPUT = unsafe { mem::zeroed() };
    unsafe {
        key_down.type_ = INPUT_KEYBOARD;
        let ki = key_down.u.ki_mut();
        ki.wVk = VK_DELETE as WORD;
        ki.wScan = 0x53; // scan code for Delete
        ki.dwFlags = 0;
        ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;
    }

    let sent = unsafe { SendInput(1, &mut key_down, mem::size_of::<INPUT>() as i32) };
    if sent != 1 {
        return Err("Failed to send Delete key down".into());
    }

    // Key up
    let mut key_up: INPUT = unsafe { mem::zeroed() };
    unsafe {
        key_up.type_ = INPUT_KEYBOARD;
        let ki = key_up.u.ki_mut();
        ki.wVk = VK_DELETE as WORD;
        ki.wScan = 0x53;
        ki.dwFlags = KEYEVENTF_KEYUP;
        ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;
    }

    let sent = unsafe { SendInput(1, &mut key_up, mem::size_of::<INPUT>() as i32) };
    if sent != 1 {
        return Err("Failed to send Delete key up".into());
    }

    Ok(())
}

/// Re-inject a key tap (down+up) that was swallowed by the hook.
/// Tagged with SYNTHETIC_INPUT_TAG so the hook passes it through.
pub fn send_key_tap(vk: u16, scan: u16) -> Result<(), Box<dyn std::error::Error>> {
    let mut inputs: [INPUT; 2] = unsafe { mem::zeroed() };

    unsafe {
        // Key down
        inputs[0].type_ = INPUT_KEYBOARD;
        let ki = inputs[0].u.ki_mut();
        ki.wVk = vk;
        ki.wScan = scan;
        ki.dwFlags = 0;
        ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;

        // Key up
        inputs[1].type_ = INPUT_KEYBOARD;
        let ki = inputs[1].u.ki_mut();
        ki.wVk = vk;
        ki.wScan = scan;
        ki.dwFlags = KEYEVENTF_KEYUP;
        ki.dwExtraInfo = SYNTHETIC_INPUT_TAG;
    }

    let sent = unsafe { SendInput(2, inputs.as_mut_ptr(), mem::size_of::<INPUT>() as i32) };
    if sent != 2 {
        return Err("Failed to re-inject key tap".into());
    }
    Ok(())
}

/// Open and close clipboard to force Windows to process pending updates.
pub fn force_clipboard_update() {
    unsafe {
        if OpenClipboard(null_mut()) != 0 {
            CloseClipboard();
        }
    }
}
