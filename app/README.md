# Keyboard Bridge controller

## Build and launch

Run these commands from the repository root. Install Rust/Cargo, a C compiler,
`pkg-config`, and libxkbcommon development files first. On CachyOS/Arch, the
corresponding packages are `rust`, `base-devel`, `pkgconf`, and `libxkbcommon`.
The native GUI also needs a working OpenGL driver and a Wayland or X11 session.
Required versions are listed in [requirements](../docs/REQUIREMENTS.md).

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

Choose the physical keyboard and bridge A's **native USB/OTG** CDC interface.
See [device permissions](config/linux-permissions.md). Then run `app/run.sh`.
The launcher does not install packages. The packaged executable lives in `app/bin`;
Cargo dependencies and build products remain in `app/build`.

## Operation

Click the rounded surface or choose **Activate** in its right-click menu to start
capture once the whole connection is ready. Clicking away, closing the window,
losing the keyboard, or a communication failure stops forwarding and releases the
local grab. Choose **Pause** from the menu to pause while retaining focus.
Reconnection does not activate capture automatically.

Steady green `(0,64,0)` on the app and both boards means the whole chain is
connected but paused: no keys are captured or transmitted. Active connection is
blue, key activity is white, and a critical error flashes red. Hover the app dot
for its state. Right-click also offers light/dark theme, debug, and exit controls.

The nonselectable local history illustrates physical input; it is separate from
the HID state delivered to the remote target. Its top/bottom fade and scrollbar
affect only that local history. Source interpretation uses the XKB layout selected
by the environment; ensure it matches the target's layout. The target receives
physical HID usages.

Preferences live in `${XDG_CONFIG_HOME:-$HOME/.config}/keyboard-bridge/config.toml`.
Debug is off by default. It enables timing exchanges and shows current timings,
errors, and the last twenty completed press samples. Firmware completion latency
covers the target-facing HID transfer. No keystroke log is written to disk.

## Local tests

```sh
CARGO_HOME="$PWD/app/build/cargo-home" CARGO_TARGET_DIR="$PWD/app/build/target" \
  cargo test --locked --manifest-path app/Cargo.toml -- --test-threads=1
```
