ClickFlow 0.1.0 - Auto Clicker for Windows x64
================================================

ClickFlow is a desktop auto clicker with adjustable speed, pre/post-click
delays, loop limits, and a macro editor.

RUNNING
-------
1. Extract this zip.
2. Run ClickFlow.exe. No installation is required.

Global shortcuts:
  F6  Start
  F7  Stop

If Windows SmartScreen shows a warning, choose "More info" and then
"Run anyway". This build is not code-signed.

FEATURES
--------
- Clicks per second or clicks per minute
- Start delay, pause before press, button hold, and pause after release
- Infinite looping or a fixed number of clicks
- Left, right, or middle mouse button
- Current cursor or fixed screen coordinate
- Macros with click, move, pause, text, key press, and scroll steps
- Per-step delays, macro loops, and a pause between passes

SETTINGS
--------
Settings and macros are saved automatically to:

  %APPDATA%\ClickFlow\settings.ron

TIMING
------
The effective interval between clicks is:

  max(rate interval, before press + button hold + after release)

The Start delay runs once before a run begins. Use it to switch to the app
you want to click.

NOTES
-----
- Keep the ClickFlow window open while a run is active.
- Press F7 or use the Stop run button to stop from any app.
- Extremely high click rates are limited by Windows and the target app.
- Screen coordinates start at the top-left corner of the primary display.
- The interface uses IBM Plex fonts under the SIL Open Font License. See
  FONT-LICENSE.txt.
