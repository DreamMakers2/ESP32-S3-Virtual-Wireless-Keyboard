"""Provisioning format tests use synthetic identities and never write secrets."""
import argparse
import unittest
from provision import mac, render


class ProvisionTests(unittest.TestCase):
    def test_rejects_invalid_identity(self):
        for value in ("bad", "00:00:00:00:00:00", "01:00:00:00:00:01"):
            with self.assertRaises(argparse.ArgumentTypeError):
                mac(value)

    def test_roles_use_opposite_peers(self):
        config = {"channel": 6, "pmk": "11" * 16, "lmk": "22" * 16,
                  "a": {"mac": "02:00:00:00:00:01", "rgb_gpio": 38},
                  "b": {"mac": "02:00:00:00:00:02", "rgb_gpio": 48}}
        header = render(config)
        a, b = header.split("#elif defined(BRIDGE_ROLE_B)")
        self.assertIn("BRIDGE_PEER_MAC {0x02, 0x00, 0x00, 0x00, 0x00, 0x02}", a)
        self.assertIn("BRIDGE_PEER_MAC {0x02, 0x00, 0x00, 0x00, 0x00, 0x01}", b)
        self.assertIn("BRIDGE_RGB_GPIO 38", a)
        self.assertIn("BRIDGE_RGB_GPIO 48", b)


if __name__ == "__main__":
    unittest.main()
