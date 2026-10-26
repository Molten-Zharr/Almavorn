use crate::model::Language;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuickButton {
    Add, Playlist, Tracks, Settings, Mode, Edit, Desk, Shuffle, Repeat, Marked,
    Lyrics, SpecialButtons, Layout, Equalizer, Effects, Animation, Compose, Undo, Redo, Search, Filter,
    Services, Downloads, Mods, Statistics, Help, Quit, Visibility, Customize,
}

impl QuickButton {
    pub const ALL: [Self; 29] = [
        Self::Add, Self::Playlist, Self::Tracks, Self::Settings, Self::Mode,
        Self::Edit, Self::Desk, Self::Shuffle, Self::Repeat, Self::Marked,
        Self::Lyrics, Self::SpecialButtons, Self::Layout, Self::Equalizer, Self::Effects, Self::Animation, Self::Compose, Self::Undo, Self::Redo,
        Self::Search, Self::Filter, Self::Services, Self::Downloads, Self::Mods,
        Self::Statistics, Self::Help, Self::Quit, Self::Visibility, Self::Customize,
    ];

    pub fn name(self, language: Language) -> &'static str {
        let (en, ru) = match self {
            Self::Add => ("Add music", "Добавить музыку"),
            Self::Playlist => ("Playlist", "Плейлист"),
            Self::Tracks => ("Tracks", "Композиции"),
            Self::Settings => ("Settings", "Настройки"),
            Self::Mode => ("Order / Chaos", "Порядок / Хаос"),
            Self::Edit => ("Edit playlist", "Редактирование плейлиста"),
            Self::Desk => ("Add to sorting desk", "Добавлять на сортировочный стол"),
            Self::Shuffle => ("Shuffle", "Случайный порядок"),
            Self::Repeat => ("Repeat", "Повтор"),
            Self::Marked => ("Checked tracks only", "Только отмеченные"),
            Self::Lyrics => ("Lyrics panel", "Панель текста песни"),
            Self::SpecialButtons => ("Player special buttons", "Спецкнопки проигрывателя"),
            Self::Layout => ("Edit layout", "Редактирование блоков"),
            Self::Equalizer => ("Equalizer", "Эквалайзер"),
            Self::Effects => ("Playback effects", "Эффекты воспроизведения"),
            Self::Animation => ("Background animation", "Анимация фона"),
            Self::Compose => ("Compose playlists", "Собрать плейлисты"),
            Self::Undo => ("Undo", "Отменить"),
            Self::Redo => ("Redo", "Повторить"),
            Self::Search => ("Search", "Поиск"),
            Self::Filter => ("Filter", "Фильтр"),
            Self::Services => ("Music services", "Музыкальные сервисы"),
            Self::Downloads => ("Downloads", "Загрузки"),
            Self::Mods => ("Mods", "Моды"),
            Self::Statistics => ("Listening statistics", "Статистика прослушивания"),
            Self::Help => ("Help", "Помощь"),
            Self::Quit => ("Quit", "Выход"),
            Self::Visibility => ("Show blocks/buttons", "Отображение блоков/кнопок"),
            Self::Customize => ("Quick access", "Быстрый доступ"),
        };
        language.text(en, ru)
    }

    pub fn category(self, language: Language) -> &'static str {
        use QuickButton::*;
        match self {
            Add | Playlist | Tracks | Settings => language.text("Main buttons", "Основные кнопки"),
            Mode | Edit | Desk | Shuffle | Repeat | Marked | Lyrics | SpecialButtons | Layout | Equalizer | Effects | Animation => language.text("Modes and state", "Режимы и состояния"),
            Compose | Undo | Redo | Search | Filter => language.text("Library", "Библиотека"),
            Services | Downloads | Mods => language.text("Music services", "Музыкальные сервисы"),
            Statistics | Help | Quit => language.text("Information", "Информация"),
            Visibility | Customize => language.text("Appearance", "Оформление"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct QuickAccess {
    pub pinned: Vec<QuickButton>,
    // Keep the saved key compatible; only layout editing appears automatically.
    pub show_active: bool,
}

impl Default for QuickAccess {
    fn default() -> Self {
        Self { pinned: vec![QuickButton::Add, QuickButton::Playlist, QuickButton::Tracks, QuickButton::Settings], show_active: true }
    }
}

#[derive(Clone, Copy)]
pub enum QuickAction { Open, Toggle(usize), All(bool), Reset, Active }
