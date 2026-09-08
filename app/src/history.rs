use crate::keymap::{key_label, modifier_label, special_token};
use std::collections::BTreeMap;
use xkbcommon::xkb;

/// Application-owned document. It has no GUI editor state, clipboard, or mouse
/// operations; only physical-key presses can change it.
#[derive(Default, Debug, Clone)]
pub struct History {
    text: Vec<char>,
    caret: usize,
    revision: u64,
    pending_modifiers: BTreeMap<u16, bool>,
}
impl History {
    pub fn rendered(&self) -> String {
        self.text.iter().collect()
    }
    pub fn rendered_with_caret(&self) -> String {
        let mut value = self.rendered();
        let offset = self.text[..self.caret]
            .iter()
            .map(|value| value.len_utf8())
            .sum();
        value.insert_str(offset, "|");
        value
    }
    /// The logical line containing the physical-key caret. The UI uses this
    /// to keep incoming text visible without turning history into an editor.
    pub fn rendered_lines_with_caret(&self) -> (Vec<String>, usize) {
        let caret_line = self.text[..self.caret]
            .iter()
            .filter(|value| **value == '\n')
            .count();
        (
            self.rendered_with_caret()
                .split('\n')
                .map(str::to_owned)
                .collect(),
            caret_line,
        )
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    fn insert(&mut self, value: &str) {
        let chars: Vec<_> = value.chars().collect();
        self.text
            .splice(self.caret..self.caret, chars.iter().copied());
        self.caret += chars.len();
    }
    /// Keep visual shortcut and special-key markers separate from neighboring
    /// text. Only the ASCII-space run touching this insertion is normalized;
    /// all other document whitespace and editor behavior remains intact.
    fn insert_token(&mut self, token: &str) {
        let before = self.text[..self.caret]
            .iter()
            .rposition(|value| *value != ' ')
            .map_or(0, |index| index + 1);
        let after = self.caret
            + self.text[self.caret..]
                .iter()
                .position(|value| *value != ' ')
                .unwrap_or(self.text.len() - self.caret);
        let value = format!(" {token} ");
        let chars: Vec<_> = value.chars().collect();
        self.text.splice(before..after, chars.iter().copied());
        self.caret = before + chars.len();
    }
    fn backspace(&mut self) {
        if self.caret > 0 {
            self.caret -= 1;
            self.text.remove(self.caret);
        }
    }
    fn delete(&mut self) {
        if self.caret < self.text.len() {
            self.text.remove(self.caret);
        }
    }
    fn line_start(&self) -> usize {
        self.text[..self.caret]
            .iter()
            .rposition(|c| *c == '\n')
            .map_or(0, |i| i + 1)
    }
    fn line_end(&self) -> usize {
        self.text[self.caret..]
            .iter()
            .position(|c| *c == '\n')
            .map_or(self.text.len(), |i| self.caret + i)
    }
    fn vertical(&mut self, direction: isize) {
        let start = self.line_start();
        let column = self.caret - start;
        if direction < 0 {
            if start == 0 {
                return;
            }
            let previous_end = start - 1;
            let previous_start = self.text[..previous_end]
                .iter()
                .rposition(|c| *c == '\n')
                .map_or(0, |i| i + 1);
            self.caret = (previous_start + column).min(previous_end);
        } else {
            let end = self.line_end();
            if end == self.text.len() {
                return;
            }
            let next_start = end + 1;
            let next_end = self.text[next_start..]
                .iter()
                .position(|c| *c == '\n')
                .map_or(self.text.len(), |i| next_start + i);
            self.caret = (next_start + column).min(next_end);
        }
    }
    pub fn press(&mut self, key: u16, translated: &str, modifiers: u8) {
        if modifier_label(key).is_some() {
            self.pending_modifiers.entry(key).or_insert(false);
            return;
        }

        let original_caret = self.caret;
        let original_length = self.text.len();
        for used in self.pending_modifiers.values_mut() {
            *used = true;
        }

        let ctrl = modifiers & 0x11 != 0;
        let alt = modifiers & 0x44 != 0;
        let shift = modifiers & 0x22 != 0;
        let super_key = modifiers & 0x88 != 0;
        // Ctrl/Alt/Super chords are represented, never executed as local editor commands.
        // Shift is included only with a shortcut modifier, so Shift+A remains normal text.
        if ctrl || alt || super_key {
            let mut parts = Vec::with_capacity(5);
            if ctrl {
                parts.push("Ctrl");
            }
            if alt {
                parts.push("Alt");
            }
            if shift {
                parts.push("Shift");
            }
            if super_key {
                parts.push("Super");
            }
            let label = key_label(key)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Key{key}"));
            parts.push(&label);
            self.insert_token(&format!("[{}]", parts.join("+")));
        } else {
            match key {
                14 => self.backspace(),
                111 => self.delete(),
                105 => self.caret = self.caret.saturating_sub(1),
                106 => self.caret = (self.caret + 1).min(self.text.len()),
                102 => self.caret = self.line_start(),
                107 => self.caret = self.line_end(),
                103 => self.vertical(-1),
                108 => self.vertical(1),
                104 | 109 => self.insert_token(special_token(key).unwrap_or("[Nav]")),
                28 | 96 => self.insert("\n"),
                57 if translated.is_empty() => self.insert(" "),
                _ if !translated.is_empty() && !translated.chars().any(char::is_control) => {
                    self.insert(translated)
                }
                _ => {
                    if let Some(token) = special_token(key) {
                        self.insert_token(token);
                    }
                }
            }
        }
        if self.caret != original_caret || self.text.len() != original_length {
            self.revision = self.revision.wrapping_add(1);
        }
    }
    pub fn release(&mut self, key: u16) {
        let Some(used) = self.pending_modifiers.remove(&key) else {
            return;
        };
        if used {
            return;
        }
        let original_caret = self.caret;
        let original_length = self.text.len();
        if let Some(label) = modifier_label(key) {
            self.insert_token(&format!("[{label}]"));
        }
        if self.caret != original_caret || self.text.len() != original_length {
            self.revision = self.revision.wrapping_add(1);
        }
    }
    /// Capture boundaries discard incomplete modifier taps without changing the
    /// accumulated display history.
    pub fn clear_pending_modifiers(&mut self) {
        self.pending_modifiers.clear();
    }
}

/// libxkbcommon applies the source desktop's keyboard layout, modifier, Caps
/// Lock and compose/dead-key interpretation to history only. HID transport
/// remains raw physical usage state elsewhere.
pub struct XkbHistory {
    _context: xkb::Context,
    keymap: xkb::Keymap,
    state: xkb::State,
}
impl XkbHistory {
    pub fn new() -> anyhow::Result<Self> {
        Self::with_layout("")
    }
    fn with_layout(layout: &str) -> anyhow::Result<Self> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            "",
            "",
            layout,
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .ok_or_else(|| anyhow::anyhow!("cannot compile XKB keymap"))?;
        let state = xkb::State::new(&keymap);
        Ok(Self {
            _context: context,
            keymap,
            state,
        })
    }
    pub fn press(&mut self, history: &mut History, linux_key: u16, modifiers: u8) {
        let key = xkb::Keycode::new(u32::from(linux_key) + 8);
        let text = self.state.key_get_utf8(key);
        history.press(linux_key, &text, self.shortcut_modifiers(key, modifiers));
        self.state.update_key(key, xkb::KeyDirection::Down);
    }
    /// A right Alt which XKB consumes as Level3 (AltGr) is text input, not an
    /// Alt shortcut. The raw HID modifier still reaches the target unchanged.
    fn shortcut_modifiers(&self, key: xkb::Keycode, modifiers: u8) -> u8 {
        let level3 = self.keymap.mod_get_index(xkb::MOD_NAME_ISO_LEVEL3_SHIFT);
        if modifiers & 0x40 != 0 && self.state.mod_index_is_consumed(key, level3) {
            modifiers & !0x40
        } else {
            modifiers
        }
    }
    pub fn release(&mut self, history: &mut History, linux_key: u16) {
        history.release(linux_key);
        self.state.update_key(
            xkb::Keycode::new(u32::from(linux_key) + 8),
            xkb::KeyDirection::Up,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_editor_operations_do_not_use_clipboard() {
        let mut h = History::default();
        h.press(0, "abc", 0);
        h.press(105, "", 0);
        h.press(14, "", 0);
        assert_eq!(h.rendered(), "ac");
    }
    #[test]
    fn shortcut_becomes_a_token() {
        let mut h = History::default();
        h.press(46, "c", 1);
        assert_eq!(h.rendered(), " [Ctrl+C] ");
    }
    #[test]
    fn shortcut_tokens_include_right_side_modifiers() {
        for (modifier, expected) in [(0x10, "[Ctrl+C]"), (0x40, "[Alt+C]"), (0x80, "[Super+C]")] {
            let mut h = History::default();
            h.press(46, "c", modifier);
            assert_eq!(h.rendered(), format!(" {expected} "));
        }
    }
    #[test]
    fn tokens_keep_one_boundary_space() {
        let mut h = History::default();
        h.press(0, "Hello", 0);
        h.press(15, "\t", 0);
        h.press(0, "World", 0);
        h.press(59, "", 0);
        h.press(60, "", 0);
        assert_eq!(h.rendered(), "Hello [Tab] World [F1] [F2] ");
    }
    #[test]
    fn token_insertions_normalize_boundary_spaces_at_the_live_caret() {
        let mut h = History::default();
        h.press(0, "Hello World", 0);
        h.caret = 6;
        h.press(15, "\t", 0);
        h.press(0, "again ", 0);
        assert_eq!(h.rendered(), "Hello [Tab] again World");
    }
    #[test]
    fn standalone_modifiers_are_side_specific_and_chords_suppress_them() {
        let mut h = History::default();
        for key in [29, 97, 42, 54, 56, 100, 125, 126] {
            h.press(key, "", 0);
            h.release(key);
        }
        assert_eq!(
            h.rendered(),
            " [LCtrl] [RCtrl] [LShift] [RShift] [LAlt] [RAlt] [LSuper] [RSuper] "
        );

        let mut h = History::default();
        h.press(29, "", 0);
        h.press(56, "", 1);
        h.press(111, "", 0x05);
        h.release(111);
        h.release(56);
        h.release(29);
        assert_eq!(h.rendered(), " [Ctrl+Alt+Delete] ");
    }
    #[test]
    fn every_function_key_has_a_token() {
        let mut h = History::default();
        for key in [59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 87, 88] {
            h.press(key, "", 0);
        }
        assert_eq!(
            h.rendered(),
            " [F1] [F2] [F3] [F4] [F5] [F6] [F7] [F8] [F9] [F10] [F11] [F12] "
        );
    }
    #[test]
    fn shortcut_labels_never_use_translated_control_characters() {
        let mut h = History::default();
        h.press(15, "\t", 0x04);
        h.press(15, "\t", 0x01);
        h.press(57, " ", 0x01);
        h.press(36, "\n", 0x04);
        assert_eq!(h.rendered(), " [Alt+Tab] [Ctrl+Tab] [Ctrl+Space] [Alt+J] ");
    }
    #[test]
    fn xkb_uses_real_control_and_shift_state() {
        let mut h = History::default();
        let mut xkb = XkbHistory::with_layout("us").unwrap();
        xkb.press(&mut h, 29, 0);
        xkb.press(&mut h, 36, 0x01);
        xkb.release(&mut h, 36);
        xkb.release(&mut h, 29);
        xkb.press(&mut h, 42, 0);
        xkb.press(&mut h, 30, 0x02);
        xkb.release(&mut h, 30);
        xkb.release(&mut h, 42);
        assert_eq!(h.rendered(), " [Ctrl+J] A");
    }
    #[test]
    fn xkb_treats_consumed_altgr_as_text() {
        let mut h = History::default();
        let mut xkb = XkbHistory::with_layout("us(intl)").unwrap();
        xkb.press(&mut h, 100, 0);
        xkb.press(&mut h, 18, 0x40);
        xkb.release(&mut h, 18);
        xkb.release(&mut h, 100);
        assert_eq!(h.rendered(), "é");
    }
    #[test]
    fn arrows_move_the_logical_caret() {
        let mut h = History::default();
        h.press(0, "abc", 0);
        h.press(28, "", 0);
        h.press(0, "def", 0);
        h.press(103, "", 0);
        h.press(0, "X", 0);
        assert_eq!(h.rendered(), "abcX\ndef");
    }
    #[test]
    fn caret_line_follows_vertical_navigation_for_scrolling_history() {
        let mut h = History::default();
        h.press(0, "first", 0);
        h.press(28, "", 0);
        h.press(0, "second", 0);
        h.press(103, "", 0);
        let (lines, caret_line) = h.rendered_lines_with_caret();
        assert_eq!(lines, ["first|", "second"]);
        assert_eq!(caret_line, 0);
        assert!(h.revision() > 0);
    }
}
