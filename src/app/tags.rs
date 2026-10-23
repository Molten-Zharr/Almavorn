use super::{
    App, Dialog,
    background::{Background, background, poll},
    database::DatabaseOutcome,
};
use crate::{
    input::{Key, KeyPress},
    model::{PlaylistKind, Track},
    tags::{Cover, Document, Field, field_keys, item_value_text, value_from_text},
};
use anyhow::{Context, Result, ensure};
use lofty::{
    picture::{Picture, PictureType},
    tag::{ItemKey, ItemValue, TagItem},
};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct TagsState {
    load: Option<Background<Document>>,
    pub saving: bool,
    pub recovery_required: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TagsTab {
    File,
    Fields,
    Extras,
    Covers,
    Cleanup,
}

#[derive(Clone, Copy)]
pub enum TagAction {
    FileInput(FileInput),
    FormatName,
    Move,
    ChooseDestination,
    MoveConfirm,
    Format(i64),
    Tab(TagsTab),
    Row(usize),
    Remove(usize),
    Add,
    Edit,
    Keys,
    ChooseKey(usize),
    Input(usize),
    ValueKind,
    Apply,
    Back,
    Save,
    Confirm,
    ClearAll,
    ClearBlock(usize),
    ClearFields,
    ClearCovers,
    Discard,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileInput {
    Name,
    Template,
    Folder,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MovePicker {
    Playlists,
    Folders,
}

#[derive(Clone)]
pub struct FileDestination {
    pub id: i64,
    pub name: String,
    pub mode: crate::model::Mode,
    pub folders: Vec<PathBuf>,
}

#[derive(Clone)]
pub struct TagEdit {
    pub file_input: Option<FileInput>,
    pub index: Option<usize>,
    pub cover: bool,
    pub extra: bool,
    pub key: ItemKey,
    pub values: [String; 3],
    pub input: usize,
    pub cursor: usize,
    pub selected_all: bool,
    pub value_kind: usize,
}

impl TagEdit {
    fn select(&mut self, input: usize) {
        self.input = input.min(if self.file_input.is_some() {
            0
        } else if self.extra {
            1
        } else {
            2
        });
        self.cursor = self.values[self.input].chars().count();
        self.selected_all = false;
    }
}

#[derive(Clone)]
pub struct TagEditor {
    pub file_stem: String,
    pub name_template: String,
    pub move_picker: Option<MovePicker>,
    pub destinations: Vec<FileDestination>,
    pub move_target: Option<usize>,
    pub move_folder: String,
    pub move_confirm: bool,
    pub desk: i64,
    pub track: Track,
    pub shared: usize,
    pub document: Option<Document>,
    pub block: usize,
    pub tab: TagsTab,
    pub selected: usize,
    pub offset: usize,
    pub edit: Option<TagEdit>,
    pub key_picker: bool,
    pub confirm: bool,
    pub discard: bool,
    pub error: Option<String>,
}

impl TagEditor {
    pub fn changed(&self) -> bool {
        self.document.as_ref().is_some_and(Document::changed)
            || self
                .track
                .path
                .file_stem()
                .is_none_or(|stem| stem != self.file_stem.as_str())
    }
    pub fn new_filename(&self) -> Result<String> {
        let stem = self.file_stem.trim();
        ensure!(
            !stem.is_empty()
                && stem != "."
                && stem != ".."
                && !stem
                    .chars()
                    .any(|c| c.is_control() || c == '/' || c == '\\'),
            "Введите имя файла без пути и расширения."
        );
        let mut name = stem.to_owned();
        if let Some(extension) = self.track.path.extension() {
            name.push('.');
            name.push_str(&extension.to_string_lossy());
        }
        ensure!(name.len() <= 255, "Имя файла слишком длинное.");
        Ok(name)
    }
    pub fn destination(&self) -> Option<&FileDestination> {
        self.move_target
            .and_then(|index| self.destinations.get(index))
    }
    pub fn target_path(&self) -> Result<PathBuf> {
        let folder = if self.move_confirm {
            ensure!(
                !self.move_folder.trim().is_empty(),
                "Выберите папку назначения."
            );
            Path::new(self.move_folder.trim())
        } else {
            self.track.path.parent().context("У файла нет папки.")?
        };
        ensure!(
            folder.is_absolute() && folder.is_dir(),
            "Выберите существующую папку по полному пути."
        );
        Ok(folder.canonicalize()?.join(self.new_filename()?))
    }
    pub fn accepts_text(&self) -> bool {
        !self.key_picker && !self.confirm && !self.discard && self.edit.is_some()
    }
    pub fn destructive(&self) -> bool {
        self.document.as_ref().is_some_and(Document::destructive)
    }
    pub fn filename(&self) -> String {
        self.track
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }
    pub fn length(&self) -> usize {
        if let Some(picker) = self.move_picker {
            return match picker {
                MovePicker::Playlists => self.destinations.len(),
                MovePicker::Folders => self.destination().map_or(0, |d| d.folders.len()),
            };
        }
        let Some(document) = &self.document else {
            return 0;
        };
        let block = &document.blocks[self.block];
        if self.key_picker {
            return field_keys(block.kind).len();
        }
        match self.tab {
            TagsTab::File => 0,
            TagsTab::Fields => block.fields.len(),
            TagsTab::Extras => block.extras.len(),
            TagsTab::Covers => block.covers.len(),
            TagsTab::Cleanup => document.blocks.len(),
        }
    }
}

impl App {
    pub fn writing_tags(&self) -> bool {
        self.tags.saving
    }

    pub(super) fn open_tag_editor(&mut self) -> Result<()> {
        ensure!(
            !self.busy() && !self.tags.saving,
            "Дождитесь завершения текущей операции."
        );
        let playlist = self.playlist().context("Выберите сортировочный стол.")?;
        ensure!(
            playlist.kind == PlaylistKind::SortingDesk,
            "Редактор тегов доступен на сортировочном столе."
        );
        let track = self.entry().context("Выберите композицию.")?.track.clone();
        let shared = self
            .library
            .playlists
            .iter()
            .filter(|playlist| {
                playlist
                    .entries
                    .iter()
                    .any(|entry| entry.track.id == track.id)
            })
            .count();
        let editor = TagEditor {
            file_stem: track
                .path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            name_template: "{artist} - {title}".into(),
            move_picker: None,
            destinations: Vec::new(),
            move_target: None,
            move_folder: String::new(),
            move_confirm: false,
            desk: playlist.id,
            track: track.clone(),
            shared,
            document: None,
            block: 0,
            tab: TagsTab::Fields,
            selected: 0,
            offset: 0,
            edit: None,
            key_picker: false,
            confirm: false,
            discard: false,
            error: None,
        };
        self.tags.load = Some(background(self.runtime(), move || {
            Document::read(&track.path)
        }));
        self.view.dialog = Some(Dialog::Tags(Box::new(editor)));
        Ok(())
    }

    pub(super) fn tick_tags(&mut self) {
        let Some(result) = self.tags.load.as_mut().and_then(poll) else {
            return;
        };
        self.tags.load = None;
        if let Some(Dialog::Tags(editor)) = &mut self.view.dialog {
            match result {
                Ok(document) => editor.document = Some(document),
                Err(error) => editor.error = Some(format!("{error:#}")),
            }
        }
    }

    pub(super) fn reload_tag_editor(&mut self, track: Track) {
        if let Some(Dialog::Tags(editor)) = &mut self.view.dialog {
            editor.track = track.clone();
            editor.file_stem = track
                .path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            editor.document = None;
            editor.confirm = false;
            editor.move_confirm = false;
            editor.move_picker = None;
            editor.tab = TagsTab::File;
            editor.block = 0;
            editor.selected = 0;
            editor.offset = 0;
            editor.error = None;
            self.tags.load = Some(background(self.runtime(), move || {
                Document::read(&track.path)
            }));
        }
    }

    pub(super) fn close_tags(&mut self) -> bool {
        let Some(Dialog::Tags(editor)) = &mut self.view.dialog else {
            return false;
        };
        if self.tags.saving {
            return true;
        }
        if editor.confirm || editor.discard {
            editor.confirm = false;
            editor.discard = false;
            editor.move_confirm = false;
        } else if editor.key_picker {
            editor.key_picker = false;
            editor.selected = 0;
        } else if editor.edit.take().is_some() {
            // Input is a draft until Apply is pressed.
        } else if let Some(picker) = editor.move_picker {
            editor.move_picker = (picker == MovePicker::Folders).then_some(MovePicker::Playlists);
            editor.selected = 0;
            editor.offset = 0;
        } else if editor.changed() {
            editor.discard = true;
        } else {
            self.tags.load = None;
            self.view.dialog = None;
        }
        true
    }

    pub(super) fn tags_text(&mut self, text: &str) {
        if self.tags.saving {
            return;
        }
        let Some(Dialog::Tags(editor)) = &mut self.view.dialog else {
            return;
        };
        if editor.key_picker || editor.discard || editor.confirm {
            return;
        }
        if let Some(edit) = &mut editor.edit {
            let value = &mut edit.values[edit.input];
            if edit.selected_all {
                value.clear();
                edit.cursor = 0;
                edit.selected_all = false;
            }
            let offset = value
                .char_indices()
                .nth(edit.cursor)
                .map_or(value.len(), |(i, _)| i);
            let text: String = text
                .chars()
                .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
                .collect();
            value.insert_str(offset, &text);
            edit.cursor += text.chars().count();
        }
    }

    pub(super) fn tags_scroll(&mut self, direction: i64) {
        let Some(Dialog::Tags(editor)) = &mut self.view.dialog else {
            return;
        };
        if editor.edit.is_some() && !editor.key_picker || editor.confirm || editor.discard {
            return;
        }
        editor.selected = super::bounded(editor.selected, direction, editor.length());
        if editor.move_picker == Some(MovePicker::Folders)
            && let Some(folder) = editor
                .destination()
                .and_then(|p| p.folders.get(editor.selected))
        {
            editor.move_folder = folder.display().to_string();
        }
    }

    pub(super) fn tags_key(&mut self, key: KeyPress) -> Result<()> {
        if self.tags.saving {
            return Ok(());
        }
        let Some(Dialog::Tags(editor)) = &mut self.view.dialog else {
            return Ok(());
        };
        if editor.discard {
            if key.key == Key::Enter {
                self.tag_action(TagAction::Discard)?;
            }
            return Ok(());
        }
        if editor.confirm {
            if key.key == Key::Enter {
                self.tag_action(TagAction::Confirm)?;
            }
            return Ok(());
        }
        if editor.key_picker {
            match key.key {
                Key::Enter => {
                    let index = editor.selected;
                    self.tag_action(TagAction::ChooseKey(index))?;
                }
                Key::Up => self.tags_scroll(-1),
                Key::Down => self.tags_scroll(1),
                Key::PageUp => self.tags_scroll(-10),
                Key::PageDown => self.tags_scroll(10),
                _ => {}
            }
            return Ok(());
        }
        if editor.move_picker.is_some() && editor.edit.is_none() {
            match key.key {
                Key::Enter => self.tag_action(TagAction::ChooseDestination)?,
                Key::Up => self.tags_scroll(-1),
                Key::Down => self.tags_scroll(1),
                Key::PageUp => self.tags_scroll(-10),
                Key::PageDown => self.tags_scroll(10),
                Key::Char('f') => self.tag_action(TagAction::FileInput(FileInput::Folder))?,
                _ => {}
            }
            return Ok(());
        }
        if let Some(edit) = &mut editor.edit {
            if key.ctrl && key.key == Key::Char('a') {
                edit.selected_all = true;
                return Ok(());
            }
            if key.key == Key::F(10) || key.ctrl && key.key == Key::Char('j') {
                self.tags_text("\n");
                return Ok(());
            }
            match key.key {
                Key::Tab => {
                    let count = if edit.file_input.is_some() {
                        1
                    } else if edit.extra {
                        2
                    } else {
                        3
                    };
                    edit.select((edit.input + if key.shift { count - 1 } else { 1 }) % count);
                }
                Key::Enter => self.tag_action(TagAction::Apply)?,
                Key::Left => {
                    edit.cursor = edit.cursor.saturating_sub(1);
                    edit.selected_all = false;
                }
                Key::Right => {
                    edit.cursor = (edit.cursor + 1).min(edit.values[edit.input].chars().count());
                    edit.selected_all = false;
                }
                Key::Home => {
                    edit.cursor = 0;
                    edit.selected_all = false;
                }
                Key::End => {
                    edit.cursor = edit.values[edit.input].chars().count();
                    edit.selected_all = false;
                }
                Key::Backspace | Key::Delete => {
                    let value = &mut edit.values[edit.input];
                    if edit.selected_all {
                        value.clear();
                        edit.cursor = 0;
                        edit.selected_all = false;
                    } else {
                        let index = if key.key == Key::Backspace {
                            edit.cursor.checked_sub(1)
                        } else {
                            Some(edit.cursor)
                        };
                        if let Some(index) = index
                            && let Some((start, c)) = value.char_indices().nth(index)
                        {
                            value.replace_range(start..start + c.len_utf8(), "");
                            edit.cursor = index;
                        }
                    }
                }
                _ => {}
            }
            return Ok(());
        }
        match key.key {
            Key::Char('s') if key.ctrl => self.tag_action(TagAction::Save)?,
            Key::Insert if editor.tab == TagsTab::File => {
                self.tag_action(TagAction::FileInput(FileInput::Name))?
            }
            Key::Insert => self.tag_action(TagAction::Add)?,
            Key::Enter if editor.tab == TagsTab::File => {
                self.tag_action(TagAction::FileInput(FileInput::Name))?
            }
            Key::Enter => self.tag_action(TagAction::Edit)?,
            Key::Char('m') => self.tag_action(TagAction::Move)?,
            Key::Char('f') => self.tag_action(TagAction::Tab(TagsTab::File))?,
            Key::Char('t') if editor.tab == TagsTab::File => {
                self.tag_action(TagAction::FileInput(FileInput::Template))?
            }
            Key::Char('g') if editor.tab == TagsTab::File => {
                self.tag_action(TagAction::FormatName)?
            }
            Key::Char(' ') | Key::Delete if editor.tab != TagsTab::File => {
                let row = editor.selected;
                let action = if editor.tab == TagsTab::Cleanup {
                    TagAction::ClearBlock(row)
                } else {
                    TagAction::Remove(row)
                };
                self.tag_action(action)?;
            }
            Key::Up => self.tags_scroll(-1),
            Key::Down => self.tags_scroll(1),
            Key::PageUp => self.tags_scroll(-10),
            Key::PageDown => self.tags_scroll(10),
            Key::Left => self.tag_action(TagAction::Format(-1))?,
            Key::Right => self.tag_action(TagAction::Format(1))?,
            Key::Tab => {
                let tab = match editor.tab {
                    TagsTab::File => TagsTab::Fields,
                    TagsTab::Fields => TagsTab::Extras,
                    TagsTab::Extras => TagsTab::Covers,
                    TagsTab::Covers => TagsTab::Cleanup,
                    TagsTab::Cleanup => TagsTab::File,
                };
                self.tag_action(TagAction::Tab(tab))?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn tag_action(&mut self, action: TagAction) -> Result<()> {
        if self.tags.saving {
            return Ok(());
        }
        // Keep errors inside the editor without losing the user's draft.
        let result = self.tag_action_inner(action);
        if let Err(error) = result {
            if let Some(Dialog::Tags(editor)) = &mut self.view.dialog {
                editor.error = Some(format!("{error:#}"));
            } else {
                return Err(error);
            }
        }
        Ok(())
    }

    fn tag_action_inner(&mut self, action: TagAction) -> Result<()> {
        if matches!(action, TagAction::Back) {
            self.close_tags();
            return Ok(());
        }
        if matches!(action, TagAction::Discard) {
            self.view.dialog = None;
            self.tags.load = None;
            return Ok(());
        }
        if matches!(action, TagAction::Confirm) {
            return self.save_file_tags();
        }
        if matches!(action, TagAction::Move) {
            let destinations = self
                .library
                .playlists
                .iter()
                .filter(|p| p.kind == PlaylistKind::Normal)
                .map(|p| {
                    let mut folders = p.folders.clone();
                    folders.sort();
                    folders.dedup();
                    FileDestination {
                        id: p.id,
                        name: p.name.clone(),
                        mode: p.mode,
                        folders,
                    }
                })
                .collect::<Vec<_>>();
            ensure!(
                !destinations.is_empty(),
                "Сначала создайте плейлист для переноса."
            );
            if let Some(Dialog::Tags(editor)) = &mut self.view.dialog {
                editor.destinations = destinations;
                editor.move_picker = Some(MovePicker::Playlists);
                editor.move_confirm = false;
                editor.selected = 0;
                editor.offset = 0;
                editor.error = None;
            }
            return Ok(());
        }
        let Some(Dialog::Tags(editor)) = &mut self.view.dialog else {
            return Ok(());
        };
        editor.error = None;
        let Some(document) = &mut editor.document else {
            return Ok(());
        };
        match action {
            TagAction::FileInput(input) => {
                let value = match input {
                    FileInput::Name => editor.file_stem.clone(),
                    FileInput::Template => editor.name_template.clone(),
                    FileInput::Folder => {
                        ensure!(editor.move_target.is_some(), "Сначала выберите плейлист.");
                        editor.move_folder.clone()
                    }
                };
                editor.edit = Some(TagEdit {
                    file_input: Some(input),
                    index: None,
                    cover: false,
                    extra: false,
                    key: ItemKey::TrackTitle,
                    cursor: value.chars().count(),
                    values: [value, String::new(), String::new()],
                    input: 0,
                    selected_all: true,
                    value_kind: 0,
                });
            }
            TagAction::FormatName => {
                editor.file_stem =
                    format_filename(&editor.name_template, &editor.track.path, document)?;
            }
            TagAction::ChooseDestination => match editor.move_picker {
                Some(MovePicker::Playlists) => {
                    ensure!(
                        editor.selected < editor.destinations.len(),
                        "Выберите плейлист."
                    );
                    editor.move_target = Some(editor.selected);
                    editor.move_folder = editor
                        .destination()
                        .and_then(|p| p.folders.first())
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    editor.move_picker = Some(MovePicker::Folders);
                    editor.selected = 0;
                    editor.offset = 0;
                }
                Some(MovePicker::Folders) => {
                    if let Some(folder) = editor
                        .destination()
                        .and_then(|p| p.folders.get(editor.selected))
                    {
                        editor.move_folder = folder.display().to_string();
                    }
                    editor.move_confirm = true;
                    editor.target_path()?;
                    editor.confirm = true;
                }
                None => {}
            },
            TagAction::MoveConfirm => {
                ensure!(editor.destination().is_some(), "Выберите плейлист.");
                editor.move_confirm = true;
                editor.target_path()?;
                editor.confirm = true;
            }
            TagAction::Format(direction) => {
                editor.block = super::bounded(editor.block, direction, document.blocks.len());
                editor.selected = 0;
                editor.offset = 0;
            }
            TagAction::Tab(tab) => {
                editor.tab = tab;
                editor.selected = 0;
                editor.offset = 0;
            }
            TagAction::Row(index) => {
                editor.selected = index;
                if editor.move_picker == Some(MovePicker::Folders)
                    && let Some(folder) = editor.destination().and_then(|p| p.folders.get(index))
                {
                    editor.move_folder = folder.display().to_string();
                }
            }
            TagAction::Remove(index) => {
                let block = &mut document.blocks[editor.block];
                if editor.tab == TagsTab::Fields {
                    if let Some(field) = block.fields.get_mut(index) {
                        field.remove = !field.remove;
                    }
                } else if editor.tab == TagsTab::Extras {
                    if let Some(field) = block.extras.get_mut(index) {
                        field.remove = !field.remove;
                    }
                } else if editor.tab == TagsTab::Covers
                    && let Some(cover) = block.covers.get_mut(index)
                {
                    cover.remove = !cover.remove;
                }
            }
            TagAction::Add => {
                let block = &document.blocks[editor.block];
                ensure!(
                    block.writable && !block.remove,
                    "Снимите очистку блока перед редактированием."
                );
                if editor.tab == TagsTab::Covers {
                    ensure!(
                        !matches!(
                            block.kind,
                            lofty::tag::TagType::Id3v1
                                | lofty::tag::TagType::RiffInfo
                                | lofty::tag::TagType::AiffText
                        ),
                        "Этот тип тегов не поддерживает обложки."
                    );
                    editor.edit = Some(TagEdit {
                        file_input: None,
                        index: None,
                        cover: true,
                        extra: false,
                        key: ItemKey::TrackTitle,
                        values: [String::new(), String::new(), "3".into()],
                        input: 0,
                        cursor: 0,
                        selected_all: false,
                        value_kind: 0,
                    });
                } else if editor.tab == TagsTab::Extras {
                    editor.edit = Some(TagEdit {
                        file_input: None,
                        index: None,
                        cover: false,
                        extra: true,
                        key: ItemKey::Comment,
                        values: Default::default(),
                        input: 1,
                        cursor: 0,
                        selected_all: false,
                        value_kind: 0,
                    });
                } else {
                    editor.key_picker = true;
                    editor.selected = 0;
                    editor.offset = 0;
                }
            }
            TagAction::Edit => {
                let block = &document.blocks[editor.block];
                ensure!(
                    block.writable && !block.remove,
                    "Снимите очистку блока перед редактированием."
                );
                if editor.tab == TagsTab::Fields {
                    if let Some(field) = block.fields.get(editor.selected) {
                        let item = &field.item;
                        let value_kind = match item.value() {
                            ItemValue::Text(_) => 0,
                            ItemValue::Locator(_) => 1,
                            ItemValue::Binary(_) => 2,
                        };
                        let mut edit = TagEdit {
                            file_input: None,
                            index: Some(editor.selected),
                            cover: false,
                            extra: false,
                            key: item.key(),
                            values: [
                                item_value_text(item.value()),
                                item.description().into(),
                                String::from_utf8_lossy(item.lang())
                                    .trim_matches('\0')
                                    .into(),
                            ],
                            input: 0,
                            cursor: 0,
                            selected_all: false,
                            value_kind,
                        };
                        edit.select(0);
                        editor.edit = Some(edit);
                    }
                } else if editor.tab == TagsTab::Extras {
                    if let Some(field) = block.extras.get(editor.selected) {
                        let mut edit = TagEdit {
                            file_input: None,
                            index: Some(editor.selected),
                            cover: false,
                            extra: true,
                            key: ItemKey::Comment,
                            values: [field.value.clone(), field.name.clone(), String::new()],
                            input: 0,
                            cursor: 0,
                            selected_all: false,
                            value_kind: field.value_kind,
                        };
                        edit.select(0);
                        editor.edit = Some(edit);
                    }
                } else if editor.tab == TagsTab::Covers
                    && let Some(cover) = block.covers.get(editor.selected)
                {
                    editor.edit = Some(TagEdit {
                        file_input: None,
                        index: Some(editor.selected),
                        cover: true,
                        extra: false,
                        key: ItemKey::TrackTitle,
                        values: [
                            String::new(),
                            cover.picture.description().unwrap_or_default().into(),
                            cover.picture.pic_type().as_u8().to_string(),
                        ],
                        input: 0,
                        cursor: 0,
                        selected_all: false,
                        value_kind: 0,
                    });
                }
            }
            TagAction::ChooseKey(index) => {
                let keys = field_keys(document.blocks[editor.block].kind);
                if let Some(key) = keys.get(index) {
                    editor.edit = Some(TagEdit {
                        file_input: None,
                        index: None,
                        cover: false,
                        extra: false,
                        key: *key,
                        values: Default::default(),
                        input: 0,
                        cursor: 0,
                        selected_all: false,
                        value_kind: 0,
                    });
                    editor.key_picker = false;
                    editor.selected = 0;
                    editor.offset = 0;
                }
            }
            TagAction::Keys => {
                editor.key_picker = true;
                editor.selected = 0;
                editor.offset = 0;
            }
            TagAction::Input(index) => {
                if let Some(edit) = &mut editor.edit {
                    edit.select(index);
                }
            }
            TagAction::ValueKind => {
                if let Some(edit) = &mut editor.edit {
                    edit.value_kind = (edit.value_kind + 1) % 3;
                }
            }
            TagAction::Apply => {
                let edit = editor.edit.as_ref().context("Выберите поле.")?;
                if let Some(input) = edit.file_input {
                    let value = edit.values[0].clone();
                    match input {
                        FileInput::Name => {
                            let before =
                                std::mem::replace(&mut editor.file_stem, value.trim().to_owned());
                            if let Err(error) = editor.new_filename() {
                                editor.file_stem = before;
                                return Err(error);
                            }
                        }
                        FileInput::Template => editor.name_template = value,
                        FileInput::Folder => {
                            let folder = Path::new(value.trim())
                                .canonicalize()
                                .context("Не удалось открыть папку назначения.")?;
                            ensure!(folder.is_dir(), "Выберите папку назначения.");
                            editor.move_folder = folder.display().to_string();
                            if let Some(target) = editor.move_target {
                                let folders = &mut editor.destinations[target].folders;
                                let index = folders
                                    .iter()
                                    .position(|p| p == &folder)
                                    .unwrap_or_else(|| {
                                        folders.push(folder);
                                        folders.len() - 1
                                    });
                                editor.selected = index;
                            }
                        }
                    }
                    editor.edit = None;
                    return Ok(());
                }
                let block = &mut document.blocks[editor.block];
                if edit.cover {
                    let mut picture = if edit.values[0].trim().is_empty() {
                        block
                            .covers
                            .get(edit.index.context("Укажите путь к изображению.")?)
                            .context("Обложка больше не существует.")?
                            .picture
                            .clone()
                    } else {
                        Picture::from_reader(
                            &mut std::fs::File::open(edit.values[0].trim())
                                .context("Не удалось открыть изображение.")?,
                        )
                        .context("Не удалось прочитать изображение.")?
                    };
                    let kind: u8 = edit.values[2]
                        .trim()
                        .parse()
                        .context("Тип обложки: число от 0 до 20.")?;
                    ensure!(kind <= 20, "Тип обложки: число от 0 до 20.");
                    picture.set_pic_type(PictureType::from_u8(kind));
                    picture.set_description(
                        (!edit.values[1].is_empty()).then(|| edit.values[1].clone()),
                    );
                    let cover = Cover {
                        picture,
                        remove: false,
                    };
                    if let Some(index) = edit.index {
                        block.covers[index] = cover;
                    } else {
                        block.covers.push(cover);
                        editor.selected = block.covers.len() - 1;
                    }
                    block.clear_covers = false;
                } else if edit.extra {
                    if let Some(index) = edit.index {
                        let field = &mut block.extras[index];
                        ensure!(
                            field.name == edit.values[1],
                            "Для другого имени создайте новое поле."
                        );
                        field.value = edit.values[0].clone();
                        field.remove = false;
                    } else {
                        block.extras.push(crate::tags::Extra::create(
                            block.kind,
                            &edit.values[1],
                            &edit.values[0],
                        )?);
                        editor.selected = block.extras.len() - 1;
                    }
                    block.clear_fields = false;
                } else {
                    let value = value_from_text(edit.value_kind, &edit.values[0])?;
                    ensure!(
                        edit.index.is_some() || !value.is_empty(),
                        "Введите значение нового поля."
                    );
                    let remove = value.is_empty();
                    let mut item = TagItem::new(edit.key, value);
                    item.set_description(edit.values[1].clone());
                    let language = edit.values[2].trim();
                    if !language.is_empty() {
                        ensure!(
                            language.len() == 3
                                && language.bytes().all(|c| c.is_ascii_alphabetic()),
                            "Код языка: три латинские буквы, например rus или eng."
                        );
                        item.set_lang(language.as_bytes().try_into().expect("three ASCII bytes"));
                    }
                    let field = Field { item, remove };
                    if let Some(index) = edit.index {
                        block.fields[index] = field;
                    } else {
                        block.fields.push(field);
                        editor.selected = block.fields.len() - 1;
                    }
                    block.clear_fields = false;
                }
                editor.edit = None;
            }
            TagAction::ClearAll => {
                let clear = !document
                    .blocks
                    .iter()
                    .filter(|block| block.existed || block.changed())
                    .all(|block| block.remove);
                for block in &mut document.blocks {
                    block.remove = clear;
                }
            }
            TagAction::ClearBlock(index) => {
                if let Some(block) = document.blocks.get_mut(index) {
                    block.remove = !block.remove;
                }
            }
            TagAction::ClearFields => {
                let block = &mut document.blocks[editor.block];
                block.clear_fields = !block.clear_fields;
            }
            TagAction::ClearCovers => {
                let block = &mut document.blocks[editor.block];
                block.clear_covers = !block.clear_covers;
            }
            TagAction::Save => {
                ensure!(editor.changed(), "Нет изменений для записи.");
                editor.move_confirm = false;
                editor.target_path()?;
                editor.confirm = true;
            }
            TagAction::Back | TagAction::Confirm | TagAction::Discard | TagAction::Move => {}
        }
        Ok(())
    }

    fn save_file_tags(&mut self) -> Result<()> {
        ensure!(
            !self.settings_saving(),
            "Дождитесь сохранения настроек и повторите запись файла."
        );
        let Some(Dialog::Tags(editor)) = &self.view.dialog else {
            return Ok(());
        };
        ensure!(editor.confirm, "Сначала подтвердите запись файла.");
        let editor = editor.clone();
        let document = editor
            .document
            .as_ref()
            .context("Теги еще не прочитаны.")?
            .clone();
        let (desk, track) = (editor.desk, editor.track.id);
        let destination = editor.target_path()?;
        let playlist = if editor.move_confirm {
            Some(editor.destination().context("Выберите плейлист.")?.id)
        } else {
            None
        };
        let mut recovery = editor.clone();
        recovery.confirm = false;
        self.start_database(Some(Dialog::Tags(recovery)), move |store| {
            Ok(DatabaseOutcome::FileEdited(store.edit_audio_file(
                desk,
                track,
                &document,
                &destination,
                playlist,
                true,
            )?))
        })?;
        self.tags.saving = true;
        Ok(())
    }
}

fn format_filename(template: &str, path: &Path, document: &Document) -> Result<String> {
    let field = |key| {
        document
            .blocks
            .iter()
            .flat_map(|b| &b.fields)
            .find_map(|f| {
                (f.item.key() == key)
                    .then(|| f.item.value().text())
                    .flatten()
            })
            .unwrap_or("")
    };
    let mut result = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        result.push_str(&rest[..start]);
        let end = rest[start..]
            .find('}')
            .context("В шаблоне не закрыта фигурная скобка.")?
            + start;
        let value = match &rest[start + 1..end] {
            "filename" => path.file_stem().and_then(|p| p.to_str()).unwrap_or(""),
            "title" => field(ItemKey::TrackTitle),
            "artist" => field(ItemKey::TrackArtist),
            "album" => field(ItemKey::AlbumTitle),
            "track" => field(ItemKey::TrackNumber),
            key => anyhow::bail!("Неизвестное поле шаблона: {{{key}}}"),
        };
        ensure!(
            !value.trim().is_empty(),
            "В шаблоне используется пустое поле: {}",
            &rest[start + 1..end]
        );
        result.extend(value.chars().map(|c| {
            if c.is_control() || c == '/' || c == '\\' {
                '_'
            } else {
                c
            }
        }));
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    ensure!(!result.trim().is_empty(), "Шаблон дал пустое имя файла.");
    Ok(result.trim().into())
}
