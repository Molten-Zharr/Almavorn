use super::{App, Dialog};
use crate::input::{Key, KeyPress};

impl App {
    pub(super) fn focus_help_search(&mut self) {
        self.view.help_search.selected_all = true;
        self.view.help_search.cursor = self.view.help_search.query.chars().count();
    }

    pub(super) fn clear_help_search(&mut self) {
        let search = &mut self.view.help_search;
        search.query.clear();
        search.cursor = 0;
        search.selected_all = false;
        search.changed();
        if let Some(Dialog::Help { offset }) = &mut self.view.dialog {
            *offset = search.browse_offset;
        }
    }

    pub(super) fn help_search_text(&mut self, text: &str) {
        let Some(Dialog::Help { offset }) = &mut self.view.dialog else {
            return;
        };
        let search = &mut self.view.help_search;
        if !search.active() {
            search.browse_offset = *offset;
        }
        if search.selected_all {
            search.query.clear();
            search.cursor = 0;
        }
        search.selected_all = false;
        let text: String = text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .take(256usize.saturating_sub(search.query.chars().count()))
            .collect();
        let byte = search
            .query
            .char_indices()
            .nth(search.cursor)
            .map_or(search.query.len(), |(byte, _)| byte);
        search.query.insert_str(byte, &text);
        search.cursor += text.chars().count();
        search.changed();
        *offset = if search.active() {
            0
        } else {
            search.browse_offset
        };
    }

    pub(super) fn move_help_match(&mut self, delta: i64) {
        let search = &mut self.view.help_search;
        search.selected = super::bounded(search.selected, delta, search.matches.len());
    }

    pub(super) fn open_help_match(&mut self, index: usize) {
        let Some(target) = self.view.help_search.matches.get(index).copied() else {
            return;
        };
        self.clear_help_search();
        self.view.help_search.pending_entry = Some(target);
    }

    pub fn copy_help_search(&mut self, cut: bool) -> bool {
        if !matches!(self.view.dialog, Some(Dialog::Help { .. })) {
            return false;
        }
        if self.view.help_search.selected_all && !self.view.help_search.query.is_empty() {
            self.view.clipboard_text = Some(self.view.help_search.query.clone());
            if cut {
                self.clear_help_search();
            }
        }
        true
    }

    pub(super) fn help_search_key(&mut self, key: KeyPress) -> bool {
        if key.ctrl && matches!(key.key, Key::Char('a' | 'f')) {
            self.focus_help_search();
            return true;
        }
        let active = self.view.help_search.active();
        match key.key {
            Key::Enter if active => self.open_help_match(self.view.help_search.selected),
            Key::Up | Key::Down if active => {
                self.move_help_match(if key.key == Key::Up { -1 } else { 1 })
            }
            Key::Tab if active => self.move_help_match(if key.shift { -1 } else { 1 }),
            Key::PageUp | Key::PageDown if active => {
                let page = self.view.help_search.page_size.max(1) as i64;
                self.move_help_match(if key.key == Key::PageUp { -page } else { page });
            }
            Key::Home | Key::End if active && key.ctrl => {
                self.move_help_match(if key.key == Key::Home {
                    i64::MIN
                } else {
                    i64::MAX
                });
            }
            Key::Left | Key::Right | Key::Home | Key::End if active => {
                let search = &mut self.view.help_search;
                search.cursor = match key.key {
                    Key::Left => search.cursor.saturating_sub(1),
                    Key::Right => (search.cursor + 1).min(search.query.chars().count()),
                    Key::Home => 0,
                    _ => search.query.chars().count(),
                };
                search.selected_all = false;
            }
            Key::Backspace | Key::Delete => {
                if self.view.help_search.selected_all {
                    self.clear_help_search();
                    return true;
                }
                let search = &mut self.view.help_search;
                let index = if key.key == Key::Backspace {
                    search.cursor.checked_sub(1)
                } else {
                    Some(search.cursor)
                };
                if let Some(index) = index
                    && let Some((byte, c)) = search.query.char_indices().nth(index)
                {
                    search.query.replace_range(byte..byte + c.len_utf8(), "");
                    search.cursor = index;
                    search.changed();
                    if let Some(Dialog::Help { offset }) = &mut self.view.dialog {
                        *offset = if search.active() {
                            0
                        } else {
                            search.browse_offset
                        };
                    }
                }
            }
            Key::Char(c) if !key.ctrl && !key.alt => self.help_search_text(&c.to_string()),
            _ => return false,
        }
        true
    }
}
