#!/usr/bin/env python3
"""Generate one private pair configuration; never print cryptographic material."""
import argparse
import json
import os
from pathlib import Path
import re
import secrets

PRIVATE = Path(__file__).resolve().parents[1] / "config" / "private"


def mac(value):
    if not re.fullmatch(r"(?:[0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}", value):
        raise argparse.ArgumentTypeError("expected six colon-separated MAC bytes")
    octets = bytes.fromhex(value.replace(":", ""))
    if octets[0] & 1 or octets == bytes(6):
        raise argparse.ArgumentTypeError("expected a nonzero unicast MAC address")
    return value.lower()


def initializer(value):
    return "{" + ", ".join(f"0x{x:02x}" for x in bytes.fromhex(value.replace(":", ""))) + "}"


def render(config):
    lines = ["/* Generated private pair configuration. Never commit. */", "#pragma once",
             f"#define BRIDGE_PAIR_CHANNEL {config['channel']}",
             f"#define BRIDGE_PMK {initializer(config['pmk'])}",
             f"#define BRIDGE_LMK {initializer(config['lmk'])}",
             "#if defined(BRIDGE_ROLE_A) && defined(BRIDGE_ROLE_B)",
             '#error "Select exactly one bridge role"',
             "#elif defined(BRIDGE_ROLE_A)"]
    for role, peer in (("a", "b"), ("b", "a")):
        if role == "b":
            lines.append("#elif defined(BRIDGE_ROLE_B)")
        lines += [f"#define BRIDGE_OWN_MAC {initializer(config[role]['mac'])}",
                  f"#define BRIDGE_PEER_MAC {initializer(config[peer]['mac'])}",
                  f'#define BRIDGE_DEVICE_LABEL "Keyboard Bridge {role.upper()}"',
                  f"#define BRIDGE_RGB_GPIO {config[role]['rgb_gpio']}"]
    return "\n".join(lines + ["#else", '#error "Bridge role is required"', "#endif", ""])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--a-mac", type=mac, required=True)
    parser.add_argument("--b-mac", type=mac, required=True)
    parser.add_argument("--a-rgb", type=int, choices=(38, 48), required=True)
    parser.add_argument("--b-rgb", type=int, choices=(38, 48), required=True)
    parser.add_argument("--channel", type=int, choices=range(1, 12), default=6)
    args = parser.parse_args()
    if args.a_mac == args.b_mac:
        parser.error("the boards must have distinct MAC addresses")
    if (PRIVATE / "pair.json").exists() or (PRIVATE / "pair_config.h").exists():
        parser.error("pair configuration already exists; reuse it instead of regenerating keys")
    config = {"channel": args.channel, "pmk": secrets.token_hex(16), "lmk": secrets.token_hex(16),
              "a": {"mac": args.a_mac, "rgb_gpio": args.a_rgb},
              "b": {"mac": args.b_mac, "rgb_gpio": args.b_rgb}}
    PRIVATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    for name, value in (("pair.json", json.dumps(config, indent=2) + "\n"),
                        ("pair_config.h", render(config))):
        fd = os.open(PRIVATE / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w") as output:
            output.write(value)
    print("Created private pair configuration. Keep both files local and reuse them for rebuilds.")


if __name__ == "__main__":
    main()
