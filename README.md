# ClickFlow

A desktop auto clicker with adjustable speed, pre/post-click timing, loop limits, and a macro editor for longer click sequences.

Built with Rust, `egui`/`eframe`, and `enigo`.

## Features

- Clicks per second or clicks per minute
- Start delay, pause before each press, button hold time, and pause after release
- Infinite looping or a fixed number of clicks
- Left, right, or middle mouse button
- Click at the current cursor position or at a fixed screen coordinate
- Macro editor with click, move, pause, type text, key press, and scroll steps
- Per-step delays, macro loops, and a pause between passes
- Global shortcuts: **F6** starts, **F7** stops
- Settings and macros save automatically

## Prebuilt releases

Download ready-to-use builds from the [latest release](https://github.com/TinaGrim/ClickFlow/releases/latest):

- **macOS (Apple silicon):** `ClickFlow-0.1.0-macos-arm64.dmg`
- **Windows x64:** `ClickFlow-0.1.0-windows-x64.zip`

The packaging scripts also write the same artifacts to `dist/`.

The macOS app is ad-hoc signed but not notarized. If Gatekeeper blocks the first launch, right-click the app, choose **Open**, and confirm. The Windows executable is unsigned, so SmartScreen may ask for confirmation.

## Run it

```bash
cargo run --release
```

Requires Rust 1.88 or newer.

## macOS permissions

macOS blocks synthetic mouse and keyboard events until the app has Accessibility access.

1. Start ClickFlow.
2. Open **System Settings → Privacy & Security → Accessibility**.
3. Enable the app that launched ClickFlow:
   - When using `cargo run`, enable your terminal app (Terminal, iTerm2, VS Code, etc.).
   - When using a packaged `.app`, enable ClickFlow itself.
4. Return to ClickFlow and press **Check again** in the warning banner.

No permission is required for the UI itself, only for sending clicks and key presses.

## Timing model

The effective interval between clicks is:

```text
max(interval from clicks-per-second/minute, before press + button hold + after release)
```

For example, 10 clicks per second means an interval of 100 ms. If the pauses and hold time add up to more than 100 ms, the pauses become the limiting factor. ClickFlow shows the effective speed and warns you when that happens.

The **Start delay** runs once before a run begins. It gives you time to switch to the app you want to click.

## Macros

A macro is a sequence of steps that runs from top to bottom. Each step has a delay after it.

| Step | What it does |
| --- | --- |
| Click | Presses the selected mouse button; use the current cursor or a fixed coordinate |
| Move | Moves the cursor to an absolute screen coordinate |
| Pause | Does nothing for the step's delay |
| Type text | Types a string |
| Key press | Presses one key, optionally with Ctrl, Shift, Alt, or Cmd/Win |
| Scroll | Scrolls vertically or horizontally |

Key names accepted by **Key press** include a single character, `enter`, `tab`, `esc`, `space`, `backspace`, `delete`, the arrow keys, `home`, `end`, `pageup`, `pagedown`, and `f1` through `f12`.

Screen coordinates are measured from the top-left corner of the main display.

## Tips

- Start with a **Start delay** of 1–2 seconds when clicking at the current cursor.
- Keep the ClickFlow window open while a run is active. Closing it stops the run.
- Use **F7** or the **Stop run** button to stop from any app.
- If your keyboard treats F6/F7 as media keys, hold **Fn** while pressing them, or enable standard function keys in macOS keyboard settings.
- Extremely high click rates are limited by the operating system and the target application.

## Settings location

- macOS: `~/Library/Application Support/ClickFlow/settings.ron`
- Windows: `%APPDATA%\ClickFlow\settings.ron`
- Linux: `$XDG_CONFIG_HOME/clickflow/settings.ron` or `~/.config/clickflow/settings.ron`

## Platform notes

- **macOS:** Accessibility permission is required as described above.
- **Windows:** No extra setup.
- **Linux:** X11 is supported. Global shortcuts are not available on Wayland with the current hotkey backend.

## Packaging

macOS DMG (Apple silicon):

```bash
./scripts/package-macos.sh
```

Windows x64 zip, cross-compiled from macOS with mingw-w64:

```bash
brew install mingw-w64
rustup target add x86_64-pc-windows-gnu
./scripts/package-windows.sh
```

Build both and write `dist/SHA256SUMS.txt`:

```bash
./scripts/package-all.sh
```

## Tests

```bash
cargo test
```

## Fonts

The interface uses IBM Plex Sans and IBM Plex Mono. Those font files are distributed under the SIL Open Font License; see `assets/fonts/OFL.txt`.
