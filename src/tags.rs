//! Explicit file-tag editing. Ordinary library operations never use this module.
use anyhow::{Context, Result, bail, ensure};
use lofty::{
    config::{ParseOptions, ParsingMode, WriteOptions},
    file::{AudioFile, TaggedFile, TaggedFileExt},
    picture::Picture,
    probe::Probe,
    tag::{ItemKey, ItemValue, Tag, TagExt, TagItem, TagType},
};
use std::{
    fs::File,
    hash::Hasher,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};
use tempfile::NamedTempFile;
mod extra;
mod files;
mod native;
mod reader;
mod relocate;
mod batch;
pub use batch::BatchEdits;
pub(crate) fn move_noreplace(source:&Path,destination:&Path)->Result<()>{
    files::rename_noreplace(source,destination)?;
    files::sync_directory(destination)
}
pub use extra::Extra;

pub const TAG_TYPES: [TagType; 7] = [
    TagType::Id3v2,
    TagType::Id3v1,
    TagType::VorbisComments,
    TagType::Mp4Ilst,
    TagType::Ape,
    TagType::RiffInfo,
    TagType::AiffText,
];

pub fn type_name(kind: TagType) -> &'static str {
    match kind {
        TagType::Id3v2 => "ID3v2",
        TagType::Id3v1 => "ID3v1",
        TagType::VorbisComments => "Vorbis",
        TagType::Mp4Ilst => "MP4",
        TagType::Ape => "APE",
        TagType::RiffInfo => "RIFF INFO",
        TagType::AiffText => "AIFF",
        _ => "Другой",
    }
}

#[derive(Clone)]
pub struct Field {
    pub item: TagItem,
    pub remove: bool,
}

#[derive(Clone)]
pub struct Cover {
    pub picture: Picture,
    pub remove: bool,
}

#[derive(Clone)]
pub struct TagBlock {
    pub kind: TagType,
    pub writable: bool,
    pub existed: bool,
    pub fields: Vec<Field>,
    pub covers: Vec<Cover>,
    pub remove: bool,
    pub clear_fields: bool,
    pub clear_covers: bool,
    pub extras: Vec<Extra>,
    original_extras: Vec<(String, String, usize)>,
    original: Tag,
    native: Option<native::Native>,
}

impl TagBlock {
    pub fn changed(&self) -> bool {
        self.remove && self.existed
            || self.clear_fields
                && (self.original.item_count() > 0 || !self.original_extras.is_empty())
            || self.clear_covers && self.original.picture_count() > 0
            || self.fields.iter().any(|field| field.remove)
            || self.covers.iter().any(|cover| cover.remove)
            || !self
                .fields
                .iter()
                .map(|field| &field.item)
                .eq(self.original.items())
            || !self
                .covers
                .iter()
                .map(|cover| &cover.picture)
                .eq(self.original.pictures())
            || self.extras.iter().any(|field| field.remove)
            || extra::signatures(&self.extras) != self.original_extras
    }

    fn rebuilt(&self) -> Tag {
        // Cloning retains format-specific frames that the generic API cannot expose.
        let mut tag = self.original.clone();
        tag.retain(|_| false);
        while tag.picture_count() > 0 {
            tag.remove_picture(0);
        }
        if !self.clear_fields {
            for field in self.fields.iter().filter(|field| !field.remove) {
                tag.push_unchecked(field.item.clone());
            }
        }
        if !self.clear_covers {
            for cover in self.covers.iter().filter(|cover| !cover.remove) {
                tag.push_picture(cover.picture.clone());
            }
        }
        tag
    }

    pub fn has_special_fields(&self) -> bool {
        !self.extras.is_empty()
    }
}

#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Stamp {
    length: u64,
    modified: SystemTime,
    digest: u64,
    #[cfg(unix)]
    identity: (u64, u64),
}

impl Stamp {
    fn read(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let before = file.metadata()?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let size = file.read(&mut buffer)?;
            if size == 0 {
                break;
            }
            hasher.write(&buffer[..size]);
        }
        let after = file.metadata()?;
        ensure!(
            before.len() == after.len() && before.modified()? == after.modified()?,
            "Файл изменился во время чтения. Откройте редактор заново."
        );
        Ok(Self {
            length: after.len(),
            modified: after.modified()?,
            digest: hasher.finish(),
            #[cfg(unix)]
            identity: {
                use std::os::unix::fs::MetadataExt;
                (after.dev(), after.ino())
            },
        })
    }
}

#[derive(Clone)]
pub struct Document {
    pub path: PathBuf,
    pub blocks: Vec<TagBlock>,
    stamp: Stamp,
    preserved_id3v1: Option<[u8; 128]>,
}

fn parse(path: &Path) -> Result<TaggedFile> {
    let probe = Probe::open(path)?.guess_file_type()?;
    let options = ParseOptions::new().parsing_mode(ParsingMode::Strict);
    if probe.file_type() != Some(lofty::file::FileType::Mpeg) {
        return probe.options(options).read().map_err(Into::into);
    }
    let reader = reader::Reader::open(path, true)?;
    let blank_year = reader.preserved_id3v1.is_some();
    let mut file = Probe::with_file_type(reader, lofty::file::FileType::Mpeg)
        .options(options)
        .read()?;
    if blank_year && let Some(tag) = file.tag_mut(TagType::Id3v1) {
        tag.remove_key(ItemKey::Year);
    }
    Ok(file)
}

impl Document {
    pub fn read(path: &Path) -> Result<Self> {
        let path = path.canonicalize()?;
        let stamp = Stamp::read(&path)?;
        let file = parse(&path)
            .context("Не удалось полностью прочитать теги. Запись этого файла запрещена.")?;
        let primary = file.primary_tag_type();
        let native_tags = native::read(&path, file.file_type())?;
        let preserved_id3v1 = reader::Reader::open(
            &path,
            file.file_type() == lofty::file::FileType::Mpeg,
        )?.preserved_id3v1;
        let mut kinds = TAG_TYPES.to_vec();
        kinds.sort_by_key(|kind| (*kind != primary, !file.contains_tag_type(*kind)));
        let blocks = kinds
            .into_iter()
            .filter(|kind| file.tag_support(*kind).is_writable() || file.contains_tag_type(*kind))
            .map(|kind| -> Result<TagBlock> {
                let native = native_tags.iter().find(|tag| tag.kind() == kind);
                let mut original = native
                    .map(native::Native::generic)
                    .or_else(|| file.tag(kind).cloned())
                    .unwrap_or_else(|| Tag::new(kind));
                // FLAC pictures belong to the file, outside the Vorbis comment block.
                if file.file_type() == lofty::file::FileType::Flac
                    && kind == TagType::VorbisComments
                {
                    while original.picture_count() > 0 {
                        original.remove_picture(0);
                    }
                    if let Some(tag) = file.tag(kind) {
                        for picture in tag.pictures() {
                            original.push_picture(picture.clone());
                        }
                    }
                }
                let extras = native
                    .map(native::Native::extras)
                    .transpose()?
                    .unwrap_or_default();
                Ok(TagBlock {
                    kind,
                    existed: file.contains_tag_type(kind),
                    writable: file.tag_support(kind).is_writable()
                        && !(kind == TagType::Id3v1 && preserved_id3v1.is_some()),
                    fields: original
                        .items()
                        .cloned()
                        .map(|item| Field {
                            item,
                            remove: false,
                        })
                        .collect(),
                    covers: original
                        .pictures()
                        .iter()
                        .cloned()
                        .map(|picture| Cover {
                            picture,
                            remove: false,
                        })
                        .collect(),
                    remove: false,
                    clear_fields: false,
                    clear_covers: false,
                    original_extras: extra::signatures(&extras),
                    extras,
                    original,
                    native: native.cloned().or_else(|| native::Native::empty(kind)),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            !blocks.is_empty(),
            "Для этого формата запись тегов не поддерживается."
        );
        ensure!(
            stamp == Stamp::read(&path)?,
            "Файл изменился во время чтения. Откройте редактор заново."
        );
        Ok(Self {
            path,
            blocks,
            stamp,
            preserved_id3v1,
        })
    }

    pub fn changed(&self) -> bool {
        self.blocks.iter().any(TagBlock::changed)
    }

    pub fn destructive(&self) -> bool {
        self.blocks.iter().any(|block| {
            block.remove
                || block.clear_fields
                || block.clear_covers
                || block.fields.iter().any(|field| field.remove)
                || block.covers.iter().any(|cover| cover.remove)
        }) || self
            .blocks
            .iter()
            .any(|block| block.extras.iter().any(|field| field.remove))
    }

    pub fn prepare(&self) -> Result<PreparedWrite> {
        ensure!(self.changed(), "Теги не изменены.");
        self.prepare_at(&self.path)
    }

    pub(crate) fn verify_source(&self) -> Result<()> {
        ensure!(
            Stamp::read(&self.path)? == self.stamp,
            "Файл изменен другой программой. Откройте редактор заново."
        );
        Ok(())
    }

    pub(crate) fn prepare_at(&self, destination: &Path) -> Result<PreparedWrite> {
        let destination = destination
            .parent()
            .context("Выберите папку назначения.")?
            .canonicalize()?
            .join(destination.file_name().context("Укажите имя файла.")?);
        let relocating = destination != self.path;
        let editing_tags = self.changed();
        ensure!(
            self.preserved_id3v1.is_none()
                || self.blocks.iter().filter(|block| block.kind == TagType::Id3v1)
                    .all(|block| !block.changed()),
            "Старый блок ID3v1 с пустым годом сохраняется без изменений. Для правки используйте ID3v2."
        );
        ensure!(editing_tags || relocating, "Нет изменений для записи.");
        ensure!(
            !relocating
                || std::fs::symlink_metadata(&destination)
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
            "Файл с таким именем уже существует или недоступен. Выберите другое имя."
        );
        self.verify_source()?;
        let metadata = std::fs::metadata(&self.path)?;
        ensure!(
            !editing_tags || !metadata.permissions().readonly(),
            "Файл доступен только для чтения."
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            ensure!(
                metadata.nlink() == 1,
                "У файла есть жесткие ссылки. Сначала создайте отдельную копию для изменения тегов."
            );
        }
        // Check the source's write permission even though replacement uses the directory.
        // A move without tag edits only reads the source and changes directory entries.
        if editing_tags {
            std::fs::OpenOptions::new().write(true).open(&self.path)?;
        }
        let mut backup = temporary(&self.path)?;
        std::io::copy(&mut File::open(&self.path)?, backup.as_file_mut())?;
        backup.as_file().sync_all()?;
        let copied = Stamp::read(backup.path())?;
        ensure!(
            (copied.length, copied.digest) == (self.stamp.length, self.stamp.digest),
            "Файл изменился во время копирования. Изменения не записаны."
        );
        ensure!(
            Stamp::read(&self.path)? == self.stamp,
            "Файл изменен другой программой. Откройте редактор заново."
        );
        let mut edited = temporary(&destination)?;
        backup.as_file_mut().rewind()?;
        std::io::copy(backup.as_file_mut(), edited.as_file_mut())?;
        let options = WriteOptions::new()
            .lossy_text_encoding(false)
            .respect_read_only(false)
            .parse_options(ParseOptions::new().parsing_mode(ParsingMode::Strict));
        let mut expected = Vec::new();
        for block in self.blocks.iter().filter(|block| block.changed()) {
            edited.as_file_mut().rewind()?;
            if block.remove {
                block
                    .kind
                    .remove_from(edited.as_file_mut(), options)
                    .with_context(|| {
                        format!("Не удалось удалить блок {}.", type_name(block.kind))
                    })?;
                expected.push((block.kind, None));
            } else {
                ensure!(
                    block.writable,
                    "Блок {} доступен только для чтения.",
                    type_name(block.kind)
                );
                let tag = block.rebuilt();
                let result = if let Some(native) = &block.native {
                    native
                        .merge_extras(
                            tag.clone(),
                            if block.clear_fields {
                                &[]
                            } else {
                                &block.extras
                            },
                        )?
                        .save_direct(edited.as_file_mut(), options)
                } else {
                    tag.save_to(edited.as_file_mut(), options)
                        .map_err(Into::into)
                };
                result.with_context(|| {
                    format!("Не удалось записать блок {}.", type_name(block.kind))
                })?;
                expected.push((block.kind, Some(tag)));
            }
        }
        edited.as_file_mut().seek(SeekFrom::Start(0))?;
        let written = parse(edited.path())
            .context("Не удалось прочитать результат записи. Исходный файл сохранен.")?;
        let written_native = native::read(edited.path(), written.file_type())?;
        if let Some(original) = self.preserved_id3v1 {
            ensure!(
                reader::Reader::open(edited.path(), true)?.preserved_id3v1 == Some(original),
                "При записи изменился старый блок ID3v1. Исходный файл сохранен."
            );
        }
        for block in &self.blocks {
            let actual = written_native.iter().find(|tag| tag.kind() == block.kind);
            if !block.changed() {
                if let Some(original) = &block.native
                    && block.existed
                {
                    let actual = actual.context("При записи исчез соседний блок тегов.")?;
                    ensure!(
                        actual.encoded()? == original.encoded()?,
                        "При записи изменился соседний блок {}. Исходный файл сохранен.",
                        type_name(block.kind)
                    );
                }
                continue;
            }
            let expected = if block.remove || block.clear_fields {
                Vec::new()
            } else {
                block
                    .extras
                    .iter()
                    .filter(|field| !field.remove)
                    .cloned()
                    .collect()
            };
            let mut expected = extra::signatures(&expected);
            let mut actual = actual
                .map(native::Native::extras)
                .transpose()?
                .map(|fields| extra::signatures(&fields))
                .unwrap_or_default();
            expected.sort();
            actual.sort();
            ensure!(
                expected == actual,
                "Формат {} изменил дополнительные поля. Исходный файл сохранен.",
                type_name(block.kind)
            );
        }
        // Refuse lossy/truncated values instead of displaying a false successful save.
        for (kind, tag) in &expected {
            let actual = written.tag(*kind);
            match tag {
                None => ensure!(
                    actual.is_none_or(|tag| tag.pictures().is_empty()
                        && tag
                            .items()
                            .all(|item| item.key() == ItemKey::EncoderSoftware
                                && item.value().is_empty())),
                    "Не удалось полностью очистить блок {}. Исходный файл сохранен.",
                    type_name(*kind)
                ),
                Some(expected) => {
                    let empty = Tag::new(*kind);
                    let actual = actual.unwrap_or(&empty);
                    ensure!(
                        same_items(expected, actual) && expected.pictures() == actual.pictures(),
                        "Формат {} не сохранил значения без потерь. Проверьте длину и допустимость полей. Исходный файл сохранен.",
                        type_name(*kind)
                    );
                }
            }
        }
        let mut track = crate::media::read_track(edited.path())
            .context("Аудиофайл после записи не читается. Исходный файл сохранен.")?;
        add_extra_metadata(&mut track, &written_native)?;
        track.path = destination.clone();
        // An empty title falls back to the real filename, never the temporary one.
        if written
            .primary_tag()
            .and_then(|tag| lofty::tag::Accessor::title(tag))
            .is_none()
        {
            track.title = destination
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
        }
        files::copy_attributes(&self.path, backup.path())?;
        files::copy_attributes(&self.path, edited.path())?;
        backup.as_file().sync_all()?;
        edited.as_file().sync_all()?;
        files::sync_directory(&self.path)?;
        files::sync_directory(&destination)?;
        Ok(PreparedWrite {
            path: self.path.clone(),
            destination: relocating.then_some(destination),
            operation: format!("{:032x}", fastrand::u128(..)),
            stamp: self.stamp.clone(),
            backup,
            edited,
            track,
        })
    }
}

fn same_items(expected: &Tag, actual: &Tag) -> bool {
    let canonical = |item: &TagItem| {
        if item.key() == ItemKey::EncoderSoftware && item.value().is_empty() {
            return None;
        }
        let mut item = item.clone();
        if matches!(
            item.key(),
            ItemKey::TrackNumber | ItemKey::TrackTotal | ItemKey::DiscNumber | ItemKey::DiscTotal
        ) && let Some(number) = item
            .value()
            .text()
            .and_then(|text| text.parse::<u32>().ok())
        {
            let mut normalized = TagItem::new(item.key(), ItemValue::Text(number.to_string()));
            normalized.set_description(item.description().into());
            normalized.set_lang(*item.lang());
            item = normalized;
        }
        Some(item)
    };
    let mut remaining: Vec<_> = actual.items().filter_map(canonical).collect();
    for item in expected.items().filter_map(canonical) {
        let Some(index) = remaining.iter().position(|other| *other == item) else {
            return false;
        };
        remaining.remove(index);
    }
    remaining.is_empty()
}

fn temporary(path: &Path) -> Result<NamedTempFile> {
    tempfile::Builder::new()
        .prefix(".almavorn-tags-")
        .suffix(&format!(
            ".{}",
            path.extension().unwrap_or_default().to_string_lossy()
        ))
        .tempfile_in(path.parent().context("У файла нет родительской папки.")?)
        .context("Не удалось создать временную копию рядом с аудиофайлом.")
}

pub struct PreparedWrite {
    path: PathBuf,
    destination: Option<PathBuf>,
    operation: String,
    stamp: Stamp,
    backup: NamedTempFile,
    edited: NamedTempFile,
    pub track: crate::model::ImportedTrack,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Recovery {
    track: i64,
    path: PathBuf,
    backup: PathBuf,
    staging: PathBuf,
    before: Stamp,
    after: Stamp,
    #[serde(default)]
    destination: Option<PathBuf>,
    #[serde(default)]
    operation: Option<String>,
}

impl PreparedWrite {
    pub(crate) fn operation_id(&self) -> String {
        self.operation.clone()
    }

    fn recovery(&self, track: i64) -> Result<Recovery> {
        Ok(Recovery {
            track,
            path: self.path.clone(),
            backup: self.backup.path().into(),
            staging: self.edited.path().into(),
            before: self.stamp.clone(),
            after: Stamp::read(self.edited.path())?,
            destination: self.destination.clone(),
            operation: Some(self.operation_id()),
        })
    }

    pub(crate) fn record_recovery(&self, marker: &Path, track: i64) -> Result<()> {
        use std::io::Write;
        ensure!(
            !marker.exists(),
            "Предыдущая запись тегов требует восстановления. Перезапустите приложение."
        );
        let record = self.recovery(track)?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(marker.parent().context("Нет папки библиотеки.")?)?;
        temporary.write_all(&serde_json::to_vec(&record)?)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist_noclobber(marker)
            .map_err(|error| error.error)?;
        files::sync_directory(marker)
    }
}

pub(crate) fn recovery_path(database: &Path) -> PathBuf {
    database.with_extension("tag-write.json")
}

pub(crate) fn finish_recovery(marker: &Path) -> Result<()> {
    if marker.exists() {
        std::fs::remove_file(marker)?;
        files::sync_directory(marker)?;
    }
    Ok(())
}

pub(crate) fn file_write_committed(
    connection: &rusqlite::Connection,
    operation: &str,
) -> Result<bool> {
    use rusqlite::OptionalExtension;
    let saved: Option<String> = connection
        .query_row(
            "SELECT value FROM almavorn_metadata WHERE key='file_write_commit'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(saved.as_deref() == Some(operation))
}

pub(crate) fn recover_file_write(connection: &rusqlite::Connection, database: &Path) -> Result<()> {
    use rusqlite::OptionalExtension;
    let marker = recovery_path(database);
    if !marker.exists() {
        return Ok(());
    }
    let recovery: Recovery = serde_json::from_reader(File::open(&marker)?)
        .context("Не удалось прочитать запись восстановления тегов.")?;
    let committed = recovery
        .operation
        .as_deref()
        .map(|operation| file_write_committed(connection, operation))
        .transpose()?;
    let saved: Option<String> = connection
        .query_row(
            "SELECT path FROM tracks WHERE id=?1",
            [recovery.track],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(destination) = &recovery.destination {
        ensure!(
            saved.as_deref() == recovery.path.to_str() || saved.as_deref() == destination.to_str(),
            "Путь перемещаемого файла не совпадает с библиотекой. Исходная копия: {}",
            recovery.backup.display()
        );
        relocate::finish(&recovery, saved.as_deref() == destination.to_str())?;
        return finish_recovery(&marker);
    }
    ensure!(
        saved.as_deref() == recovery.path.to_str(),
        "Путь восстановления тегов не совпадает с библиотекой. Копия файла: {}",
        if recovery.staging.exists() {
            &recovery.staging
        } else {
            &recovery.backup
        }
        .display()
    );
    let current = Stamp::read(&recovery.path)?;
    ensure!(
        current == recovery.before || current == recovery.after,
        "После прерванной записи аудиофайл изменился. Восстановление остановлено. Исходная копия: {}",
        if recovery.staging.exists() {
            &recovery.staging
        } else {
            &recovery.backup
        }
        .display()
    );
    if current == recovery.after {
        if committed == Some(false) {
            ensure!(
                recovery.staging.parent() == recovery.path.parent()
                    && recovery
                        .staging
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with(".almavorn-tags-"))
                    && Stamp::read(&recovery.staging).ok().as_ref() == Some(&recovery.before),
                "Исходная версия требует ручного восстановления: {}",
                recovery.backup.display()
            );
            files::exchange(&recovery.staging, &recovery.path)?;
            if Stamp::read(&recovery.staging).ok().as_ref() != Some(&recovery.after) {
                // Preserve a concurrent replacement rather than deleting it as a staging file.
                if Stamp::read(&recovery.path).ok().as_ref() == Some(&recovery.before) {
                    files::exchange(&recovery.staging, &recovery.path)?;
                }
                bail!(
                    "Файл изменен во время восстановления. Копия сохранена: {}",
                    recovery.staging.display()
                );
            }
            files::sync_directory(&recovery.path)?;
        } else {
            let mut track = crate::media::read_track(&recovery.path)?;
            let file = parse(&recovery.path)?;
            add_extra_metadata(&mut track, &native::read(&recovery.path, file.file_type())?)?;
            connection.execute(
                "UPDATE tracks SET title=?1,artist=?2,album=?3,duration_ms=?4,tags=?5,revision=revision+1 WHERE id=?6",
                rusqlite::params![track.title, track.artist, track.album, track.duration_ms.min(i64::MAX as u64) as i64, track.tags, recovery.track],
            )?;
        }
    }
    // Only task-created sibling copies with expected contents may be removed.
    for temporary in [&recovery.backup, &recovery.staging] {
        if temporary.parent() == recovery.path.parent()
            && temporary
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".almavorn-tags-"))
            && temporary != &recovery.path
            && std::fs::symlink_metadata(temporary)
                .is_ok_and(|metadata| metadata.file_type().is_file())
            && Stamp::read(temporary).is_ok_and(|stamp| {
                (stamp.digest, stamp.length) == (recovery.before.digest, recovery.before.length)
                    || (stamp.digest, stamp.length)
                        == (recovery.after.digest, recovery.after.length)
            })
        {
            std::fs::remove_file(temporary)?;
        }
    }
    files::sync_directory(&recovery.path)?;
    finish_recovery(&marker)
}

fn add_extra_metadata(
    track: &mut crate::model::ImportedTrack,
    native: &[native::Native],
) -> Result<()> {
    let mut tags: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_str(&track.tags)?;
    for tag in native {
        for field in tag.extras()? {
            let key = format!("{:?}.{}", tag.kind(), field.name);
            tags.entry(key).or_default().push(if field.value_kind == 2 {
                format!("{:?}", value_from_text(2, &field.value)?)
            } else {
                format!("{:?}", ItemValue::Text(field.value))
            });
        }
    }
    track.tags = serde_json::to_string(&tags)?;
    Ok(())
}

pub enum Replacement {
    Tags(TagReplacement),
    Moved(Recovery),
}

impl Replacement {
    pub fn accept(self) -> Result<Option<PathBuf>> {
        match self {
            Self::Tags(replacement) => replacement.accept(),
            Self::Moved(recovery) => relocate::finish(&recovery, true),
        }
    }
    pub fn rollback(self) -> Result<()> {
        match self {
            Self::Tags(replacement) => replacement.rollback(),
            Self::Moved(recovery) => relocate::finish(&recovery, false).map(|_| ()),
        }
    }
}

pub struct TagReplacement {
    path: PathBuf,
    backup: Option<NamedTempFile>,
    written: Stamp,
    before: Stamp,
}

impl PreparedWrite {
    pub fn replace(self) -> Result<Replacement> {
        ensure!(
            Stamp::read(&self.path)? == self.stamp,
            "Файл изменен другой программой. Изменения тегов не записаны."
        );
        let written = Stamp::read(self.edited.path())?;
        if let Some(destination) = &self.destination {
            let recovery = self.recovery(0)?;
            files::rename_noreplace(self.edited.path(), destination)?;
            // A durable marker owns the copies until the database result is known.
            self.backup.keep().map_err(|error| error.error)?;
            if let Err(error) = files::sync_directory(destination) {
                relocate::finish(&recovery, false)?;
                return Err(error);
            }
            return Ok(Replacement::Moved(recovery));
        }
        files::exchange(self.edited.path(), &self.path)?;
        if Stamp::read(self.edited.path()).ok().as_ref() != Some(&self.stamp) {
            if Stamp::read(&self.path).ok().as_ref() == Some(&written) {
                files::exchange(self.edited.path(), &self.path)?;
            } else {
                let (_, displaced) = self.edited.keep().map_err(|error| error.error)?;
                bail!(
                    "Файл одновременно изменен другой программой. Вытесненная версия сохранена: {}",
                    displaced.display()
                );
            }
            bail!(
                "Файл изменен другой программой. Его версия восстановлена; откройте редактор заново."
            );
        }
        // The displaced original remains available until the database commits.
        let replacement = TagReplacement {
            path: self.path,
            backup: Some(self.edited),
            written,
            before: self.stamp,
        };
        if let Err(error) = files::sync_directory(&replacement.path) {
            replacement.rollback()?;
            return Err(error);
        }
        Ok(Replacement::Tags(replacement))
    }
}

impl TagReplacement {
    pub fn accept(mut self) -> Result<Option<PathBuf>> {
        let Some(backup) = self.backup.take() else {
            return Ok(None);
        };
        if Stamp::read(backup.path()).ok().as_ref() != Some(&self.before) {
            let (_, path) = backup.keep().map_err(|error| error.error)?;
            files::sync_directory(&path)?;
            return Ok(Some(path));
        }
        Ok(None)
    }

    pub fn rollback(mut self) -> Result<()> {
        let Some(backup) = self.backup.take() else {
            return Ok(());
        };
        if Stamp::read(&self.path).ok().as_ref() != Some(&self.written) {
            let (_, path) = backup.keep().map_err(|error| error.error)?;
            bail!(
                "Не удалось обновить базу; файл затем изменился. Исходная копия сохранена: {}",
                path.display()
            );
        }
        if let Err(error) = files::exchange(backup.path(), &self.path) {
            let (_, path) = backup.keep().map_err(|error| error.error)?;
            bail!(
                "Не удалось восстановить файл: {error:#}. Исходная копия: {}",
                path.display()
            );
        }
        if Stamp::read(backup.path()).ok().as_ref() != Some(&self.written) {
            // Another writer won the race before rollback; put that version back.
            if let Err(error) = files::exchange(backup.path(), &self.path) {
                let (_, path) = backup.keep().map_err(|error| error.error)?;
                bail!(
                    "Файл изменен во время восстановления: {error:#}. Сохраненная версия: {}",
                    path.display()
                );
            }
            let (_, path) = backup.keep().map_err(|error| error.error)?;
            bail!(
                "Файл изменен другой программой. Исходная копия сохранена: {}",
                path.display()
            );
        }
        files::sync_directory(&self.path)
    }
}

impl Drop for TagReplacement {
    fn drop(&mut self) {
        if let Some(backup) = self.backup.take() {
            // On unwinding, keep a recoverable original rather than deleting it.
            let _ = backup.keep();
        }
    }
}

pub fn item_value_text(value: &ItemValue) -> String {
    match value {
        ItemValue::Text(text) | ItemValue::Locator(text) => text.clone(),
        ItemValue::Binary(bytes) => bytes.iter().map(|byte| format!("{byte:02X}")).collect(),
    }
}

pub fn value_from_text(kind: usize, text: &str) -> Result<ItemValue> {
    match kind {
        0 => Ok(ItemValue::Text(text.to_owned())),
        1 => Ok(ItemValue::Locator(text.to_owned())),
        _ => {
            let hex: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            ensure!(
                hex.len().is_multiple_of(2) && hex.is_ascii(),
                "Двоичные данные: введите пары шестнадцатеричных цифр, например FF 00 A2."
            );
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
                .collect::<std::result::Result<Vec<_>, _>>()
                .context("Двоичные данные содержат недопустимые цифры.")?;
            Ok(ItemValue::Binary(bytes))
        }
    }
}

pub fn field_keys(kind: TagType) -> Vec<ItemKey> {
    let common = [
        ItemKey::TrackTitle,
        ItemKey::TrackArtist,
        ItemKey::AlbumTitle,
        ItemKey::AlbumArtist,
        ItemKey::Genre,
        ItemKey::RecordingDate,
        ItemKey::TrackNumber,
        ItemKey::TrackTotal,
        ItemKey::DiscNumber,
        ItemKey::Composer,
        ItemKey::Comment,
        ItemKey::Lyrics,
    ];
    let mut keys = ItemKey::supported_keys(kind).to_vec();
    keys.sort_by_key(|key| {
        (
            common
                .iter()
                .position(|common| common == key)
                .unwrap_or(usize::MAX),
            format!("{key:?}"),
        )
    });
    keys
}
