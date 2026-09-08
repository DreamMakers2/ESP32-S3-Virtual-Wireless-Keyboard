# ESP32-S3 Virtual Wireless Keyboard

A focused wireless keyboard bridge for Linux, built around two ESP32-S3 boards and encrypted ESP-NOW.

![Hardware](https://img.shields.io/badge/hardware-ESP32--S3-blue)
![Desktop](https://img.shields.io/badge/desktop-Linux-blue)
![Status](https://img.shields.io/badge/status-working-green)

Sometimes the machine you need to type on is not at your desk. It might be a server in another room, a bench PC, a BIOS screen, or a machine you simply do not want to keep moving a keyboard between.

This project turns one physical keyboard into a small wireless input bridge. Plug bridge A into the Linux computer, plug bridge B into the remote target, launch the app, and type on the remote machine without disconnecting and carrying your keyboard back and forth.

```text
Linux keyboard → native app → USB CDC → bridge A
                                          │ encrypted ESP-NOW
Target / BIOS  ← USB HID keyboard ← bridge B
```

## Built to stay deliberate

The desktop app only captures the configured physical keyboard after you explicitly activate its focused input window. Click away, close the window, lose the device, or lose the bridge connection and capture stops; the remote key state is released as part of that fail-closed behavior.

That makes the bridge easy to trust in day-to-day use: outside the active window, your keystrokes stay local. While the bridge is connected, the app and both ESP32-S3 boards provide clear status feedback. Connected/paused, active, key activity, and error states are visible through the app and LEDs, so you can tell when the link is ready and when input is actually being transmitted.

The local typing history is display-only and separate from the HID state sent to the target. No keystroke log is written to disk.

## A deliberately hardened bridge protocol

Security and transport behavior are core parts of the design rather than an add-on. The radio link uses encrypted ESP-NOW unicast with pair-specific PMK/LMK keys and explicit peer identity checks. The bridge protocol adds fresh sessions and receiver epochs, ordered transition sequencing, acknowledgements, liveness timeouts, and CRC-32/ISO-HDLC validation for every packet.

Malformed packets, bad CRCs, stale sessions, unexpected peers, sequence errors, and link failures are rejected or fail closed instead of being treated as keyboard input. Pair-specific keys and identifiers are generated locally and never belong in Git, and the app has no runtime account or network-service dependency.

See [the protocol](docs/PROTOCOL.md) and [security notes](docs/SECURITY.md) for the details.

## Hardware

The project uses two ESP32-S3 N16R8 boards:

- **Bridge A** connects to the Linux computer through the port marked **USB/OTG** on the PCB and uses native USB CDC.
- **Bridge B** connects to the target through the port marked **USB/OTG** on the PCB and appears as a native USB HID keyboard.
- The port marked **COM/UART** on the PCB is used for flashing and maintenance.
- The port marked **USB/OTG** on the PCB is used for normal bridge operation.

## Getting started

Start with the [setup guide](docs/SETUP.md). It walks through the process from a fresh checkout to a working pair.

More detailed references:

- [Application build, configuration and operation](app/README.md)
- [Firmware build and provisioning](firmware/README.md)
- [Requirements and compatibility](docs/REQUIREMENTS.md)
- [Binary bridge protocol](docs/PROTOCOL.md)
- [Security](docs/SECURITY.md)
- [Contributing](docs/CONTRIBUTING.md)

For first setup, flash and identify bridge B first, move it to the target machine, then connect it through the port marked **USB/OTG** on the PCB. Flash bridge A next and connect it to the Linux computer through its **USB/OTG**-marked PCB port. Use a blank editor on the target for the first typing test before trying BIOS/UEFI or another sensitive screen.
