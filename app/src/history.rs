use crate::keymap::special_token;
use xkbcommon::xkb;

/// Application-owned document. It has no GUI editor state, clipboard, or mouse
/// operations; only physical-key presses can change it.
#[derive(Default, Debug, Clone)]
pub struct History {
    text: Vec<char>,
    caret: usize,
    revision: u64,
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
        let original_caret = self.caret;
        let original_length = self.text.len();
        // Ctrl/Alt/Super chords are represented, never executed as local editor commands.
        if modifiers & 0xdd != 0 && !translated.is_empty() {
            let prefix = if modifiers & 0x11 != 0 {
                "Ctrl+"
            } else if modifiers & 0x44 != 0 {
                "Alt+"
            } else {
                "Super+"
            };
            self.insert(&format!("[{prefix}{}]", translated.to_uppercase()));
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
                104 | 109 => self.insert(special_token(key).unwrap_or("[Nav]")),
                28 | 96 => self.insert("\n"),
                _ if !translated.is_empty() && !translated.chars().any(char::is_control) => {
                    self.insert(translated)
                }
                _ => {
                    if let Some(token) = special_token(key) {
                        self.insert(token);
                    }
                }
            }
        }
        if self.caret != original_caret || self.text.len() != original_length {
            self.revision = self.revision.wrapping_add(1);
        }
    }
}

/// libxkbcommon applies the source desktop's keyboard layout, modifier, Caps
/// Lock and compose/dead-key interpretation to history only. HID transport
/// remains raw physical usage state elsewhere.
pub struct XkbHistory {
    _context: xkb::Context,
    _keymap: xkb::Keymap,
    state: xkb::State,
}
impl XkbHistory {
    pub fn new() -> anyhow::Result<Self> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            "",
            "",
            "",
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .ok_or_else(|| anyhow::anyhow!("cannot compile XKB keymap"))?;
        let state = xkb::State::new(&keymap);
        Ok(Self {
            _context: context,
            _keymap: keymap,
            state,
        })
    }
    pub fn press(&mut self, history: &mut History, linux_key: u16, modifiers: u8) {
        let key = xkb::Keycode::new(u32::from(linux_key) + 8);
        let text = self.state.key_get_utf8(key);
        history.press(linux_key, &text, modifiers);
        self.state.update_key(key, xkb::KeyDirection::Down);
    }
    pub fn release(&mut self, linux_key: u16) {
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
        assert_eq!(h.rendered(), "[Ctrl+C]");
    }
    #[test]
    fn shortcut_tokens_include_right_side_modifiers() {
        for (modifier, expected) in [(0x10, "[Ctrl+C]"), (0x40, "[Alt+C]"), (0x80, "[Super+C]")] {
            let mut h = History::default();
            h.press(46, "c", modifier);
            assert_eq!(h.rendered(), expected);
        }
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
