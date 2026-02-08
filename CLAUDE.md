# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Standalone Windows text expander for a rheumatology practice, written in Rust. Hooks global keyboard events, tracks a typing buffer, and when a trigger phrase is followed by a separator key (Space/Enter), deletes the trigger and pastes the expansion via clipboard. This is a focused port of Module 2 from a larger Python app (`SPEC.md` has the full original spec for reference).

**Scope**: Text expansion only. Other modules from the Python app (automation, dialogs, dictation) will be a separate project later.

## Project Memory

Detailed architecture notes, known issues, and an iteration log are maintained in `reference/memory.md`. Consult that file for full context on design decisions and session history.

## Build & Run

Development happens in WSL2 but the binary targets Windows 11. **Builds must use the Windows-side Rust toolchain** because the crate links `winapi` Windows system libraries.

```powershell
# From Windows terminal or via powershell.exe from WSL, in C:\Projects\text_expander
cargo build              # Debug build (debug_println! output enabled)
cargo build --release    # Release build (debug_println! stripped, use for deployment)
cargo run                # Run debug
cargo run --release      # Run release
```

**Build order**: Always build debug first, then release. Debug compiles faster, so errors surface sooner.

From WSL, you can invoke the Windows toolchain:
```bash
powershell.exe -Command "cd C:\Projects\text_expander; cargo build"            # Debug first
powershell.exe -Command "cd C:\Projects\text_expander; cargo build --release"  # Then release
```

No tests exist yet. The project targets Windows 11 only.

## Architecture

### Event Flow
`rdev::listen()` on the main thread captures keyboard/mouse events → sends `KeyEventMessage` over `mpsc::channel` → processing thread (owns `Arc<Mutex<ExpansionData>>`) runs the state machine.

### State Machine (TypingState)
- **Empty** → printable char → **Typing** (buffer accumulates)
- **Typing** → Space/Enter → check match → expand if found, else **NoMatch**
- **NoMatch** → next printable resets to Empty; Backspace returns to Typing
- Mouse click / nav keys (Up, Down, Esc, Tab, PgUp, PgDn, Home, End) → reset to **Empty**

### Expansion Path
1. Disable `GLOBAL_LISTENING` AtomicBool (prevent hook from seeing own simulated keys)
2. Save clipboard → set expansion text → `SendInput` backspaces → 100ms delay → `SendInput` Ctrl+V → restore clipboard
3. Re-enable listening

### Key Files
| File | Role |
|------|------|
| `src/main.rs` | Entry point, event loop, state machine, expansion logic, special triggers |
| `src/windows_input.rs` | Win32 `SendInput` wrappers: backspaces, Ctrl+V, Unicode text, clipboard |
| `expansions.toml` | Trigger definitions: `[case_sensitive]` and `[case_insensitive]` sections |
| `SPEC.md` | Original Python app spec (reference only — only Module 2 applies here) |

### Special Triggers (hardcoded in main.rs)
| Trigger | Action |
|---------|--------|
| `ff` | Delete to end of line (Shift+End, Space) |
| `nn` | Insert current date as `M/D/YY: ` |
| `/wksN` | Date N weeks from now |
| `/daysN` | Date N days from now |
| `/moN` | Date N months from now |

## Important Patterns

- **Listening toggle**: `GLOBAL_LISTENING` must be disabled during simulated input and re-enabled after. Forgetting this causes infinite loops.
- **Clipboard round-trip**: Saves/restores user's clipboard around each expansion. The 100ms delay between backspace and paste is necessary for target apps to process.
- **`SendInput` over `rdev::simulate`**: The `winapi` `SendInput` path was chosen because `rdev::simulate` had reliability problems on Windows.
- **Expansion file path**: Currently hardcoded to `C:\Projects\text_expander\expansions.toml` in `load_expansion_table()`.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `rdev` | Global keyboard/mouse event hooking |
| `winapi` | `SendInput`, clipboard Win32 APIs |
| `arboard` | Clipboard get/set (used alongside direct WinAPI) |
| `serde` + `toml` | Deserialize `expansions.toml` |
| `chrono` | Date formatting for date triggers |
