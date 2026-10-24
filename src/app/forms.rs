use super::{App, Dialog};
use crate::{
    input::{Key, KeyPress},
    lyrics::Lyric,
    model::Track,
};
use anyhow::Result;

#[derive(Clone)]
pub enum FormPurpose {
    LyricsQuery(Track),
    LyricsEdit(i64, Lyric),
    LyricsExport(Lyric),
}

#[derive(Clone)]
pub struct FormField {
    pub label: String,
    pub value: String,
    pub secret: bool,
    pub multiline: bool,
}
impl FormField {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            secret: false,
            multiline: false,
        }
    }
}

#[derive(Clone)]
pub struct Form {
    pub title: String,
    pub description: String,
    pub fields: Vec<FormField>,
    pub selected: usize,
    pub cursor: usize,
    pub selected_all: bool,
    pub error: Option<String>,
    pub discard: bool,
    pub purpose: FormPurpose,
    pub back: Option<Box<Dialog>>,
    original: Vec<String>,
}

#[derive(Clone, Copy)]
pub enum FormAction {
    Field(usize),
    Save,
    Back,
    Discard,
}

impl App {
    pub(super) fn open_form(
        &mut self,
        title: &str,
        description: &str,
        fields: Vec<FormField>,
        purpose: FormPurpose,
    ) {
        let cursor = fields.first().map_or(0, |f| f.value.chars().count());
        let original = fields.iter().map(|f| f.value.clone()).collect();
        let back = self.view.dialog.take().map(Box::new);
        self.view.dialog = Some(Dialog::Form(Box::new(Form {
            title: title.into(),
            description: description.into(),
            fields,
            selected: 0,
            cursor,
            selected_all: true,
            error: None,
            discard: false,
            purpose,
            back,
            original,
        })));
    }

    pub(super) fn form_close(&mut self) -> bool {
        let Some(Dialog::Form(form)) = &mut self.view.dialog else {
            return false;
        };
        if form.discard {
            form.discard = false;
        } else if form
            .fields
            .iter()
            .map(|f| &f.value)
            .ne(form.original.iter())
        {
            form.discard = true;
        } else {
            let back = form.back.take();
            self.view.dialog = back.map(|d| *d);
        }
        true
    }

    pub(super) fn form_text(&mut self, text: &str) {
        let Some(Dialog::Form(form)) = &mut self.view.dialog else {
            return;
        };
        if form.discard {
            return;
        }
        let field = &mut form.fields[form.selected];
        if field.value.len().saturating_add(text.len()) > 256 * 1024 {
            form.error = Some(
                self.settings
                    .preferences
                    .language
                    .text("Text is too long", "Текст слишком длинный")
                    .into(),
            );
            return;
        }
        if form.selected_all {
            field.value.clear();
            form.cursor = 0;
            form.selected_all = false;
        }
        let text = text
            .chars()
            .filter(|c| !c.is_control() || field.multiline && (*c == '\n' || *c == '\t'))
            .collect::<String>();
        let offset = field
            .value
            .char_indices()
            .nth(form.cursor)
            .map_or(field.value.len(), |(i, _)| i);
        field.value.insert_str(offset, &text);
        form.cursor += text.chars().count();
        form.error = None;
    }

    pub(super) fn form_key(&mut self, key: KeyPress) -> Result<()> {
        let Some(Dialog::Form(form)) = &mut self.view.dialog else {
            return Ok(());
        };
        if form.discard {
            return match key.key {
                Key::Char('d') => self.form_action(FormAction::Discard),
                Key::Enter => self.form_action(FormAction::Back),
                _ => Ok(()),
            };
        }
        if key.ctrl && key.key == Key::Char('s') {
            return self.form_action(FormAction::Save);
        }
        if key.ctrl && key.key == Key::Char('a') {
            form.selected_all = true;
            return Ok(());
        }
        if key.key == Key::Tab {
            let length = form.fields.len();
            let index = (form.selected + if key.shift { length - 1 } else { 1 }) % length;
            return self.form_action(FormAction::Field(index));
        }
        let field_count = form.fields.len();
        let field = &mut form.fields[form.selected];
        match key.key {
            Key::Enter if field.multiline && !key.ctrl => self.form_text("\n"),
            Key::Enter if !key.ctrl && form.selected + 1 < field_count => {
                let index = form.selected + 1;
                self.form_action(FormAction::Field(index))?;
            }
            Key::Enter => self.form_action(FormAction::Save)?,
            Key::Left => {
                form.cursor = form.cursor.saturating_sub(1);
                form.selected_all = false;
            }
            Key::Right => {
                form.cursor = (form.cursor + 1).min(field.value.chars().count());
                form.selected_all = false;
            }
            Key::Home => {
                form.cursor = 0;
                form.selected_all = false;
            }
            Key::End => {
                form.cursor = field.value.chars().count();
                form.selected_all = false;
            }
            Key::Backspace | Key::Delete => {
                if form.selected_all {
                    field.value.clear();
                    form.cursor = 0;
                    form.selected_all = false;
                } else {
                    let index = if key.key == Key::Backspace {
                        form.cursor.checked_sub(1)
                    } else {
                        Some(form.cursor)
                    };
                    if let Some(index) = index
                        && let Some((byte, c)) = field.value.char_indices().nth(index)
                    {
                        field.value.replace_range(byte..byte + c.len_utf8(), "");
                        form.cursor = index;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn form_action(&mut self, action: FormAction) -> Result<()> {
        if matches!(action, FormAction::Back) {
            self.form_close();
            return Ok(());
        }
        let Some(Dialog::Form(form)) = &mut self.view.dialog else {
            return Ok(());
        };
        match action {
            FormAction::Field(index) if index < form.fields.len() => {
                form.selected = index;
                form.cursor = form.fields[index].value.chars().count();
                form.selected_all = true;
            }
            FormAction::Discard => {
                let back = form.back.take();
                self.view.dialog = back.map(|d| *d);
            }
            FormAction::Save => {
                let form = form.clone();
                if let Err(error) = self.apply_form(&form)
                    && let Some(Dialog::Form(current)) = &mut self.view.dialog
                {
                    current.error = Some(format!("{error:#}"));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_form(&mut self, form: &Form) -> Result<()> {
        match &form.purpose {
            FormPurpose::LyricsQuery(track) => {
                self.start_lyrics(
                    crate::lyrics::Query {
                        track: track.clone(),
                        artist: form.fields[0].value.trim().into(),
                        title: form.fields[1].value.trim().into(),
                    },
                    true,
                );
            }
            FormPurpose::LyricsEdit(track, lyric) => {
                let mut lyric = lyric.clone();
                if lyric.synced.is_empty() {
                    lyric.plain = form.fields[0].value.clone();
                } else {
                    lyric.synced = form.fields[0].value.clone();
                }
                lyric.instrumental = false;
                crate::lyrics::save(&self.data_directory(), *track, &lyric)?;
                self.lyrics.results = vec![lyric];
                self.lyrics.saved = true;
                self.view.dialog = Some(Dialog::Lyrics(super::lyrics::LyricsView::content()));
            }
            FormPurpose::LyricsExport(lyric) => {
                crate::lyrics::export(std::path::Path::new(form.fields[0].value.trim()), lyric)?;
                self.view.dialog = form.back.clone().map(|d| *d);
                self.message(
                    self.text("Lyrics saved to the file", "Текст песни сохранен в файл")
                        .into(),
                );
            }
        }
        Ok(())
    }

    pub(crate) fn data_directory(&self) -> std::path::PathBuf {
        self.store
            .path
            .parent()
            .expect("library directory")
            .to_owned()
    }
}
