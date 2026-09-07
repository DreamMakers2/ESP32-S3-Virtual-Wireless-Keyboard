#!/usr/bin/env python3
"""Run a bounded maintenance command over COM/UART (never native USB CDC)."""
import argparse
import time
import serial


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", required=True)
    parser.add_argument("command", choices=("status", "identify on", "identify off"))
    parser.add_argument("--seconds", type=float, default=3.0)
    args = parser.parse_args()
    if not 0 < args.seconds <= 10:
        parser.error("capture duration must be between 0 and 10 seconds")
    with serial.Serial(port=None, baudrate=115200, timeout=0.1) as uart:
        uart.dtr = False
        uart.rts = False
        uart.port = args.port
        uart.open()
        uart.reset_input_buffer()
        # Some USB/UART bridges pulse reset when opened despite preset DTR/RTS.
        # Let firmware initialize its maintenance task before sending commands.
        time.sleep(3.0)
        uart.write((args.command + "\n").encode("ascii"))
        uart.flush()
        deadline = time.monotonic() + args.seconds
        while time.monotonic() < deadline:
            line = uart.readline()
            if line:
                print(line.decode("utf-8", errors="replace").rstrip())


if __name__ == "__main__":
    main()
