# ESP32-S3 Virtual Wireless Keyboard

A focused Linux keyboard window connected to a remote USB keyboard over encrypted ESP-NOW.

![Hardware](https://img.shields.io/badge/hardware-ESP32--S3-blue)
![Desktop](https://img.shields.io/badge/desktop-Linux-blue)
![Status](https://img.shields.io/badge/status-working%20prototype-green)

The project uses two ESP32-S3 N16R8 boards:

```text
Linux keyboard → native app → USB CDC → bridge A
                                          │ encrypted ESP-NOW
Target / BIOS  ← USB HID keyboard ← bridge B
```

This hobby project has been tested with two ESP32-S3 N16R8 boards, a Linux KDE
Wayland source desktop, remote typing and shortcuts, and the target BIOS/UEFI.
See [verified acceptance and remaining measurements](docs/REQUIREMENTS.md).

The app captures one configured physical keyboard only after an explicit click
inside its focused window. Mouse focus loss pauses capture and releases the remote
keys. Shortcuts go to the target while capture is active. The app's visible typing
history is separate from the physical HID state sent to the target.

## Getting started

- [Implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Setup](docs/SETUP.md) and [verified requirements](docs/REQUIREMENTS.md)
- [Application launch and configuration](app/README.md)
- [Firmware build and provisioning](firmware/README.md)
- [Technical concept](Technical%20Concept%20%E2%80%94%20ESP32-S3%20Wireless%20USB%20Keyboard%20Bridge.md)
- [Binary protocol](docs/PROTOCOL.md)
- [Security](SECURITY.md), [contributing](CONTRIBUTING.md), and [release checklist](docs/PUBLIC_RELEASE_CHECKLIST.md)

Use connector labels: **COM/UART** for flashing and maintenance, **USB/OTG** for
native CDC on A or native HID on B. Left/right descriptions depend on board layout.
Flash and verify B first, identify it, move it to the target PC, then flash A and
move it to the local native USB connection. Do not type test input until the target
has a suitable text editor focused and both devices are ready.

Pair-specific keys and identifiers are generated locally and never belong in Git.
The app has no runtime network service or account dependency.

## License

Project code is licensed under Apache License 2.0 with Commons Clause 1.0.
Use, modification and redistribution are permitted subject to those terms,
including preservation of required notices. It is source-available; the Commons
Clause restricts selling the software, including covered paid services, as defined
in [LICENSE](LICENSE). Third-party
dependencies retain their own licenses and notices; see [NOTICE](NOTICE).
