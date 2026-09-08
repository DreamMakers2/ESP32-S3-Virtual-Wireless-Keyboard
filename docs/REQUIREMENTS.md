# Requirements and compatibility

## Hardware

- Two ESP32-S3 N16R8 boards with 16 MB flash and 8 MB PSRAM.
- Two data-capable USB connections for setup.
- One native USB connection from bridge A to the Linux computer during normal use, through the port marked **USB/OTG** on the PCB.
- One native USB connection from bridge B to the target during normal use, through the port marked **USB/OTG** on the PCB.
- A physical keyboard connected to the Linux computer.
- A mouse or other pointing device for entering and leaving the app's capture window.
- Onboard addressable RGB LED support. GPIO48 is the normal configuration for the boards used by this project; GPIO38 is available for compatible revisions wired that way.

The firmware configuration targets ESP32-S3 N16R8 hardware.

## Linux application

- Linux desktop with Wayland or X11.
- Rust and Cargo.
- libxkbcommon development files.
- A working OpenGL-capable graphics driver for the egui renderer.
- Access to the selected `/dev/input` keyboard device and bridge A's serial device as the normal desktop user.

The current reference environment has been verified with CachyOS / KDE Wayland,
Rust 1.98.1 / Cargo 1.98.1, libxkbcommon 1.13.2, Wayland client 1.26.0,
fontconfig 2.18.3, and libudev 261. These are tested versions rather than claims
that every listed version is a minimum compatibility requirement.

Cargo.lock defines the application's resolved Rust packages. Internet access is required when fetching build dependencies; ordinary application use has no runtime account or network-service dependency.

## Firmware toolchain

- ESP-IDF 5.5.5 with the ESP32-S3 toolchain.
- Python 3.
- CMake 3.30.9.
- esptool 5.3.1 for the maintenance and flashing helpers.

Firmware component manifests and dependency locks define the resolved embedded dependencies.

## Runtime layout

The normal bridge path is:

```text
physical keyboard → Linux app → bridge A → encrypted ESP-NOW → bridge B → USB HID target
```

Bridge A uses native USB CDC through the port marked **USB/OTG** on the PCB. Bridge B presents itself to the target as a native USB HID keyboard through its **USB/OTG**-marked PCB port. The port marked **COM/UART** on each PCB is used for flashing, identification and maintenance.
