# Linux device permissions

The application opens one configured keyboard through `/dev/input` and bridge A
through `/dev/serial`. It must run as the desktop user, never as root.

Grant that user access through the system's normal device permissions or through
a narrowly matched udev rule. Set persistent device paths in
`$XDG_CONFIG_HOME/keyboard-bridge/config.toml`, normally using paths below
`/dev/input/by-id` and `/dev/serial/by-id`.

## Persistent desktop access with uaccess

`71-keyboard-bridge.rules.example` is a template for granting the active local
desktop user access through systemd's `uaccess` mechanism. Do not install it
unchanged. Replace its placeholder device properties with stable properties from
the keyboard and bridge you actually intend to use.

Inspect a device with `udevadm info --query=property --name=DEVICE` and choose
matches that are specific enough for the intended device. Do not treat the bridge
firmware's default serial value as a unique physical-device identity.

Install the completed rule before the systemd seat rules, reload udev, and reconnect
the affected devices. Permissions granted through `uaccess` are recreated when a
matching device reconnects; temporary ACL changes alone do not survive device
recreation.

Reference: systemd's `73-seat-late.rules.in` documents the corresponding seat-access
handling.

The program intentionally requests `EVIOCGRAB` only after clicking the input
surface while it has window focus. Focus loss and orderly exit release the grab.
