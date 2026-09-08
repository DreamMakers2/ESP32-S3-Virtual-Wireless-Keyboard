# Native visual checks

Build from the repository root with `app/build.sh` and run the application on KDE
Wayland at 460x300, 780x520, and a wide desktop viewport, at 100% and HiDPI scale.
Use `--screenshot /absolute/path/frame.png` to request one native egui viewport
screenshot without adding capture controls to the UI. Use
`--screenshot-size 460x300` to select a bounded viewport. Desktop scaling still
applies to physical image dimensions.

The screenshot route never activates evdev capture; capture requires a
surface click plus remote readiness.
