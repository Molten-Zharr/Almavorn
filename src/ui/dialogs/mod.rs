mod bindings;
mod browser;
mod color_picker;
mod equalizer;
mod forms;
mod spotify;
mod lyrics;
mod downloads;
mod download_form;
pub(super) mod services;
pub(super) mod mods;
pub(super) mod help;
mod panels;
mod playlists;
mod search;
mod scan_folders;
mod statistics;
mod tags;
mod transfer;
mod text;
mod tuner;
pub(crate) use text::keyboard;

use self::{bindings::binding_dialog, browser::browser_dialog, text::text_dialog};
use super::{
    theme::Palette,
    widgets::{button, buttons, clean, dialog_footer, modal},
};
use crate::{
    app::{App, Dialog, Target},
    model::duration_text,
};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};

pub(super) fn render_dialog(
    frame: &mut Frame,
    app: &mut App,
    dialog: &mut Dialog,
    palette: Palette,
) {
    let mut area = frame.area();
    area.height = area
        .height
        .saturating_sub(if dialog.is_settings() { 6 } else { 3 });
    if dialog.is_settings() && (area.width < 24 || area.height < 12) {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window to edit this setting. Esc: back",
                "Увеличьте окно для редактирования. Esc: назад",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1)),
        );
        button(
            frame,
            app,
            Rect::new(
                area.x,
                area.bottom().saturating_sub(1),
                area.width,
                area.height.min(1),
            ),
            app.text("Back", "Назад"),
            Target::CloseDialog,
            true,
            palette,
        );
        return;
    }
    match dialog {
        Dialog::ScanFolders(view) => scan_folders::render(frame, app, view, area, palette),
        Dialog::Form(form)=>forms::render(frame,app,form,area,palette),
        Dialog::Lyrics(view)=>lyrics::render(frame,app,view,area,palette),
        Dialog::Downloads(view)=>downloads::render(frame,app,view,area,palette),
        Dialog::Services(view)=>services::render(frame,app,view,area,palette),
        Dialog::Mods(view)=>mods::render(frame,app,view,area,palette),
        Dialog::Tags(editor) => tags::render(frame, app, editor, area, palette),
        Dialog::Statistics => statistics::render(frame, app, area, palette),
        Dialog::PlaybackTuner { selected } => tuner::render(frame, app, selected, area, palette),
        Dialog::Equalizer {
            selected,
            presets,
            preset_selected,
        } => {
            equalizer::render(
                frame,
                app,
                selected,
                *presets,
                preset_selected,
                area,
                palette,
            );
        }
        Dialog::Commands {
            menu,
            selected,
            anchor,
        } => {
            super::menus::render(frame, app, *menu, selected, *anchor, palette);
        }
        Dialog::Panels { selected, expanded } => {
            panels::render(frame, app, selected, expanded, area, palette);
        }
        Dialog::Text(dialog) => text_dialog(frame, app, dialog, area, palette),
        Dialog::Search(search) => self::search::render(frame, app, search, area, palette),
        Dialog::Browser(browser) => browser_dialog(frame, app, browser, area, palette),
        Dialog::Folders { playlist, selected } => {
            playlists::folders(frame, app, *playlist, selected, area, palette)
        }
        Dialog::ComposePlaylists { selected, ids } => {
            playlists::compose(frame, app, selected, ids, None, area, palette)
        }
        Dialog::ComposeTracks {
            selected,
            entries,
            included,
            ..
        } => {
            let chosen: Vec<_> = entries
                .iter()
                .copied()
                .filter(|id| included.contains(id))
                .collect();
            playlists::compose(frame, app, selected, &chosen, Some(entries), area, palette)
        }
        Dialog::RemovePlaylist { name, confirm, .. } => {
            let body = format!(
                "{}\n\n{}",
                name,
                app.text(
                    "Music files and saved tags are kept.",
                    "Музыкальные файлы и сохраненные теги останутся."
                )
            );
            let width = area.width.saturating_sub(6).clamp(1, 86);
            let height = (Paragraph::new(body.as_str())
                .wrap(Wrap { trim: false })
                .line_count(width) as u16)
                .saturating_add(5);
            let inner = modal(
                frame,
                app,
                area,
                app.text("Delete playlist?", "Удалить плейлист?").into(),
                height,
                palette,
            );
            frame.render_widget(
                Paragraph::new(body)
                    .style(palette.text())
                    .wrap(Wrap { trim: false }),
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(2),
                ),
            );
            buttons(
                frame,
                app,
                Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
                vec![
                    (
                        app.text("Yes", "Да").into(),
                        if *confirm {
                            Target::Submit
                        } else {
                            Target::DeletePlaylistAnswer(true)
                        },
                        !app.busy(),
                    ),
                    (
                        app.text("No", "Нет").into(),
                        if *confirm {
                            Target::DeletePlaylistAnswer(false)
                        } else {
                            Target::Submit
                        },
                        true,
                    ),
                ],
                palette,
            );
        }
        Dialog::RemoveEntry { title, .. } => {
            let inner = modal(
                frame,
                app,
                area,
                app.text("Remove track?", "Убрать композицию?").into(),
                10,
                palette,
            );
            frame.render_widget(
                Paragraph::new(format!(
                    "{}\n\n{}",
                    title,
                    app.text(
                        "Remove from this playlist? The music file and saved tags are kept.",
                        "Убрать из этого плейлиста? Музыкальный файл и сохраненные теги останутся."
                    )
                ))
                .style(palette.text())
                .wrap(Wrap { trim: false }),
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(2),
                ),
            );
            buttons(
                frame,
                app,
                Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
                vec![
                    (app.text("Remove", "Убрать").into(), Target::Submit, true),
                    (
                        app.text("Cancel", "Отмена").into(),
                        Target::CloseDialog,
                        true,
                    ),
                ],
                palette,
            );
        }
        Dialog::RemoveGroup { name, entries, playlist, .. } => {
            let inner = modal(frame, app, area, app.text("Remove group?", "Убрать группу?").into(), 13, palette);
            let playlist_name = app.playlists().iter().find(|item| item.id == *playlist)
                .map_or("", |item| item.display_name(app.settings.preferences.language));
            frame.render_widget(
                Paragraph::new(format!("{}\n{}: {}\n{}: {}\n\n{}", name,
                    app.text("Playlist", "Плейлист"), playlist_name,
                    app.text("Tracks", "Композиции"), entries.len(),
                    app.text("Remove every track in this group, including tracks hidden by the filter? Music files, tags and other playlists are kept.",
                        "Убрать все композиции группы, включая скрытые фильтром? Музыкальные файлы, теги и остальные плейлисты сохранятся.")))
                    .style(palette.text()).wrap(Wrap { trim: false }),
                Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(2)),
            );
            buttons(frame, app, Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2), vec![
                (app.text("Remove group", "Убрать группу").into(), Target::Submit, !app.busy()),
                (app.text("Cancel", "Отмена").into(), Target::CloseDialog, true),
            ], palette);
        }
        Dialog::Transfer { selected, .. } => transfer::render(frame, app, *selected, area, palette),
        Dialog::MoveFiles(view) => transfer::move_files(frame, app, view, area, palette),
        Dialog::Settings { .. } => super::settings::render_settings(frame, app, palette),
        Dialog::ConfirmResetBindings { confirm } => {
            let body = app.text(
                "Replace this profile's shortcuts with the defaults?",
                "Заменить горячие клавиши текущего профиля стандартными?",
            );
            let width = area.width.saturating_sub(6).clamp(1, 86);
            let height = (Paragraph::new(body)
                .wrap(Wrap { trim: false })
                .line_count(width) as u16)
                .saturating_add(5);
            let inner = modal(
                frame,
                app,
                area,
                app.text("Reset shortcuts?", "Сбросить горячие клавиши?")
                    .into(),
                height,
                palette,
            );
            frame.render_widget(
                Paragraph::new(body)
                    .style(palette.text())
                    .wrap(Wrap { trim: false }),
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(2),
                ),
            );
            buttons(
                frame,
                app,
                Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
                vec![
                    (
                        app.text("Yes, reset", "Да, сбросить").into(),
                        if *confirm {
                            Target::Submit
                        } else {
                            Target::ResetBindingsAnswer(true)
                        },
                        true,
                    ),
                    (
                        app.text("CANCEL!", "ОТМЕНА!").into(),
                        if *confirm {
                            Target::ResetBindingsAnswer(false)
                        } else {
                            Target::Submit
                        },
                        true,
                    ),
                ],
                palette,
            );
        }
        Dialog::ConfirmSettings { catalog, name, .. } => {
            let inner = modal(
                frame,
                app,
                area,
                app.text("Delete settings item?", "Удалить элемент настроек?")
                    .into(),
                14,
                palette,
            );
            let explanation = match catalog {
                crate::app::SettingsCatalog::Profile => app.text("The profile will be removed. If active, another profile is activated. Your music library is kept.", "Профиль будет удалён. Если он активен, включится другой. Музыкальная библиотека сохраняется."),
                crate::app::SettingsCatalog::Palette => app.text("The palette will be removed. Its themes and profiles use the first remaining palette.", "Палитра будет удалена. Использующие её темы и профили перейдут на первую оставшуюся палитру."),
                crate::app::SettingsCatalog::Preset => app.text("The preset will be removed. Your current appearance is kept.", "Пресет будет удалён. Текущее оформление сохраняется."),
            };
            frame.render_widget(
                Paragraph::new(format!(
                    "{name}\n\n{explanation}\n\n{}",
                    app.text(
                        "Enter: delete - Esc: cancel",
                        "Enter: удалить - Esc: отмена"
                    )
                ))
                .style(palette.text())
                .wrap(Wrap { trim: false }),
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(2),
                ),
            );
            buttons(
                frame,
                app,
                Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
                vec![
                    (app.text("Delete", "Удалить").into(), Target::Submit, true),
                    (
                        app.text("Cancel", "Отмена").into(),
                        Target::CloseDialog,
                        true,
                    ),
                ],
                palette,
            );
        }
        Dialog::SettingsHelp {
            title,
            description,
            offset,
        } => {
            let inner = modal(frame, app, area, title.clone(), 22, palette);
            help::scroll_text(
                frame,
                app,
                offset,
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(3),
                ),
                format!(
                    "{description}\n\n{}",
                    app.text(
                        "↑↓: scroll · Esc: return to settings.",
                        "↑↓: прокрутка · Esc: вернуться в настройки."
                    )
                ),
                palette,
            );
            dialog_footer(frame, app, inner, palette);
        }
        Dialog::CopyableHelp {
            title,
            description,
            offset,
        } | Dialog::FileMoveReport {
            title,
            description,
            offset,
        } => {
            let inner = modal(frame, app, area, title.clone(), 24, palette);
            help::copyable_text(frame, app, offset, inner, description, palette);
        }
        Dialog::Help { offset } => {
            let inner = modal(frame, app, area,
                app.text("Help and keyboard shortcuts", "Помощь и горячие клавиши").into(), 44, palette);
            let lines = [
                ("Order — your saved collection. To change a playlist, click Edit. When finished, click Done to protect it from accidental changes. Track order is saved.",
                 "Порядок — ваша постоянная коллекция. Чтобы изменить плейлист, нажмите \"Редактировать\". Закончили — нажмите \"Готово\", и плейлист снова защищен от случайных изменений. Порядок треков сохраняется."),
                ("Chaos — a place to build mixes. Add, remove and rearrange tracks right away. Build a mix creates a new playlist and keeps the originals.",
                 "Хаос — место для подборок. Сразу добавляйте, убирайте и переставляйте треки. \"Собрать подборку\" создает новый плейлист, сохраняя исходные."),
                ("Sorting desk — sort out music before adding it to your collection. Add files, edit their names and tags, then move them to playlists. The desk is shared by Order and Chaos.",
                 "Сортировочный стол — место для разбора музыки. Добавьте файлы, исправьте названия и теги, затем перенесите в плейлисты. Стол общий для Порядка и Хаоса."),
                ("Add files sends music to the desk in Order. In Chaos it uses the open playlist; check Add to desk to use the desk instead.",
                 "\"Добавить файлы\": в Порядке музыка попадает на стол. В Хаосе — в открытый плейлист. Чтобы отправить ее на стол, включите \"Добавлять на стол\"."),
                ("Add folder puts music on the desk, grouped by folder. Folders to playlists creates a separate playlist for each folder you check.",
                 "\"Добавить папку\": музыка появится на столе группами по папкам. \"Папки в плейлисты\": из каждой отмеченной папки получится отдельный плейлист."),
                ("Add new finds tracks that appeared in your music folders. In a playlist it checks that playlist's folders; on the desk it checks the folders of its groups. Existing tracks are skipped.",
                 "\"Добавить новые\" находит музыку, которая появилась в ваших папках. В плейлисте проверяются его папки, на столе — папки его групп. Уже добавленные треки пропускаются."),
                ("To scan several playlists, open Add new from the Add music menu and check the folders. New tracks go to their own playlists or desk groups.",
                 "Чтобы проверить несколько плейлистов, откройте \"Добавить новые\" через меню добавления музыки и отметьте папки. Новые треки попадут каждый в свой плейлист или группу стола."),
                ("Edit several tracks: check them on the desk and open Tags and file name. Only fields you change and tags you mark for removal will be changed in every file.",
                 "Чтобы изменить несколько треков, отметьте их на столе и откройте \"Теги и имя файла\". Ко всем файлам применятся только ваши правки и выбранное удаление тегов."),
                ("Saving tags changes the audio files themselves, so every playlist using them sees the changes. Undo for playlists cannot undo a file write.",
                 "Сохранение тегов меняет сами аудиофайлы. Изменения будут видны во всех плейлистах с этими треками. Отмена действий плейлиста не отменяет запись в файл."),
                ("Rename a file: select one track and open the File tab in its tag editor. Enter a name or use a naming template.",
                 "Чтобы переименовать файл, выберите один трек и откройте вкладку \"Файл\" в редакторе тегов. Введите имя или задайте шаблон названия."),
                ("Move a file: click Move and choose a playlist folder. The file moves there, leaves the desk and becomes the first track in the destination playlist. Almavorn updates its path everywhere.",
                 "Чтобы перенести файл, нажмите \"Перенести\" и выберите папку плейлиста. Файл переедет туда, исчезнет со стола и станет первым в выбранном плейлисте. Плеер сам обновит путь везде, где используется трек."),
                ("Undo keeps up to 100 actions until you close the player. Chaos and the desk have separate histories. If you undo and then make a new change, the old undone action can no longer be redone.",
                 "Отмена хранит до 100 действий до закрытия плеера. У Хаоса и стола свои истории. Если после отмены сделать новое действие, вернуть старое через \"Повторить\" уже не получится."),
                ("Shuffle and repeat work the same in both modes. You can browse another playlist while music continues; the player still shows which playlist is playing.",
                 "Случайный порядок и повтор работают одинаково в обоих режимах. Пока играет музыка, можно открывать другие плейлисты. В проигрывателе по-прежнему видно, какой плейлист звучит."),
                ("Play checked tracks only loops the tracks with checkboxes. To include the rest again, choose Play the whole playlist.",
                 "\"Прослушивать только отмеченные\" повторяет по кругу треки с галочками. Чтобы снова слушать все, выберите \"Воспроизводить весь плейлист\"."),
                ("Profiles save your themes, layout, buttons and shortcuts. Change a shortcut in Settings and Help will show the new keys.",
                 "Профили сохраняют ваши темы, расположение блоков, кнопки и клавиши. Измените сочетание в настройках — в справке появятся новые клавиши."),
            ].into_iter().map(|(en, ru)| app.text(en, ru).to_owned()).collect();
            help::help_dialog(frame, app, offset, inner, lines, palette);
        }

        Dialog::Metadata { track, offset } => {
            let inner = modal(
                frame,
                app,
                area,
                app.text(
                    "Saved track information",
                    "Сохраненные сведения о композиции",
                )
                .into(),
                40,
                palette,
            );
            let mut lines = vec![
                format!(
                    "{}: {}",
                    app.text("Title", "Название"),
                    metadata_text(app, &track.title)
                ),
                format!(
                    "{}: {}",
                    app.text("Artist", "Исполнитель"),
                    metadata_text(app, &track.artist)
                ),
                format!(
                    "{}: {}",
                    app.text("Album", "Альбом"),
                    metadata_text(app, &track.album)
                ),
                format!(
                    "{}: {}",
                    app.text("Duration", "Длительность"),
                    duration_text(track.duration_ms)
                ),
                format!("{}: {}", app.text("File", "Файл"), track.path.display()),
                String::new(),
                app.text(
                    "Saved file tags (read-only view):",
                    "Сохраненные теги файла (просмотр):",
                )
                .into(),
            ];
            if let Ok(tags) =
                serde_json::from_str::<std::collections::BTreeMap<String, Vec<String>>>(&track.tags)
            {
                if tags.is_empty() {
                    lines.push(app.text("Empty", "Пусто").into());
                }
                lines.extend(tags.into_iter().map(|(key, values)| {
                    let values: Vec<_> = values
                        .iter()
                        .map(|value| metadata_tag_value(app, value))
                        .collect();
                    format!(
                        "{}: {}",
                        metadata_tag_name(app, &key),
                        metadata_text(app, &values.join("; "))
                    )
                }));
            } else {
                lines.push(
                    app.text(
                        "Saved tags could not be read",
                        "Не удалось прочитать сохраненные теги",
                    )
                    .into(),
                );
            }
            help::scroll_text(
                frame,
                app,
                offset,
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(3),
                ),
                lines.join("\n"),
                palette,
            );
            dialog_footer(frame, app, inner, palette);
        }
        Dialog::CaptureBinding { index } => {
            binding_dialog(frame, app, *index, area, palette);
        }
    }
}

fn metadata_tag_name(app: &App, key: &str) -> String {
    let field = key.split_once('.').map_or(key, |(_, field)| field);
    let (en, ru) = match field {
        "TrackTitle" => ("Title", "Название"),
        "TrackSubtitle" => ("Subtitle", "Подзаголовок"),
        "TrackArtist" => ("Artist", "Исполнитель"),
        "TrackArtists" => ("Artists", "Исполнители"),
        "AlbumTitle" => ("Album", "Альбом"),
        "AlbumArtist" => ("Album artist", "Исполнитель альбома"),
        "AlbumArtists" => ("Album artists", "Исполнители альбома"),
        "Composer" => ("Composer", "Композитор"),
        "Lyricist" => ("Lyricist", "Автор текста"),
        "Conductor" => ("Conductor", "Дирижер"),
        "Performer" => ("Performer", "Участник исполнения"),
        "Producer" => ("Producer", "Продюсер"),
        "Publisher" => ("Publisher", "Издатель"),
        "Label" => ("Label", "Лейбл"),
        "Genre" => ("Genre", "Жанр"),
        "TrackNumber" => ("Track number", "Номер композиции"),
        "TrackTotal" => ("Total tracks", "Всего композиций"),
        "DiscNumber" => ("Disc number", "Номер диска"),
        "DiscTotal" => ("Total discs", "Всего дисков"),
        "Year" => ("Year", "Год"),
        "RecordingDate" => ("Recording date", "Дата записи"),
        "ReleaseDate" => ("Release date", "Дата выпуска"),
        "OriginalReleaseDate" => ("Original release date", "Дата первого выпуска"),
        "EncoderSoftware" => ("Encoding software", "Программа кодирования"),
        "EncoderSettings" => ("Encoding settings", "Настройки кодирования"),
        "EncodedBy" => ("Encoded by", "Кодирование"),
        "Comment" => ("Comment", "Комментарий"),
        "Description" => ("Description", "Описание"),
        "Lyrics" => ("Lyrics", "Текст песни"),
        "Language" => ("Language", "Язык"),
        "CopyrightMessage" => ("Copyright", "Авторские права"),
        "MetadataNote" => ("Metadata note", "Примечание к тегам"),
        _ => {
            let mut name = String::new();
            let mut previous_lowercase = false;
            for c in field.chars() {
                if c.is_uppercase() && previous_lowercase {
                    name.push(' ');
                }
                name.push(c);
                previous_lowercase = c.is_lowercase();
            }
            return clean(&name);
        }
    };
    app.text(en, ru).into()
}

fn metadata_text(app: &App, value: &str) -> String {
    let text = clean(value);
    if text.trim().is_empty() {
        app.text("Empty", "Пусто").into()
    } else {
        text
    }
}

fn metadata_tag_value(app: &App, value: &str) -> String {
    if let Some(quoted) = value
        .strip_prefix("Text(")
        .or_else(|| value.strip_prefix("Locator("))
        .and_then(|value| value.strip_suffix(')'))
    {
        return metadata_text(
            app,
            &decode_tag_text(quoted).unwrap_or_else(|| quoted.to_owned()),
        );
    }
    if let Some(bytes) = value
        .strip_prefix("Binary(")
        .and_then(|value| value.strip_suffix(')'))
        .and_then(|value| serde_json::from_str::<Vec<u8>>(value).ok())
    {
        if bytes.is_empty() {
            return app.text("Empty", "Пусто").into();
        }
        return format!(
            "{}: {} {}",
            app.text("Binary data", "Двоичные данные"),
            bytes.len(),
            app.text("bytes", "байт")
        );
    }
    metadata_text(app, value)
}

// Imported values use Rust's Debug string escapes, including \u{...} and \0.
fn decode_tag_text(quoted: &str) -> Option<String> {
    let mut chars = quoted.strip_prefix('"')?.strip_suffix('"')?.chars();
    let mut text = String::new();
    while let Some(c) = chars.next() {
        if c != '\\' {
            text.push(c);
            continue;
        }
        text.push(match chars.next()? {
            '"' => '"',
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '0' => '\0',
            'u' => {
                if chars.next()? != '{' {
                    return None;
                }
                let mut hex = String::new();
                loop {
                    match chars.next()? {
                        '}' => break,
                        c if c.is_ascii_hexdigit() && hex.len() < 6 => hex.push(c),
                        _ => return None,
                    }
                }
                char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?
            }
            _ => return None,
        });
    }
    Some(text)
}
