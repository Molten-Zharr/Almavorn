use crate::input::Action;

/// Only help content is indexed. Queries never leave the application.
#[derive(Default)]
pub(crate) struct HelpSearch {
    pub query: String,
    pub cursor: usize,
    pub selected_all: bool,
    pub selected: usize,
    pub matches: Vec<(usize, usize)>,
    pub fingerprint: Option<u64>,
    pub browse_offset: usize,
    pub pending_entry: Option<(usize, usize)>,
    pub page_size: usize,
}

impl HelpSearch {
    pub fn active(&self) -> bool {
        !self.query.trim().is_empty()
    }
    pub fn changed(&mut self) {
        self.fingerprint = None;
        self.selected = 0;
        self.matches.clear();
        self.pending_entry = None;
    }
}

pub(crate) fn normalize(text: &str) -> String {
    match text.trim() {
        "+" => return "plus".into(),
        "-" => return "minus".into(),
        "/" => return "slash".into(),
        _ => {}
    }
    text.to_lowercase()
        .replace("++", "+ plus")
        .chars()
        .map(|c| match c {
            'ё' => 'е',
            '↑' => '↑',
            '↓' => '↓',
            '←' => '←',
            '→' => '→',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn stem(word: &str) -> &str {
    // A small suffix list handles common inflections without changing short words.
    for ending in [
        "ировать",
        "овать",
        "иями",
        "ение",
        "ания",
        "ений",
        "ться",
        "ыми",
        "ами",
        "ями",
        "ого",
        "ему",
        "ому",
        "ить",
        "ать",
        "ять",
        "ете",
        "ешь",
        "ing",
        "ов",
        "ев",
        "ах",
        "ях",
        "ые",
        "ый",
        "ий",
        "ая",
        "ое",
        "ую",
        "ии",
        "ия",
        "ие",
        "ей",
        "ed",
        "es",
        "а",
        "я",
        "ы",
        "и",
        "у",
        "ю",
        "е",
        "s",
    ] {
        if let Some(root) = word.strip_suffix(ending)
            && root.chars().count() >= 4
        {
            return root;
        }
    }
    word
}

fn stop_word(word: &str) -> bool {
    matches!(
        word,
        "как"
            | "где"
            | "чтобы"
            | "чтоб"
            | "хочу"
            | "можно"
            | "мне"
            | "меня"
            | "пожалуйста"
            | "это"
            | "этот"
            | "эту"
            | "такое"
            | "так"
            | "ли"
            | "бы"
            | "в"
            | "во"
            | "на"
            | "с"
            | "со"
            | "к"
            | "из"
            | "и"
            | "а"
            | "the"
            | "how"
            | "where"
            | "can"
            | "could"
            | "please"
            | "want"
            | "to"
            | "a"
            | "an"
            | "my"
            | "i"
            | "it"
            | "do"
            | "is"
            | "of"
    )
}

fn words(text: &str) -> Vec<String> {
    let mut result: Vec<_> = normalize(text)
        .split_whitespace()
        .filter(|word| !stop_word(word))
        .map(|word| stem(word).to_owned())
        .collect();
    result.sort();
    result.dedup();
    result
}

fn concept(word: &str) -> Option<usize> {
    const GROUPS: &[&[&str]] = &[
        &["песн", "песня", "композиц", "трек", "song", "track"],
        &["плейлист", "подборк", "playlist", "mix"],
        &[
            "папк",
            "папка",
            "каталог",
            "директор",
            "folder",
            "directory",
        ],
        &[
            "обложк",
            "картинк",
            "изображен",
            "artwork",
            "cover",
            "image",
        ],
        &[
            "горяч",
            "сочетан",
            "комбинац",
            "shortcut",
            "hotkey",
            "binding",
        ],
        &["офлайн", "offline"],
        &[
            "измен",
            "менят",
            "смен",
            "помен",
            "настро",
            "change",
            "edit",
            "customiz",
        ],
        &[
            "перен",
            "передвин",
            "перемес",
            "перетащ",
            "подвин",
            "двиг",
            "move",
            "drag",
            "rearrang",
            "relocat",
        ],
        &["отмен", "вернут", "восстанов", "undo", "restore", "recover"],
        &["шрифт", "букв", "font", "textsize"],
        &["размер", "расшир", "растян", "масштаб", "resize", "size"],
        &[
            "увелич",
            "повыс",
            "повыш",
            "прибав",
            "поднят",
            "increas",
            "rais",
            "boost",
        ],
        &[
            "уменьш",
            "пониз",
            "пониж",
            "убав",
            "сниз",
            "decreas",
            "lower",
            "reduc",
        ],
        &["лист", "прокрут", "скрол", "scroll"],
        &["страниц", "экран", "page", "screen"],
        &["скач", "загруз", "download"],
        &["тег", "теги", "метадан", "tag", "metadata"],
        &[
            "окн",
            "окон",
            "окош",
            "модал",
            "диалог",
            "window",
            "dialog",
            "popup",
        ],
        &["кноп", "button"],
    ];
    GROUPS
        .iter()
        .position(|group| group.iter().any(|prefix| word.starts_with(prefix)))
}

fn one_typo(left: &str, right: &str) -> bool {
    let a: Vec<_> = left.chars().collect();
    let b: Vec<_> = right.chars().collect();
    if a.len().min(b.len()) < 4 || a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    let first = a
        .iter()
        .zip(&b)
        .position(|(a, b)| a != b)
        .unwrap_or(a.len().min(b.len()));
    if a.len() == b.len() {
        a[first.saturating_add(1).min(a.len())..] == b[first.saturating_add(1).min(b.len())..]
            || first + 1 < a.len()
                && a[first] == b[first + 1]
                && a[first + 1] == b[first]
                && a[first + 2..] == b[first + 2..]
    } else if a.len() > b.len() {
        a[first + 1..] == b[first..]
    } else {
        a[first..] == b[first + 1..]
    }
}

fn word_score(query: &str, document: &[String]) -> u32 {
    document
        .iter()
        .map(|word| {
            if query == word {
                12
            } else if query.chars().count() >= 3 && word.starts_with(query) {
                9
            } else if let Some(group) = concept(query)
                && concept(word) == Some(group)
            {
                7
            } else if one_typo(query, word) {
                5
            } else {
                0
            }
        })
        .max()
        .unwrap_or(0)
}

pub(crate) struct Query {
    phrase: String,
    words: Vec<String>,
}

impl Query {
    pub fn new(text: &str) -> Self {
        Self {
            phrase: normalize(text),
            words: words(text),
        }
    }

    pub fn score(&self, text: &str, aliases: &str, category: &str, shortcut: &str) -> u32 {
        if self.words.is_empty() {
            return 0;
        }
        if shortcut
            .split(';')
            .any(|key| !key.is_empty() && normalize(key) == self.phrase)
        {
            return 1000;
        }
        let shortcut = normalize(shortcut);
        let body = words(text);
        let aliases_words = words(aliases);
        let category_words = words(category);
        let key_words = words(&shortcut);
        let mut matched = 0;
        let mut score = 0;
        for word in &self.words {
            let value = word_score(word, &body)
                .max(word_score(word, &aliases_words))
                .max(word_score(word, &key_words))
                .max(word_score(word, &category_words) / 2);
            if value > 0 {
                matched += 1;
                score += value;
            }
        }
        // Do not return a whole category for one common word in a specific question.
        if matched * 3 < self.words.len() * 2 {
            return 0;
        }
        score = score * matched as u32 / self.words.len() as u32;
        if normalize(text).contains(&self.phrase) {
            score += 24;
        }
        if aliases
            .split(';')
            .any(|example| normalize(example).contains(&self.phrase))
        {
            score += 32;
        }
        score
    }
}

/// Extra wording is never displayed as a command or used to execute an action.
pub(crate) fn aliases(action: Action) -> &'static str {
    use Action::*;
    match action {
        TogglePlay => {
            "поставить на паузу; начать слушать; включить песню; продолжить музыку; временно замолчать; pause resume start listening play music"
        }
        Stop => "остановить музыку совсем; выключить песню; тишина; stop silence",
        Next => "пропустить песню; надоела композиция; следующая песня; skip next song",
        Previous => {
            "предыдущая песня; вернуть прошлый трек; что играло перед этим; previous last song"
        }
        VolumeUp => {
            "сделать громче; погромче; плохо слышно; повысить звук; прибавить звук; поднять громкость; make it louder; turn it up; louder increase volume"
        }
        VolumeDown => {
            "сделать тише; потише; слишком громко; понизить звук; убавить звук; уменьшить громкость; make it quieter; turn it down; quieter lower volume"
        }
        SeekForward => {
            "промотать вперед; пропустить вступление; перемотать песню; fast forward seek skip intro"
        }
        SeekBackward => "отмотать назад; переслушать момент; вернуть начало; rewind seek backward",
        CycleRepeat => {
            "убрать повторы; играет по кругу; зацикливается; снова та же песня; повторять один трек; loop repeat same song"
        }
        ToggleShuffle => "случайная музыка; вперемешку; рандом; случайный порядок; shuffle random",
        Equalizer => {
            "настроить звучание; больше низких частот; высокие частоты; глухой звук; эквалайзер; еквалайзер; equaliser eq bass treble sound"
        }
        PlaybackTuner => {
            "ускорить песню; замедлить музыку; высокий голос; низкий голос; тональность; басы; питч; найткор; наоборот; speed tempo pitch nightcore slowed reverse bass"
        }
        CycleVisualizer => {
            "фон мешает; убрать анимацию; выключить искры; мигает фон; чтобы фон не горел; фон раздражает; заставка; визуализация; animation background flashing sparks visualizer"
        }
        NewPlaylist => {
            "создать сборник; новый список музыки; собрать альбом; название папки; куда создается папка; create name new playlist folder"
        }
        CopyPlaylist => {
            "дублировать список; копия подборки; второй такой же плейлист; duplicate copy playlist"
        }
        ComposePlaylists => {
            "объединить подборки; смешать плейлисты; собрать песни вместе; merge combine playlists mix"
        }
        PlaylistFolders => {
            "откуда берется музыка; привязать каталог; папка плейлиста; путь источник; playlist source linked folder path"
        }
        PlaylistUp => {
            "поднять плейлист; список выше; поменять порядок плейлистов; move playlist up order"
        }
        PlaylistDown => {
            "опустить плейлист; список ниже; поменять порядок плейлистов; move playlist down order"
        }
        ToggleEdit => {
            "не могу изменить плейлист; нельзя удалить; не дает редактировать; разблокировать плейлист; защита от изменений; замок; запрет правки; unlock protected locked read only editing"
        }
        Rename => "переименовать композицию; поменять название; новое имя; rename title name",
        Delete => {
            "убрать композицию; удалить песню из списка; очистить плейлист; remove delete song playlist"
        }
        MoveUp => "песня выше; поднять композицию; поменять порядок треков; move track up reorder",
        MoveDown => {
            "песня ниже; опустить композицию; поменять порядок треков; move track down reorder"
        }
        Undo => {
            "вернуть удаленное; случайно удалил; отменить ошибку; откатить изменения; восстановить песню; undo recover restore accidentally deleted"
        }
        Redo => "вернуть отмененное; повторить действие; redo repeat change",
        Mark => {
            "выделить песню; поставить галочку; отметить трек; выбрать композицию; checkbox mark select song"
        }
        PlayMarked => {
            "слушать только выбранное; только отмеченные; галочки; play selected checked marked only"
        }
        PlayWholePlaylist => {
            "вернуть все песни; перестало играть неотмеченное; весь плейлист; play all whole playlist"
        }
        Transfer => {
            "перенести музыку; разложить по папкам; убрать со стола; перенести на флешку; скопировать в плейлист; move files copy destination sorting desk usb"
        }
        Sort => {
            "по алфавиту; по длине; отсортировать песни; порядок названий; sort alphabetical duration artist"
        }
        Metadata => {
            "сведения о файле; формат аудио; битрейт; длительность песни; codec bitrate properties information"
        }
        EditTags => {
            "поменять обложку; исправить исполнителя; название альбома; метаданные; тэги; массовое редактирование; сразу все песни; имя файла; artwork cover artist album metadata tags batch filename"
        }
        ToggleDesk => {
            "черновик музыки; разобрать сырую музыку; сортировочный стол; куда добавлять; sorting desk staging inbox"
        }
        AddFiles => {
            "добавить отдельные песни; открыть mp3 flac; импорт файлов; add import open files"
        }
        AddFolder => {
            "добавить папку группой; сырая музыка из папки; импорт каталога на стол; folder group raw music import"
        }
        AddFolderPlaylists => {
            "несколько папок сразу; из каждой папки отдельный плейлист; импорт коллекции; folders into playlists multiple import collection"
        }
        AddNew => {
            "обновить библиотеку; появились новые песни; досканировать папку; найти недостающие композиции; refresh rescan new missing tracks library"
        }
        Search => {
            "найти песню; поиск по исполнителю; забыл где композиция; search find song artist"
        }
        Filter => {
            "оставить только совпадения; скрыть лишние треки; фильтр списка; filter narrow matches"
        }
        PlaylistPanel => {
            "список плейлистов справа слева; переместить плейлисты; playlist sidebar left right"
        }
        PlayerPanel => "проигрыватель снизу сверху; переместить плеер; player bottom top position",
        Panels => {
            "передвинуть окно; подвинуть блок; переставить панели; изменить размеры; вернуть скрытый блок; интерфейс под себя; move a panel; layout move drag resize panels blocks"
        }
        Lyrics => {
            "слова песни; о чем поют; караоке; субтитры; текст не найден; нет интернета; читать офлайн; другая версия текста; lyrics words singing karaoke subtitles offline"
        }
        Downloads => {
            "скачать музыку; вставить ссылку; куда сохранилось; где скачанная музыка; папка загрузок; ютуб; youtube yt dlp cobalt download link destination saved folder"
        }
        Services => {
            "войти в аккаунт; подключить спотифай; яндекс музыка; ютуб музыка; вход авторизация; spotify yandex youtube music account login streaming connect"
        }
        Mods => {
            "плагины; расширения; модификации; установить мод; создать дополнение; mods plugins extensions addons modding"
        }
        Statistics => {
            "сколько слушал; любимые песни; что недавно играло; количество запусков; история прослушивания; statistics history recent played time starts count"
        }
        Settings => {
            "параметры; изменить язык; сменить тему; поменять цвета; увеличить буквы; переназначить клавиши; настройки; preferences settings language theme colors font key bindings"
        }
        Help => {
            "инструкция; руководство; как пользоваться; список кнопок; help manual guide shortcuts"
        }
        Quit => "закрыть программу; выйти из плеера; сохранить перед выходом; exit quit close app",
        SwitchMode => {
            "порядок хаос; защищенная коллекция; свободная подборка; режим редактирования; order chaos mode collection mix"
        }
        ToggleToolbar => {
            "слишком много кнопок; больше меньше; спрятать верхние кнопки; меню больше; библиотека сервисы оформление; toolbar clutter hide more less menu"
        }
        OpenAddMenu => "меню добавления музыки; add music menu",
        OpenPlaylistMenu => "кнопки плейлиста; playlist menu commands",
        OpenTracksMenu => "кнопки композиции; контекстное меню песни; track song menu commands",
        OpenApplicationMenu => "главное меню; меню программы; application main menu",
    }
}
