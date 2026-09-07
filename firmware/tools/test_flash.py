"""Check manifest and fixture guards without connecting to hardware."""
import json
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch
import flash


class FlashTests(unittest.TestCase):
    def test_manifest_cannot_select_external_image(self):
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[2]) as work:
            build = Path(work) / "build"
            build.mkdir()
            outside = Path(work) / "outside.bin"
            outside.write_bytes(b"not firmware")
            with self.assertRaises(ValueError):
                flash.images(build, {"flash_files": {"0x0": "../outside.bin"}})

    def test_fixture_is_rejected_before_serial_access(self):
        pair = {"a": {"mac": "02:00:00:00:00:01"}}
        with patch("sys.argv", ["flash.py", "a", "--port", "unused"]), \
             patch.object(Path, "read_text", side_effect=[json.dumps(pair), '{"BRIDGE_TEST_FIXTURE":true}']), \
             patch("subprocess.run") as command, \
             patch("argparse.ArgumentParser.error", side_effect=ValueError) as error:
            with self.assertRaises(ValueError):
                flash.main()
            command.assert_not_called()
            self.assertIn("nondeployable", error.call_args.args[0])

    def test_a_requires_verified_b_before_serial_access(self):
        with patch("sys.argv", ["flash.py", "a", "--port", "unused"]), \
             patch.object(Path, "read_text", side_effect=['{}', '{}', '{}', '{"verified":false}']), \
             patch.object(flash, "images", return_value=[]), \
             patch.object(flash, "digest", return_value="pair-hash"), \
             patch("subprocess.run") as command, \
             patch("argparse.ArgumentParser.error", side_effect=ValueError) as error:
            with self.assertRaises(ValueError):
                flash.main()
            command.assert_not_called()
            self.assertIn("bridge B must", error.call_args.args[0])

    def test_wrong_board_is_never_written(self):
        pair = {"b": {"mac": "02:00:00:00:00:02"}}
        with patch("sys.argv", ["flash.py", "b", "--port", "unused"]), \
             patch.object(Path, "read_text", side_effect=[json.dumps(pair), '{}', '{}']), \
             patch.object(flash, "images", return_value=[]), \
             patch.object(flash, "digest", return_value="pair-hash"), \
             patch("subprocess.run", return_value=SimpleNamespace(stdout="MAC: 02:00:00:00:00:01")) as command, \
             patch("argparse.ArgumentParser.error", side_effect=ValueError) as error:
            with self.assertRaises(ValueError):
                flash.main()
            self.assertIn("identity does not match", error.call_args.args[0])
            self.assertEqual(command.call_count, 1)
            self.assertEqual(command.call_args.args[0][-1], "read-mac")


if __name__ == "__main__":
    unittest.main()
