# MacroEngine NextGen - Complete Specification

## Overview

MacroEngine NextGen is a Windows desktop overlay application built in Python (PySide6/Qt6) for a rheumatology private practice. It presents as a tiny (58x20px) frameless always-on-top menu in the top-left corner of the screen. From this menu and via global hotkeys, the user can:

- Expand medical abbreviations into full text (text expansion engine)
- Generate structured ultrasound diagnostic notes via a modal dialog
- Generate structured joint injection procedure notes via a modal dialog
- Track joint exam findings and calculate CDAI scores via a visual homunculus
- Generate weight management plans and patient education materials
- Automate EHR (Electronic Health Record) workflows using screen image matching + keyboard/mouse simulation
- Dictate clinical notes via microphone with LLM-assisted transcription
- Open quick-reference tools (BMI calculator, TEE calculator)

The application starts via `run.bat` → `uv run ui.py`.

---

## Module 1: Application Shell & Menu System

### Files
- `ui.py` (~800 lines) - Main entry point, menu, hotkeys, toast system, orchestration
- `run.bat` - Launch script (`uv run ui.py`)

### Startup Sequence
1. Create `PySide6.QApplication`
2. Initialize `TextExpander` (starts global keyboard hook)
3. Initialize `DictationClient` (loads audio/LLM adapters, loads API keys from `llm.env`)
4. Create `zautogui.Interface` (automation engine: image cache, screenshot system)
5. Create `FramelessWindow` - a 58x20px frameless, always-on-top widget at screen position (0,0)
6. Build the dropdown menu with all submenus
7. Register global hotkeys (F3, F4, F6, F7, F8, F10, F12, Alt+1, Alt+2, Alt+Q, Alt+R, Alt+T, Alt+P)
8. Post toast notification "UI Loaded!"
9. Enter Qt event loop

### Menu Structure
```
MACRO (visible label on overlay bar)
├── Close Visit
│   ├── Close visit (99214) [default]
│   ├── Close visit (99205)
│   ├── Close visit (99215)
│   ├── Close visit (99204)
│   ├── Close visit (99214-95) [telehealth modifier]
│   └── Phone visit variants
├── Set LLM Prompts
│   ├── [Multiple medical transcription prompt presets]
│   └── Toggle LLM: Raw dictation only
├── Set Adapters
│   ├── Audio Adapter → Local | Groq (default) | Deepgram Nova-3
│   └── LLM Adapter   → Local | Hugging Face (default) | Groq
├── Set Scale Ratio → 1.0 | 0.75 | 0.66 | 0.5 | 0.4
├── Quick Links
│   ├── TEE Calculator (web URL)
│   └── BMI Calculator (web URL)
├── Unload Ollama model from memory
├── Open Ultrasound Dialog       (also F7)
├── Open Joint Injection Dialog
├── Open Homunculus Dialog        (also F8)
├── Open Weight Management Dialog
└── Exit
```

### Toast Notification System
- `ToastNotification(QLabel)` - a styled label widget (black background, white text, semi-transparent)
- `ToastManager(QThread)` - background thread that monitors a global `queue.Queue()`
- `post_toast(message)` - enqueue a message string
- Toasts appear at bottom-right corner of screen, persist for ~3 seconds, then fade/hide
- Used throughout the app to confirm actions (e.g. "Note copied to clipboard", "UI Loaded!")

### Global Hotkeys
| Hotkey | Action | Description |
|--------|--------|-------------|
| Alt+1 | `click_1()` | Click EHR "Save" button via image search |
| Alt+2 | `click_2()` | Click EHR "Save & Add" button via image search |
| Alt+Q | `quest_hotkey()` | Automate Quest lab order entry |
| Alt+R | `rads_hotkey()` | Automate radiology order entry |
| Alt+T | `TTE_shortcut()` | Automate echocardiogram order (with CPT/diagnosis) |
| Alt+P | `PFT_shortcut()` | Automate pulmonary function test order |
| F3 | `referral_general()` | Automate general referral template in EHR |
| F4 | `referral_PT()` | Automate physical therapy referral in EHR |
| F6 | `close_visit()` | Run close-visit wizard (billing code + signoff) |
| F7 | `open_ultrasound()` | Open Ultrasound Dialog |
| F8 | `open_homunculus()` | Open Homunculus (joint exam tracker) |
| F10 | `forms_hotkey()` | Open patient forms folder |
| F12 | `dm.hotkey_toggle()` | Toggle dictation recording |

### Threading Model
- Dialog openings use `QMetaObject.invokeMethod(..., Qt.QueuedConnection)` to marshal from hotkey threads to the GUI thread
- Hotkeys registered via the `keyboard` library run on background threads
- Automation sequences use a `threading.Lock` to prevent overlapping execution

### Dependencies
- `PySide6` (Qt6) - GUI framework
- `keyboard` - global hotkey registration
- `queue` - thread-safe toast queue

---

## Module 2: Text Expansion Engine

### Files
- `TextExpander/__init__.py` (~370 lines) - Engine implementation
- `TextExpander/config.toml` (~183KB) - Expansion database (500+ entries)

### Configuration Format (TOML)
```toml
[case_sensitive]
"Rtx"  = "Rituximab"
"MTX"  = "Methotrexate"

[case_insensitive]
"mtx"  = "methotrexate"
"hcq"  = "hydroxychloroquine"
"dxus" = "LIMITED DIAGNOSTIC ULTRASOUND EXAM\nPerforming Physician: ..."
```

Two dictionaries: `case_sensitive` (exact match) and `case_insensitive` (lowercased comparison). Multi-line expansions use `\n` which gets converted to `\r\n` at expansion time.

### State Machine

**States:** `empty` → `typing` → `no match`

**Transitions:**
- **`empty`**: On printable character → move to `typing`, start building buffer
- **`typing`**: On printable character → append to buffer. On separator (space, enter, tab, period, comma, etc.) → check for match:
  - Match found → expand (delete trigger, paste expansion) → back to `empty`
  - No match → move to `no match`
- **`no match`**: On backspace → return to `typing` (allows correction). On other keys → `empty`

**Special behaviors:**
- Arrow keys (left/right) adjust a cursor position within the buffer (allows editing the trigger before pressing space)
- Navigation keys (up/down/esc/page up/page down/home/end) → reset to `empty`
- Mouse click → reset to `empty`

### Expansion Mechanism
1. Match found in config dictionary
2. Send N backspaces to delete the trigger characters (N = len(trigger) + 1 for the separator)
3. Copy expansion text to clipboard via `pyperclip.copy()`
4. Send Ctrl+V to paste
5. If the separator was a space, send a trailing space

### Special Expansions (Hardcoded)
| Trigger | Behavior |
|---------|----------|
| `ff` | Delete rest of line (Shift+End, then Backspace) |
| `nn` | Insert current date in `M/D/YY` format |
| `wks<N>` | Insert date N weeks from today (e.g., `wks4` → `3/15/25`) |

### Keyboard Hook
- Uses `keyboard.hook(self._event_handler)` to intercept every keystroke globally
- Mouse clicks detected via `mouse.on_click(self._on_click)`
- The hook receives `keyboard.KeyboardEvent` objects and processes key-down events

### Test Mode
`TextExpanderTestResult` dataclass allows unit testing without live keyboard:
```python
@dataclass
class TextExpanderTestResult:
    buffer: str
    cursor: int
    state: str
    expansion: Optional[str]
```

### Dependencies
- `keyboard` - global keyboard hook
- `mouse` - mouse click detection
- `pyperclip` - clipboard read/write
- `tomllib` (or `toml`) - TOML config parsing
- `datetime` - date calculations for `nn`/`wks` triggers

### Porting Notes
- The core is a pure state machine driven by keyboard events - maps well to Rust
- Clipboard interaction on Windows: use `clipboard-win` crate
- Keyboard hooking: use `winapi` `SetWindowsHookEx` with `WH_KEYBOARD_LL`
- The TOML config is straightforward to parse with the `toml` crate

---

## Module 3: Ultrasound Documentation Dialog

### Files
- `ultrasound.py` (~600 lines) - Dialog logic and note builder
- `ui_files/ultrasound.py` (~237 lines) - Auto-generated UI layout (from Qt Designer)

### Dialog Layout
A modal, always-on-top dialog with:
- **Joint input** - free-text field that parses joint abbreviations
- **Synovitis section** - 8 checkboxes (mutually exclusive "No synovitis" vs gradings)
  - No synovitis
  - Gray scale: mild, moderate, severe
  - Power Doppler: mild, moderate, severe
  - "Dry" synovitis (gray without power)
- **Bony changes section** - 5 checkboxes (mutually exclusive "Normal" vs findings)
  - Normal
  - Erosions
  - Osteophytosis
  - Chondrocalcinosis
  - Calcifications
- **Tendon section** - 3 checkboxes
  - Normal
  - Acute tenosynovitis (power doppler signal)
  - Chronic tenosynovitis (effusion)
- **Effusion section** - 4 radio buttons (mutually exclusive)
  - None, Small, Moderate, Large
- **Other findings** - Tophi checkbox
- **Bilateral checkbox** - duplicates all findings for the contralateral joint
- **Full note preview** - editable text area showing the composed note
- **OK/Cancel buttons**

### Joint Parsing
Input examples and their parsed results:
```
"R MCP2"     → "Right Hand MCP2 (Index)"
"L knee"     → "Left Knee"
"B wrist"    → "Right Wrist" AND "Left Wrist"
"R MTP1"     → "Right Foot MTP1 (Great Toe)"
"L PIP3"     → "Left Hand PIP3 (Middle)"
```

The parser:
1. Strips whitespace, normalizes case
2. Extracts laterality prefix (R/L/B for Right/Left/Bilateral)
3. Maps abbreviation to full anatomical name
4. Looks up ICD-10 code from `icd_codes_lookup` dictionary (90+ entries)

### ICD-10 Code Lookup
```python
icd_codes_lookup = {
    'R MCP': 'M25.541', 'L MCP': 'M25.542',
    'R KNEE': 'M25.561', 'L KNEE': 'M25.562',
    'R WRIST': 'M25.531', 'L WRIST': 'M25.532',
    # ... 90+ entries
}
```

### Note Generation
Text is built incrementally as checkboxes change. Each section has a builder method:

- `build_synovitis_text()` → e.g., "Mild gray synovitis present."
- `build_bony_text()` → e.g., "No significant bony disruption noted."
- `build_tendon_text()` → e.g., "Normal tendon appearance."
- `build_effusion_text()` → e.g., "Small effusion present."

Final template:
```
LIMITED DIAGNOSTIC ULTRASOUND EXAM
Performing Physician: Zachary Fellows, MD
Indication: Pain
Pre-procedure Diagnosis: [JOINT_NAME] - [ICD_CODE]
Post-procedure Diagnosis: [JOINT_NAME] - [ICD_CODE]

Equipment: Clarius HD L15 Linear Array Ultrasound Probe

Findings:
[JOINT_NAME]:
[CONSTRUCTED_FINDINGS_TEXT]

Conclusion: [Normal/Abnormal] limited ultrasound exam
```

If bilateral, the findings section is duplicated for both sides.

### Output
On OK, the dialog returns a dictionary:
```python
{
    "full_note": str,      # Complete formatted clinical note
    "joints": list,        # Joint objects with individual findings
    "bilateral": bool,
    "codes": list[str]     # ICD-10 codes
}
```

The note is typically copied to clipboard for pasting into the EHR.

### Dependencies
- `PySide6` - dialog UI
- `re` - regex for joint name parsing

### Porting Notes
- The UI could use `egui` or `iced` in Rust, or a webview-based approach
- The note generation is pure string building - trivial to port
- The joint/ICD-10 lookup tables are static data, easily embedded in Rust

---

## Module 4: Joint Injection Procedure Dialog

### Files
- `procedure.py` (~900 lines) - Dialog logic, structure parsing, note generation
- `procedure_joints.py` (~124 lines) - Joint definitions (87 joints with display names, ICD codes, sizes)
- `procedure_codes.py` (~203 lines) - CPT codes, J-codes, medications, default doses, special patterns

### Dialog Layout
Supports up to **4 independent injection sections** per session. Each section has:
- **Structure input** - free-text field for joint/structure name
- **Type badge** - color-coded label showing parsed structure type
- **Medication dropdown** - Dexamethasone 4mg/ml | Triamcinolone 40mg/ml | Lidocaine 1%
- **Amount spinbox** - dose amount (units change: mg for steroids, ml for lidocaine)
- **Ultrasound guided checkbox** - toggles CPT code variant

### Structure Parsing (`parse_structure_input()`)
Regex-based parser that handles multiple input patterns:

| Input | Parsed Type | Parsed Name | Size |
|-------|------------|-------------|------|
| `R MCP2` | joint | Right Hand MCP2 (Index) | small |
| `L knee` | joint | Left Knee | large |
| `cts` | carpal_tunnel | Carpal Tunnel | medium |
| `lat epi` | tendon_origin | Lateral Epicondyle | medium |
| `dequervain` | tendon_sheath | De Quervain's | medium |
| `lat hip` | tendon_origin | Lateral Hip/Greater Trochanter | large |
| `TP: R trap, L paraspinal` | trigger_point | (multiple muscles) | N/A |

Special injection patterns are defined in `INJECTION_PATTERNS`:
```python
INJECTION_PATTERNS = {
    "cts": {"type": "carpal_tunnel", "name": "Carpal Tunnel", "size": "medium"},
    "lat epi": {"type": "tendon_origin", "name": "Lateral Epicondyle", "size": "medium"},
    "dequervain": {"type": "tendon_sheath", "name": "De Quervain's", "size": "medium"},
    "lat hip": {"type": "tendon_origin", "name": "Lateral Hip/Greater Trochanter", "size": "large"},
    # ...
}
```

### Auto-Medication Selection
When a structure is parsed:
- **Trigger points** → auto-select Lidocaine
- **All other structures** → default to Dexamethasone or Triamcinolone based on preference

### Default Dosing
Doses auto-populate based on joint size:
```python
DEFAULT_DOSES = {"small": 10, "medium": 20, "large": 40}           # Triamcinolone (mg)
DEXAMETHASONE_DOSES = {"small": 1, "medium": 2, "large": 4}        # Dexamethasone (mg)
```

### CPT Code Assignment
```python
CPT_CODES = {
    "small_joint":        "20600",
    "small_joint_us":     "20604",   # Ultrasound-guided variant
    "medium_joint":       "20605",
    "medium_joint_us":    "20606",
    "large_joint":        "20610",
    "large_joint_us":     "20611",
    "carpal_tunnel":      "20526",
    "tendon_sheath":      "20550",
    "tendon_origin":      "20551",
    "trigger_point_12":   "20552",   # 1-2 muscles
    "trigger_point_3plus":"20553",   # 3+ muscles
    "ultrasound_guidance":"76942",
}
```

### J-Code Assignment (Drug Billing Codes)
```python
# Dexamethasone → J1100
# Triamcinolone → J3301
# Lidocaine     → J2001
```

### Procedure Note Template
```
[STRUCTURE_NAME] injection with [MEDICATION] [AMOUNT][UNIT]
[If US guided: Ultrasound-guided]
CPT: [CODE] | J-Code: [JCODE]
```

For trigger points, the note lists all muscles:
```
Trigger Point Injection: Right trapezius, Left paraspinal
Medication: Lidocaine 1% 3ml per site
CPT: 20552
```

### Output Signal
The dialog emits a `dataReady` signal with:
```python
{
    "procedure_notes": [
        ["Joint Injections", "R MCP2 injection with Triamcinolone 10mg..."],
        ["Trigger Points", "Right trapezius TPI with Lidocaine 1% 3ml..."]
    ],
    "cpt_codes": ["20604", "20552"],
    "j_codes": ["J3301", "J2001"]
}
```

### Joint Database (`procedure_joints.py`)
87 joint definitions, each with:
```python
{
    "abbrev": "R MCP2",
    "display": "Right Hand MCP2 (Index)",
    "icd10": "M25.541",
    "size": "small"   # small | medium | large
}
```

### Dependencies
- `PySide6` - dialog UI
- `re` - regex for structure parsing

### Porting Notes
- The structure parser is the most complex part - multiple regex patterns and a lookup table
- CPT/ICD code tables and dosing data are static, easily embedded
- The dialog supports dynamic show/hide of injection sections which will need UI framework support

---

## Module 5: Automation Engine (zautogui)

### Files
- `zautogui/interface.py` (~652 lines) - Core engine: screenshot, image matching, keyboard/mouse
- `zautogui/data_structures.py` (~162 lines) - Point, Box, Person dataclasses
- `zautogui/screencap.py` - Screenshot utilities
- `zautogui/recorder.py` - Hotkey recording
- `zautogui/web.py` (~232 lines) - Playwright browser automation

### Image Template Library
Located in `images/` directory (30+ JPG files). These are small screenshots of UI elements from the EHR application:
- `addproblem_save.jpg`, `chart_orders.jpg`, `chart_rx.jpg`, `chart_msg.jpg`
- `problem list.jpg`, `referral.jpg`, `referral_PT.jpg`
- `orders_lab.jpg`, `orders_imaging.jpg`, `orders_cardiac.jpg`
- `summary_brother.jpg`, `summary_printer_destination.jpg`
- And many more

### Screenshot Capture (`_screenshot()`)
Uses Windows native API (win32gui/win32ui) for performance:
```
1. GetDesktopWindow() → get full desktop HWND
2. GetWindowDC() → get device context
3. CreateCompatibleDC() + CreateBitmap() → create offscreen buffer
4. BitBlt() → fast copy from screen to buffer
5. GetBitmapBits() → extract raw pixel data
6. np.frombuffer() → convert to NumPy array
7. cv2.cvtColor(BGRA→GRAY) → convert to grayscale
```

**Parameters:**
- `box=(left, top, width, height)` - capture specific region
- `quadrant=1|2|3|4` - divide screen into halves, capture one quadrant (1=upper-left, 2=upper-right, 3=lower-left, 4=lower-right)
- `HSV=True` - capture in HSV color space (for histogram comparison)

**Returns:** NumPy grayscale (or HSV) image array

### Image Matching (`_image_search()`)
Uses OpenCV template matching:
```
1. Load template image from file (grayscale) → cache it
2. Optional: resize template by self.scaling_factor_images
3. Capture fresh screenshot (grayscale)
4. cv2.matchTemplate(screenshot, template, TM_CCOEFF_NORMED)
5. cv2.minMaxLoc(result) → get max confidence and location
6. If max_val >= precision_threshold (default 0.8):
     Return ImageFindResult(found=True, x=midpoint_x, y=midpoint_y, confidence=max_val)
   Else:
     Return ImageFindResult(found=False)
```

**ImageFindResult fields:** `found: bool, x: int, y: int, confidence: float`

**Features:**
- **Image caching** - templates loaded once from disk, stored in `self.image_cache` dict
- **Scaling factor** - `self.scaling_factor_images` (0.3-2.0) resizes templates for high-DPI displays. Adjustable from the menu (Set Scale Ratio submenu)
- **Precision threshold** - configurable per call, default 0.8
- **Quadrant search** - restrict search to a screen quadrant for speed and accuracy
- **Multi-image parallel search** - search for multiple templates simultaneously using threads

### Convenience Methods

#### `find_image(filename, click=None, wait=0.0, sequence=False, debug=False, precision=0.8, quadrant=None)`
High-level wrapper:
1. Call `_image_search(filename, ...)`
2. If found and `click` specified → `move_click(result.x, result.y, click=click)`
3. If `sequence=True` and not found → raise `StopIteration` (breaks automation sequence)
4. If `debug=True` → post toast with result info
5. Return `ImageFindResult`

#### `find_images(filenames, click=None, ...)`
Search for multiple templates, return first match. Useful for "find any of these UI states."

### Mouse Control
```python
@staticmethod
def move_click(x, y, click=None, wait=0.0):
    match click:
        case None:      pyautogui.moveTo(x, y)
        case 'left':    pyautogui.click(x, y, button='left')
        case 'right':   pyautogui.click(x, y, button='right')
        case 'double':  pyautogui.click(x, y); sleep(0.2); pyautogui.click(x, y)
        case int(n):    pyautogui.click(x, y, clicks=n)
    if wait > 0:
        time.sleep(wait)
```

### Keyboard Control
```python
def send_keystroke(self, keystroke, repeat=1, delay=0.05):
    # Maps string names to pynput.keyboard.Key enum:
    # 'enter', 'backspace', 'tab', 'up', 'down', 'left', 'right',
    # 'shift', 'ctrl', 'alt', 'delete', 'home', 'end', 'esc', 'f1'-'f12'
    for _ in range(repeat):
        self.keyboard.tap(key)
        time.sleep(delay)

def send_string(self, string, delay=0.0):
    # Type string character by character via pynput
    for char in string:
        self.keyboard.type(char)
        if delay > 0:
            time.sleep(delay)

def send_hotkey(self, *keys):
    # Press key combo (e.g., send_hotkey('ctrl', 'v'))
    # Hold all modifier keys, tap last key, release
```

### Clipboard
```python
@staticmethod
def save_clipboard(string):
    pyperclip.copy(string)

@staticmethod
def get_clipboard():
    return pyperclip.paste()
```

### Sequence Decorator
Thread-safe wrapper for multi-step automations:
```python
@sequence
def my_automation():
    ux.find_image("button1.jpg", click="left", sequence=True)  # Raises StopIteration if not found
    time.sleep(0.5)
    ux.find_image("button2.jpg", click="left", sequence=True)  # Skipped if above failed
    ux.send_string("Hello")
```

- Acquires a `threading.Lock` before running
- If any step raises `StopIteration`, the entire sequence aborts gracefully
- Prevents overlapping automation runs (double-press hotkey protection)

### OCR Support
```python
def tesseract(self, image):
    pytesseract.pytesseract.tesseract_cmd = r"E:\Tesseract\tesseract"
    return pytesseract.image_to_string(image, lang='eng')
```

### Histogram Comparison
```python
def compare_histogram(self, image, screenshot=None):
    # HSV histogram comparison for color-based image matching
    # Uses cv2.calcHist + cv2.compareHist (correlation method)
    # Returns: correlation score 0.0-1.0
```

### Screen Quadrant Helpers (`data_structures.py`)
```python
def to_quadrant(x1, y1, x2, y2, quadrant) -> tuple:
    # Convert quadrant-local coordinates to full-screen coordinates

def from_quadrant(x1, y1, x2, y2, quadrant) -> tuple:
    # Convert full-screen coordinates to quadrant-local
```

Screen is divided:
- Quadrant 1: upper-left (0,0 to mid_x, mid_y)
- Quadrant 2: upper-right (mid_x,0 to max_x, mid_y)
- Quadrant 3: lower-left (0,mid_y to mid_x, max_y)
- Quadrant 4: lower-right (mid_x,mid_y to max_x, max_y)

### Browser Automation (`web.py`)
Uses Playwright to control an existing Chrome instance via CDP (Chrome DevTools Protocol):
```python
def find_existing_tab(playwright, url='', title=''):
    browser = playwright.chromium.connect_over_cdp("http://localhost:9214")
    # Iterate contexts and pages, return first match by URL or title

def open_new_tab_in_browser(playwright, url):
    # Open new tab in existing Chrome via CDP

def get_url(url):
    # High-level: find existing tab or open new one
```

Requires Chrome launched with `--remote-debugging-port=9214`.

### Dependencies
- `pyautogui` - mouse movement/clicks
- `cv2` (OpenCV) - template matching, image processing, histogram
- `numpy` - image arrays
- `win32gui`, `win32ui`, `win32api`, `win32con` - Windows native screenshot
- `pynput` - keyboard controller (more reliable than pyautogui for key input)
- `pytesseract` - OCR
- `pyperclip` - clipboard
- `playwright` - browser automation
- `threading` - sequence lock

### Porting Notes
- **Screenshot**: use `winapi` crate with `BitBlt` - direct translation of the win32 approach
- **Image matching**: use `opencv-rust` crate (bindings to OpenCV C++)
- **Mouse/keyboard**: use `enigo` or `winapi` `SendInput` directly
- **Clipboard**: use `clipboard-win` crate
- **Threading**: use `std::sync::Mutex` or `tokio::sync::Mutex`
- **Browser**: use `chromiumoxide` crate for CDP or call Chrome directly
- **Image cache**: `HashMap<String, Mat>` with lazy loading

---

## Module 6: EHR Automation Sequences

### Files
- `ui.py` (lines ~400-790) - All automation functions defined here

These are the `@sequence`-decorated functions that automate multi-step EHR workflows by combining image matching, mouse clicks, keyboard input, and waits.

### Close Visit (`close_visit(code="99214")`)
1. Find and click the "Close Visit" button in EHR via image search
2. Wait for dialog to load
3. Enter billing code (e.g., 99214)
4. Tab through fields
5. Click sign/submit
6. Handle any confirmation dialogs

Supports codes: 99214, 99205, 99215, 99204, 99214-95 (telehealth), and phone visit variants.

### Lab Orders (`quest_hotkey()`, `labcorp_hotkey()`)
1. Find "Orders" section via image search (`chart_orders.jpg`)
2. Click to open orders
3. Find "Lab" tab (`orders_lab.jpg`)
4. Click into order entry
5. Navigate to appropriate lab (Quest or Labcorp)
6. The user then manually completes the order

### Radiology Orders (`rads_hotkey()`)
1. Find "Orders" section
2. Click "Imaging" tab (`orders_imaging.jpg`)
3. Open imaging order dialog

### Echocardiogram Order (`TTE_shortcut()`)
1. Open orders
2. Navigate to cardiac tab (`orders_cardiac.jpg`)
3. Type "TTE" or echocardiogram order
4. Fill in CPT code and diagnosis
5. Submit

### Pulmonary Function Test (`PFT_shortcut()`)
1. Open orders
2. Navigate to appropriate order type
3. Fill in PFT order details

### Referral - Physical Therapy (`referral_PT()`)
1. Find "Referral" button (`referral.jpg`) → click
2. Wait 0.5s for dialog
3. Type "Physical Therapy" in provider field
4. Tab through 2 fields
5. Wait 0.8s
6. Click template dropdown at fixed coordinate (3760, 771)
7. Find "PT" template image (`referral_PT.jpg`) → click
8. Click diagnostic codes field at fixed coordinate (2313, 904)

### Referral - General (`referral_general()`)
Similar to PT referral but without pre-filled specialty.

### Print Summary (`print_summary_shortcut()`)
1. Find print button
2. Navigate printer selection
3. Find Brother printer option (`summary_brother.jpg`)
4. Confirm print

### Smart Click (`smart_click()`)
Context-aware click that checks for multiple possible UI states and acts on whichever is found. Uses `find_images()` to detect the current state.

### Common Pattern
All automation sequences follow the same pattern:
```
@sequence
def automation_name():
    ux.find_image("element.jpg", click="left", sequence=True)  # Find + click
    time.sleep(0.3-1.0)                                         # Wait for UI
    ux.send_string("text to type")                               # Type
    ux.send_keystroke("tab", repeat=N)                          # Navigate
    ux.move_click(x, y, click="left")                           # Fixed-position click
    ux.find_image("next_element.jpg", click="left", sequence=True)
```

Key characteristics:
- **Image matching** for finding UI elements (resilient to minor layout changes)
- **Fixed coordinates** for elements that don't move (faster, but brittle)
- **Timing waits** between steps (for UI animations/loading)
- **Sequence mode** for early termination if any step fails

### Porting Notes
- Each automation is essentially a script/recipe of steps - could be defined declaratively in TOML/JSON
- The `@sequence` pattern maps to a Rust `Result<(), StopError>` chain with `?` operator
- Consider making automations data-driven (loadable config) rather than hardcoded

---

## Module 7: Dictation & LLM Processing

### Files
- `dictation/client.py` (~604 lines) - Main client with adapter pattern
- `dictation/prompts.py` (~291 lines) - Medical transcription prompts
- `dictation/server.py` (~148 lines) - Local Flask server for Whisper/Ollama

### Architecture
```
Microphone → AudioAdapter (transcribe) → raw text → LLMAdapter (process) → cleaned text → callback
```

### Audio Recording
```python
def record_audio(self, duration=30):
    # PyAudio: 44100 Hz, 16-bit mono, default microphone
    # Returns NumPy array of audio samples
```

Toggle behavior: F12 starts recording, F12 again stops. Or auto-stops after duration.

### Audio Adapters (Speech-to-Text)

| Adapter | Backend | Notes |
|---------|---------|-------|
| `LocalAudioAdapter` | `POST http://localhost:5000/transcribe` | Local Whisper instance |
| `GroqAudioAdapter` | Groq API (`groq.audio.transcriptions.create()`) | Cloud, fast, default |
| `DeepgramAudioAdapter` | Deepgram Nova-3 model | Cloud, alternative |

All adapters implement:
```python
class AudioAdapter(ABC):
    @abstractmethod
    def transcribe(self, audio_data: np.ndarray) -> str: ...
```

The Groq adapter supports a custom prompt hint for medical terminology:
```python
prompt = "Cimzia, Humira, Remicade, Enbrel, methotrexate, hydroxychloroquine..."
```

### LLM Adapters (Text Post-Processing)

| Adapter | Backend | Model |
|---------|---------|-------|
| `LocalLLMAdapter` | `POST http://localhost:5000/llm_process` | Ollama (local) |
| `HFAdapter` | Hugging Face Inference API | Nous-Hermes-2-Mixtral-8x7B-DPO (default) |
| `GroqAdapter` | Groq API | Mixtral-8x7b-32768 |

All adapters implement:
```python
class LLMAdapter(ABC):
    @abstractmethod
    def process_text(self, text: str, config: dict) -> str: ...
```

### LLM Configuration
```python
DEFAULT_LLM_CONFIG = {
    "model": "phi3.5:3.8b-mini-instruct-q5_K_M",
    "prompt": "You are a helpful transcriber...",
    "temperature": 0.2
}
```

The prompt can be switched at runtime via the "Set LLM Prompts" menu.

### Medical Prompts (`prompts.py`)
Multiple prompt templates optimized for different use cases:
- `prompt_transcription` - General: correct grammar, punctuation, maintain meaning
- `prompt_medical_transcription` - Rheumatology-specific: expand abbreviations, format as clinical note
- Additional prompt variants for specific documentation styles

### Local Server (`server.py`)
Flask application serving two endpoints:
```
POST /transcribe  - Receive audio bytes, run Whisper, return {"text": "..."}
POST /llm_process - Receive {"text": "...", "config": {...}}, run Ollama, return {"text": "..."}
```

### Pipeline Flow
1. User presses F12 → `hotkey_toggle()` called
2. Record audio from microphone (up to 30s or until F12 pressed again)
3. Send audio to selected AudioAdapter → get raw transcription text
4. If LLM enabled: send text to selected LLMAdapter with configured prompt → get processed text
5. If LLM disabled: use raw transcription as-is
6. Call `callback(processed_text)` → typically copies to clipboard and shows toast

### API Keys
Stored in `llm.env`:
```
GROQ_API_KEY=...
HF_API_KEY=...
DEEPGRAM_API_KEY=...
```

### Dependencies
- `pyaudio` - microphone recording
- `numpy` - audio sample arrays
- `soundfile` - WAV encoding
- `wave` - WAV file format
- `groq` - Groq API client
- `deepgram-sdk` - Deepgram API client
- `huggingface_hub` - HF Inference API
- `requests` - HTTP calls to local server
- `flask` - local server framework
- `python-dotenv` - load `.env` files

### Porting Notes
- Audio recording: use `cpal` crate in Rust
- HTTP API calls: use `reqwest` crate
- The adapter pattern maps well to Rust traits
- The local Flask server could become a Rust `axum` or `actix-web` server
- Consider whether to keep LLM processing as a separate service or embed it

---

## Module 8: Homunculus (Visual Joint Exam Tracker)

### Files
- `homunculus/homunculus.py` (~480 lines)

### Purpose
Interactive visual diagram for documenting a 28-joint examination and calculating CDAI (Clinical Disease Activity Index) scores.

### Layout
Two side-by-side body diagrams (homunculus figures):
- **Left diagram**: Swollen joints (red when selected)
- **Right diagram**: Tender joints (yellow when selected)

Plus input fields for:
- Patient Global Assessment (0-10 VAS)
- Physician Global Assessment (0-10 VAS)

### Joint Representation
```python
class JointItem(QGraphicsEllipseItem):
    # Properties: joint_name, joint_type ('swollen'|'tender'), is_selected
    # Visual: white circle when unselected, red/yellow when selected
    # Interaction: click to toggle selection
```

28 joints are represented (standard CDAI joint set):
- Shoulders (2), Elbows (2), Wrists (2)
- MCPs 1-5 bilateral (10), PIPs 1-5 bilateral (10)
- Knees (2)

### CDAI Calculation
```
CDAI = Patient_Global + Physician_Global + Tender_Joint_Count + Swollen_Joint_Count
```

Score interpretation:
- 0-2.8: Remission
- 2.9-10: Low disease activity
- 10.1-22: Moderate disease activity
- >22: High disease activity

### Clipboard Integration
- **Load**: On dialog open, attempts to parse joint data from clipboard (allows pre-population from EHR copy)
- **Export**: Button copies current state to clipboard in a structured format

### Dependencies
- `PySide6.QtWidgets` - dialog
- `PySide6.QtCore` - signals/slots
- `PySide6.QtGui` - graphics scene for interactive joint items

### Porting Notes
- The interactive diagram uses Qt's QGraphicsScene - would need equivalent in Rust (e.g., custom canvas rendering in `egui` or `iced`)
- The CDAI calculation is trivial arithmetic
- Clipboard integration straightforward with `clipboard-win`

---

## Module 9: Weight Management Dialog

### Files
- `weight_summary.py` (~29KB)

### Purpose
Generate comprehensive weight management plans for patients, including caloric targets, macronutrient breakdowns, and patient education materials.

### Inputs
- Height, weight, age, sex
- Activity level (sedentary → very active)
- Goal (lose/maintain/gain)
- Protein preference (standard → high)

### Calculations

**BMR (Basal Metabolic Rate) - Mifflin-St Jeor:**
```
Male:   BMR = 10 × weight(kg) + 6.25 × height(cm) - 5 × age - 161 + 166
Female: BMR = 10 × weight(kg) + 6.25 × height(cm) - 5 × age - 161
```

**TDEE (Total Daily Energy Expenditure):**
```
TDEE = BMR × activity_multiplier
Activity multipliers: 1.2 (sedentary), 1.375 (light), 1.55 (moderate), 1.725 (very active)
```

**Macronutrient Targets:**
- Protein: 0.8-1.2g per lb body weight (based on preference)
- Fat: 0.25-0.35g per lb body weight
- Carbohydrates: remaining calories after protein + fat
- Fiber: activity-level based target

### Output Formats
1. **Patient email template** - Friendly, educational format with meal plan suggestions
2. **Clinical note** - Structured for EHR documentation
3. **Clipboard copy** - For direct pasting

### Dependencies
- `PySide6` - dialog UI

### Porting Notes
- Pure arithmetic and string formatting - trivial to port
- The dialog UI is the main porting consideration

---

## Module 10: Billing & Reimbursement

### Files
- `billing/functions.py` - Utility functions
- `billing/reimbursement.py` - Medicare reimbursement lookup

### Data Files
- `billing/PFALL20_cleaned.txt` - Medicare 2020 fee schedule
- `billing/PFALL24_cleaned.txt` - Medicare 2024 fee schedule
- CSV files with locality-specific data

### Functionality
- Look up Medicare reimbursement rates by CPT code and geographic locality
- Compare 2020 vs 2024 rates

### Dependencies
- `pandas` - CSV/data handling
- Standard library file I/O

### Porting Notes
- Data parsing with `csv` crate
- Could be a standalone CLI tool

---

## Module 11: PDF Processing

### Files
- `PDF/` directory (~5.7KB)

### Functionality
```python
def ocr_pdf_first_two_pages(filename):
    # Convert first 2 PDF pages to images
    # Run Tesseract OCR on each
    # Return extracted text

def compress_text(text):
    # Remove excessive newlines, clean whitespace
    # Return compressed text
```

### Dependencies
- `pytesseract` - OCR
- `cv2` - image conversion
- `pdf2image` or similar - PDF to image conversion

### Porting Notes
- Could use `tesseract-rs` or `leptonica` bindings
- PDF rendering: `pdfium` or `poppler` bindings
- This is a utility module, low priority for porting

---

## Module 12: Data Structures

### Files
- `zautogui/data_structures.py` (~162 lines)

### Types

**Point:**
```python
@dataclass
class Point:
    x: int
    y: int
    # Flexible constructor: Point(x,y), Point((x,y)), Point(pyautogui.Point)
```

**Box:**
```python
class Box:
    top_left: Point
    bottom_right: Point

    def as_width_height(self) -> tuple:  # (left, top, width, height)
    def midpoint(self) -> Point           # center point
```

**Person:**
```python
@dataclass
class Person:
    last: str
    first: str
    dob: str
    sex: str
```

**ImageFindResult:**
```python
@dataclass
class ImageFindResult:
    found: bool
    x: int = 0
    y: int = 0
    confidence: float = 0.0
```

### Porting Notes
- Direct mapping to Rust structs with `#[derive(Debug, Clone)]`
- Point/Box are geometric primitives - trivial
- Person is a simple data record

---

## Dependency Summary (Full)

### Core GUI
| Python Package | Purpose | Rust Equivalent |
|---------------|---------|-----------------|
| PySide6 | Qt6 GUI framework | `egui`, `iced`, `druid`, or `tauri` (webview) |

### Automation & Imaging
| Python Package | Purpose | Rust Equivalent |
|---------------|---------|-----------------|
| pyautogui | Mouse/keyboard control | `enigo` or raw `winapi` `SendInput` |
| opencv-python | Template matching | `opencv-rust` |
| numpy | Image arrays | Built-in arrays or `ndarray` |
| Pillow | Image handling | `image` crate |
| pytesseract | OCR | `tesseract-rs` or `leptess` |

### Windows APIs
| Python Package | Purpose | Rust Equivalent |
|---------------|---------|-----------------|
| pywin32 (win32gui, win32ui, win32api) | Screenshot, window management | `winapi` or `windows` crate |

### Input Hooking
| Python Package | Purpose | Rust Equivalent |
|---------------|---------|-----------------|
| keyboard | Global keyboard hooks | `winapi` `SetWindowsHookEx` / `device_query` |
| mouse | Mouse click detection | `winapi` mouse hooks |
| pynput | Keyboard controller | `enigo` |

### Clipboard
| Python Package | Purpose | Rust Equivalent |
|---------------|---------|-----------------|
| pyperclip | Clipboard read/write | `clipboard-win` or `arboard` |

### Audio
| Python Package | Purpose | Rust Equivalent |
|---------------|---------|-----------------|
| PyAudio | Microphone recording | `cpal` |
| soundfile | Audio I/O | `hound` |

### Network/API
| Python Package | Purpose | Rust Equivalent |
|---------------|---------|-----------------|
| requests | HTTP client | `reqwest` |
| Flask | HTTP server | `axum` or `actix-web` |
| groq | Groq API | `reqwest` (REST calls) |
| deepgram-sdk | Deepgram API | `reqwest` (REST calls) |
| huggingface_hub | HF API | `reqwest` (REST calls) |

### Browser
| Python Package | Purpose | Rust Equivalent |
|---------------|---------|-----------------|
| playwright | Chrome CDP | `chromiumoxide` or `headless-chrome` |

### Configuration
| Python Package | Purpose | Rust Equivalent |
|---------------|---------|-----------------|
| tomllib | TOML parsing | `toml` |
| python-dotenv | .env loading | `dotenvy` |

### Data
| Python Package | Purpose | Rust Equivalent |
|---------------|---------|-----------------|
| pandas | CSV/data | `csv` crate or `polars` |
| regex | Advanced regex | `regex` crate |

---

## Suggested Rust Module Decomposition

Based on the analysis, the application naturally splits into these independent Rust binaries/crates:

### 1. `macro-overlay` (Main Shell)
- System tray or tiny overlay window
- Menu system
- Global hotkey registration
- Toast notifications
- Orchestrates other modules

### 2. `text-expander`
- Keyboard hook state machine
- TOML config loading
- Clipboard-based expansion
- Special triggers (date, weeks)
- Could be a standalone daemon

### 3. `zautogui` (Automation Engine)
- Screenshot via Win32 API
- OpenCV template matching
- Mouse/keyboard simulation
- Sequence execution with abort
- Image caching
- Quadrant system

### 4. `ultrasound-notes`
- Joint parsing & ICD-10 lookup
- Findings checkboxes → text builder
- Note template generation
- GUI dialog

### 5. `procedure-notes`
- Structure parsing (joints, trigger points, special patterns)
- CPT/J-code assignment
- Default dosing
- Multi-injection support
- GUI dialog

### 6. `homunculus`
- Interactive joint diagram
- CDAI calculation
- Clipboard import/export
- GUI dialog

### 7. `dictation-client`
- Audio recording
- Adapter pattern for STT backends
- Adapter pattern for LLM backends
- Configurable prompts

### 8. `ehr-automations`
- Data-driven automation recipes (TOML/JSON config)
- Uses `zautogui` as a library
- Close visit, lab orders, referrals, etc.

### 9. `weight-mgmt`
- BMR/TDEE calculations
- Macro targets
- Note generation
- GUI dialog

### 10. `billing-lookup`
- Medicare fee schedule parsing
- CPT code reimbursement lookup
- CLI tool

---

## Known Issues (From User Report)

1. **Memory leak** - causes unpredictable crashes over time. Likely candidates:
   - Repeated screenshot capture without proper cleanup of win32 DC objects
   - OpenCV Mat objects accumulating
   - PySide6 widget lifecycle issues
   - Image cache growing unbounded
   - PyAudio stream not being properly closed

2. **Long load time** - likely from:
   - Importing heavy libraries (OpenCV, PySide6, Playwright)
   - Loading 183KB TOML config
   - Initializing audio devices
   - Pre-caching images

3. **"Too many features" in one app** - monolithic design means:
   - All modules loaded even if unused
   - Single crash kills everything
   - Hard to update one module without risking others
   - Resource usage is cumulative

All three issues are naturally addressed by the Rust decomposition into separate binaries.
