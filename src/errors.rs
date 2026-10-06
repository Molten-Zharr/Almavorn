use crate::model::Language;
use std::{error::Error, fmt};

/// User-facing failures are identified by type, independently of their wording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppError {
    OrderProtected,
    TrackRenameProtected,
    SharedTrackProtected,
    PlaylistConfirmationRequired,
    PlaylistRemovalConfirmationRequired,
    InvalidTrackName,
    PlaylistMissing,
    EntryMissing,
    MusicSelectionRequired,
    FolderNotReady,
    ImportBusy,
    ShortcutConflict,
    ReservedShortcut,
    InvalidLegacyColor,
    InvalidColor,
    InvalidFontSize,
    InvalidCatalogName,
    CatalogNameConflict,
    PlaylistNameConflict,
    DatabaseWriterFailed,
    PaletteFileBusy,
    FilePathRequired,
    InvalidPaletteJson,
    UnsupportedPaletteVersion,
    UnsupportedPaletteFormat,
    PaletteFileTooLarge,
    LastCatalogItem,
    LastProfile,
    LastPalette,
    LastPreset,
    UndoConflict,
    LibraryConflict,
    LibraryBusy,
    BackgroundFailed,
    BackgroundInterrupted,
    AudioUnavailable,
    InvalidAudio,
    SeekingUnavailable,
    TrackSelectionRequired,
    PlaylistSelectionRequired,
    TrackMissing,
    TrackNotInPlaylist,
    DeskRenameForbidden,
    DeskRemovalForbidden,
    DeskPositionFixed,
    ReservedPlaylistName,
    InvalidPathEncoding,
    PaletteMissing,
    ImportInterrupted,
    WaveformInterrupted,
}

impl AppError {
    pub fn message(self, language: Language) -> &'static str {
        match self {
            Self::OrderProtected => language.text(
                "Enable Order editing before changing this playlist",
                "Сначала включите редактирование Порядка.",
            ),
            Self::TrackRenameProtected => language.text(
                "Enable Order editing before renaming tracks",
                "Для переименования включите редактирование Порядка.",
            ),
            Self::SharedTrackProtected => language.text(
                "Track is also used by a protected Order playlist",
                "Композиция используется в защищенном плейлисте Порядка.",
            ),
            Self::PlaylistConfirmationRequired => language.text(
                "Enter the exact playlist name",
                "Введите точное имя плейлиста для подтверждения.",
            ),
            Self::PlaylistRemovalConfirmationRequired => language.text(
                "Enter the exact playlist name to confirm removal",
                "Введите точное имя плейлиста для подтверждения.",
            ),
            Self::InvalidTrackName => language.text(
                "Name must contain 1–160 printable characters",
                "Имя должно содержать от 1 до 160 символов.",
            ),
            Self::PlaylistMissing => language.text(
                "Playlist no longer exists",
                "Плейлист больше не существует.",
            ),
            Self::EntryMissing => language.text(
                "Track no longer exists in this playlist",
                "Композиции больше нет в этом плейлисте.",
            ),
            Self::MusicSelectionRequired => language.text(
                "Select music files first",
                "Сначала выберите музыкальные файлы.",
            ),
            Self::FolderNotReady => language.text(
                "Choose an available folder before adding music",
                "Выберите доступную папку и дождитесь ее загрузки.",
            ),
            Self::ImportBusy => language.text(
                "Music import is already running",
                "Добавление музыкальных файлов уже выполняется.",
            ),
            Self::ShortcutConflict => language.text(
                "This shortcut is already assigned",
                "Это сочетание клавиш уже назначено.",
            ),
            Self::ReservedShortcut => language.text(
                "Navigation keys are reserved",
                "Эти клавиши зарезервированы для навигации.",
            ),
            Self::InvalidLegacyColor => language.text(
                "Use a six-digit color, for example E89E4A",
                "Введите шесть цифр цвета, например E89E4A.",
            ),
            Self::InvalidColor => language.text(
                "Use a six-digit color, for example FF0000",
                "Введите цвет из шести шестнадцатеричных цифр, например FF0000.",
            ),
            Self::InvalidFontSize => language.text(
                "Font size must be 10-32 pixels",
                "Размер шрифта должен быть от 10 до 32 пикселей.",
            ),
            Self::InvalidCatalogName => language.text(
                "Name must contain 1-80 printable characters",
                "Имя должно содержать от 1 до 80 печатных символов.",
            ),
            Self::CatalogNameConflict => language.text(
                "This name is already used",
                "Это имя уже используется. Выберите другое.",
            ),
            Self::PlaylistNameConflict => language.text(
                "Playlist with this name already exists",
                "Плейлист с таким именем уже существует.",
            ),
            Self::DatabaseWriterFailed => language.text(
                "Database writer failed",
                "Не удалось выполнить запись в библиотеку. Перезапустите плеер.",
            ),
            Self::PaletteFileBusy => language.text(
                "Wait for the palette file operation to finish",
                "Дождитесь завершения импорта или экспорта палитры.",
            ),
            Self::FilePathRequired => language.text("Enter a file path", "Введите путь к файлу."),
            Self::InvalidPaletteJson => language.text(
                "Cannot read palette JSON",
                "Не удалось прочитать JSON палитры. Проверьте файл.",
            ),
            Self::UnsupportedPaletteVersion => language.text(
                "Unsupported palette format or version",
                "Формат или версия палитры не поддерживаются.",
            ),
            Self::UnsupportedPaletteFormat => language.text(
                "Expected an Almavorn or Molten-Zharr palette",
                "Ожидается палитра Almavorn или Molten-Zharr.",
            ),
            Self::PaletteFileTooLarge => language.text(
                "Palette file is larger than 1 MiB",
                "Файл палитры должен быть не больше 1 МиБ.",
            ),
            Self::LastCatalogItem => language.text(
                "Keep at least one item in this catalog",
                "В этом списке должен остаться хотя бы один элемент.",
            ),
            Self::LastProfile => language.text(
                "Keep at least one profile",
                "Должен остаться хотя бы один профиль.",
            ),
            Self::LastPalette => language.text(
                "Keep at least one palette",
                "Должна остаться хотя бы одна палитра.",
            ),
            Self::LastPreset => language.text(
                "Keep at least one theme preset",
                "Должен остаться хотя бы один пресет темы.",
            ),
            Self::UndoConflict => language.text(
                "Playlist changed concurrently; undo is no longer safe",
                "Плейлист изменен другой задачей. Отмена могла бы затронуть более новые изменения.",
            ),
            Self::LibraryConflict => language.text(
                "Library changed concurrently; no changes were saved. Repeat the action",
                "Библиотека изменена другой задачей. Изменения не сохранены. Повторите действие.",
            ),
            Self::LibraryBusy => language.text(
                "Wait for the current library operation to finish",
                "Дождитесь завершения текущей операции с библиотекой.",
            ),
            Self::BackgroundFailed => language.text(
                "Background operation failed",
                "Фоновая операция завершилась с ошибкой. Повторите действие.",
            ),
            Self::BackgroundInterrupted => language.text(
                "Background operation was interrupted",
                "Фоновая операция прервана. Повторите действие.",
            ),
            Self::AudioUnavailable => language.text(
                "Audio output unavailable",
                "Устройство вывода звука недоступно. Проверьте подключение и настройки звука.",
            ),
            Self::InvalidAudio => language.text(
                "Unsupported or damaged audio file",
                "Аудиофайл поврежден или его формат не поддерживается.",
            ),
            Self::SeekingUnavailable => language.text(
                "Seeking is not available for this file",
                "Для этого файла перемотка недоступна.",
            ),
            Self::TrackSelectionRequired => {
                language.text("Select a track first", "Сначала выберите композицию.")
            }
            Self::PlaylistSelectionRequired => {
                language.text("Select a playlist first", "Сначала выберите плейлист.")
            }
            Self::TrackMissing => {
                language.text("Track no longer exists", "Композиция больше не существует.")
            }
            Self::TrackNotInPlaylist => language.text(
                "Track is not in this playlist",
                "Композиция отсутствует в этом плейлисте.",
            ),
            Self::DeskRenameForbidden => language.text(
                "The sorting desk cannot be renamed",
                "Сортировочный стол нельзя переименовать.",
            ),
            Self::DeskRemovalForbidden => language.text(
                "The sorting desk cannot be deleted",
                "Сортировочный стол нельзя удалить.",
            ),
            Self::DeskPositionFixed => language.text(
                "The sorting desk has a fixed position",
                "Позиция сортировочного стола фиксирована.",
            ),
            Self::ReservedPlaylistName => language.text(
                "This name is reserved for the sorting desk",
                "Это имя зарезервировано для сортировочного стола.",
            ),
            Self::InvalidPathEncoding => language.text(
                "File path is not valid Unicode",
                "Путь к файлу содержит неподдерживаемые символы.",
            ),
            Self::PaletteMissing => {
                language.text("Palette no longer exists", "Палитра больше не существует.")
            }
            Self::ImportInterrupted => language.text(
                "Music import was interrupted",
                "Добавление музыкальных файлов прервано.",
            ),
            Self::WaveformInterrupted => language.text(
                "Waveform worker was interrupted",
                "Обработка аудиоволны прервана.",
            ),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message(Language::English))
    }
}

impl Error for AppError {}

pub(crate) fn message(error: &anyhow::Error, language: Language) -> String {
    error.downcast_ref::<AppError>().map_or_else(
        || error.to_string(),
        |error| error.message(language).to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_wording_does_not_change_the_localized_error() {
        let error = anyhow::Error::new(AppError::InvalidFontSize)
            .context("The editor supplied a different diagnostic");
        assert_eq!(
            message(&error, Language::Russian),
            "Размер шрифта должен быть от 10 до 32 пикселей."
        );
        assert_eq!(
            message(&error, Language::English),
            "Font size must be 10-32 pixels"
        );
        let external = anyhow::anyhow!("Unexpected external error");
        assert_eq!(
            message(&external, Language::Russian),
            "Unexpected external error"
        );
    }
}
