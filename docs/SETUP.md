# Setup

This guide takes you from a fresh checkout to a working wireless keyboard bridge.

You will use two ESP32-S3 boards:

- **Bridge A** stays with the Linux computer and talks to the desktop app over USB CDC.
- **Bridge B** stays with the remote target and appears there as a USB keyboard.

Always identify the USB connectors by the markings printed on the PCB:

- Port marked **COM/UART**: flashing, identification and maintenance.
- Port marked **USB/OTG**: normal bridge operation.

## 1. Get the project and install the tools

Clone the repository and open its root folder in a terminal or VS Code.

Follow the toolchain instructions in [firmware/README.md](../firmware/README.md), then install the Linux application dependencies from [app/README.md](../app/README.md).

The firmware setup creates project-local tools under `.tools/`, so the repository can use the expected ESP-IDF, CMake and esptool versions without changing the normal app launch flow.

## 2. Connect both ESP32-S3 boards through the COM/UART-marked port

For initial setup, connect both boards through the port marked **COM/UART** on the PCB.

Find their persistent Linux device paths:

```sh
ls -l /dev/serial/by-id/
```

Use those `/dev/serial/by-id/...` paths in the commands below instead of temporary names such as `/dev/ttyACM0`.

## 3. Read each board's MAC address

Run these commands once for each board, replacing `<UART-path>` with its persistent serial path:

```sh
.tools/python/bin/python -m esptool --chip esp32s3 --port <UART-path> flash-id
.tools/python/bin/python -m esptool --chip esp32s3 --port <UART-path> read-mac
```

Choose which board will be **A** and which will be **B**, then keep their MAC addresses handy for the next step.

## 4. Create the private pair configuration

Generate a private configuration for the two boards:

```sh
.tools/python/bin/python firmware/tools/provision.py \
  --a-mac <A-MAC> \
  --b-mac <B-MAC> \
  --a-rgb <GPIO> \
  --b-rgb <GPIO>
```

Use GPIO48 for the usual onboard RGB configuration. Use GPIO38 only for a compatible board revision wired that way.

The generated pair configuration contains the device identities and ESP-NOW keys. It stays local and is ignored by Git. The provisioning tool will not silently replace an existing pair or rotate its keys.

## 5. Build both firmware images

From the repository root:

```sh
firmware/tools/idf.sh -C firmware/bridge-b build
firmware/tools/idf.sh -C firmware/bridge-a build
```

## 6. Flash bridge B first

Flash the board you assigned as bridge B while it is connected through the port marked **COM/UART** on the PCB:

```sh
.tools/python/bin/python firmware/tools/flash.py b --port <B-UART-path>
```

The flash helper checks the board identity, writes the firmware and verifies the written images.

Turn on its identification pattern:

```sh
.tools/python/bin/python firmware/tools/uart.py --port <B-UART-path> 'identify on'
```

Bridge B uses a repeating **two-pulse magenta** identification pattern.

Once identified:

1. Disconnect bridge B from the **COM/UART**-marked port.
2. Move it to the remote target machine.
3. Connect the target to the port marked **USB/OTG** on the PCB.
4. The target should see it as a USB keyboard.

## 7. Flash bridge A

Flash the remaining board as bridge A through the port marked **COM/UART** on the PCB:

```sh
.tools/python/bin/python firmware/tools/flash.py a --port <A-UART-path>
```

You can identify it the same way:

```sh
.tools/python/bin/python firmware/tools/uart.py --port <A-UART-path> 'identify on'
```

Bridge A uses a repeating **one-pulse magenta** identification pattern.

Then disconnect it from the **COM/UART**-marked port and reconnect it to the Linux computer through the port marked **USB/OTG** on the PCB.

## 8. Configure the Linux app

Find the persistent path for your physical keyboard and bridge A:

```sh
ls -l /dev/input/by-id/*-event-kbd /dev/serial/by-id/*
```

Follow [app/README.md](../app/README.md) to create the app configuration and set:

- `keyboard_path` to the physical keyboard you want the app to capture.
- `cdc_path` to the serial device exposed by bridge A while it is connected through the port marked **USB/OTG** on the PCB.
- the keyboard layout to match the source and target layout.

Give your normal desktop user access to the selected input and serial devices. Do **not** run the GUI as root. See [Linux permissions](../app/config/linux-permissions.md) for the recommended setup.

## 9. Launch and try it

Start the app from the repository root:

```sh
app/run.sh
```

For the first test, open a blank text editor on the remote target.

When both bridges are connected and the app shows the link as ready, click the app's input surface to activate keyboard capture. Type a few characters and confirm they appear on the target.

Click anywhere outside the app window to stop capture and return the keyboard to normal local use.

Once ordinary desktop input is working, you can use the same bridge for BIOS/UEFI or other remote-machine input.

## Troubleshooting

- **Permission denied:** make sure your desktop user can access the configured `/dev/input` and `/dev/serial` devices. A valid `/dev/.../by-id` path can still point to a device your user cannot open.
- **No USB device on the target:** make sure bridge B is connected through the port marked **USB/OTG** on the PCB, not the **COM/UART**-marked port, and use a data-capable USB cable.
- **Bridge A is not appearing in the app:** make sure A is connected through the port marked **USB/OTG** on the PCB and that `cdc_path` points to its persistent serial path.
- **Dark RGB LED:** check the configured RGB GPIO and remember that the ordinary status indication sleeps after five minutes.
- **Wrong characters:** make sure the source XKB layout and target keyboard layout match. The target receives physical HID usages.
- **Connection lost:** capture stays paused until the bridge chain is ready again and you explicitly reactivate the app.

For hardware and software prerequisites, see [Requirements and compatibility](REQUIREMENTS.md).
