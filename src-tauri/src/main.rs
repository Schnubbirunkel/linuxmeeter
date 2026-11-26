#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod control;
mod notify;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use lm_protocol::{
    AppState, BusId, CompParams, EngineCommand, EngineEvent, EqParams, GateParams, LimiterParams,
    StripId, Target,
};
use tauri::{Emitter, Manager};

struct AppData {
    cmd_tx: pipewire::channel::Sender<EngineCommand>,
    state: Arc<RwLock<Option<AppState>>>,
    notifier: notify::Notifier,
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
    // Shortcut targets waiting for the engine's next State to report on.
    let pending: Arc<Mutex<VecDeque<Target>>> = Arc::new(Mutex::new(VecDeque::new()));

    tauri::Builder::default()
        .manage(AppData { cmd_tx: engine.cmd_tx.clone(), state: state.clone(), notifier: notifier.clone() })
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
            get_autostart,
            set_autostart,
            rename,
            load_profile,
            save_profile_as,
            delete_profile,
            get_notifications,
            set_notifications,
            get_exe_path,
        ])
        .setup(move |app| {
            // Pump engine events into the webview.
            let handle = app.handle().clone();
            let evt_rx = engine.evt_rx;
            let pump_state = state.clone();
            let pump_pending = pending.clone();
            let pump_notifier = notifier.clone();
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
                                let targets: Vec<Target> = pump_pending
                                    .lock()
                                    .expect("pending lock")
                                    .drain(..)
                                    .collect();
                                for t in targets {
                                    if let Some(note) = channel_note(&s, t) {
                                        pump_notifier.send(note);
                                    }
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
            // devices) keeps running until Quit.
            let show = tauri::menu::MenuItem::with_id(app, "show", "Show mixer", true, None::<&str>)?;
            let quit = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show, &quit])?;
            tauri::tray::TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().expect("window icon").clone())
                .tooltip("linuxmeeter")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // Global shortcuts / scripts: `linuxmeeter ctl …`.
            if let Some(listener) = listener {
                let handle = app.handle().clone();
                let ctl_state = state.clone();
                let ctl_pending = pending.clone();
                let cmd_tx = engine.cmd_tx.clone();
                control::serve(listener, move |args| {
                    let Some(current) = ctl_state.read().expect("state lock").clone() else {
                        return "error: mixer is still starting".into();
                    };
                    match control::parse(&args, &current) {
                        Ok(control::Action::Engine { cmd, notify, reply }) => {
                            if let Some(t) = notify {
                                ctl_pending.lock().expect("pending lock").push_back(t);
                            }
                            match cmd_tx.send(cmd) {
                                Ok(()) => format!("ok: {reply}"),
                                Err(_) => "error: engine unavailable".into(),
                            }
                        }
                        Ok(control::Action::Show) => {
                            show_window(&handle);
                            "ok: shown".into()
                        }
                        Ok(control::Action::Reply(text)) => format!("ok: {text}"),
                        Err(e) => format!("error: {e}"),
                    }
                });
            }

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
                // Graceful engine shutdown -> modules unload -> zero lm.* nodes left.
                let _ = cmd_tx.send(EngineCommand::Shutdown);
                std::thread::sleep(Duration::from_millis(200));
            }
        });
}
