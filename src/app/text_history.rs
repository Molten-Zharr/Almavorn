use super::{App, Dialog};
use crate::input::{Key, KeyPress};
use std::collections::VecDeque;

/// In-memory history for one editable field, never a file or playlist undo.
#[derive(Clone, Default)]
pub struct TextHistory {
    undo: VecDeque<(String, usize)>,
    redo: VecDeque<(String, usize)>,
}
impl TextHistory {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    fn push(stack: &mut VecDeque<(String, usize)>, value: (String, usize)) {
        stack.push_back(value);
        let mut bytes: usize = stack.iter().map(|(text, _)| text.len()).sum();
        while stack.len() > 64 || bytes > 4 * 1024 * 1024 {
            if let Some((text, _)) = stack.pop_front() {
                bytes -= text.len();
            } else {
                break;
            }
        }
    }
    pub fn record(&mut self, before: String, cursor: usize, after: &str) {
        if before != after {
            Self::push(&mut self.undo, (before, cursor));
            self.redo.clear();
        }
    }
    pub fn restore(&mut self, redo: bool, value: &mut String, cursor: &mut usize) -> bool {
        let (source, destination) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let Some((text, position)) = source.pop_back() else {
            return false;
        };
        Self::push(destination, (std::mem::replace(value, text), *cursor));
        *cursor = position.min(value.chars().count());
        true
    }
}

impl App {
    pub(super) fn edit_text_history(&mut self, redo: bool) {
        let saving = self.writing_tags();
        match &mut self.view.dialog {
            Some(Dialog::Text(dialog)) => {
                let mut cursor = dialog
                    .color_picker
                    .as_ref()
                    .map_or_else(|| dialog.text.chars().count(), |color| color.hex_cursor);
                if dialog.history.restore(redo, &mut dialog.text, &mut cursor) {
                    dialog.selected_all = false;
                    dialog.sync_color_text();
                    if let Some(color) = &mut dialog.color_picker {
                        color.hex_cursor = cursor;
                        color.hex_anchor = cursor;
                    }
                }
            }
            Some(Dialog::Form(form))
                if !form.discard
                    && !matches!(form.purpose, super::FormPurpose::SpotifyConnect(_, 0 | 1)) =>
            {
                if let Some(field) = form.fields.get_mut(form.selected)
                    && !field.checkbox
                    && field
                        .history
                        .restore(redo, &mut field.value, &mut form.cursor)
                {
                    form.selected_all = false;
                    form.error = None;
                }
            }
            Some(Dialog::Tags(editor))
                if !saving && !editor.confirm && !editor.discard && !editor.key_picker =>
            {
                if let Some(edit) = &mut editor.edit
                    && edit.history[edit.input].restore(
                        redo,
                        &mut edit.values[edit.input],
                        &mut edit.cursor,
                    )
                {
                    edit.selected_all = false;
                    editor.error = None;
                }
            }
            _ => {}
        }
    }
    pub(super) fn text_history_key(&mut self, key: KeyPress) -> bool {
        let key = key.normalized();
        if !key.ctrl || key.alt || !matches!(key.key, Key::Char('z' | 'y')) {
            return false;
        }
        let editing = match &self.view.dialog {
            Some(Dialog::Text(_)) => true,
            Some(Dialog::Form(form)) => !form.discard,
            Some(Dialog::Tags(editor)) => {
                editor.edit.is_some() && !editor.key_picker && !editor.confirm && !editor.discard
            }
            _ => false,
        };
        if editing {
            self.edit_text_history(key.key == Key::Char('y') || key.shift);
        }
        editing
    }
}
