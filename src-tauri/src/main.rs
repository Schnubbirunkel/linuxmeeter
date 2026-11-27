#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backup;
mod clock;
mod control;
mod notify;
mod remote;
mod settings;
mod shortcuts;
mod updater;
mod volkeys;

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
    remote: Arc<remote::Remote>,
    clock: Arc<OnceLock<Arc<clock::Clock>>>,
}

// ---- clock helper ------------------------------------------------------------------

#[tauri::command]
fn get_clock(data: tauri::State<AppData>) -> Option<clock::ClockInfo> {
    data.clock.get().map(|c| c.info())
}

#[tauri::command]
fn dismiss_clock(data: tauri::State<AppData>, card: String) {
    if let Some(c) = data.clock.get() {
        c.dismiss(&card);
    }
}

/// Let `card` set the clock (or stop it), then restart PipeWire and linuxmeeter.
#[tauri::command]
fn clock_lead(app: tauri::AppHandle, card: String, lead: bool) -> Result<(), String> {
    clock::set_leader(&card, lead)?;
    clock::restart_audio_and_app(std::process::id())?;
    app.exit(0);
    Ok(())
}

// ---- phone remote ---------------------------------------------------------------

/// Turn the phone remote on (new key on first use) or off.
#[tauri::command]
fn set_remote(data: tauri::State<AppData>, enabled: bool) -> remote::RemoteInfo {
    let mut s = settings::get();
    if enabled && s.remote_key.is_empty() {
        s.remote_key = remote::new_token();
    }
    s.remote_enabled = enabled;
    let _ = settings::update(|x| {
        x.remote_enabled = s.remote_enabled;
        x.remote_key = s.remote_key.clone();
    });
    remote_apply(&data.remote, &s)
}

/// A new key: every phone has to scan the new QR code.
#[tauri::command]
fn new_remote_key(data: tauri::State<AppData>) -> remote::RemoteInfo {
    let key = remote::new_token();
    let _ = settings::update(|x| x.remote_key = key.clone());
    remote_apply(&data.remote, &settings::get())
}

#[tauri::command]
fn get_remote(data: tauri::State<AppData>) -> remote::RemoteInfo {
    let s = settings::get();
    data.remote.info(
        s.remote_enabled && data.remote.is_running(),
        s.remote_port,
        &s.remote_key,
        None,
    )
}

fn remote_apply(r: &remote::Remote, s: &settings::Settings) -> remote::RemoteInfo {
    if !s.remote_enabled {
        r.stop();
        return r.info(false, s.remote_port, &s.remote_key, None);
    }
    let err = r.start(s.remote_port, &s.remote_key).err();
    r.info(err.is_none(), s.remote_port, &s.remote_key, err)
}

/// Meters run while the window is visible or a phone is connected.
static WINDOW_METERS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
static REMOTE_CLIENTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn apply_meters(cmd_tx: &pipewire::channel::Sender<EngineCommand>) {
    use std::sync::atomic::Ordering::Relaxed;
    let on = WINDOW_METERS.load(Relaxed) || REMOTE_CLIENTS.load(Relaxed) > 0;
    let _ = cmd_tx.send(EngineCommand::SetMetersEnabled { on });
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
async fn export_settings(
    app: tauri::AppHandle,
    data: tauri::State<'_, AppData>,
) -> Result<Option<String>, String> {
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
    let state = data
        .state
        .read()
        .expect("state lock")
        .clone()
        .ok_or("mixer is still starting")?;
    let text =
        serde_json::to_string_pretty(&backup::collect(&state)?).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// Restore a backup's text: app settings and shortcuts, then the profiles.
fn import_text(app: &tauri::AppHandle, data: &AppData, text: &str) -> Result<Vec<String>, String> {
    let b = backup::parse(text)?;
    let old = settings::get().shortcuts;
    settings::update(|s| *s = b.app.clone())?;
    data.notifier
        .enabled
        .store(b.app.notifications, std::sync::atomic::Ordering::Relaxed);
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
        .send(EngineCommand::ImportProfiles {
            files: b.profiles.into_iter().collect(),
            active: b.active_profile,
        })
        .map_err(|_| "engine unavailable".to_string())?;
    let _ = app.emit("shortcuts_changed", ());
    Ok(names)
}

/// Restore a file written by `export_settings`: profiles, active profile,
/// notifications and shortcuts. Returns the restored profile names.
#[tauri::command]
async fn import_settings(
    app: tauri::AppHandle,
    data: tauri::State<'_, AppData>,
) -> Result<Option<Vec<String>>, String> {
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
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    import_text(&app, &data, &text).map(Some)
}

/// Bind `keys` ("Ctrl+Alt+M", None = remove) to a ctl `action`; returns the stored keys.
#[tauri::command]
fn set_shortcut(
    data: tauri::State<AppData>,
    action: String,
    keys: Option<String>,
) -> Result<Option<String>, String> {
    let sc = data.shortcuts.get().ok_or("shortcuts are still starting")?;
    sc.set(&action, keys.as_deref())
}

/// The profile's main output for the volume keys (None = the desktop's default).
#[tauri::command]
fn set_volume_device(data: tauri::State<AppData>, node: Option<String>) -> Result<(), String> {
    send(&data, EngineCommand::SetVolumeDevice { node })
}

/// A volume-key step on the profile's main output, with Plasma's volume OSD.
fn device_volume(state: &AppState, step: &str, sc: Option<&Arc<shortcuts::Shortcuts>>) -> String {
    // A main output that is not plugged in: fall back to the default device
    // rather than leaving the keys dead.
    let target = state
        .volume_device
        .as_ref()
        .and_then(|node| {
            state
                .devices
                .iter()
                .find(|d| &d.node_name == node && d.id != 0)
        })
        .map(|d| d.id.to_string())
        .unwrap_or_else(|| "@DEFAULT_AUDIO_SINK@".to_string());
    let args = volkeys::wpctl_args(&target, step);
    match std::process::Command::new("wpctl").args(&args).status() {
        Ok(st) if st.success() => {}
        Ok(st) => return format!("error: wpctl {st}"),
        Err(e) => return format!("error: wpctl: {e}"),
    }
    let now = std::process::Command::new("wpctl")
        .args(["get-volume", &target])
        .output()
        .ok()
        .and_then(|o| volkeys::parse_wpctl_volume(&String::from_utf8_lossy(&o.stdout)));
    match now {
        Some((pct, muted)) => {
            if let Some(sc) = sc {
                sc.osd_volume(if muted { 0 } else { pct });
            }
            format!("ok: {pct} %{}", if muted { " (muted)" } else { "" })
        }
        None => "ok: done".into(),
    }
}

fn send(data: &tauri::State<AppData>, cmd: EngineCommand) -> Result<(), String> {
    data.cmd_tx
        .send(cmd)
        .map_err(|_| "engine unavailable".to_string())
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
    send(
        &data,
        EngineCommand::SetGain {
            target,
            db: db as f32,
        },
    )
}

#[tauri::command]
fn set_mute(data: tauri::State<AppData>, target: Target, mute: bool) -> Result<(), String> {
    send(&data, EngineCommand::SetMute { target, mute })
}

#[tauri::command]
fn set_solo(data: tauri::State<AppData>, strip_id: StripId, solo: bool) -> Result<(), String> {
    send(
        &data,
        EngineCommand::SetSolo {
            strip: strip_id,
            solo,
        },
    )
}

#[tauri::command]
fn set_route(
    data: tauri::State<AppData>,
    strip_id: StripId,
    bus: BusId,
    on: bool,
) -> Result<(), String> {
    send(
        &data,
        EngineCommand::SetRoute {
            strip: strip_id,
            bus,
            on,
        },
    )
}

#[tauri::command]
fn set_gate_params(
    data: tauri::State<AppData>,
    strip_id: StripId,
    params: GateParams,
) -> Result<(), String> {
    send(
        &data,
        EngineCommand::SetGateParams {
            strip: strip_id,
            params,
        },
    )
}

#[tauri::command]
fn set_comp_params(
    data: tauri::State<AppData>,
    strip_id: StripId,
    params: CompParams,
) -> Result<(), String> {
    send(
        &data,
        EngineCommand::SetCompParams {
            strip: strip_id,
            params,
        },
    )
}

#[tauri::command]
fn set_eq_params(
    data: tauri::State<AppData>,
    strip_id: StripId,
    params: EqParams,
) -> Result<(), String> {
    send(
        &data,
        EngineCommand::SetEqParams {
            strip: strip_id,
            params,
        },
    )
}

#[tauri::command]
fn set_limiter_params(
    data: tauri::State<AppData>,
    bus: BusId,
    params: LimiterParams,
) -> Result<(), String> {
    send(&data, EngineCommand::SetLimiterParams { bus, params })
}

#[tauri::command]
fn set_strip_device(
    data: tauri::State<AppData>,
    strip_id: StripId,
    hw_key: String,
) -> Result<(), String> {
    send(
        &data,
        EngineCommand::SetStripDevice {
            strip: strip_id,
            hw_key,
        },
    )
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

/// Check for an update on `channel` (saved as the chosen channel).
#[tauri::command]
async fn check_update(channel: updater::Channel) -> Result<updater::UpdateInfo, String> {
    settings::update(|s| s.update_channel = channel)?;
    updater::check(channel).await
}

/// Version and commit of this build, e.g. ("0.4.1.r312", "4cc88db…").
#[tauri::command]
fn get_build_info() -> (String, String) {
    (
        updater::BUILD_VERSION.to_string(),
        updater::BUILD_SHA.to_string(),
    )
}

/// Open a page of this project on GitHub in the desktop's browser.
#[tauri::command]
fn open_release_page(url: String) -> Result<(), String> {
    if !url.starts_with(&format!("https://github.com/{}/", updater::REPO)) {
        return Err("not a page of this project".into());
    }
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_update_channel() -> updater::Channel {
    settings::get().update_channel
}

/// Download + install `asset`, then restart into the new version. Progress
/// goes out as `update_progress` events.
#[tauri::command]
async fn install_update(
    app: tauri::AppHandle,
    platform: updater::Platform,
    asset: updater::Asset,
) -> Result<(), String> {
    let progress_app = app.clone();
    updater::install(platform, &asset, move |text| {
        let _ = progress_app.emit("update_progress", text);
    })
    .await?;
    let _ = app.emit("update_progress", "Restarting…");
    restart_into(&app);
    Ok(())
}

/// Start the (new) binary, which waits for this process to finish shutting
/// down — its virtual devices must be gone before the new ones appear — and
/// exit.
fn restart_into(app: &tauri::AppHandle) {
    let exe = updater::exe_after_update();
    let pid = std::process::id().to_string();
    match std::process::Command::new(&exe)
        .args(["--replace", &pid])
        .spawn()
    {
        Ok(_) => app.exit(0),
        Err(e) => tracing::error!("could not restart {}: {e}", exe.display()),
    }
}

#[tauri::command]
fn get_mic_tone() -> bool {
    settings::get().mic_tone
}

#[tauri::command]
fn set_mic_tone(on: bool) -> Result<(), String> {
    settings::update(|s| s.mic_tone = on)
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
    WINDOW_METERS.store(on, std::sync::atomic::Ordering::Relaxed);
    if let Some(data) = manager.try_state::<AppData>() {
        apply_meters(&data.cmd_tx);
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
            Ok(control::Action::DeviceVolume(step)) => {
                let data = self.handle.state::<AppData>();
                device_volume(&current, &step, data.shortcuts.get())
            }
            Ok(control::Action::Clock) => {
                let data = self.handle.state::<AppData>();
                let Some(info) = data.clock.get().map(|c| c.info()) else {
                    return "error: clock helper is still starting".into();
                };
                let mut out = String::from("dropping out (follows another clock):");
                if info.problems.is_empty() {
                    out += " none";
                }
                for p in &info.problems {
                    out += &format!("\n  {}  {} ({} errors)", p.card, p.description, p.errors);
                }
                out += "\nsets the clock (linuxmeeter rule):";
                if info.leaders.is_empty() {
                    out += " none";
                }
                for l in &info.leaders {
                    out += &format!("\n  {l}");
                }
                if !info.available {
                    out += "\n(pw-top not found: detection is off)";
                }
                format!("ok: {out}")
            }
            Ok(control::Action::ClockLead(card, lead)) => {
                if let Err(e) = clock::set_leader(&card, lead) {
                    return format!("error: {e}");
                }
                match clock::restart_audio_and_app(std::process::id()) {
                    Ok(()) => {
                        let h = self.handle.clone();
                        // Answer first, then go.
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(200));
                            h.exit(0);
                        });
                        format!(
                            "ok: {card} {}; restarting PipeWire and linuxmeeter",
                            if lead {
                                "sets the clock"
                            } else {
                                "follows again"
                            }
                        )
                    }
                    Err(e) => format!("error: {e}"),
                }
            }
            Ok(control::Action::Export(path)) => {
                let data = self.handle.state::<AppData>();
                match export_to(&data, std::path::Path::new(&path)) {
                    Ok(()) => format!("ok: saved to {path}"),
                    Err(e) => format!("error: {e}"),
                }
            }
            Ok(control::Action::Import(path)) => {
                let data = self.handle.state::<AppData>();
                match std::fs::read_to_string(&path)
                    .map_err(|e| format!("could not read {path}: {e}"))
                    .and_then(|t| import_text(&self.handle, &data, &t))
                {
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
        summary: if muted {
            "Microphones muted".into()
        } else {
            "Microphones live".into()
        },
        body: "linuxmeeter".into(),
        icon: if muted {
            "microphone-sensitivity-muted"
        } else {
            "audio-input-microphone"
        },
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

fn tray_menu<R: tauri::Runtime, M: Manager<R>>(
    app: &M,
    m: &TrayModel,
) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{CheckMenuItem, MenuItem, PredefinedMenuItem};
    let menu = tauri::menu::Menu::new(app)?;
    menu.append(&CheckMenuItem::with_id(
        app,
        "mics",
        "Mute all microphones",
        m.has_mics,
        m.mics_muted,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "profiles-label",
        "Profile",
        false,
        None::<&str>,
    )?)?;
    for name in &m.profiles {
        let id = format!("{TRAY_PROFILE}{name}");
        menu.append(&CheckMenuItem::with_id(
            app,
            id,
            format!("  {name}"),
            true,
            *name == m.profile,
            None::<&str>,
        )?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "show",
        "Show mixer",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
    Ok(menu)
}

fn refresh_tray(handle: &tauri::AppHandle, m: TrayModel) {
    let h = handle.clone();
    let _ = handle.run_on_main_thread(move || {
        let Some(tray) = h.tray_by_id("main") else {
            return;
        };
        match tray_menu(&h, &m) {
            Ok(menu) => {
                let _ = tray.set_menu(Some(menu));
                let tip = if m.mics_muted {
                    "linuxmeeter — microphones muted"
                } else {
                    "linuxmeeter"
                };
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
            std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME not set"))
                .join(".config")
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
    // After an update: wait for the old process (still removing its devices)
    // to be gone before taking over — and never give up into "nothing runs".
    let replacing = args.iter().position(|a| a == "--replace");
    if let Some(pid) = replacing.and_then(|i| args.get(i + 1)) {
        let proc_dir = std::path::PathBuf::from(format!("/proc/{pid}"));
        for _ in 0..200 {
            if !proc_dir.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let mut listened = control::listen();
    if replacing.is_some() {
        for _ in 0..100 {
            match &listened {
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    std::thread::sleep(Duration::from_millis(200));
                    listened = control::listen();
                }
                _ => break,
            }
        }
    }

    let listener = match listened {
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
    let clock_cell: Arc<OnceLock<Arc<clock::Clock>>> = Arc::new(OnceLock::new());
    let volkeys_cell: Arc<OnceLock<Arc<volkeys::VolumeKeys>>> = Arc::new(OnceLock::new());
    let exit_volkeys = volkeys_cell.clone();
    let remote = {
        let cmd_tx = engine.cmd_tx.clone();
        remote::Remote::new(engine.cmd_tx.clone(), state.clone(), move |n| {
            REMOTE_CLIENTS.store(n, std::sync::atomic::Ordering::Relaxed);
            apply_meters(&cmd_tx);
        })
    };
    {
        let s = settings::get();
        if s.remote_enabled && !s.remote_key.is_empty() {
            let _ = remote_apply(&remote, &s);
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppData {
            cmd_tx: engine.cmd_tx.clone(),
            state: state.clone(),
            notifier: notifier.clone(),
            shortcuts: shortcut_cell.clone(),
            remote: remote.clone(),
            clock: clock_cell.clone(),
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
            set_volume_device,
            set_shortcut,
            get_mic_tone,
            set_mic_tone,
            check_update,
            get_update_channel,
            get_build_info,
            open_release_page,
            set_remote,
            get_clock,
            dismiss_clock,
            clock_lead,
            new_remote_key,
            get_remote,
            install_update,
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
            let pump_cmd = engine.cmd_tx.clone();
            let pump_remote = remote.clone();
            let pump_volkeys = volkeys_cell.clone();
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
                                pump_remote.publish_state(&s);
                                if let Some(vk) = pump_volkeys.get() {
                                    vk.sync(s.volume_device.is_some());
                                }
                                if let Some(prev) = &previous {
                                    if let Some(rising) = control::mic_mute_change(prev, &s) {
                                        if settings::get().mic_tone {
                                            let _ = pump_cmd.send(EngineCommand::Beep { rising });
                                        }
                                    }
                                }
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
                                pump_remote.publish_meters(&f);
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
            // Volume keys -> the profile's main output (takes them from Plasma).
            let vk = Arc::new(volkeys::VolumeKeys::new(sc.clone()));
            if let Some(s) = state.read().expect("state lock").as_ref() {
                vk.sync(s.volume_device.is_some());
            }
            let _ = volkeys_cell.set(vk);
            let _ = shortcut_cell.set(sc);

            // Logout / `kill`: quit like the tray's Quit, so the volume keys go
            // back to Plasma and the devices are removed cleanly.
            let sig_handle = app.handle().clone();
            std::thread::Builder::new().name("lm-signals".into()).spawn(move || {
                use tokio::signal::unix::{signal, SignalKind};
                let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_io().build() else { return };
                rt.block_on(async {
                    let (Ok(mut term), Ok(mut int), Ok(mut hup)) =
                        (signal(SignalKind::terminate()), signal(SignalKind::interrupt()), signal(SignalKind::hangup()))
                    else {
                        return;
                    };
                    tokio::select! {
                        _ = term.recv() => {}
                        _ = int.recv() => {}
                        _ = hup.recv() => {}
                    }
                });
                tracing::info!("signal received, quitting");
                sig_handle.exit(0);
            })?;

            // Clock helper: devices that drop out following another clock.
            let clock_handle = app.handle().clone();
            let clock_notifier = notifier.clone();
            let _ = clock_cell.set(clock::Clock::start(state.clone(), move |p| {
                let _ = clock_handle.emit("clock_problem", p);
                clock_notifier.send(notify::Note {
                    key: format!("clock-{}", p.card),
                    summary: format!("{} keeps dropping out", p.description),
                    body: "It follows another device's clock. Open linuxmeeter to let it set the clock.".into(),
                    icon: "dialog-warning",
                    value: None,
                    timeout_ms: 10_000,
                });
            }));
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
                // Plasma gets its volume keys back.
                if let Some(vk) = exit_volkeys.get() {
                    vk.sync(false);
                }
                if let Some(sc) = exit_shortcuts.get() {
                    sc.deactivate();
                }
                // Graceful engine shutdown -> modules unload -> zero lm.* nodes left.
                let _ = cmd_tx.send(EngineCommand::Shutdown);
                std::thread::sleep(Duration::from_millis(200));
            }
        });
}
