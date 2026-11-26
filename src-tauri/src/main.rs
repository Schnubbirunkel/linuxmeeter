#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backup;
mod control;
mod notify;
mod settings;
mod shortcuts;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use control::Notify;
use lm_protocol::{
    AppState, BusId, CompParams, EngineCommand, EngineEvent, EqParams, GateParams, LimiterParams,
    StripId, StripKind, Target,
};
use tauri::{Emitter, Manager};

struct AppData {
    cmd_tx: pipewire::channel::Sender<EngineCommand>,
    state: Arc<RwLock<Option<AppState>>>,
    notifier: notify::Notifier,
    shortcuts: Arc<OnceLock<Arc<shortcuts::Shortcuts>>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ShortcutInfo {
    /// KDE's global shortcut service is there.
    available: bool,
    /// ctl action -> keys.
    bindings: std::collections::BTreeMap<String, String>,
}

#[tauri::command]
fn get_shortcuts(data: tauri::State<AppData>) -> ShortcutInfo {
    ShortcutInfo {
        available: data.shortcuts.get().is_some_and(|s| s.available()),
        bindings: settings::get().shortcuts,
    }
}

/// Save everything (all profiles + app settings) to a file the user picks.
/// Returns the path, or None if the dialog was cancelled.
#[tauri::command]
async fn export_settings(app: tauri::AppHandle, data: tauri::State<'_, AppData>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Save linuxmeeter settings")
        .set_file_name("linuxmeeter-settings.json")
        .add_filter("linuxmeeter settings", &["json"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    export_to(&data, &path)?;
    Ok(Some(path.display().to_string()))
}

fn export_to(data: &AppData, path: &std::path::Path) -> Result<(), String> {
    let state = data.state.read().expect("state lock").clone().ok_or("mixer is still starting")?;
    let text = serde_json::to_string_pretty(&backup::collect(&state)?).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// Restore a backup's text: app settings and shortcuts, then the profiles.
fn import_text(app: &tauri::AppHandle, data: &AppData, text: &str) -> Result<Vec<String>, String> {
    let b = backup::parse(text)?;
    let old = settings::get().shortcuts;
    settings::update(|s| *s = b.app.clone())?;
    data.notifier.enabled.store(b.app.notifications, std::sync::atomic::Ordering::Relaxed);
    if let Some(sc) = data.shortcuts.get() {
        for action in old.keys().filter(|a| !b.app.shortcuts.contains_key(*a)) {
            let _ = sc.set(action, None);
        }
        for (action, keys) in &b.app.shortcuts {
            if let Err(e) = sc.set(action, Some(keys)) {
                tracing::warn!("shortcut {action} ({keys}): {e}");
            }
        }
    }
    let names: Vec<String> = b.profiles.keys().cloned().collect();
    data.cmd_tx
        .send(EngineCommand::ImportProfiles { files: b.profiles.into_iter().collect(), active: b.active_profile })
        .map_err(|_| "engine unavailable".to_string())?;
    let _ = app.emit("shortcuts_changed", ());
    Ok(names)
}

/// Restore a file written by `export_settings`: profiles, active profile,
/// notifications and shortcuts. Returns the restored profile names.
#[tauri::command]
async fn import_settings(app: tauri::AppHandle, data: tauri::State<'_, AppData>) -> Result<Option<Vec<String>>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Load linuxmeeter settings")
        .add_filter("linuxmeeter settings", &["json"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    import_text(&app, &data, &text).map(Some)
}

/// Bind `keys` ("Ctrl+Alt+M", None = remove) to a ctl `action`; returns the stored keys.
#[tauri::command]
fn set_shortcut(data: tauri::State<AppData>, action: String, keys: Option<String>) -> Result<Option<String>, String> {
    let sc = data.shortcuts.get().ok_or("shortcuts are still starting")?;
    sc.set(&action, keys.as_deref())
}

fn send(data: &tauri::State<AppData>, cmd: EngineCommand) -> Result<(), String> {
    data.cmd_tx.send(cmd).map_err(|_| "engine unavailable".to_string())
}

#[tauri::command]
fn get_state(data: tauri::State<AppData>) -> Result<AppState, String> {
    // The engine builds its graph asynchronously at startup; wait briefly.
    for _ in 0..50 {
        if let Some(s) = data.state.read().expect("state lock").clone() {
            return Ok(s);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("engine not ready".into())
}

#[tauri::command]
fn set_gain(data: tauri::State<AppData>, target: Target, db: f64) -> Result<(), String> {
    send(&data, EngineCommand::SetGain { target, db: db as f32 })
}

#[tauri::command]
fn set_mute(data: tauri::State<AppData>, target: Target, mute: bool) -> Result<(), String> {
    send(&data, EngineCommand::SetMute { target, mute })
}

#[tauri::command]
fn set_solo(data: tauri::State<AppData>, strip_id: StripId, solo: bool) -> Result<(), String> {
    send(&data, EngineCommand::SetSolo { strip: strip_id, solo })
}

#[tauri::command]
fn set_route(data: tauri::State<AppData>, strip_id: StripId, bus: BusId, on: bool) -> Result<(), String> {
    send(&data, EngineCommand::SetRoute { strip: strip_id, bus, on })
}

#[tauri::command]
fn set_gate_params(data: tauri::State<AppData>, strip_id: StripId, params: GateParams) -> Result<(), String> {
    send(&data, EngineCommand::SetGateParams { strip: strip_id, params })
}

#[tauri::command]
fn set_comp_params(data: tauri::State<AppData>, strip_id: StripId, params: CompParams) -> Result<(), String> {
    send(&data, EngineCommand::SetCompParams { strip: strip_id, params })
}

#[tauri::command]
fn set_eq_params(data: tauri::State<AppData>, strip_id: StripId, params: EqParams) -> Result<(), String> {
    send(&data, EngineCommand::SetEqParams { strip: strip_id, params })
}

#[tauri::command]
fn set_limiter_params(data: tauri::State<AppData>, bus: BusId, params: LimiterParams) -> Result<(), String> {
    send(&data, EngineCommand::SetLimiterParams { bus, params })
}

#[tauri::command]
fn set_strip_device(data: tauri::State<AppData>, strip_id: StripId, hw_key: String) -> Result<(), String> {
    send(&data, EngineCommand::SetStripDevice { strip: strip_id, hw_key })
}

#[tauri::command]
fn set_bus_target(data: tauri::State<AppData>, bus: BusId, hw_key: String) -> Result<(), String> {
    send(&data, EngineCommand::SetBusTarget { bus, hw_key })
}

#[tauri::command]
fn rename(data: tauri::State<AppData>, target: Target, label: String) -> Result<(), String> {
    send(&data, EngineCommand::Rename { target, label })
}

#[tauri::command]
fn load_profile(data: tauri::State<AppData>, name: String) -> Result<(), String> {
    send(&data, EngineCommand::LoadProfile { name })
}

#[tauri::command]
fn save_profile_as(data: tauri::State<AppData>, name: String) -> Result<(), String> {
    send(&data, EngineCommand::SaveProfileAs { name })
}

#[tauri::command]
fn delete_profile(data: tauri::State<AppData>, name: String) -> Result<(), String> {
    send(&data, EngineCommand::DeleteProfile { name })
}

#[tauri::command]
fn get_notifications(data: tauri::State<AppData>) -> bool {
    data.notifier
        .enabled
        .load(std::sync::atomic::Ordering::Relaxed)
}

#[tauri::command]
fn set_notifications(data: tauri::State<AppData>, on: bool) -> Result<(), String> {
    data.notifier.set_enabled(on)
}

/// Absolute path of this binary, for shortcut commands.
#[tauri::command]
fn get_exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "linuxmeeter".into())
}

#[tauri::command]
fn add_strip(data: tauri::State<AppData>, kind: StripKind) -> Result<(), String> {
    send(&data, EngineCommand::AddStrip { kind, hw_key: None })
}

#[tauri::command]
fn remove_strip(data: tauri::State<AppData>, strip_id: StripId) -> Result<(), String> {
    send(&data, EngineCommand::RemoveStrip { strip: strip_id })
}

#[tauri::command]
fn add_bus(data: tauri::State<AppData>, hardware: bool) -> Result<(), String> {
    send(&data, EngineCommand::AddBus { hardware })
}

#[tauri::command]
fn remove_bus(data: tauri::State<AppData>, bus: BusId) -> Result<(), String> {
    send(&data, EngineCommand::RemoveBus { bus })
}

#[tauri::command]
fn set_default_output(data: tauri::State<AppData>, on: bool) -> Result<(), String> {
    send(&data, EngineCommand::SetDefaultOutput { on })
}

/// Start/stop meter frames to follow window visibility. Each frame costs a
/// script evaluation in the webview plus a canvas repaint per meter, so a
/// mixer parked in the tray should not be paying for either.
fn set_meters<R: tauri::Runtime, M: Manager<R>>(manager: &M, on: bool) {
    if let Some(data) = manager.try_state::<AppData>() {
        let _ = data.cmd_tx.send(EngineCommand::SetMetersEnabled { on });
    }
}

fn show_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        set_meters(app, true);
    }
}

/// Runs `ctl`-style commands for every remote entry point: the control
/// socket, the tray menu and the global shortcuts.
#[derive(Clone)]
struct Ctl {
    state: Arc<RwLock<Option<AppState>>>,
    /// What to report on once the engine's next State arrives.
    pending: Arc<Mutex<VecDeque<Notify>>>,
    cmd_tx: pipewire::channel::Sender<EngineCommand>,
    handle: tauri::AppHandle,
}

impl Ctl {
    fn run(&self, args: &[String]) -> String {
        let Some(current) = self.state.read().expect("state lock").clone() else {
            return "error: mixer is still starting".into();
        };
        match control::parse(args, &current) {
            Ok(control::Action::Engine { cmd, notify, reply }) => {
                if let Some(n) = notify {
                    self.pending.lock().expect("pending lock").push_back(n);
                }
                match self.cmd_tx.send(cmd) {
                    Ok(()) => format!("ok: {reply}"),
                    Err(_) => "error: engine unavailable".into(),
                }
            }
            Ok(control::Action::Show) => {
                show_window(&self.handle);
                "ok: shown".into()
            }
            Ok(control::Action::Reply(text)) => format!("ok: {text}"),
            Ok(control::Action::Export(path)) => {
                let data = self.handle.state::<AppData>();
                match export_to(&data, std::path::Path::new(&path)) {
                    Ok(()) => format!("ok: saved to {path}"),
                    Err(e) => format!("error: {e}"),
                }
            }
            Ok(control::Action::Import(path)) => {
                let data = self.handle.state::<AppData>();
                match std::fs::read_to_string(&path).map_err(|e| format!("could not read {path}: {e}")).and_then(|t| import_text(&self.handle, &data, &t)) {
                    Ok(names) => format!("ok: restored {}", names.join(", ")),
                    Err(e) => format!("error: {e}"),
                }
            }
            Err(e) => format!("error: {e}"),
        }
    }

    fn run_words(&self, args: &[&str]) {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let reply = self.run(&args);
        if let Some(e) = reply.strip_prefix("error: ") {
            tracing::warn!("{args:?}: {e}");
        }
    }
}

fn mics_note(state: &AppState) -> notify::Note {
    let muted = control::mics_muted(state);
    notify::Note {
        key: "mics".into(),
        summary: if muted { "Microphones muted".into() } else { "Microphones live".into() },
        body: "linuxmeeter".into(),
        icon: if muted { "microphone-sensitivity-muted" } else { "audio-input-microphone" },
        value: None,
        timeout_ms: 2000,
    }
}

// ---- tray --------------------------------------------------------------------

const TRAY_PROFILE: &str = "profile:";

/// What the tray menu shows; it is rebuilt only when this changes.
#[derive(PartialEq, Clone, Default)]
struct TrayModel {
    has_mics: bool,
    mics_muted: bool,
    profile: String,
    profiles: Vec<String>,
}

impl TrayModel {
    fn of(s: &AppState) -> Self {
        Self {
            has_mics: s.strips.iter().any(|x| x.kind == StripKind::Hardware),
            mics_muted: control::mics_muted(s),
            profile: s.profile_name.clone(),
            profiles: s.profiles.clone(),
        }
    }
}

fn tray_menu<R: tauri::Runtime, M: Manager<R>>(app: &M, m: &TrayModel) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{CheckMenuItem, MenuItem, PredefinedMenuItem};
    let menu = tauri::menu::Menu::new(app)?;
    menu.append(&CheckMenuItem::with_id(app, "mics", "Mute all microphones", m.has_mics, m.mics_muted, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "profiles-label", "Profile", false, None::<&str>)?)?;
    for name in &m.profiles {
        let id = format!("{TRAY_PROFILE}{name}");
        menu.append(&CheckMenuItem::with_id(app, id, format!("  {name}"), true, *name == m.profile, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "show", "Show mixer", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
    Ok(menu)
}

fn refresh_tray(handle: &tauri::AppHandle, m: TrayModel) {
    let h = handle.clone();
    let _ = handle.run_on_main_thread(move || {
        let Some(tray) = h.tray_by_id("main") else { return };
        match tray_menu(&h, &m) {
            Ok(menu) => {
                let _ = tray.set_menu(Some(menu));
                let tip = if m.mics_muted { "linuxmeeter — microphones muted" } else { "linuxmeeter" };
                let _ = tray.set_tooltip(Some(tip));
            }
            Err(e) => tracing::warn!("tray menu: {e}"),
        }
    });
}

/// Notification for a strip/bus after a shortcut changed it.
fn channel_note(state: &AppState, target: Target) -> Option<notify::Note> {
    let (label, gain, mute, key) = match target {
        Target::Strip { strip } => {
            let s = state.strips.iter().find(|s| s.id == strip)?;
            (s.label.clone(), s.gain_db, s.mute, format!("s{strip}"))
        }
        Target::Bus { bus } => {
            let b = state.buses.iter().find(|b| b.id == bus)?;
            (
                format!("{bus:?} {}", b.label),
                b.gain_db,
                b.mute,
                format!("b{bus:?}"),
            )
        }
    };
    let pct = notify::percent(gain);
    Some(notify::Note {
        key,
        summary: if mute {
            format!("{label}: muted")
        } else {
            format!("{label}: {pct} %")
        },
        body: notify::fmt_db(gain),
        icon: match (mute, pct) {
            (true, _) | (false, 0) => "audio-volume-muted",
            (false, 1..=33) => "audio-volume-low",
            (false, 34..=66) => "audio-volume-medium",
            _ => "audio-volume-high",
        },
        value: Some(if mute { 0 } else { pct }),
        timeout_ms: 2000,
    })
}

fn autostart_path() -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME not set")).join(".config")
        })
        .join("autostart/linuxmeeter.desktop")
}

#[tauri::command]
fn get_autostart() -> bool {
    autostart_path().exists()
}

#[tauri::command]
fn set_autostart(on: bool) -> Result<(), String> {
    let path = autostart_path();
    if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=linuxmeeter\nComment=Virtual audio mixer\nExec={} --minimized\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            exe.display()
        );
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, entry).map_err(|e| e.to_string())
    } else {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn main() {
    // `linuxmeeter ctl …` talks to the running instance and exits; it never
    // starts a second mixer (or GTK).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("ctl") {
        let rest = &args[1..];
        if rest.is_empty() || matches!(rest[0].as_str(), "help" | "--help" | "-h") {
            println!("{}", control::HELP);
            return;
        }
        match control::send(rest) {
            Ok(reply) => {
                let reply = reply.replace("\\n", "\n");
                if let Some(err) = reply.strip_prefix("error: ") {
                    eprintln!("{err}");
                    std::process::exit(1);
                }
                println!("{}", reply.strip_prefix("ok: ").unwrap_or(&reply));
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // One mixer per session: a second launch just brings the window up.
    let listener = match control::listen() {
        Ok(l) => Some(l),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            let _ = control::send(&["show".to_string()]);
            return;
        }
        Err(e) => {
            eprintln!("linuxmeeter: control socket unavailable, shortcuts will not work: {e}");
            None
        }
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let engine = lm_engine::engine::spawn();
    let cmd_tx = engine.cmd_tx.clone();
    let state: Arc<RwLock<Option<AppState>>> = Arc::new(RwLock::new(None));
    let notifier = notify::Notifier::spawn();
    // What to report on once the engine's next State arrives.
    let pending: Arc<Mutex<VecDeque<Notify>>> = Arc::new(Mutex::new(VecDeque::new()));
    let shortcut_cell: Arc<OnceLock<Arc<shortcuts::Shortcuts>>> = Arc::new(OnceLock::new());
    let exit_shortcuts = shortcut_cell.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppData {
            cmd_tx: engine.cmd_tx.clone(),
            state: state.clone(),
            notifier: notifier.clone(),
            shortcuts: shortcut_cell.clone(),
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            set_gain,
            set_mute,
            set_solo,
            set_route,
            set_gate_params,
            set_comp_params,
            set_eq_params,
            set_limiter_params,
            set_strip_device,
            set_bus_target,
            set_default_output,
            add_strip,
            remove_strip,
            add_bus,
            remove_bus,
            get_autostart,
            set_autostart,
            rename,
            load_profile,
            save_profile_as,
            delete_profile,
            get_notifications,
            set_notifications,
            get_exe_path,
            get_shortcuts,
            set_shortcut,
            export_settings,
            import_settings,
        ])
        .setup(move |app| {
            let ctl = Ctl {
                state: state.clone(),
                pending: pending.clone(),
                cmd_tx: engine.cmd_tx.clone(),
                handle: app.handle().clone(),
            };

            // Pump engine events into the webview.
            let handle = app.handle().clone();
            let evt_rx = engine.evt_rx;
            let pump_state = state.clone();
            let pump_pending = pending.clone();
            let pump_notifier = notifier.clone();
            let mut tray_model = TrayModel::default();
            std::thread::Builder::new()
                .name("lm-event-pump".into())
                .spawn(move || {
                    while let Ok(evt) = evt_rx.recv() {
                        match evt {
                            EngineEvent::State(s) => {
                                let previous =
                                    pump_state.write().expect("state lock").replace(s.clone());
                                let _ = handle.emit("state_changed", &s);
                                if let Some(prev) = previous {
                                    if prev.profile_name != s.profile_name {
                                        pump_notifier.send(notify::Note {
                                            key: "profile".into(),
                                            summary: format!("Profile: {}", s.profile_name),
                                            body: "linuxmeeter".into(),
                                            icon: "audio-card",
                                            value: None,
                                            timeout_ms: 3000,
                                        });
                                    }
                                }
                                let pending: Vec<Notify> = pump_pending.lock().expect("pending lock").drain(..).collect();
                                for n in pending {
                                    let note = match n {
                                        Notify::Channel(t) => channel_note(&s, t),
                                        Notify::Mics => Some(mics_note(&s)),
                                    };
                                    if let Some(note) = note {
                                        pump_notifier.send(note);
                                    }
                                }
                                let model = TrayModel::of(&s);
                                if model != tray_model {
                                    tray_model = model.clone();
                                    refresh_tray(&handle, model);
                                }
                            }
                            EngineEvent::Meters(f) => {
                                let _ = handle.emit("meters", &f);
                            }
                        }
                    }
                })
                .expect("spawn event pump");

            // Tray: closing the window hides it; the engine (and all virtual
            // devices) keeps running until Quit. Clicking it opens the menu:
            // mute all mics, switch profile, show, quit. The pump rebuilds the
            // menu whenever what it shows changes.
            let tray_ctl = ctl.clone();
            tauri::tray::TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().expect("window icon").clone())
                .tooltip("linuxmeeter")
                .menu(&tray_menu(app, &TrayModel::default())?)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => show_window(app),
                    "quit" => app.exit(0),
                    "mics" => tray_ctl.run_words(&["toggle-mute-mics"]),
                    id => {
                        if let Some(name) = id.strip_prefix(TRAY_PROFILE) {
                            tray_ctl.run_words(&["profile", name]);
                        }
                    }
                })
                .build(app)?;

            // Scripts and desktop shortcut commands: `linuxmeeter ctl …`.
            if let Some(listener) = listener {
                let sock_ctl = ctl.clone();
                control::serve(listener, move |args| sock_ctl.run(&args));
            }
            // In-app global shortcuts (KDE): a key press runs its ctl action.
            let key_ctl = ctl.clone();
            let changed_handle = app.handle().clone();
            let sc = shortcuts::Shortcuts::start(
                move |action| key_ctl.run_words(&action.split(' ').collect::<Vec<_>>()),
                move || {
                    let _ = changed_handle.emit("shortcuts_changed", ());
                },
            );
            let _ = shortcut_cell.set(sc);
            app.manage(ctl);

            if std::env::args().any(|a| a == "--minimized") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                    set_meters(app, false);
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Close-to-tray: keep the mixer (and its devices) alive, but
                // stop metering — nothing can see it.
                api.prevent_close();
                let _ = window.hide();
                set_meters(window, false);
            }
        })
        .build(tauri::generate_context!())
        .expect("error building linuxmeeter")
        .run(move |_app, event| {
            if let tauri::RunEvent::Exit = event {
                let _ = std::fs::remove_file(control::socket_path());
                if let Some(sc) = exit_shortcuts.get() {
                    sc.deactivate();
                }
                // Graceful engine shutdown -> modules unload -> zero lm.* nodes left.
                let _ = cmd_tx.send(EngineCommand::Shutdown);
                std::thread::sleep(Duration::from_millis(200));
            }
        });
}
