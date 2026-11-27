//! In-app updates from this fork's GitHub releases.
//!
//! Two channels: "stable" (the numbered releases, compared by version) and
//! "edge" (the rolling pre-release every push to main refreshes, compared by
//! commit — each platform has its own `linuxmeeter-edge-<platform>.sha` so a
//! half-finished CI run never offers a package that is not there yet).
//!
//! Installing downloads the package for this system (Arch: .pkg.tar.zst,
//! Debian: .deb), checks GitHub's SHA-256 digest, installs it through pkexec
//! (the desktop's password prompt), and restarts the app.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const REPO: &str = "Schnubbirunkel/linuxmeeter";
const USER_AGENT: &str = concat!("linuxmeeter-updater/", env!("CARGO_PKG_VERSION"));

/// What this binary is: set by build.rs from CI (or git for local builds).
pub const BUILD_SHA: &str = env!("LM_BUILD_SHA");
pub const BUILD_VERSION: &str = env!("LM_BUILD_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Edge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Arch,
    Debian,
    Unsupported,
}

impl Platform {
    pub fn detect() -> Self {
        let text = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        Self::from_os_release(&text)
    }

    fn from_os_release(text: &str) -> Self {
        let mut ids = String::new();
        for line in text.lines() {
            if let Some(v) = line
                .strip_prefix("ID=")
                .or_else(|| line.strip_prefix("ID_LIKE="))
            {
                ids.push(' ');
                ids.push_str(&v.trim_matches('"').to_lowercase());
            }
        }
        let has = |id: &str| ids.split_whitespace().any(|w| w == id);
        if has("arch") {
            Platform::Arch
        } else if has("debian") || has("ubuntu") {
            Platform::Debian
        } else {
            Platform::Unsupported
        }
    }

    fn edge_asset(self) -> Option<&'static str> {
        match self {
            Platform::Arch => Some("linuxmeeter-edge-x86_64.pkg.tar.zst"),
            Platform::Debian => Some("linuxmeeter-edge_amd64.deb"),
            Platform::Unsupported => None,
        }
    }

    fn edge_sha_asset(self) -> Option<&'static str> {
        match self {
            Platform::Arch => Some("linuxmeeter-edge-arch.sha"),
            Platform::Debian => Some("linuxmeeter-edge-debian.sha"),
            Platform::Unsupported => None,
        }
    }

    /// Is this release asset the package for this system (stable releases)?
    fn matches_stable_asset(self, name: &str) -> bool {
        !name.contains("-edge")
            && match self {
                Platform::Arch => name.ends_with("-x86_64.pkg.tar.zst"),
                Platform::Debian => name.ends_with("_amd64.deb"),
                Platform::Unsupported => false,
            }
    }
}

/// The result of a check, as the settings panel shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub channel: Channel,
    pub platform: Platform,
    pub current_version: String,
    pub current_sha: String,
    /// Newer than what is running.
    pub available: bool,
    /// Version (stable) or "<version> (<short sha>)" (edge) on offer.
    pub latest: String,
    pub release_url: String,
    /// What install() downloads; None when there is nothing for this system.
    pub asset: Option<Asset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub name: String,
    pub url: String,
    /// "sha256:<hex>" from GitHub, when it provides one.
    pub digest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
}

/// "0.4.1", "0.4.1.r312", "v0.5.0" -> [0, 4, 1] (the release part only).
pub fn version_key(v: &str) -> Vec<u64> {
    let v = v.trim().trim_start_matches('v');
    let base = v
        .split(['.', '+', '-'])
        .take_while(|p| p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty());
    base.map(|p| p.parse().unwrap_or(0)).collect()
}

pub async fn check(channel: Channel) -> Result<UpdateInfo, String> {
    let platform = Platform::detect();
    let http = client()?;
    let releases: Vec<GhRelease> = http
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases?per_page=30"
        ))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("could not reach GitHub: {e}"))?
        .error_for_status()
        .map_err(|e| format!("GitHub answered with an error: {e}"))?
        .json()
        .await
        .map_err(|e| format!("unexpected answer from GitHub: {e}"))?;

    let mut info = UpdateInfo {
        channel,
        platform,
        current_version: BUILD_VERSION.to_string(),
        current_sha: BUILD_SHA.to_string(),
        available: false,
        latest: String::new(),
        release_url: format!("https://github.com/{REPO}/releases"),
        asset: None,
    };

    match channel {
        Channel::Stable => {
            let Some(rel) = releases
                .iter()
                .find(|r| !r.draft && !r.prerelease && r.tag_name != "edge")
            else {
                return Ok(info);
            };
            info.latest = rel.tag_name.trim_start_matches('v').to_string();
            info.release_url = rel.html_url.clone();
            info.available = version_key(&rel.tag_name) > version_key(BUILD_VERSION);
            info.asset = rel
                .assets
                .iter()
                .find(|a| platform.matches_stable_asset(&a.name))
                .map(to_asset);
        }
        Channel::Edge => {
            let Some(rel) = releases.iter().find(|r| r.tag_name == "edge") else {
                return Ok(info);
            };
            info.release_url = rel.html_url.clone();
            let (Some(pkg), Some(sha_name)) = (platform.edge_asset(), platform.edge_sha_asset())
            else {
                return Ok(info);
            };
            let Some(sha_asset) = rel.assets.iter().find(|a| a.name == sha_name) else {
                return Ok(info);
            };
            // "<commit> <version>"
            let text = http
                .get(&sha_asset.browser_download_url)
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(|e| format!("could not read the edge build info: {e}"))?
                .text()
                .await
                .map_err(|e| e.to_string())?;
            let mut parts = text.split_whitespace();
            let sha = parts.next().unwrap_or_default().to_string();
            let version = parts.next().unwrap_or_default().to_string();
            info.latest = format!("{version} ({})", sha.get(..7).unwrap_or(&sha));
            info.available = !sha.is_empty() && sha != BUILD_SHA;
            info.asset = rel.assets.iter().find(|a| a.name == pkg).map(to_asset);
        }
    }
    if info.asset.is_none() {
        info.available = false;
    }
    Ok(info)
}

fn to_asset(a: &GhAsset) -> Asset {
    Asset {
        name: a.name.clone(),
        url: a.browser_download_url.clone(),
        digest: a.digest.clone(),
    }
}

fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")
        })
        .join("linuxmeeter")
}

/// Download the package, check its digest, and install it with pkexec.
/// `progress(text)` reports each step.
pub async fn install(
    platform: Platform,
    asset: &Asset,
    progress: impl Fn(&str),
) -> Result<(), String> {
    if platform == Platform::Unsupported {
        return Err("updates can be installed on Arch and Debian; download the package from the releases page".into());
    }
    if which("pkexec").is_none() {
        return Err("pkexec (polkit) is needed to install updates; install polkit or update from the releases page".into());
    }

    progress("Downloading…");
    let bytes = download_verified(asset).await?;
    let dir = cache_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&asset.name);
    let mut f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    f.write_all(&bytes).map_err(|e| e.to_string())?;
    drop(f);

    progress("Installing… (enter your password)");
    let status = install_command(platform, &path)
        .status()
        .map_err(|e| format!("could not start the installer: {e}"))?;
    let _ = std::fs::remove_file(&path);
    match status.code() {
        Some(0) => Ok(()),
        // pkexec: 126 = not authorized / dialog dismissed, 127 = no agent.
        Some(126) => Err("installation cancelled".into()),
        Some(127) => {
            Err("no password prompt available (polkit agent); update from the releases page".into())
        }
        _ => Err(format!("the package manager failed ({status})")),
    }
}

/// The asset's bytes, checked against GitHub's SHA-256 digest when given.
pub async fn download_verified(asset: &Asset) -> Result<Vec<u8>, String> {
    let bytes = client()?
        .get(&asset.url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("download failed: {e}"))?;
    if let Some(want) = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
    {
        let got = hex(&Sha256::digest(&bytes));
        if !got.eq_ignore_ascii_case(want) {
            return Err("the download is damaged (checksum mismatch); try again".into());
        }
    }
    Ok(bytes.to_vec())
}

fn install_command(platform: Platform, path: &Path) -> std::process::Command {
    let mut c = std::process::Command::new("pkexec");
    match platform {
        Platform::Arch => {
            c.args(["pacman", "-U", "--noconfirm"]).arg(path);
        }
        // Absolute path, so apt treats it as a file; downgrades allowed for
        // switching from edge back to stable.
        _ => {
            c.args([
                "env",
                "DEBIAN_FRONTEND=noninteractive",
                "apt-get",
                "install",
                "-y",
                "--allow-downgrades",
            ])
            .arg(path);
        }
    }
    c
}

fn which(cmd: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|d| Path::new(d).join(cmd))
        .find(|p| p.is_file())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The binary to start after an update. Once the package manager has
/// replaced it, /proc/self/exe reads "<path> (deleted)".
pub fn exe_after_update() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("/usr/bin/linuxmeeter"));
    let s = exe.to_string_lossy();
    match s.strip_suffix(" (deleted)") {
        Some(p) => PathBuf::from(p),
        None => exe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_release_part() {
        assert!(version_key("0.5.0") > version_key("0.4.1.r312"));
        assert!(version_key("v0.4.10") > version_key("0.4.9"));
        assert_eq!(version_key("0.4.1.r312"), version_key("0.4.1"));
        assert_eq!(version_key("0.4.1+r5"), vec![0, 4, 1]);
    }

    #[test]
    fn platforms_from_os_release() {
        assert_eq!(Platform::from_os_release("ID=arch\n"), Platform::Arch);
        assert_eq!(
            Platform::from_os_release("ID=cachyos\nID_LIKE=arch\n"),
            Platform::Arch
        );
        assert_eq!(Platform::from_os_release("ID=debian\n"), Platform::Debian);
        assert_eq!(
            Platform::from_os_release("ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n"),
            Platform::Debian
        );
        assert_eq!(
            Platform::from_os_release("ID=fedora\n"),
            Platform::Unsupported
        );
    }

    #[test]
    fn stable_assets_skip_edge_and_other_platforms() {
        assert!(Platform::Arch.matches_stable_asset("linuxmeeter-0.4.1.r3-1-x86_64.pkg.tar.zst"));
        assert!(!Platform::Arch.matches_stable_asset("linuxmeeter-edge-x86_64.pkg.tar.zst"));
        assert!(!Platform::Arch.matches_stable_asset("linuxmeeter_0.4.1_amd64.deb"));
        assert!(Platform::Debian.matches_stable_asset("linuxmeeter_0.4.1+r3_amd64.deb"));
        assert!(!Platform::Debian.matches_stable_asset("linuxmeeter-0.4.1-x86_64.tar.gz"));
    }

    #[test]
    fn deleted_exe_suffix_is_stripped() {
        // Only the suffix logic; current_exe itself is the test binary.
        let p = PathBuf::from("/usr/bin/linuxmeeter (deleted)");
        assert_eq!(
            p.to_string_lossy().strip_suffix(" (deleted)"),
            Some("/usr/bin/linuxmeeter")
        );
    }
}

#[cfg(test)]
mod live {
    /// Talks to GitHub: `cargo test -p linuxmeeter live_check -- --ignored --nocapture`.
    #[test]
    #[ignore = "network"]
    fn live_check() {
        for ch in [super::Channel::Stable, super::Channel::Edge] {
            let r = tauri::async_runtime::block_on(super::check(ch));
            println!("{ch:?}: {r:#?}");
            let info = r.expect("check");
            if let Some(mut asset) = info.asset {
                let bytes = tauri::async_runtime::block_on(super::download_verified(&asset))
                    .expect("download");
                println!("downloaded {} bytes, digest ok", bytes.len());
                asset.digest = Some(format!("sha256:{}", "0".repeat(64)));
                let bad = tauri::async_runtime::block_on(super::download_verified(&asset));
                assert!(
                    bad.unwrap_err().contains("checksum"),
                    "a wrong digest must be refused"
                );
            }
        }
    }
}
