# Text Expander — Project Memory

## Purpose

Standalone Windows text expander for a rheumatology practice. Ported from Module 2 of a larger Python (PySide6/Qt6) application (`SPEC.md` describes the full original). The Python version works but occasionally crashes due to other modules in the monolith. This Rust port isolates text expansion into its own lightweight, efficient process — important because the laptop runs on battery during patient visits all day.

A separate Rust project will eventually port the remaining Python modules (automation, dialogs, dictation, etc.), but that is **not** part of this repo.

## Environment

- **Development**: WSL2 on Windows 11 laptop
- **Build**: Windows-side Rust toolchain (`cargo.exe` / `rustc.exe` 1.85.1) — required because the crate depends on `winapi` and links Windows system libraries
- **Deploy**: Windows 11 only
- **Rust edition**: 2024 (requires Rust 1.85+)

## Architecture Summary

### Event Flow
```
WH_KEYBOARD_LL + WH_MOUSE_LL hooks [main thread, src/keyboard_hook.rs]
  → mpsc::channel → HookMessage (KeyDown { key, vk_code, scan_code } | MouseDown)
    → processing thread [holds Arc<Mutex<ExpansionData>>]
      → keyboard_hook::resolve_character(vk_code, scan_code) → Option<String>
      → handle_key_press() / handle_mouse_press()
```

### Hook Design — Lightweight by Design
- Hook callbacks are **minimal**: check dwExtraInfo tag → check GLOBAL_LISTENING → send to channel → CallNextHookEx. No `ToUnicode`/`GetKeyboardState` in hook.
- Character resolution (`resolve_character()`) happens on the **processing thread** using `ToUnicode` + `GetAsyncKeyState`. Safe there — doesn't affect hook thread keyboard state.
- `install_hooks_and_run()` blocks main thread with `GetMessageW` message pump.
- Replaced `rdev` (which called `AttachThreadInput` + `GetKeyboardState` + `ToUnicodeEx` in hook callback for every keystroke, interfering with SendInput).

### Separator Swallowing
Space and Enter are **swallowed by the hook** (return 1, not passed to target app) and sent to the channel only. The processing thread decides:
- **Expansion match**: backspaces delete only the trigger chars (no +1 for separator since it never reached the app)
- **No match**: re-injects the key via `send_key_tap()` with SYNTHETIC_INPUT_TAG so it passes through the hook

**Why**: Scintilla (Notepad++) inserts printable characters on `WM_CHAR`, not `WM_KEYDOWN`. `TranslateMessage` in Notepad++'s message pump posts `WM_CHAR` to the END of the queue — behind our already-queued backspace `WM_KEYDOWN` events. So backspaces would fire before the space character existed in the document, wasting one backspace on preceding text, then the space would appear via the late `WM_CHAR`. Swallowing eliminates this entirely.

### State Machine (TypingState)
```
Empty  →(printable char)→  Typing  →(Space/Enter, no match)→  NoMatch
  ↑                           |                                    |
  |                           ↓ (match found → expand)             |
  +←←←←←←←←←←(reset)←←←←←←←+←←←←←←(next char or reset)←←←←←←←+
```

- **Empty**: no active buffer. Printable char transitions to Typing.
- **Typing**: buffer accumulating. Space/Enter checks for match → expand or transition to NoMatch.
- **NoMatch**: a space was typed after no match. Next printable char resets to Empty then Typing. Backspace returns to Typing (allows correction).

### Expansion Mechanism
1. Disable `GLOBAL_LISTENING` (AtomicBool) — hook blocks non-synthetic keydowns during expansion
2. Send backspaces individually (30ms delay each, with scan codes + dwExtraInfo tag) — trigger length only, separator was swallowed
3. 30ms delay for target app to process deletions
4. Save clipboard, set expansion text to clipboard
5. 10ms delay for clipboard to settle
6. `SendInput` Ctrl+V to paste (batched, atomic chord)
7. 50ms delay for paste to complete
8. Restore original clipboard
9. Drain pending events from mpsc channel (discard any leaked events)
10. Re-enable listening

### Special Triggers (hardcoded in main.rs)
| Trigger | Action |
|---------|--------|
| `ff` | Delete to end of line (Shift+End, Space) |
| `nn` | Insert current date as `M/D/YY: ` |
| `/wksN` | Insert date N weeks from now (e.g., `/wks4` → `3/15/25`) |
| `/daysN` | Insert date N days from now |

### File Layout
| File | Role |
|------|------|
| `src/main.rs` | Entry point, processing thread, state machine, expansion logic |
| `src/keyboard_hook.rs` | Custom WH_KEYBOARD_LL + WH_MOUSE_LL hooks, KeyId/HookMessage types, resolve_character(), separator swallowing |
| `src/windows_input.rs` | Win32 `SendInput` wrappers: backspaces, Ctrl+V, key re-injection, clipboard |
| `expansions.toml` | Trigger definitions: `[case_sensitive]` and `[case_insensitive]` sections |
| `Cargo.toml` | Dependencies, edition 2024 |
| `SPEC.md` | Full spec of the original Python app (reference only) |
| `snippet.rs` | Scratch/prototype code (unused) |

### Dependencies
| Crate | Purpose |
|-------|---------|
| `winapi` | `SendInput`, `SetWindowsHookExW`, clipboard, keyboard state Win32 APIs |
| `arboard` | Clipboard get/set (used alongside direct WinAPI) |
| `serde` + `toml` | Deserialize `expansions.toml` |
| `chrono` | Date formatting for `nn`/`/wks`/`/days` |

## Resolved Issues

### Issue 1 — thread::spawn race condition (fixed session 2)
Expansion was called via `thread::spawn`, causing unpredictable timing. Fixed by making it synchronous.

### Issue 2 — rdev heavyweight hook callback (fixed session 3)
`rdev::listen()` called `AttachThreadInput` + `GetKeyboardState` + `ToUnicodeEx` for every keystroke including synthetic backspaces, interfering with SendInput delivery. Fixed by replacing rdev with custom lightweight WH_KEYBOARD_LL/WH_MOUSE_LL hooks.

### Issue 3 — WM_CHAR ordering / separator ghost (fixed session 3)
**Symptom**: Second consecutive expansion in Notepad++ always left the first char(s) of the trigger + a space. e.g. `mtx`→expand→`nn`→expand produced "methotrexaten 2/6/26: " instead of "methotrexate2/6/26: ". 100% reproducible.

**Root cause**: Scintilla (Notepad++) inserts printable characters on `WM_CHAR`, not `WM_KEYDOWN`. When user presses Space to trigger expansion, `TranslateMessage` in Notepad++'s message pump posts `WM_CHAR` to the **end** of the queue — behind our already-queued backspace `WM_KEYDOWN` events. So backspaces fired before the space character existed in the document, wasting one backspace on preceding text. The space then appeared via the late `WM_CHAR`.

**Fix**: Hook swallows Space/Enter (returns 1, never reaches target app). Processing thread re-injects via `send_key_tap()` if no expansion occurs. Backspace count changed from `length + 1` to `length`.

## Known Issues & Technical Debt

- ~~**Duplicate function**: `force_clipboard_update()` defined in both `main.rs` and `windows_input.rs`.~~ **FIXED** session 1.
- ~~**Hardcoded expansion file path**~~ **FIXED** session 2 — now uses `env::current_exe().parent()`.
- ~~**`case_insensitive` lookup doesn't lowercase the buffer**~~ **FIXED** session 2 — `check_for_completion()` now lowercases before lookup.
- ~~**`ff` trigger doesn't reset state**~~ **FIXED** session 2 — rewritten with `send_backspaces_fast` + `send_shift_end` + `send_delete_key`, proper reset/return, wrapped in listening toggle.
- ~~**Unused code warnings**~~ **FIXED** session 2 — removed all dead functions and unused imports from both files.
- **`snippet.rs`** at project root is unused scratch code.
- **No tests** — the Python version had a `TextExpanderTestResult` dataclass for testing the state machine; no equivalent exists in the Rust port yet.

## Iteration Log

### Session 1 — 2026-02-07
- Initial codebase review and CLAUDE.md creation
- Created `reference/memory.md` (this file)
- Documented architecture, state machine, known issues
- Set up development environment notes for WSL → Windows build workflow
- Fixed compile errors: removed duplicate `force_clipboard_update()`, fixed `GlobalAlloc`/`GlobalLock`/`GlobalUnlock` imports (moved from `memoryapi` to `winbase`), added missing `use std::ptr::null_mut` in main.rs
- Confirmed release build succeeds via `powershell.exe` from WSL
- Identified `thread::spawn` race condition as primary cause of unreliable backspace behavior
- Catalogued 4 bugs: thread::spawn race, case_insensitive not lowering, ff no reset, hardcoded path

### Session 2 — 2026-02-07
- **Rewrote `send_backspaces_fast`**: individual key down+up pairs with 5ms delays (tunable `BACKSPACE_DELAY_MS`), hardware scan codes (0x0E), dwExtraInfo tag (0x5445_5854) to identify synthetic events
- **Added scan codes to `send_ctrl_v`**: Ctrl=0x1D, V=0x2F, plus dwExtraInfo tag
- **Added `send_shift_end()` and `send_delete_key()`** helpers for `ff` trigger
- **Removed dead code from windows_input.rs**: `get_clipboard_sequence`, `set_clipboard_text_winapi`, `send_text_via_unicode`, `expand_text_directly`, and many unused imports
- **Reordered `expand_trigger_phrase`**: backspaces first (clean focus), then clipboard ops, then paste — prevents clipboard timing issues
- **Added self-event filtering**: GLOBAL_LISTENING check in rdev callback skips sending to channel; `drain_pending_events()` flushes any that leaked through
- **Changed processing thread** from `for message in receiver` to explicit `loop { receiver.recv() }` to pass `&Receiver` through for draining
- **Made main expansion synchronous**: removed `thread::spawn`, drop mutex before calling expand (like nn/wks already did)
- **Fixed `check_for_completion`**: added `buffer.to_lowercase()` for case_insensitive lookup
- **Fixed `ff` trigger**: replaced `rdev::simulate` with SendInput helpers, added reset+return, wrapped in disable/enable listening
- **Fixed `load_expansion_table`**: uses `env::current_exe().parent()` instead of hardcoded path
- **Cleaned up all unused imports** in main.rs (sleep, expand_text_directly, null_mut, PathBuf, fs, MutexGuard, etc.)
- **Removed `delete_characters`** function (was using unreliable rdev::simulate)
- Both debug and release builds compile with zero warnings

### Session 3 — 2026-02-07
- **Replaced rdev with custom keyboard hooks** (`src/keyboard_hook.rs`):
  - `KeyId` enum mirrors old `rdev::Key` variant names for minimal churn
  - `HookMessage` enum: `KeyDown { key, vk_code, scan_code }` | `MouseDown(MouseButton)`
  - `vk_to_key_id()` maps VK codes to KeyId
  - `resolve_character()` on processing thread: `ToUnicode` + `GetAsyncKeyState` for shift/caps
  - Minimal hook callbacks: check dwExtraInfo → check GLOBAL_LISTENING → send to channel → CallNextHookEx
  - `install_hooks_and_run()` blocks main thread with GetMessageW message pump
- **Removed rdev dependency** from Cargo.toml, added `libloaderapi` to winapi features
- **Updated main.rs**: `KeyEventMessage` → `HookMessage`, `Key::` → `KeyId::`, `Button` → `MouseButton`
- **Diagnosed WM_CHAR ordering bug**: Space/Enter `WM_CHAR` posted by `TranslateMessage` to end of queue, behind our backspace `WM_KEYDOWN`s — caused separator to appear AFTER backspaces in Scintilla
- **Implemented separator swallowing**: hook returns 1 for Space/Enter (never reaches target app), processing thread re-injects via `send_key_tap()` if no expansion match
- **Added `send_key_tap()`** to windows_input.rs for re-injecting swallowed keys
- **Hook blocks non-synthetic keydowns** when GLOBAL_LISTENING is false (prevents screen/buffer desync)
- **Backspace count**: `length` instead of `length + 1` (separator swallowed, not in target app)
- **BACKSPACE_DELAY_MS**: tuned to 30ms (5ms too unreliable across apps)
- Both debug and release builds compile with zero warnings
- Tested reliable in both VSCode and Notepad++
