# Requirements and validation status

## Hardware observed

- Two ESP32-S3 chips, revision v0.2, with 16 MB quad flash at 3.3 V and embedded
  8 MB AP_3v3 PSRAM, confirmed by esptool over COM/UART.
- Two data-capable USB connections for setup. In use, one native USB connection
  joins A to Linux and one joins B to the target.
- Keychron K5 source keyboard and a mouse that remains available to change focus.
- Onboard addressable RGB LED. GPIO48 is visibly verified on this pair;
  reference-board revisions may instead use GPIO38.

The minimum supported firmware configuration is the tested N16R8 target. Smaller
flash/PSRAM variants have not been qualified. No minimum radio range is claimed.

## Software observed

- Linux KDE Wayland desktop; CachyOS is the intended source platform.
- Rust 1.98.1 / Cargo 1.98.1 available for the application build.
- libxkbcommon 1.13.2, Wayland client 1.26.0, fontconfig 2.18.3 and libudev 261.
- Project-local ESP-IDF 5.5.5, its selected ESP32-S3 toolchain and Python environment;
  CMake 3.30.9. System esptool 5.3.1 handles the maintenance tools.

Cargo.lock and the firmware dependency locks define the build's resolved packages.
Native runtime shared-library dependencies must be checked against the actual
release executable before packaging. A GPU/driver capable of the selected egui
renderer is required; exact minimum GPU, RAM and storage have not been measured.
Internet access is needed to fetch build dependencies, not for ordinary app use.

## Verified acceptance

- Both firmware targets build. Both boards were flashed in B-first order; all
  image digests and production role/radio startup were verified.
- Source is Linux KDE Wayland with a Keychron K5. The user confirmed remote
  typing, modifiers/shortcuts, repeated activation after focus loss, and sustained
  connection. The exact target OS/version has not yet been recorded.
- The user confirmed keyboard operation in the actual target BIOS/UEFI. Its
  vendor/version has not been recorded; this is not a universal BIOS claim.
- The user accepted steady paused green on both devices and app, Pause/Activate
  menu, history fading, overlay width, dot placement and scrollbar behavior.
- Native dark/normal and light/small/debug renders were inspected with populated
  history. App release tests cover protocol framing, physical key state, focus
  revocation, repeated activation, live serial disconnection and timeout recovery.
- The release executable resolves its linked libraries on the source machine:
  libxkbcommon, libgcc_s, libm, libc and the x86-64 ELF loader. The native window
  also needs the appropriate desktop/graphics runtime libraries.

## Unmeasured or unrecorded items

- Explicit Caps/Num Lock feedback and unplug/reconnect/suspend recovery results.
- Measured startup, latency/jitter and disconnect release deadlines.
- Exact target OS and BIOS/UEFI versions, if broader compatibility is to be claimed.

The user accepted the final activation fix and requested closeout without further
tests or reviews. The items above remain explicitly unverified; they are not
release blockers for this accepted hobby-project scope. Unmeasured timing limits
and untested target versions are not certified by a successful build or a USB
boot-keyboard descriptor.
