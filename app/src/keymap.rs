//! Linux evdev keycodes to USB HID Keyboard/Keypad usages.
/// Returns (HID usage, modifier bit), where modifier usages are represented by
/// the modifier byte and therefore return usage zero.
pub fn hid_usage(key: u16) -> Option<(u8, u8)> {
    let usage = match key {
        1 => 0x29,
        2..=10 => 0x1e + (key as u8 - 2),
        11 => 0x27,
        12 => 0x2d,
        13 => 0x2e,
        14 => 0x2a,
        15 => 0x2b,
        16 => 0x14,
        17 => 0x1a,
        18 => 0x08,
        19 => 0x15,
        20 => 0x17,
        21 => 0x1c,
        22 => 0x18,
        23 => 0x0c,
        24 => 0x12,
        25 => 0x13,
        26 => 0x2f,
        27 => 0x30,
        28 => 0x28,
        29 => return Some((0, 0x01)),
        30 => 0x04,
        31 => 0x16,
        32 => 0x07,
        33 => 0x09,
        34 => 0x0a,
        35 => 0x0b,
        36 => 0x0d,
        37 => 0x0e,
        38 => 0x0f,
        39 => 0x33,
        40 => 0x34,
        41 => 0x35,
        42 => return Some((0, 0x02)),
        43 => 0x31,
        44 => 0x1d,
        45 => 0x1b,
        46 => 0x06,
        47 => 0x19,
        48 => 0x05,
        49 => 0x11,
        50 => 0x10,
        51 => 0x36,
        52 => 0x37,
        53 => 0x38,
        54 => return Some((0, 0x20)),
        55 => 0x55,
        56 => return Some((0, 0x04)),
        57 => 0x2c,
        58 => 0x39,
        59..=68 => 0x3a + (key as u8 - 59),
        69 => 0x53,
        70 => 0x47,
        71 => 0x5f,
        72 => 0x60,
        73 => 0x61,
        74 => 0x56,
        75 => 0x5c,
        76 => 0x5d,
        77 => 0x5e,
        78 => 0x57,
        79 => 0x59,
        80 => 0x5a,
        81 => 0x5b,
        82 => 0x62,
        83 => 0x63,
        87 => 0x44,
        88 => 0x45,
        96 => 0x58,
        97 => return Some((0, 0x10)),
        98 => 0x64,
        99 => 0x46,
        100 => return Some((0, 0x40)),
        102 => 0x4a,
        103 => 0x52,
        104 => 0x4b,
        105 => 0x50,
        106 => 0x4f,
        107 => 0x4d,
        108 => 0x51,
        109 => 0x4e,
        110 => 0x49,
        111 => 0x4c,
        119 => 0x48,
        125 => return Some((0, 0x08)),
        126 => return Some((0, 0x80)),
        _ => return None,
    };
    Some((usage, 0))
}

pub fn special_token(key: u16) -> Option<&'static str> {
    Some(match key {
        1 => "[Esc]",
        15 => "[Tab]",
        28 | 96 => "[Enter]",
        57 => " ",
        59..=68 => match key {
            59 => "[F1]",
            60 => "[F2]",
            61 => "[F3]",
            62 => "[F4]",
            63 => "[F5]",
            64 => "[F6]",
            65 => "[F7]",
            66 => "[F8]",
            67 => "[F9]",
            _ => "[F10]",
        },
        87 => "[F11]",
        88 => "[F12]",
        102 => "[Home]",
        103 => "[Up]",
        104 => "[PgUp]",
        105 => "[Left]",
        106 => "[Right]",
        107 => "[End]",
        108 => "[Down]",
        109 => "[PgDn]",
        110 => "[Insert]",
        111 => "[Delete]",
        119 => "[Pause]",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_modifier_sides_use_their_hid_bits() {
        for (key, bit) in [(29, 0x01), (42, 0x02), (56, 0x04), (125, 0x08),
                           (97, 0x10), (54, 0x20), (100, 0x40), (126, 0x80)] {
            assert_eq!(hid_usage(key), Some((0, bit)), "evdev key {key}");
        }
    }
    #[test]
    fn modifiers_do_not_enter_bitmap() {
        assert_eq!(hid_usage(29), Some((0, 1)));
        assert_eq!(hid_usage(30), Some((4, 0)));
    }
}
