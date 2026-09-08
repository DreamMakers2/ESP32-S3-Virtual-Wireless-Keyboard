# Keyboard Bridge controller

## Build and launch

Run these commands from the repository root. Install Rust/Cargo, a C compiler,
`pkg-config`, and libxkbcommon development files first. On CachyOS/Arch, the
corresponding packages are `rust`, `base-devel`, `pkgconf`, and `libxkbcommon`.
The native GUI also needs a working OpenGL driver and a Wayland or X11 session.
Requirements and tested versions are listed in [requirements](../docs/REQUIREMENTS.md).

```sh
app/build.sh
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/keyboard-bridge"
mkdir -p "$config_dir"
cp app/config/defaults.toml "$config_dir/config.toml"
```

Edit that configuration before launching; do not overwrite an existing configured
file. Set `keyboard_path` and `cdc_path` to the persistent paths listed by:

```sh
ls -l /dev/input/by-id/*-event-kbd /dev/serial/by-id/*
```

Choose the physical keyboard for `keyboard_path`. For `cdc_path`, choose bridge A's
CDC serial device while bridge A is connected through the port marked **USB/OTG**
on the PCB. The port marked **COM/UART** on the PCB is for flashing and maintenance,
not normal app communication.

See [device permissions](config/linux-permissions.md). Then run `app/run.sh`.
The launcher does not install packages. The packaged executable lives in `app/bin`;
Cargo dependencies and build products remain in `app/build`.

As an optional launch method, double-click `Keyboard Bridge.desktop` in the
repository root after the app has been built and configured. The launcher resolves
the checkout from its own location and runs the same `app/run.sh`. It does not
install packages or use a project-specific configuration directory.

## Operation

Click the rounded surface or choose **Activate** in its right-click menu to start
capture once the whole connection is ready. Clicking away, closing the window,
losing the keyboard, or an unrecoverable communication failure stops forwarding
and releases the local grab. Choose **Pause** from the menu to pause while
retaining focus.

After activation, temporary target USB disconnects show **Target USB unavailable -
waiting for reconnect**. The app releases capture and discards pending input,
then starts a fresh session automatically when the target is ready. Keys pressed
during the outage are not queued; keys still held at reconnection must be released
and pressed again. **Pause** remains available while waiting. Pause, focus loss,
keyboard failure, or a lost app-to-A connection cancels activation, so recovery
from those conditions still requires **Activate**.

Serial failures are written to stderr with the operation, configured device path,
and original error. Failed connections discard queued serial data before closing
so stale output does not delay reopening a replacement CDC endpoint.

Steady green `(0,64,0)` on the app and both boards means the whole chain is
connected but paused: no keys are captured or transmitted. Active connection is
blue, key activity is white, and a critical error flashes red. Hover the app dot
for its state. Right-click also offers light/dark theme, debug, and exit controls.

The nonselectable local history illustrates physical input; it is separate from
the HID state delivered to the remote target. Its top/bottom fade and scrollbar
affect only that local history. Source interpretation uses the XKB layout selected
by the environment; ensure it matches the target's layout. The target receives
physical HID usages.

Special keys appear as spaced tokens, such as `Hello [Tab] World [F2] Test`.
Shortcuts include all held modifiers, such as `[Ctrl+Alt+Delete]`, without
translated control characters. Modifier taps appear on release with side-specific
names such as `[RCtrl]` and `[RAlt]`; modifiers used with another key do not also
produce standalone tokens. Shift and AltGr keep producing normal printable text.
Unmodified Enter, deletion, and caret navigation retain their live history behavior.

Preferences live in `${XDG_CONFIG_HOME:-$HOME/.config}/keyboard-bridge/config.toml`.
Debug is off by default. It enables timing exchanges and shows current timings,
errors, and the last twenty completed press samples. Firmware completion latency
covers the target-facing HID transfer. No keystroke log is written to disk.

## Local tests

```sh
CARGO_HOME="$PWD/app/build/cargo-home" CARGO_TARGET_DIR="$PWD/app/build/target" \
  cargo test --locked --manifest-path app/Cargo.toml -- --test-threads=1
```
