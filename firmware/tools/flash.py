#!/usr/bin/env python3
"""Flash one provisioned board, verify every image, then record a private receipt."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import time

FIRMWARE = Path(__file__).resolve().parents[1]
PRIVATE = FIRMWARE / "config" / "private"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def images(build, manifest):
    result = []
    for address, name in manifest["flash_files"].items():
        image = (build / name).resolve()
        if not image.is_relative_to(build.resolve()) or not image.is_file():
            raise ValueError("flash manifest references a missing or external image")
        int(address, 0)
        result.append((address, image))
    if not result:
        raise ValueError("empty flash manifest")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("role", choices=("a", "b"))
    parser.add_argument("--port", required=True, help="persistent COM/UART device path")
    args = parser.parse_args()
    build = FIRMWARE / f"bridge-{args.role}" / "build"
    pair_path = PRIVATE / "pair.json"
    try:
        config = json.loads(pair_path.read_text())
        sdkconfig = json.loads((build / "config" / "sdkconfig.json").read_text())
        if sdkconfig.get("BRIDGE_TEST_FIXTURE", False):
            raise ValueError("refusing to flash a nondeployable test-fixture build")
        manifest = json.loads((build / "flasher_args.json").read_text())
        artifacts = images(build, manifest)
        pair_hash = digest(pair_path)
        if args.role == "a":
            receipt = json.loads((PRIVATE / "flash-b.json").read_text())
            if receipt.get("pair_sha256") != pair_hash or not receipt.get("verified"):
                raise ValueError("bridge B must be flashed and verified for this pair first")
    except (OSError, ValueError, KeyError) as error:
        parser.error(str(error))
    command = [sys.executable, "-m", "esptool", "--chip", "esp32s3", "--port", args.port]
    probe = subprocess.run(command + ["read-mac"], capture_output=True, text=True, check=True)
    found = re.search(r"MAC:\s*([0-9a-fA-F:]{17})", probe.stdout)
    if not found or found.group(1).lower() != config[args.role]["mac"].lower():
        parser.error("connected chip identity does not match the selected bridge role")
    print(f"Identity confirmed for bridge {args.role.upper()}; flashing.", flush=True)
    files = [value for address, image in artifacts for value in (address, str(image))]
    flash_options = [value.replace("_", "-") if value.startswith("--") else value
                     for value in manifest.get("write_flash_args", [])]
    subprocess.run(command + ["write-flash"] + flash_options + files, check=True)
    subprocess.run(command + ["verify-flash"] + flash_options + files, check=True)
    receipt = {"role": args.role, "verified": True, "pair_sha256": pair_hash,
               "verified_at_unix": int(time.time()),
               "images": {str(image.relative_to(build)): digest(image) for _, image in artifacts}}
    (PRIVATE / f"flash-{args.role}.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print("Flash bytes verified.")


if __name__ == "__main__":
    main()
