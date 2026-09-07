# Setup

1. Clone this repository and open its root folder in VS Code. Git Source Control
   discovers the root `.git` directory; no editor extension settings are required.
2. Follow [firmware setup](../firmware/README.md) to install the pinned ESP-IDF
   toolchain locally. Install application build dependencies as documented in
   [app/README.md](../app/README.md). Package installation is an explicit setup step;
   the ordinary app launcher does not install anything.
3. Connect both boards through their labelled COM/UART ports. Record persistent
   `/dev/serial/by-id/` paths. Run `.tools/python/bin/python -m esptool --chip esp32s3 --port <UART-path>
   flash-id` against each to inspect its flash and connection identity. Use
   `read-mac` to record the distinct MACs for provisioning; firmware startup
   separately checks the required PSRAM.
4. Confirm each board's RGB GPIO. Generate the private pair configuration with
   `.tools/python/bin/python firmware/tools/provision.py --a-mac <A-MAC> --b-mac <B-MAC>
   --a-rgb <GPIO> --b-rgb <GPIO>`. Supply actual values locally; do not commit them.
   The command refuses to replace an existing pair or silently rotate its keys.
5. Build both firmware targets. Test-fixture builds are nondeployable. Flash B
   first using `.tools/python/bin/python firmware/tools/flash.py b --port <B-UART-path>`. The tool
   checks its chip identity, writes the images, reads/verifies them, and records
   a private verification receipt. Check UART startup separately.
6. Send `.tools/python/bin/python firmware/tools/uart.py --port <B-UART-path> 'identify on'`.
   Confirm the repeating two-pulse magenta indicator. Move this board to the
   target PC using its USB/OTG port and confirm USB keyboard enumeration.
7. Flash and verify A with the same tool using role `a`. Confirm its one-pulse
   magenta identification, then move its cable to USB/OTG on the Linux computer.
8. Configure the app's persistent keyboard/CDC paths and matching keyboard layout.
   Grant only the required input/serial access; do not run the GUI as root.
   Temporary device ACLs may be lost after unplugging. Follow the app's permissions
   documentation for an explicitly approved persistent setup if desired.
9. Launch `app/run.sh`, focus a blank editor on the target, and click the app's
   input surface only when ready to test. Clicking away stops capture. Test desktop
   behavior before entering BIOS/UEFI; do not change BIOS settings during testing.

## Troubleshooting

- **Permission denied:** check access to the resolved serial/input device. A valid
  symlink does not imply permission; use a device-specific ACL or documented rule.
- **No native USB device:** check the printed USB/OTG connector label and data cable.
  COM/UART firmware verification does not test the native USB interface.
- **Dark RGB LED:** verify the board revision/GPIO and five-minute status sleep.
  Do not diagnose radio failure from the LED alone; use UART status.
- **Wrong characters:** match the configured source XKB layout and target layout.
  The target receives physical HID usages, not translated text.
- **Lost connection:** capture must remain paused until the chain is ready and you
  explicitly click to activate again. Check both boards' current status.

The [requirements page](REQUIREMENTS.md) distinguishes observed hardware/tooling
from pending runtime acceptance. It is the authority for current compatibility.
