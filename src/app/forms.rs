use super::{App, Dialog};
use crate::{
    input::{Key, KeyPress},
    lyrics::Lyric,
    model::Track,
};
use anyhow::Result;

#[derive(Clone)]
pub enum FormPurpose {
    MoveFilesFolder(super::MoveFiles),
    ModNew,
    ModInstall,
    ModCode(crate::mods::Mod),
    ModSettings(crate::mods::Mod),
    ModManifest(crate::mods::Mod),
    ModExport(crate::mods::Mod),
    ServiceConnection(crate::services::Connection),
    SpotifyConnect(crate::services::Connection, usize),
    ServiceHeaders(crate::services::Connection),
    ModFields(crate::mods::Mod, Vec<String>),
    ServiceSearch,
    DownloadSearch,
    DownloadNew(Option<String>),
    DownloadSettings,
    LyricsQuery(Track),
    LyricsEdit(i64, Lyric),
    LyricsExport(Lyric),
    LyricsKey(crate::lyrics::Provider),
}

#[derive(Clone)]
pub struct FormField {
    pub label: String,
    pub value: String,
    pub secret: bool,
    pub multiline: bool,
    pub checkbox: bool,
}
impl FormField {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            secret: false,
            multiline: false,
            checkbox: false,
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
    SpotifyDashboard,
    SpotifyRedirect,
    SpotifyNext,
    Field(usize),
    ToggleField(usize),
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
        } else if let FormPurpose::SpotifyConnect(_, step) = &mut form.purpose
            && *step > 0
        {
            *step -= 1;
            form.error = None;
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
        if form.discard || matches!(form.purpose, FormPurpose::SpotifyConnect(_, 0 | 1)) {
            return;
        }
        let field = &mut form.fields[form.selected];
        if field.checkbox {
            return;
        }
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
        if let FormPurpose::SpotifyConnect(_, step) = form.purpose {
            if key.ctrl {
                match key.key {
                    Key::Char('1') => return self.form_action(FormAction::SpotifyDashboard),
                    Key::Char('2') => return self.form_action(FormAction::SpotifyRedirect),
                    _ => {},
                }
            }
            if step < 2 {
                if key.key == Key::Enter { return self.form_action(FormAction::SpotifyNext); }
                return Ok(());
            }
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
            Key::Enter if field.checkbox => {
                field.value = (field.value != "true").to_string();
            }
            Key::Enter if field.multiline && !key.ctrl => self.form_text("\n"),
            Key::Enter if !key.ctrl && form.selected + 1 < field_count => {
                let index = form.selected + 1;
                self.form_action(FormAction::Field(index))?;
            }
            Key::Enter => self.form_action(FormAction::Save)?,
            Key::Up | Key::Down | Key::PageUp | Key::PageDown if field.multiline => {
                let characters = field.value.chars().collect::<Vec<_>>();
                let position = form.cursor.min(characters.len());
                let start = characters[..position]
                    .iter()
                    .rposition(|c| *c == '\n')
                    .map_or(0, |i| i + 1);
                let column = position - start;
                let mut target = start;
                let steps = if matches!(key.key, Key::PageUp | Key::PageDown) {
                    10
                } else {
                    1
                };
                for _ in 0..steps {
                    if matches!(key.key, Key::Up | Key::PageUp) {
                        if target == 0 {
                            break;
                        }
                        target = characters[..target - 1]
                            .iter()
                            .rposition(|c| *c == '\n')
                            .map_or(0, |i| i + 1);
                    } else {
                        if let Some(length) = characters[target..].iter().position(|c| *c == '\n') {
                            target += length + 1;
                        } else {
                            break;
                        }
                    }
                }
                let length = characters[target..]
                    .iter()
                    .position(|c| *c == '\n')
                    .unwrap_or(characters.len() - target);
                form.cursor = target + column.min(length);
                form.selected_all = false;
            }
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
            Key::Backspace | Key::Delete if !field.checkbox => {
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
            FormAction::SpotifyDashboard if matches!(form.purpose, FormPurpose::SpotifyConnect(..)) => {
                form.error = None;
                if let Err(error) = super::services::open_service_page("https://developer.spotify.com/dashboard") {
                    form.error = Some(error.to_string());
                }
            }
            FormAction::SpotifyRedirect if matches!(form.purpose, FormPurpose::SpotifyConnect(..)) => {
                form.error = None;
                self.view.clipboard_text = Some(crate::services::SPOTIFY_REDIRECT.into());
            }
            FormAction::SpotifyNext => {
                if let FormPurpose::SpotifyConnect(_, step) = &mut form.purpose {
                    *step = (*step + 1).min(2);
                    form.error = None;
                }
            }
            FormAction::ToggleField(index)
                if index < form.fields.len() && form.fields[index].checkbox =>
            {
                form.selected = index;
                form.fields[index].value = (form.fields[index].value != "true").to_string();
            }
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
            FormPurpose::SpotifyConnect(previous, step) => {
                anyhow::ensure!(*step == 2, "Перейдите к шагу с Client ID.");
                let mut connection = previous.clone();
                connection.client_id = form.fields[0].value.trim().to_owned();
                self.connect_spotify(connection)?;
            }
            FormPurpose::MoveFilesFolder(view) => {
                let folder = std::path::PathBuf::from(form.fields[0].value.trim()).canonicalize()?;
                anyhow::ensure!(folder.is_dir(), "Выберите существующую папку.");
                let mut view = view.clone();
                view.selected = view.folders.iter().position(|p| *p == folder).unwrap_or_else(|| {
                    view.folders.push(folder);
                    view.folders.len() - 1
                });
                self.view.dialog = Some(Dialog::MoveFiles(view));
            }
            FormPurpose::ModFields(previous, keys) => {
                let mut value = previous.clone();
                for (key, field) in keys.iter().zip(&form.fields) {
                    let original = &previous.settings[key];
                    value.settings[key] = if original.is_string() {
                        serde_json::Value::String(field.value.clone())
                    } else {
                        let parsed: serde_json::Value = serde_json::from_str(&field.value)?;
                        anyhow::ensure!(
                            (original.is_boolean() && parsed.is_boolean())
                                || (original.is_number() && parsed.is_number()),
                            "Проверьте значение поля {}",
                            field.label
                        );
                        parsed
                    };
                }
                value.validate()?;
                self.save_mod(value);
            }
            FormPurpose::ModNew => {
                let value = crate::mods::Mod::new(form.fields[0].value.trim().into());
                value.validate()?;
                self.save_mod(value);
            }
            FormPurpose::ModInstall => self.import_mod(form.fields[0].value.trim().into()),
            FormPurpose::ModCode(previous) => {
                let mut value = previous.clone();
                value.script = form.fields[0].value.clone();
                value.validate()?;
                self.save_mod(value);
            }
            FormPurpose::ModSettings(previous) => {
                let mut value = previous.clone();
                value.settings = serde_json::from_str(&form.fields[0].value)?;
                value.validate()?;
                self.save_mod(value);
            }
            FormPurpose::ModManifest(previous) => {
                let mut value = previous.clone();
                value.manifest = serde_json::from_str(&form.fields[0].value)?;
                anyhow::ensure!(
                    value.manifest.id == previous.manifest.id,
                    "Идентификатор установленного мода менять нельзя. Создайте копию через архив."
                );
                value.validate()?;
                self.save_mod(value);
            }
            FormPurpose::ModExport(value) => {
                self.export_mod(value.clone(), form.fields[0].value.trim().into())
            }
            FormPurpose::ServiceConnection(previous) => {
                let mut connection = previous.clone();
                connection.name = form.fields[0].value.trim().into();
                connection.client_id = form
                    .fields
                    .get(1)
                    .map(|f| f.value.trim().to_owned())
                    .unwrap_or_default();
                self.service_save(connection);
            }
            FormPurpose::ServiceHeaders(connection) => {
                self.service_headers(connection.clone(), form.fields[0].value.clone())
            }
            FormPurpose::ServiceSearch => {
                self.service_search(Some(form.fields[0].value.trim().into()), 0)?
            }
            FormPurpose::DownloadSearch => {
                self.search_downloads(form.fields[0].value.trim().into())
            }
            FormPurpose::DownloadNew(previous) => {
                let job = crate::downloads::Download::new(
                    form.fields[0].value.trim().into(),
                    form.fields[2].value.trim().into(),
                    std::path::PathBuf::from(form.fields[1].value.trim()),
                    self.downloads.config.engine,
                )?;
                self.queue_download(job, previous.as_deref())?;
            }
            FormPurpose::DownloadSettings => {
                let mut config = self.downloads.config.clone();
                config.directory = crate::downloads::destination_folder(
                    std::path::Path::new(form.fields[0].value.trim()),
                )?;
                config.cobalt_url = form.fields[1].value.trim().into();
                let key = (!form.fields[2].value.trim().is_empty())
                    .then(|| form.fields[2].value.trim().to_owned());
                self.configure_downloads(config, key);
            }
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
            FormPurpose::LyricsKey(provider) => {
                self.lyrics_set_key(*provider, Some(form.fields[0].value.clone()))?;
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
                self.update_panel_from_dialog();
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
