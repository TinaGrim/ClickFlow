ClickFlow 0.1.0 - Auto Clicker for macOS (Apple silicon)
=========================================================

INSTALL
-------
1. Drag ClickFlow to the Applications folder.
2. Open ClickFlow from Applications.

This build is ad-hoc signed but not notarized. If macOS blocks the first
launch, right-click ClickFlow in Applications, choose Open, then confirm.

ACCESSIBILITY PERMISSION
------------------------
macOS blocks synthetic mouse and keyboard events until ClickFlow is allowed
in:

  System Settings > Privacy & Security > Accessibility

Enable ClickFlow, then use "Check again" in ClickFlow's warning banner.

GLOBAL SHORTCUTS
----------------
  F6  Start
  F7  Stop

If F6/F7 control media features on your keyboard, hold Fn while pressing
them or enable standard function keys in keyboard settings.

SETTINGS
--------
Settings and macros are saved automatically to:

  ~/Library/Application Support/ClickFlow/settings.ron

TIPS
----
- Keep the ClickFlow window open while a run is active.
- Start with a 1-2 second Start delay when clicking at the current cursor.
- Use F7 or the Stop run button to stop from any app.
- Extremely high click rates are limited by macOS and the target app.
- The interface uses IBM Plex fonts under the SIL Open Font License. See
  the OFL.txt file in the app resources.
