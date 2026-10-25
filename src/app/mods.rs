use super::{
    App, Dialog, FormField, FormPurpose,
    background::{LatestJob, background},
};
use crate::{
    input::{Key, KeyPress},
    mods::{self, Command, Mod, Output},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
enum Operation {
    Load,
    Package(PathBuf),
    Install(Mod),
    Save(Mod),
    Remove(String),
    Export(Mod, PathBuf),
    Run(Mod, String, Value),
}
#[derive(Clone)]
struct Request {
    operation: Operation,
    cancel: Arc<AtomicBool>,
}
enum Response {
    Loaded(Vec<Mod>),
    Package(Mod),
    Saved(Mod),
    Removed(String),
    Exported,
    Run(String, Output),
    Failed(String, String),
}
pub struct ModsState {
    pub rows: Vec<Mod>,
    pub error: Option<String>,
    pub panels: HashMap<String, String>,
    pub(super) pending_install: Option<Box<Mod>>,
    job: LatestJob<Request, Response>,
    cancel: Option<Arc<AtomicBool>>,
    events: VecDeque<(String, String)>,
    initialized: bool,
    last_track: Option<i64>,
    last_tick: Instant,
}
impl Default for ModsState {
    fn default() -> Self {
        Self {
            rows: vec![],
            error: None,
            panels: HashMap::new(),
            pending_install: None,
            job: LatestJob::default(),
            cancel: None,
            events: VecDeque::new(),
            initialized: false,
            last_track: None,
            last_tick: Instant::now(),
        }
    }
}
impl ModsState {
    pub(super) fn cancel_all(&self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}
impl Drop for ModsState {
    fn drop(&mut self) {
        self.cancel_all();
    }
}
#[derive(Clone, Default)]
pub struct ModsView {
    pub selected: usize,
    pub offset: usize,
    pub opened: bool,
    pub action: usize,
    pub panel_offset: usize,
    pub remove: Option<String>,
    pub install: Option<Box<Mod>>,
}
#[derive(Clone, Copy)]
pub enum ModAction {
    ActionPage(i32),
    Row(usize),
    Open,
    New,
    Install,
    ConfirmInstall,
    Enable,
    Edit,
    Code,
    Settings,
    Remove,
    ConfirmRemove,
    Export,
    Run(usize),
    Refresh,
    Cancel,
    Back,
    Guide,
}

impl App {
    pub fn mods_loading(&self) -> bool {
        self.mods.job.requested().is_some()
    }
    pub(super) fn initialize_mods(&mut self) {
        self.mod_request(Operation::Load);
    }
    pub(super) fn open_mods(&mut self) {
        self.open_settings_page(super::SettingsPage::Mods);
        self.view.settings.focus = super::SettingsFocus::Parameters;
    }
    pub(super) fn ensure_mods_loaded(&mut self) {
        if !self.mods.initialized && !self.mods_loading() {
            self.initialize_mods();
        }
    }
    fn mod_request(&mut self, operation: Operation) {
        self.mods.cancel_all();
        let cancel = Arc::new(AtomicBool::new(false));
        self.mods.cancel = Some(cancel.clone());
        self.mods.error = None;
        self.mods.job.request(Request { operation, cancel });
        self.start_mod_job();
    }
    fn start_mod_job(&mut self) {
        let data = self.data_directory();
        let runtime = self.runtime.as_ref().expect("runtime");
        self.mods.job.start(|request| {
            let request = request.clone();
            background(runtime, move || {
                ensure!(
                    !request.cancel.load(Ordering::Relaxed),
                    "Операция отменена."
                );
                match request.operation {
                    Operation::Load => Ok(Response::Loaded(mods::list(&data)?)),
                    Operation::Package(path) => Ok(Response::Package(mods::read_package(&path)?)),
                    Operation::Install(value) => Ok(Response::Saved(mods::install(&data, value)?)),
                    Operation::Save(value) => {
                        Ok(Response::Saved(mods::save_configuration(&data, value)?))
                    }
                    Operation::Remove(id) => {
                        mods::remove(&data, &id)?;
                        Ok(Response::Removed(id))
                    }
                    Operation::Export(value, path) => {
                        mods::export(&value, &path)?;
                        Ok(Response::Exported)
                    }
                    Operation::Run(mut value, event, context) => {
                        match mods::execute(&value, &event, context, request.cancel.clone()) {
                            Ok(output) => {
                                ensure!(
                                    !request.cancel.load(Ordering::Relaxed),
                                    "Операция отменена."
                                );
                                if let Some(state) = &output.state {
                                    value.state = state.clone();
                                    mods::persist_runtime(&data, &value)?;
                                }
                                Ok(Response::Run(value.manifest.id, output))
                            }
                            Err(error) => {
                                if !request.cancel.load(Ordering::Relaxed) {
                                    value.enabled = false;
                                    mods::persist_runtime(&data, &value)?;
                                }
                                Ok(Response::Failed(value.manifest.id, format!("{error:#}")))
                            }
                        }
                    }
                }
            })
        });
    }
    pub(super) fn save_mod(&mut self, value: Mod) {
        self.view.dialog = Some(Dialog::Mods(ModsView::default()));
        self.mod_request(Operation::Save(value));
    }
    pub(super) fn import_mod(&mut self, path: PathBuf) {
        self.mods.pending_install = None;
        self.view.dialog = Some(Dialog::Mods(ModsView::default()));
        self.mod_request(Operation::Package(path));
    }
    pub(super) fn export_mod(&mut self, value: Mod, path: PathBuf) {
        self.view.dialog = Some(Dialog::Mods(ModsView::default()));
        self.mod_request(Operation::Export(value, path));
    }
    fn queue_mod_event(&mut self, id: String, event: String) {
        if event.starts_with("action:") {
            let position = self
                .mods
                .events
                .iter()
                .position(|(_, e)| !e.starts_with("action:"))
                .unwrap_or(self.mods.events.len());
            if position >= 64 {
                self.mods.error = Some("Дождитесь выполнения предыдущих действий мода.".into());
                return;
            }
            self.mods.events.insert(position, (id, event));
        } else if !self
            .mods
            .events
            .iter()
            .any(|(i, e)| i == &id && e == &event)
        {
            self.mods.events.push_back((id, event));
        }
    }
    pub(super) fn tick_mods(&mut self) -> Result<()> {
        if let Some(completed) = self.mods.job.poll() {
            self.start_mod_job();
            if completed.current {
                match completed.result {
                    Ok(Response::Loaded(rows)) => {
                        self.mods.rows = rows;
                        self.mods.initialized = true;
                        let ids = self
                            .mods
                            .rows
                            .iter()
                            .filter(|m| {
                                m.enabled && m.manifest.events.iter().any(|e| e == "startup")
                            })
                            .map(|m| m.manifest.id.clone())
                            .collect::<Vec<_>>();
                        for id in ids {
                            self.queue_mod_event(id, "startup".into());
                        }
                    }
                    Ok(Response::Package(value)) => {
                        if let Some(Dialog::Mods(view)) = &mut self.view.dialog {
                            view.install = Some(Box::new(value));
                        } else {
                            self.mods.pending_install = Some(Box::new(value));
                        }
                    }
                    Ok(Response::Saved(value)) => {
                        let id = value.manifest.id.clone();
                        let startup =
                            value.enabled && value.manifest.events.iter().any(|e| e == "startup");
                        if let Some(row) = self.mods.rows.iter_mut().find(|m| m.manifest.id == id) {
                            *row = value;
                        } else {
                            self.mods.rows.push(value);
                        }
                        self.mods.events.retain(|(i, _)| i != &id);
                        if startup {
                            self.queue_mod_event(id, "startup".into());
                        }
                        self.message(self.text("Mod saved", "Мод сохранен").into());
                    }
                    Ok(Response::Removed(id)) => {
                        self.mods.rows.retain(|m| m.manifest.id != id);
                        self.mods.panels.remove(&id);
                        self.mods.events.retain(|(i, _)| i != &id);
                    }
                    Ok(Response::Exported) => {
                        self.message(self.text("Mod archive saved", "Архив мода сохранен").into())
                    }
                    Ok(Response::Run(id, output)) => {
                        if let Some(index) = self
                            .mods
                            .rows
                            .iter()
                            .position(|m| m.manifest.id == id && m.enabled)
                        {
                            if let Some(state) = output.state {
                                let mut value = self.mods.rows[index].clone();
                                value.state = state;
                                self.mods.rows[index] = value;
                            }
                            if !output.message.is_empty() {
                                self.message(output.message);
                            }
                            self.mods.panels.insert(id, output.panel);
                            for command in output.commands {
                                if let Err(error) = self.mod_command(command) {
                                    self.mods.error = Some(format!("{error:#}"));
                                    break;
                                }
                            }
                        }
                    }
                    Ok(Response::Failed(id, error)) => {
                        self.mods.error = Some(error);
                        if let Some(row) = self.mods.rows.iter_mut().find(|m| m.manifest.id == id) {
                            row.enabled = false;
                        }
                        self.mods.events.retain(|(i, _)| i != &id);
                    }
                    Err(error) => {
                        self.mods.error = Some(format!("{error:#}"));
                    }
                }
            }
        }
        let track = self.playback.current.as_ref().map(|t| t.id);
        if track != self.mods.last_track {
            self.mods.last_track = track;
            let ids = self
                .mods
                .rows
                .iter()
                .filter(|m| m.enabled && m.manifest.events.iter().any(|e| e == "track_changed"))
                .map(|m| m.manifest.id.clone())
                .collect::<Vec<_>>();
            for id in ids {
                self.queue_mod_event(id, "track_changed".into());
            }
        }
        if self.mods.last_tick.elapsed() >= Duration::from_secs(1) {
            self.mods.last_tick = Instant::now();
            let ids = self
                .mods
                .rows
                .iter()
                .filter(|m| m.enabled && m.manifest.events.iter().any(|e| e == "tick"))
                .map(|m| m.manifest.id.clone())
                .collect::<Vec<_>>();
            for id in ids {
                self.queue_mod_event(id, "tick".into());
            }
        }
        if !self.mods_loading()
            && let Some((id, event)) = self.mods.events.pop_front()
        {
            if let Some(value) = self
                .mods
                .rows
                .iter()
                .find(|m| m.manifest.id == id && m.enabled)
                .cloned()
            {
                let context = self.mod_context(&value);
                self.mod_request(Operation::Run(value, event, context));
            }
        }
        Ok(())
    }
    fn mod_context(&self, value: &Mod) -> Value {
        let track = |t: &crate::model::Track| json!({"id":t.id,"title":t.title,"artist":t.artist,"album":t.album,"duration_ms":t.duration_ms});
        let mut context = json!({"api":1,"playing":self.playback.current.as_ref().map(track),"selected":self.entry().map(|e|track(&e.track)),"position_ms":self.position_ms(),"paused":self.paused(),"volume":self.settings.preferences.volume,"mode":self.settings.mode.key()});
        if value.permission("library_read") {
            let mut remaining = 2000;
            let playlists = self
                .library
                .playlists
                .iter()
                .take(128)
                .map(|p| {
                    let entries = p
                        .entries
                        .iter()
                        .take(remaining.min(200))
                        .map(|e| {
                            let mut t = track(&e.track);
                            t["entry_id"] = json!(e.id);
                            t
                        })
                        .collect::<Vec<_>>();
                    remaining = remaining.saturating_sub(entries.len());
                    json!({"id":p.id,"name":p.name,"track_count":p.entries.len(),"tracks":entries})
                })
                .collect::<Vec<_>>();
            context["playlists"] = json!(playlists);
        }
        if value.permission("appearance") {
            context["palettes"] = json!(
                self.settings
                    .palettes
                    .iter()
                    .map(|p| json!({"id":p.id,"name":p.name}))
                    .collect::<Vec<_>>()
            );
        }
        context
    }
    fn mod_command(&mut self, command: Command) -> Result<()> {
        ensure!(
            !self.writing_tags(),
            "Дождитесь записи изменений аудиофайла."
        );
        match command {
            Command::PlayPause => self.toggle_playback()?,
            Command::Stop => self.stop_playback(),
            Command::Next => self.next(1, true)?,
            Command::Previous => self.next(-1, true)?,
            Command::Volume { value } => self.set_volume(value)?,
            Command::Seek { milliseconds } => self.seek_to(milliseconds),
            Command::PlayTrack { playlist, entry } => {
                let playlist = self
                    .library
                    .playlists
                    .iter()
                    .find(|p| p.id == playlist)
                    .context("Плейлист мода больше не существует.")?;
                let index = playlist
                    .entries
                    .iter()
                    .position(|e| e.id == entry)
                    .context("Композиция мода больше не существует.")?;
                self.prepare_playback(playlist.entries.clone().into(), playlist.id, index)?;
            }
            Command::Palette { id } => {
                ensure!(
                    self.settings.palettes.iter().any(|p| p.id == id),
                    "Палитра мода больше не существует."
                );
                self.settings.preferences.appearance.palette_ids[self.settings.mode.index()] = id;
                self.save_settings()?;
            }
            Command::Open { page } => {
                if self.view.dialog.is_none() {
                    match page.as_str() {
                        "lyrics" => self.open_lyrics()?,
                        "downloads" => self.open_downloads(),
                        "services" => self.open_services(),
                        "mods" => self.open_mods(),
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn mods_close(&mut self) -> bool {
        let Some(Dialog::Mods(view)) = &mut self.view.dialog else {
            return false;
        };
        if view.remove.take().is_some() || view.install.take().is_some() {
            return true;
        }
        if view.opened {
            view.opened = false;
        } else {
            self.view.dialog = None;
        }
        true
    }
    pub(super) fn mods_scroll(&mut self, direction: i64) {
        if let Some(Dialog::Mods(view)) = &mut self.view.dialog {
            if view.opened || view.install.is_some() {
                view.panel_offset = super::bounded(
                    view.panel_offset,
                    direction,
                    self.view.dialog_scroll_max + 1,
                );
            } else {
                view.selected = super::bounded(view.selected, direction, self.mods.rows.len());
            }
        }
    }
    pub(super) fn mods_key(&mut self, key: KeyPress) -> Result<()> {
        let Some(Dialog::Mods(view)) = &self.view.dialog else {
            return Ok(());
        };
        if view.remove.is_some() {
            return match key.key {
                Key::Char('d') => self.mod_action(ModAction::ConfirmRemove),
                Key::Enter => self.mod_action(ModAction::Back),
                _ => Ok(()),
            };
        }
        if view.install.is_some() {
            return match key.key {
                Key::Up => {
                    self.mods_scroll(-1);
                    Ok(())
                }
                Key::Down => {
                    self.mods_scroll(1);
                    Ok(())
                }
                Key::PageUp => {
                    self.mods_scroll(-10);
                    Ok(())
                }
                Key::PageDown => {
                    self.mods_scroll(10);
                    Ok(())
                }
                Key::Enter => self.mod_action(ModAction::Back),
                Key::Char('i') => self.mod_action(ModAction::ConfirmInstall),
                _ => Ok(()),
            };
        }
        match key.key {
            Key::Up => self.mods_scroll(-1),
            Key::Down => self.mods_scroll(1),
            Key::Home => self.mods_scroll(i64::MIN),
            Key::End => self.mods_scroll(i64::MAX),
            Key::PageUp => self.mods_scroll(-10),
            Key::PageDown => self.mods_scroll(10),
            Key::Enter => self.mod_action(ModAction::Open)?,
            Key::Char('n') => self.mod_action(ModAction::New)?,
            Key::Char('i') => self.mod_action(ModAction::Install)?,
            Key::Char('e') => self.mod_action(ModAction::Edit)?,
            Key::Char('c') => self.mod_action(ModAction::Code)?,
            Key::Char('s') => self.mod_action(ModAction::Settings)?,
            Key::Char(' ') => self.mod_action(ModAction::Enable)?,
            Key::Delete => self.mod_action(ModAction::Remove)?,
            Key::Char(c) if c.is_ascii_digit() && c != '0' => {
                self.mod_action(ModAction::Run(c as usize - '1' as usize))?
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn mod_action(&mut self, action: ModAction) -> Result<()> {
        self.view.settings.focus = super::SettingsFocus::Parameters;
        if let Err(error) = self.mod_action_inner(action) {
            self.mods.error = Some(format!("{error:#}"));
        }
        Ok(())
    }
    fn mod_action_inner(&mut self, action: ModAction) -> Result<()> {
        if matches!(action, ModAction::Back) {
            self.mods_close();
            return Ok(());
        }
        let busy = self.mods_loading();
        let Some(Dialog::Mods(view)) = &mut self.view.dialog else {
            return Ok(());
        };
        if let ModAction::Row(index) = action {
            view.selected = index;
            return Ok(());
        }
        let selected = view.selected;
        ensure!(
            !busy
                || matches!(
                    action,
                    ModAction::Cancel | ModAction::Open | ModAction::Run(_) | ModAction::Guide
                ),
            "Дождитесь выполнения мода или отмените его."
        );
        match action{
            ModAction::ActionPage(direction)=>{let count=self.mods.rows.get(selected).map_or(0,|m|m.manifest.actions.len());view.action=super::bounded(view.action,i64::from(direction)*4,count);view.action=view.action/4*4;},
            ModAction::Open=>{view.opened=true;view.panel_offset=0;},
            ModAction::New=>self.open_form(self.text("Create a mod","Создать мод"),self.text("A working example will appear in the mod list. Edit its code and actions there.","В списке появится рабочий пример. Его код и действия можно изменить."),vec![FormField::new(self.text("Mod name","Название мода"),"")],FormPurpose::ModNew),
            ModAction::Install=>self.open_form(self.text("Install a mod","Установить мод"),self.text("Select a ZIP archive or a folder with manifest.json and main.rhai.","Укажите ZIP-архив или папку с manifest.json и main.rhai."),vec![FormField::new(self.text("Archive or folder","Архив или папка"),"")],FormPurpose::ModInstall),
            ModAction::ConfirmInstall=>{let value=view.install.take().context("Выберите архив мода.")?;self.mod_request(Operation::Install(*value));},
            ModAction::Enable=>{let mut value=self.mods.rows.get(selected).cloned().context("Выберите мод.")?;value.enabled=!value.enabled;self.mod_request(Operation::Save(value));},
            ModAction::Edit|ModAction::Code|ModAction::Settings=>{
                let value=self.mods.rows.get(selected).cloned().context("Выберите мод.")?;
                if matches!(action,ModAction::Settings)&&let Some(values)=value.settings.as_object()&& !values.is_empty()&&values.len()<=32&&values.values().all(|v|v.is_boolean()||v.is_number()||v.is_string()){
                    let keys=values.keys().cloned().collect::<Vec<_>>();let fields=values.iter().map(|(key,v)|{let mut field=FormField::new(value.manifest.setting_labels.get(key).unwrap_or(key),v.as_str().map(str::to_owned).unwrap_or_else(||v.to_string()));field.checkbox=v.is_boolean();field}).collect();let description=value.manifest.description.clone();self.open_form(self.text("Mod settings","Настройки мода"),&description,fields,FormPurpose::ModFields(value,keys));return Ok(());
                }
                let(field,purpose)=match action{
                    ModAction::Code=>{let mut field=FormField::new(self.text("Code · Rhai","Код · Rhai"),value.script.clone());field.multiline=true;(field,FormPurpose::ModCode(value))},
                    ModAction::Settings=>{let mut field=FormField::new(self.text("Settings · JSON","Настройки · JSON"),serde_json::to_string_pretty(&value.settings)?);field.multiline=true;(field,FormPurpose::ModSettings(value))},
                    _=>{let mut field=FormField::new(self.text("Name, description and actions · JSON","Название, описание и действия · JSON"),serde_json::to_string_pretty(&value.manifest)?);field.multiline=true;(field,FormPurpose::ModManifest(value))},
                };self.open_form(self.text("Edit mod","Изменить мод"),self.text("Ctrl+S saves changes. Esc returns to the mod list.","Ctrl+S сохраняет изменения. Esc возвращает к списку модов."),vec![field],purpose);
            }
            ModAction::Remove=>{view.remove=Some(self.mods.rows.get(selected).context("Выберите мод.")?.manifest.id.clone());},
            ModAction::ConfirmRemove=>{let id=view.remove.take().context("Выберите мод.")?;self.mod_request(Operation::Remove(id));},
            ModAction::Export=>{let value=self.mods.rows.get(selected).cloned().context("Выберите мод.")?;let destination=dirs::download_dir().unwrap_or_else(std::env::temp_dir).join(format!("{}.zip",value.manifest.id));self.open_form(self.text("Export mod","Сохранить архив мода"),self.text("The archive contains code and default settings. Personal mod settings stay in Almavorn.","В архив попадут код и исходные настройки. Личные настройки останутся в Almavorn."),vec![FormField::new(self.text("ZIP file","ZIP-файл"),destination.display().to_string())],FormPurpose::ModExport(value));},
            ModAction::Run(index)=>{let value=self.mods.rows.get(selected).context("Выберите мод.")?;ensure!(value.enabled,"Включите мод.");let action=value.manifest.actions.get(index).context("У мода нет такого действия.")?;self.queue_mod_event(value.manifest.id.clone(),format!("action:{}",action.id));},
            ModAction::Refresh=>self.initialize_mods(),
            ModAction::Cancel=>{self.mods.cancel_all();self.mods.job.cancel();self.mods.events.clear();},
            ModAction::Guide=>{self.open_settings_page(super::SettingsPage::ModGuide);self.view.settings.focus=super::SettingsFocus::Parameters;},
            ModAction::Row(_)|ModAction::Back=>{}
        }
        Ok(())
    }
}
