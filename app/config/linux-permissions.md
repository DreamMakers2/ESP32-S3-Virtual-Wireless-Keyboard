# CachyOS permissions

The application opens one configured keyboard through `/dev/input` and bridge A
through `/dev/serial`. It must run as the desktop user, never as root. Configure
the system's normal `input` and serial-device group or udev permission rules for
that user, then log out and back in. Set persistent paths in
`$XDG_CONFIG_HOME/keyboard-bridge/config.toml`, normally paths below
`/dev/input/by-id` and `/dev/serial/by-id`.

The program intentionally requests `EVIOCGRAB` only after clicking the input
surface while it has window focus. Focus loss and orderly exit release the grab.
