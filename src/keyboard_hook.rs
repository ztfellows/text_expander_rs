// src/keyboard_hook.rs
//
// Custom lightweight WH_KEYBOARD_LL + WH_MOUSE_LL hooks.
// Replaces rdev to avoid heavyweight hook callbacks that interfere with SendInput.

use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use std::{mem, ptr};

use winapi::shared::minwindef::{LPARAM, LRESULT, WPARAM};
use winapi::shared::windef::HHOOK;
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::um::winuser::{
    CallNextHookEx, DispatchMessageW, GetAsyncKeyState, GetKeyState, GetMessageW,
    SetWindowsHookExW, ToUnicode, TranslateMessage, UnhookWindowsHookEx, HC_ACTION,
    KBDLLHOOKSTRUCT, MSG, VK_CAPITAL, VK_CONTROL, VK_MENU, VK_SHIFT, WH_KEYBOARD_LL,
    WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_SYSKEYDOWN,
};

use crate::windows_input::SYNTHETIC_INPUT_TAG;
use crate::GLOBAL_LISTENING;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyId {
    Space,
    Return,
    Backspace,
    Tab,
    Escape,
    Delete,
    LeftArrow,
    RightArrow,
    UpArrow,
    DownArrow,
    Home,
    End,
    PageUp,
    PageDown,
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Minus,
    Equal,
    LeftBracket,
    RightBracket,
    Quote,
    Comma,
    Dot,
    Slash,
    SemiColon,
    BackSlash,
    BackQuote,
    Unknown(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug)]
pub enum HookMessage {
    KeyDown {
        key: KeyId,
        vk_code: u32,
        scan_code: u32,
    },
    MouseDown(MouseButton),
}

// ---------------------------------------------------------------------------
// VK → KeyId mapping
// ---------------------------------------------------------------------------

fn vk_to_key_id(vk: u32) -> KeyId {
    match vk {
        0x20 => KeyId::Space,
        0x0D => KeyId::Return,
        0x08 => KeyId::Backspace,
        0x09 => KeyId::Tab,
        0x1B => KeyId::Escape,
        0x2E => KeyId::Delete,
        0x25 => KeyId::LeftArrow,
        0x27 => KeyId::RightArrow,
        0x26 => KeyId::UpArrow,
        0x28 => KeyId::DownArrow,
        0x24 => KeyId::Home,
        0x23 => KeyId::End,
        0x21 => KeyId::PageUp,
        0x22 => KeyId::PageDown,
        0x41 => KeyId::KeyA,
        0x42 => KeyId::KeyB,
        0x43 => KeyId::KeyC,
        0x44 => KeyId::KeyD,
        0x45 => KeyId::KeyE,
        0x46 => KeyId::KeyF,
        0x47 => KeyId::KeyG,
        0x48 => KeyId::KeyH,
        0x49 => KeyId::KeyI,
        0x4A => KeyId::KeyJ,
        0x4B => KeyId::KeyK,
        0x4C => KeyId::KeyL,
        0x4D => KeyId::KeyM,
        0x4E => KeyId::KeyN,
        0x4F => KeyId::KeyO,
        0x50 => KeyId::KeyP,
        0x51 => KeyId::KeyQ,
        0x52 => KeyId::KeyR,
        0x53 => KeyId::KeyS,
        0x54 => KeyId::KeyT,
        0x55 => KeyId::KeyU,
        0x56 => KeyId::KeyV,
        0x57 => KeyId::KeyW,
        0x58 => KeyId::KeyX,
        0x59 => KeyId::KeyY,
        0x5A => KeyId::KeyZ,
        0x30 => KeyId::Num0,
        0x31 => KeyId::Num1,
        0x32 => KeyId::Num2,
        0x33 => KeyId::Num3,
        0x34 => KeyId::Num4,
        0x35 => KeyId::Num5,
        0x36 => KeyId::Num6,
        0x37 => KeyId::Num7,
        0x38 => KeyId::Num8,
        0x39 => KeyId::Num9,
        0xBD => KeyId::Minus,       // VK_OEM_MINUS
        0xBB => KeyId::Equal,       // VK_OEM_PLUS (=/+ key)
        0xDB => KeyId::LeftBracket, // VK_OEM_4
        0xDD => KeyId::RightBracket,// VK_OEM_6
        0xDE => KeyId::Quote,       // VK_OEM_7
        0xBC => KeyId::Comma,       // VK_OEM_COMMA
        0xBE => KeyId::Dot,         // VK_OEM_PERIOD
        0xBF => KeyId::Slash,       // VK_OEM_2
        0xBA => KeyId::SemiColon,   // VK_OEM_1
        0xDC => KeyId::BackSlash,   // VK_OEM_5
        0xC0 => KeyId::BackQuote,   // VK_OEM_3
        other => KeyId::Unknown(other),
    }
}

// ---------------------------------------------------------------------------
// Character resolution (called on processing thread, NOT in hook callback)
// ---------------------------------------------------------------------------

pub fn resolve_character(vk_code: u32, scan_code: u32) -> Option<String> {
    unsafe {
        // If Ctrl or Alt are held, skip — these are control-key combos, not printable
        if GetAsyncKeyState(VK_CONTROL) < 0 || GetAsyncKeyState(VK_MENU) < 0 {
            return None;
        }

        // Build keyboard state manually
        let mut keyboard_state = [0u8; 256];

        // Shift
        if GetAsyncKeyState(VK_SHIFT) < 0 {
            keyboard_state[VK_SHIFT as usize] = 0x80;
        }

        // Caps Lock (toggle state)
        if GetKeyState(VK_CAPITAL) & 0x01 != 0 {
            keyboard_state[VK_CAPITAL as usize] = 0x01;
        }

        let mut buf = [0u16; 4];
        let result = ToUnicode(
            vk_code,
            scan_code,
            keyboard_state.as_ptr(),
            buf.as_mut_ptr(),
            buf.len() as i32,
            0,
        );

        if result == 1 {
            String::from_utf16(&buf[..1]).ok()
        } else if result > 1 {
            // Multi-char output (rare)
            String::from_utf16(&buf[..result as usize]).ok()
        } else {
            // result <= 0: dead key or no translation
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Static sender for hook callbacks
// ---------------------------------------------------------------------------

static HOOK_SENDER: OnceLock<Sender<HookMessage>> = OnceLock::new();

// ---------------------------------------------------------------------------
// Hook callbacks
// ---------------------------------------------------------------------------

unsafe extern "system" fn keyboard_hook_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code == HC_ACTION as i32 {
        let kb = unsafe { &*(l_param as *const KBDLLHOOKSTRUCT) };

        // Always let our own synthetic events through to the target app
        if kb.dwExtraInfo == SYNTHETIC_INPUT_TAG {
            return unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) };
        }

        let msg_type = w_param as u32;

        // When not listening (expansion in progress), block non-synthetic
        // keydown events from reaching the target app.
        if !GLOBAL_LISTENING.load(Ordering::SeqCst) {
            if msg_type == WM_KEYDOWN as u32 || msg_type == WM_SYSKEYDOWN as u32 {
                return 1;
            }
        }

        if msg_type == WM_KEYDOWN as u32 || msg_type == WM_SYSKEYDOWN as u32 {
            if let Some(sender) = HOOK_SENDER.get() {
                let key = vk_to_key_id(kb.vkCode);
                let _ = sender.send(HookMessage::KeyDown {
                    key,
                    vk_code: kb.vkCode,
                    scan_code: kb.scanCode,
                });

                // Swallow Space and Enter so they never reach the target app.
                // The processing thread will re-inject them if no expansion
                // occurs. This prevents the WM_CHAR ordering problem where
                // Notepad++/Scintilla processes the character AFTER our
                // backspaces (TranslateMessage posts WM_CHAR to the end
                // of the queue, behind already-queued backspace events).
                if key == KeyId::Space || key == KeyId::Return {
                    return 1;
                }
            }
        }
    }

    unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) }
}

unsafe extern "system" fn mouse_hook_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code == HC_ACTION as i32 && GLOBAL_LISTENING.load(Ordering::SeqCst) {
        let button = match w_param as u32 {
            WM_LBUTTONDOWN => Some(MouseButton::Left),
            WM_RBUTTONDOWN => Some(MouseButton::Right),
            WM_MBUTTONDOWN => Some(MouseButton::Middle),
            _ => None,
        };

        if let Some(btn) = button {
            if let Some(sender) = HOOK_SENDER.get() {
                let _ = sender.send(HookMessage::MouseDown(btn));
            }
        }
    }

    unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) }
}

// ---------------------------------------------------------------------------
// Hook installation + message pump
// ---------------------------------------------------------------------------

pub fn install_hooks_and_run(sender: Sender<HookMessage>) -> Result<(), Box<dyn std::error::Error>> {
    HOOK_SENDER
        .set(sender)
        .map_err(|_| "HOOK_SENDER already initialized")?;

    unsafe {
        let h_instance = GetModuleHandleW(ptr::null());

        let kb_hook: HHOOK =
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), h_instance, 0);
        if kb_hook.is_null() {
            return Err("Failed to install keyboard hook".into());
        }

        let mouse_hook: HHOOK =
            SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), h_instance, 0);
        if mouse_hook.is_null() {
            UnhookWindowsHookEx(kb_hook);
            return Err("Failed to install mouse hook".into());
        }

        println!("Hooks installed. Listening...");

        // Standard Windows message pump — required for low-level hooks to work
        let mut msg: MSG = mem::zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        UnhookWindowsHookEx(kb_hook);
        UnhookWindowsHookEx(mouse_hook);
    }

    Ok(())
}
