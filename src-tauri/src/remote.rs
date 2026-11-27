//! Phone remote: a small web server on the local network. The phone opens
//! the page from a QR code in Settings; a WebSocket then carries the mixer
//! state and ~10 meter frames per second out, and fader / mute / profile
//! commands in.
//!
//! Off by default. Every request needs the random key from the QR code, and
//! only addresses from the local network (or loopback / Tailscale-style
//! 100.64/10) are served, even if the port is forwarded by accident.

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use lm_protocol::{AppState, EngineCommand, MeterFrame, Target};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, watch};

const PAGE: &str = include_str!("remote.html");
pub const DEFAULT_PORT: u16 = 8723;

/// Commands from the phone.
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
enum ClientCmd {
    Gain {
        target: Target,
        db: f32,
        /// Last value of a drag: the desktop window catches up now.
        #[serde(default)]
        done: bool,
    },
    Mute {
        target: Target,
        mute: bool,
    },
    Mics {
        mute: bool,
    },
    Profile {
        name: String,
    },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ServerMsg<'a> {
    State { state: &'a AppState },
    /// Peak dBFS per strip / bus, in `state` order.
    Meters { strips: Vec<f32>, buses: Vec<f32> },
}

struct Inner {
    cmd_tx: pipewire::channel::Sender<EngineCommand>,
    state: Arc<RwLock<Option<AppState>>>,
    token: RwLock<String>,
    tx: broadcast::Sender<Arc<str>>,
    clients: AtomicUsize,
    on_clients: Box<dyn Fn(usize) + Send + Sync>,
    shutdown: watch::Sender<bool>,
}

pub struct Remote {
    inner: Arc<Inner>,
    /// Port of the running server, if any.
    running: Mutex<Option<u16>>,
    meter_tick: AtomicUsize,
}

/// The remote's address for the QR code.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub enabled: bool,
    pub port: u16,
    pub url: String,
    pub qr_svg: String,
    pub clients: usize,
    pub error: Option<String>,
}

impl Remote {
    pub fn new(
        cmd_tx: pipewire::channel::Sender<EngineCommand>,
        state: Arc<RwLock<Option<AppState>>>,
        on_clients: impl Fn(usize) + Send + Sync + 'static,
    ) -> Arc<Self> {
        let (tx, _) = broadcast::channel(64);
        let (shutdown, _) = watch::channel(false);
        Arc::new(Self {
            inner: Arc::new(Inner {
                cmd_tx,
                state,
                token: RwLock::new(String::new()),
                tx,
                clients: AtomicUsize::new(0),
                on_clients: Box::new(on_clients),
                shutdown,
            }),
            running: Mutex::new(None),
            meter_tick: AtomicUsize::new(0),
        })
    }

    pub fn clients(&self) -> usize {
        self.inner.clients.load(Ordering::Relaxed)
    }

    pub fn is_running(&self) -> bool {
        self.running.lock().expect("lock").is_some()
    }

    /// Start serving on `port` with `token`. Errors (port taken) come back
    /// right away.
    pub fn start(&self, port: u16, token: &str) -> Result<(), String> {
        self.stop();
        *self.inner.token.write().expect("lock") = token.to_string();
        let listener = std::net::TcpListener::bind(("0.0.0.0", port)).map_err(|e| match e.kind() {
            std::io::ErrorKind::AddrInUse => format!("port {port} is already in use"),
            _ => format!("cannot listen on port {port}: {e}"),
        })?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        self.inner.shutdown.send_replace(false);
        let inner = self.inner.clone();
        let mut stop = self.inner.shutdown.subscribe();
        tauri::async_runtime::spawn(async move {
            let listener = match tokio::net::TcpListener::from_std(listener) {
                Ok(l) => l,
                Err(e) => return tracing::error!("remote: {e}"),
            };
            let app = Router::new()
                .route("/", get(page))
                .route("/ws", get(ws))
                .with_state(inner);
            let served = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(async move {
                    let _ = stop.wait_for(|s| *s).await;
                })
                .await;
            if let Err(e) = served {
                tracing::error!("remote server stopped: {e}");
            }
        });
        *self.running.lock().expect("lock") = Some(port);
        tracing::info!("phone remote listening on port {port}");
        Ok(())
    }

    /// Stop the server and drop every phone.
    pub fn stop(&self) {
        if self.running.lock().expect("lock").take().is_some() {
            self.inner.shutdown.send_replace(true);
            tracing::info!("phone remote stopped");
        }
    }

    pub fn publish_state(&self, state: &AppState) {
        if self.clients() == 0 {
            return;
        }
        if let Ok(text) = serde_json::to_string(&ServerMsg::State { state }) {
            let _ = self.inner.tx.send(text.into());
        }
    }

    /// Every third engine frame (~10/s) goes to the phones.
    pub fn publish_meters(&self, frame: &MeterFrame) {
        if self.clients() == 0 || !self.meter_tick.fetch_add(1, Ordering::Relaxed).is_multiple_of(3) {
            return;
        }
        let peak = |v: &[f32; 4]| v[0].max(v[1]);
        let msg = ServerMsg::Meters {
            strips: frame.strips.iter().map(peak).collect(),
            buses: frame.buses.iter().map(peak).collect(),
        };
        if let Ok(text) = serde_json::to_string(&msg) {
            let _ = self.inner.tx.send(text.into());
        }
    }

    /// The URL and QR code for the settings panel.
    pub fn info(&self, enabled: bool, port: u16, token: &str, error: Option<String>) -> RemoteInfo {
        let url = format!("http://{}:{port}/?k={token}", lan_ip());
        let qr_svg = qrcode::QrCode::new(url.as_bytes())
            .map(|c| c.render::<qrcode::render::svg::Color>().min_dimensions(180, 180).quiet_zone(true).build())
            .unwrap_or_default();
        RemoteInfo { enabled, port, url, qr_svg, clients: self.clients(), error }
    }
}

/// A fresh random key (128 bits, hex).
pub fn new_token() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("system randomness");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// This machine's address on the local network: the source address the
/// kernel would use to reach the internet (no packet is sent).
fn lan_ip() -> IpAddr {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("192.0.2.1:9")?;
            s.local_addr()
        })
        .map(|a| a.ip())
        .unwrap_or(IpAddr::from([127, 0, 0, 1]))
}

/// Local network, loopback, link-local, or CGNAT/Tailscale (100.64/10).
pub fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || (o[0] == 100 && (64..128).contains(&o[1]))
        }
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_local(IpAddr::V4(v4)),
            None => {
                let s = v6.segments()[0];
                v6.is_loopback() || (s & 0xfe00) == 0xfc00 || (s & 0xffc0) == 0xfe80
            }
        },
    }
}

#[derive(Deserialize)]
struct KeyQuery {
    #[serde(default)]
    k: String,
}

fn allowed(inner: &Inner, key: &str, addr: SocketAddr) -> bool {
    let token = inner.token.read().expect("lock");
    !token.is_empty() && *token == key && is_local(addr.ip())
}

async fn page(State(inner): State<Arc<Inner>>, ConnectInfo(addr): ConnectInfo<SocketAddr>, Query(q): Query<KeyQuery>) -> Response {
    if !allowed(&inner, &q.k, addr) {
        return (
            StatusCode::FORBIDDEN,
            "linuxmeeter: scan the QR code in linuxmeeter's Settings (Phone remote) to open the remote.",
        )
            .into_response();
    }
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8"), (header::CACHE_CONTROL, "no-store")],
        PAGE,
    )
        .into_response()
}

async fn ws(
    State(inner): State<Arc<Inner>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(q): Query<KeyQuery>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !allowed(&inner, &q.k, addr) {
        return StatusCode::FORBIDDEN.into_response();
    }
    upgrade.on_upgrade(move |socket| client(socket, inner))
}

async fn client(mut socket: WebSocket, inner: Arc<Inner>) {
    let n = inner.clients.fetch_add(1, Ordering::Relaxed) + 1;
    (inner.on_clients)(n);
    let mut rx = inner.tx.subscribe();
    let mut stop = inner.shutdown.subscribe();
    let mut last_refresh = Instant::now() - Duration::from_secs(1);

    // Start with the full state.
    let first = inner.state.read().expect("lock").clone();
    if let Some(s) = first {
        if let Ok(text) = serde_json::to_string(&ServerMsg::State { state: &s }) {
            let _ = socket.send(Message::Text(text.into())).await;
        }
    }

    loop {
        tokio::select! {
            msg = socket.recv() => {
                let Some(Ok(msg)) = msg else { break };
                let Message::Text(text) = msg else { continue };
                let Ok(cmd) = serde_json::from_str::<ClientCmd>(text.as_str()) else { continue };
                handle(&inner, cmd, &mut last_refresh);
            }
            out = rx.recv() => match out {
                Ok(text) => {
                    if socket.send(Message::Text(text.as_ref().into())).await.is_err() {
                        break;
                    }
                }
                // Slow phone: skip what it missed, the next state/meters catch up.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    break;
                }
            }
        }
    }
    let _ = socket.send(Message::Close(None)).await;
    let n = inner.clients.fetch_sub(1, Ordering::Relaxed) - 1;
    (inner.on_clients)(n);
}

fn handle(inner: &Inner, cmd: ClientCmd, last_refresh: &mut Instant) {
    let send = |c: EngineCommand| {
        let _ = inner.cmd_tx.send(c);
    };
    match cmd {
        ClientCmd::Gain { target, db, done } => {
            let db = if db.is_finite() { db.clamp(lm_protocol::taper::MIN_DB, lm_protocol::taper::MAX_DB) } else { lm_protocol::taper::MIN_DB };
            send(EngineCommand::SetGain { target, db });
            // SetGain emits no state (the desktop fader is optimistic); the
            // window and other phones follow at ~8/s and on release.
            if done || last_refresh.elapsed() > Duration::from_millis(120) {
                *last_refresh = Instant::now();
                send(EngineCommand::RequestState);
            }
        }
        ClientCmd::Mute { target, mute } => send(EngineCommand::SetMute { target, mute }),
        ClientCmd::Mics { mute } => send(EngineCommand::SetMicsMute { mute }),
        ClientCmd::Profile { name } => send(EngineCommand::LoadProfile { name }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_protocol::BusId;

    #[test]
    fn only_local_addresses_are_served() {
        for ok in ["127.0.0.1", "192.168.1.20", "10.0.0.5", "172.16.3.4", "169.254.1.1", "100.101.102.103", "::1", "fd12::1", "fe80::1", "::ffff:192.168.0.2"] {
            assert!(is_local(ok.parse().unwrap()), "{ok}");
        }
        for bad in ["8.8.8.8", "100.128.0.1", "2001:db8::1", "::ffff:1.1.1.1"] {
            assert!(!is_local(bad.parse().unwrap()), "{bad}");
        }
    }

    #[test]
    fn commands_parse_from_the_page() {
        let c: ClientCmd = serde_json::from_str(r#"{"cmd":"gain","target":{"strip":3},"db":-6.5,"done":true}"#).unwrap();
        assert!(matches!(c, ClientCmd::Gain { target: Target::Strip { strip: 3 }, done: true, .. }));
        let c: ClientCmd = serde_json::from_str(r#"{"cmd":"mute","target":{"bus":"B1"},"mute":true}"#).unwrap();
        assert!(matches!(c, ClientCmd::Mute { target: Target::Bus { bus: BusId::B1 }, mute: true }));
        let c: ClientCmd = serde_json::from_str(r#"{"cmd":"mics","mute":false}"#).unwrap();
        assert!(matches!(c, ClientCmd::Mics { mute: false }));
        let c: ClientCmd = serde_json::from_str(r#"{"cmd":"profile","name":"Gaming"}"#).unwrap();
        assert!(matches!(c, ClientCmd::Profile { .. }));
    }

    #[test]
    fn tokens_are_random_hex() {
        let a = new_token();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, new_token());
    }
}
