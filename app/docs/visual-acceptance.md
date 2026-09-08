# Native visual checks

Build from the repository root with `app/build.sh` and run the application on KDE
Wayland at 460x300, 780x520, and a wide desktop viewport, at 100% and HiDPI scale.
Use `--screenshot /absolute/path/frame.png` to request one native egui viewport
screenshot without adding capture controls to the UI. Use
`--screenshot-size 460x300` to select a bounded viewport. Desktop scaling still
applies to physical image dimensions.

Check light and dark paused states, debug on/off, rounded-surface margins, soft
wrapping, status tooltip/menu, ordinary KDE title-bar controls, and focus loss.
The screenshot route never activates evdev capture; capture still requires a
surface click plus remote readiness.

For history rendering, check adjacent tokens, modifier taps/chords, and normal
Shift/AltGr text. During an activated target USB disconnect, check the waiting
overlay and status, confirm the menu still offers Pause, and verify automatic
resume after a fresh handshake. Repeat with a quick USB cycle and a held key;
outage input must not replay. Pause or focus loss while waiting must prevent
automatic resume.
