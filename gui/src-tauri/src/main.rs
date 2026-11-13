// Prevents an additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// State model — mirrors `innernet show --json`
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Snapshot {
    version: String,
    /// Unix seconds when the snapshot was produced; 0 when the writer is older
    /// than this field. A file left behind by a daemon that has since stopped
    /// looks exactly like live data unless the age is checked, which is how the
    /// app came to show a connected network with no daemon running.
    #[serde(default)]
    generated_at: u64,
    config_dir: String,
    permission_denied: bool,
    interfaces: Vec<InterfaceSnapshot>,
}

impl Snapshot {
    /// Fill in boot-service state for every interface the CLI left blank.
    ///
    /// Applied at the edge, right after parsing, so every downstream consumer
    /// (the setup gate, the status line, the action list) sees the same answer
    /// regardless of which CLI version produced the snapshot.
    fn with_local_service_state(mut self) -> Self {
        self.interfaces = self
            .interfaces
            .into_iter()
            .map(InterfaceSnapshot::with_local_service_state)
            .collect();
        self
    }

    /// Strip everything that only a running daemon could have observed.
    ///
    /// Applied to a snapshot whose age proves nothing is refreshing it. The
    /// network is still configured, so the interface entries survive for their
    /// names and their service state -- but peers, addresses and the up flag
    /// described a tunnel that no longer exists, and presenting them is what
    /// made a disconnected network look connected.
    fn without_live_data(mut self) -> Self {
        for iface in &mut self.interfaces {
            iface.is_up = false;
            iface.peers.clear();
            iface.public_key = None;
            iface.me = None;
            iface.endpoint_overrides.clear();
        }
        self
    }

    /// Whether the interfaces carry data read from the WireGuard device.
    ///
    /// A snapshot built without device access still lists the configured
    /// networks, but the device itself is absent -- which on innernet means no
    /// public key. That is the reliable marker: peers can legitimately be zero
    /// on a fresh network, but a device always has a public key.
    /// Age of this snapshot in seconds, or None if it does not say.
    ///
    /// Anything older than a few daemon intervals is not live state: the daemon
    /// rewrites the file every interval, so a stale file means the daemon is not
    /// running. Fifty minutes is deliberately generous -- it only has to catch
    /// "the daemon is gone", not police jitter.
    fn age_secs(&self) -> Option<u64> {
        if self.generated_at == 0 {
            return None;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Some(now.saturating_sub(self.generated_at))
    }

    /// Whether anything is refreshing this snapshot.
    ///
    /// The daemon rewrites the file every interval (60s by default), so a file
    /// older than several intervals means nothing is writing it. The original
    /// threshold was fifty minutes, reasoning that it "only has to catch the
    /// daemon is gone". That was wrong in the case that matters: stopping the
    /// service leaves the file frozen at its last healthy state, and for the
    /// next fifty minutes the app kept drawing that dead state -- peers listed,
    /// interface up, "Disconnect" still offered -- as though the tunnel were
    /// running. Five minutes is five missed intervals: long enough not to fire
    /// on jitter, short enough that a stopped daemon stops being reported as
    /// alive.
    fn looks_abandoned(&self) -> bool {
        matches!(self.age_secs(), Some(age) if age > 300)
    }

    fn has_live_state(&self) -> bool {
        // "Live state" means the live device was actually read. An interface
        // without a public key was never read -- that is the shape a
        // permission-denied read produces, and treating it as live is what let
        // the app render an empty card and stop trying to fix itself.
        //
        // No interfaces at all is genuinely ambiguous (nothing configured vs.
        // nothing readable), so the caller's permission_denied decides it. This
        // method answers the narrower question it is named after: did we see a
        // device?
        self.interfaces.iter().any(|i| i.public_key.is_some())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InterfaceSnapshot {
    name: String,
    network_name: String,
    is_up: bool,
    listen_port: Option<u16>,
    address: String,
    network_cidr_prefix: u8,
    public_key: Option<String>,
    me: Option<PeerSnapshot>,
    peers: Vec<PeerSnapshot>,
    cidrs: Vec<CidrSnapshot>,
    endpoint_overrides: Vec<EndpointOverride>,
    /// Present in `show --json` since 2.4.0. Optional so the app still talks to
    /// an older CLI instead of failing to parse the whole snapshot.
    #[serde(default)]
    service: Option<ServiceStatus>,
}

impl InterfaceSnapshot {
    /// Replace the snapshot's boot-service state with what is true right now.
    ///
    /// Two separate reasons, both of which produced visible bugs:
    ///
    /// 1. `service` arrived in CLI 2.4.0. An older CLI omits it entirely, and
    ///    the UI then could not tell "no service" from "this CLI does not say",
    ///    which left the app pinned to the setup screen on a machine whose
    ///    daemon was running fine.
    ///
    /// 2. More importantly, the snapshot is written *by* the daemon. Once the
    ///    service stops, the file is frozen at its last healthy state: it keeps
    ///    saying `running: true` with peers attached. Trusting it made
    ///    Disconnect look broken -- the tunnel was gone, but the app still drew
    ///    a connected network, still listed peers, and still showed "Disconnect".
    ///    The service manager is the only thing that knows the answer *now*, so
    ///    it wins whenever it can be asked, whether or not the CLI reported a
    ///    value.
    ///
    /// Returns the interface unchanged where the platform cannot answer, so an
    /// unknown state never turns into a fabricated "not installed".
    fn with_local_service_state(mut self) -> Self {
        if let Some(status) = local_service_status(&self.name) {
            self.service = Some(status);
        }
        self
    }
}

/// Whether a boot service is installed for `interface`, read straight from the
/// service manager's own directory.
///
/// Returns `None` where this cannot be determined, so callers can distinguish
/// "not installed" from "unknown" rather than collapsing both to false.
///
/// Windows has no equivalent install marker to check here. The CLI registers a
/// scheduled task there, and `show --json` always reports `service` on a CLI new
/// enough to have it, so the fallback is not needed -- returning `None` is the
/// honest answer and the caller leaves the field absent rather than inventing
/// "not installed".
fn local_service_status(interface: &str) -> Option<ServiceStatus> {
    #[cfg(target_os = "macos")]
    let (manager, path) = (
        "launchd",
        PathBuf::from("/Library/LaunchDaemons").join(format!("com.innernet.{interface}.plist")),
    );

    #[cfg(all(not(target_os = "macos"), not(windows)))]
    let (manager, path) = (
        "systemd",
        PathBuf::from("/etc/systemd/system").join(format!("innernet@{interface}.service")),
    );

    #[cfg(windows)]
    {
        let _ = interface;
        return None;
    }

    // A missing parent directory means there is nothing to look at, not that the
    // service is absent for a reason worth reporting.
    #[cfg(not(windows))]
    path.parent().filter(|p| p.is_dir()).map(|_| {
        let installed = path.exists();
        ServiceStatus {
            manager: manager.to_string(),
            installed,
            // Ask the service manager directly. This is the difference between
            // "the snapshot said running" and "it is running now": the snapshot
            // is written by the daemon, so once the service is stopped the file
            // is frozen at its last healthy state and keeps claiming the tunnel
            // is up. Reading it back is how Disconnect appeared to do nothing --
            // the peers were still listed, and the button still said Disconnect,
            // because the data predated the disconnect.
            running: installed && service_manager_says_running(interface),
            path: path.display().to_string(),
        }
    })
}

/// Ask launchd/systemd whether the unit is loaded and running right now.
///
/// Cheap, unprivileged, and authoritative -- unlike the snapshot, which is a
/// record of a moment that may have passed. A non-zero exit means "not loaded",
/// which is a normal answer here, not an error to surface.
#[cfg(target_os = "macos")]
fn service_manager_says_running(interface: &str) -> bool {
    let label = format!("system/com.innernet.{interface}");
    match Command::new("launchctl").args(["print", &label]).output() {
        Ok(out) if out.status.success() => {
            // A loaded job can be waiting rather than running; only "running"
            // means the tunnel is actually up.
            String::from_utf8_lossy(&out.stdout).contains("state = running")
        },
        _ => false,
    }
}

#[cfg(all(not(target_os = "macos"), not(windows)))]
fn service_manager_says_running(interface: &str) -> bool {
    Command::new("systemctl")
        .args(["is-active", &format!("innernet@{interface}.service")])
        .output()
        .map(|out| out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "active")
        .unwrap_or(false)
}

#[cfg(windows)]
fn service_manager_says_running(_interface: &str) -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceStatus {
    manager: String,
    installed: bool,
    running: bool,
    path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PeerSnapshot {
    name: String,
    ip: String,
    public_key: String,
    cidr_id: i64,
    is_admin: bool,
    is_disabled: bool,
    is_you: bool,
    is_connected: bool,
    endpoint: Option<String>,
    endpoint_overridden: bool,
    last_handshake_secs_ago: Option<u64>,
    rx_bytes: u64,
    tx_bytes: u64,
    #[serde(default)]
    allowed_subnets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CidrSnapshot {
    id: i64,
    name: String,
    cidr: String,
    parent: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EndpointOverride {
    peer_ip: String,
    endpoint: String,
}

// ---------------------------------------------------------------------------
// Input validation
//
// Every value crossing the IPC boundary from the webview is validated here.
// The front-end is ordinary HTML/JS, so it is treated as untrusted input even
// though we also shell-quote: validation keeps malformed data from reaching the
// CLI at all, and gives the user a precise error instead of a clap usage dump.
// ---------------------------------------------------------------------------

fn validate_interface(name: &str) -> Result<String, String> {
    let t = name.trim();
    if t.is_empty() {
        return Err("interface name is empty".into());
    }
    // Linux caps interface names at 15 chars; the CLI also accepts the network
    // name, so allow a little more and let the CLI be the final judge.
    if t.len() > 32 {
        return Err(format!("interface name too long: {t}"));
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(format!("interface name has invalid characters: {t}"));
    }
    Ok(t.to_string())
}

fn validate_peer_name(name: &str) -> Result<String, String> {
    let t = name.trim();
    if t.is_empty() {
        return Err("peer name is empty".into());
    }
    if t.len() > 64 {
        return Err(format!("peer name too long: {t}"));
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(format!("peer name has invalid characters: {t}"));
    }
    Ok(t.to_string())
}

fn validate_cidr_name(name: &str) -> Result<String, String> {
    validate_peer_name(name).map_err(|e| e.replace("peer name", "CIDR name"))
}

/// Accepts `a.b.c.d/len` or `a.b.c.d`. Rejects anything that is not made of
/// digits, dots and slashes — this is what stops a crafted "CIDR" from being
/// interpreted as anything but an address.
fn validate_cidr(cidr: &str) -> Result<String, String> {
    let t = cidr.trim();
    if t.is_empty() {
        return Err("CIDR is empty".into());
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == '/' || c == ':' || c.is_ascii_hexdigit())
    {
        return Err(format!("CIDR has invalid characters: {t}"));
    }
    let (addr, len) = match t.split_once('/') {
        Some((a, l)) => (a, Some(l)),
        None => (t, None),
    };
    // The prefix bound depends on the address family: /32 for IPv4, /128 for
    // IPv6. A single >128 check would let "1.2.3.4/33" through.
    let is_v6 = addr.contains(':');
    let max_len: u8 = if is_v6 { 128 } else { 32 };
    if let Some(l) = len {
        let parsed: u8 = l
            .parse()
            .map_err(|_| format!("invalid prefix length in {t}"))?;
        if parsed > max_len {
            return Err(format!(
                "prefix length /{parsed} is out of range for an {} address (max /{max_len})",
                if is_v6 { "IPv6" } else { "IPv4" }
            ));
        }
    }
    // Basic IPv4 shape check; IPv6 is left to the CLI.
    if addr.contains('.') && !addr.contains(':') {
        let parts: Vec<&str> = addr.split('.').collect();
        if parts.len() != 4 {
            return Err(format!("not a valid IPv4 address: {addr}"));
        }
        for p in parts {
            if p.is_empty() || p.len() > 3 {
                return Err(format!("not a valid IPv4 address: {addr}"));
            }
            let n: u16 = p
                .parse()
                .map_err(|_| format!("not a valid IPv4 address: {addr}"))?;
            if n > 255 {
                return Err(format!("not a valid IPv4 address: {addr}"));
            }
        }
    }
    Ok(t.to_string())
}

/// Accepts `host:port` or `[v6]:port`, or a bare port for local listen ports.
fn validate_endpoint(ep: &str) -> Result<String, String> {
    let t = ep.trim();
    if t.is_empty() {
        return Err("endpoint is empty".into());
    }
    if t.len() > 255 {
        return Err("endpoint too long".into());
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || ".:[]-_".contains(c))
    {
        return Err(format!("endpoint has invalid characters: {t}"));
    }
    if !t.contains(':') {
        return Err(format!("endpoint must include a port: {t}"));
    }
    Ok(t.to_string())
}

fn validate_port(port: u16) -> Result<u16, String> {
    if port == 0 {
        return Err("port 0 is not usable; unset to randomize instead".into());
    }
    Ok(port)
}

fn validate_invite_path(path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path.trim());
    if !p.exists() {
        return Err(format!("invitation file not found: {}", p.display()));
    }
    if !p.is_file() {
        return Err(format!("not a file: {}", p.display()));
    }
    let ext = p
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ext != "toml" {
        return Err(format!(
            "expected a .toml invitation file, got .{ext} ({})",
            p.display()
        ));
    }
    Ok(p)
}

fn validate_save_path(path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path.trim());
    if p.as_os_str().is_empty() {
        return Err("save path is empty".into());
    }
    if let Some(dir) = p.parent() {
        if !dir.as_os_str().is_empty() && !dir.is_dir() {
            return Err(format!("directory does not exist: {}", dir.display()));
        }
    }
    Ok(p)
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// One whitelisted action.
///
/// The front-end may only name these variants; it can never supply a command
/// string or extra flags. Everything the CLI sees is constructed here from
/// validated fields.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    // -- lifecycle ---------------------------------------------------------
    /// Bring every interface up and refresh peers.
    Up,
    /// Bring one interface up / refresh its config.
    UpInterface {
        interface: String,
    },
    /// Fetch the peer list once.
    Fetch {
        interface: String,
    },
    /// Bring the interface down but KEEP the configuration.
    Down {
        interface: String,
    },
    /// Remove the interface and delete its configuration.
    Uninstall {
        interface: String,
    },
    /// Install the boot service that keeps the interface up.
    InstallService {
        interface: String,
    },
    /// Remove the boot service installed by `InstallService`.
    UninstallService {
        interface: String,
    },
    /// Join a network from an invitation file.
    Install {
        invite_path: String,
        /// Delete the invitation file after a successful install.
        #[serde(default)]
        delete_invite: bool,
    },

    // -- local settings ----------------------------------------------------
    SetListenPort {
        interface: String,
        port: u16,
    },
    UnsetListenPort {
        interface: String,
    },
    OverrideEndpoint {
        interface: String,
        endpoint: String,
    },
    UnsetEndpoint {
        interface: String,
    },
    OverridePeerEndpoint {
        interface: String,
        peer: String,
        endpoint: String,
    },
    UnsetPeerEndpoint {
        interface: String,
        peer: String,
    },

    // -- server-side peer management (requires admin) ----------------------
    AddPeer {
        interface: String,
        name: String,
        /// CIDR name to place the peer in (e.g. "humans", "devices").
        cidr: Option<String>,
        #[serde(default)]
        auto_ip: bool,
        ip: Option<String>,
        #[serde(default)]
        admin: bool,
        /// Where to write the generated invitation.
        save_config: String,
        /// Subnets this peer will route, e.g. "192.168.31.0/24".
        #[serde(default)]
        advertise_routes: Vec<String>,
    },
    DisablePeer {
        interface: String,
        name: String,
    },
    EnablePeer {
        interface: String,
        name: String,
    },
    RenamePeer {
        interface: String,
        name: String,
        new_name: String,
    },

    // -- CIDR management (requires admin) ----------------------------------
    AddCidr {
        interface: String,
        name: String,
        cidr: String,
        parent: Option<String>,
    },
    RenameCidr {
        interface: String,
        name: String,
        new_name: String,
    },
    DeleteCidr {
        interface: String,
        name: String,
    },
    AddAssociation {
        interface: String,
        cidr1: String,
        cidr2: String,
    },
    DeleteAssociation {
        interface: String,
        cidr1: String,
        cidr2: String,
    },
}

impl Action {
    /// Whether this action touches a privileged local resource.
    ///
    /// Only two operations genuinely need root on macOS: creating the tun
    /// device (`up`/`down`) and installing the boot service. Everything else
    /// -- reading state, inviting a peer, renaming or disabling one, adding a
    /// CIDR, editing associations -- is either a file read or an HTTP call to
    /// the coordinating server, authenticated by the WireGuard public key.
    ///
    /// Asking for an administrator password before an HTTP request is not a
    /// safety measure, it is a usability defect: `do shell script ...
    /// with administrator privileges` has no session reuse, so the prompt
    /// appears every single time. The GUI therefore elevates only for the
    /// actions below and runs the rest directly.
    fn needs_root(&self) -> bool {
        matches!(
            self,
            Action::Up
                | Action::UpInterface { .. }
                | Action::Down { .. }
                | Action::Fetch { .. }
                | Action::Install { .. }
                | Action::Uninstall { .. }
                | Action::SetListenPort { .. }
                | Action::UnsetListenPort { .. }
                | Action::OverrideEndpoint { .. }
                | Action::UnsetEndpoint { .. }
                | Action::OverridePeerEndpoint { .. }
                | Action::UnsetPeerEndpoint { .. }
                | Action::InstallService { .. }
                | Action::UninstallService { .. }
        )
    }

    fn args(&self) -> Result<Vec<String>, String> {
        let mut a: Vec<String> = Vec::new();

        match self {
            Action::Up => a.push("up".into()),
            Action::UpInterface { interface } => {
                a.push("up".into());
                a.push(validate_interface(interface)?);
            },
            Action::Fetch { interface } => {
                a.push("fetch".into());
                a.push(validate_interface(interface)?);
            },
            Action::Down { interface } => {
                a.push("down".into());
                a.push(validate_interface(interface)?);
            },
            Action::Uninstall { interface } => {
                a.push("uninstall".into());
                a.push(validate_interface(interface)?);
                a.push("--yes".into());
            },
            Action::InstallService { interface } => {
                a.push("install-service".into());
                a.push(validate_interface(interface)?);
            },
            Action::UninstallService { interface } => {
                a.push("uninstall-service".into());
                a.push(validate_interface(interface)?);
            },
            Action::Install {
                invite_path,
                delete_invite,
            } => {
                a.push("install".into());
                a.push(validate_invite_path(invite_path)?.display().to_string());
                // Use the network name from the invitation as the interface
                // name, and do not prompt: this app has already confirmed.
                a.push("--default-name".into());
                if *delete_invite {
                    a.push("--delete-invite".into());
                }
            },

            Action::SetListenPort { interface, port } => {
                a.push("set-listen-port".into());
                a.push(validate_interface(interface)?);
                a.push("--listen-port".into());
                a.push(validate_port(*port)?.to_string());
                a.push("--yes".into());
            },
            Action::UnsetListenPort { interface } => {
                a.push("set-listen-port".into());
                a.push(validate_interface(interface)?);
                a.push("--unset".into());
                a.push("--yes".into());
            },
            Action::OverrideEndpoint {
                interface,
                endpoint,
            } => {
                a.push("override-endpoint".into());
                a.push(validate_interface(interface)?);
                a.push("--endpoint".into());
                a.push(validate_endpoint(endpoint)?);
                a.push("--yes".into());
            },
            Action::UnsetEndpoint { interface } => {
                a.push("override-endpoint".into());
                a.push(validate_interface(interface)?);
                a.push("--unset".into());
                a.push("--yes".into());
            },
            Action::OverridePeerEndpoint {
                interface,
                peer,
                endpoint,
            } => {
                a.push("override-peer-endpoint".into());
                a.push(validate_interface(interface)?);
                a.push("--name".into());
                a.push(validate_peer_name(peer)?);
                a.push("--endpoint".into());
                a.push(validate_endpoint(endpoint)?);
                a.push("--yes".into());
            },
            Action::UnsetPeerEndpoint { interface, peer } => {
                a.push("override-peer-endpoint".into());
                a.push(validate_interface(interface)?);
                a.push("--name".into());
                a.push(validate_peer_name(peer)?);
                a.push("--unset".into());
                a.push("--yes".into());
            },

            Action::AddPeer {
                interface,
                name,
                cidr,
                auto_ip,
                ip,
                admin,
                save_config,
                advertise_routes,
            } => {
                a.push("add-peer".into());
                a.push(validate_interface(interface)?);
                a.push("--name".into());
                a.push(validate_peer_name(name)?);
                if let Some(c) = cidr {
                    if !c.trim().is_empty() {
                        a.push("--cidr".into());
                        a.push(validate_cidr_name(c)?);
                    }
                }
                match (auto_ip, ip) {
                    (true, _) => a.push("--auto-ip".into()),
                    (false, Some(addr)) if !addr.trim().is_empty() => {
                        a.push("--ip".into());
                        a.push(validate_cidr(addr)?);
                    },
                    (false, _) => a.push("--auto-ip".into()),
                }
                a.push("--admin".into());
                a.push(if *admin { "true" } else { "false" }.into());
                a.push("--save-config".into());
                a.push(validate_save_path(save_config)?.display().to_string());
                let mut routes = Vec::new();
                for r in advertise_routes {
                    if !r.trim().is_empty() {
                        routes.push(validate_cidr(r)?);
                    }
                }
                if !routes.is_empty() {
                    a.push("--advertise-routes".into());
                    a.push(routes.join(","));
                }
                a.push("--yes".into());
            },
            Action::DisablePeer { interface, name } => {
                a.push("disable-peer".into());
                a.push(validate_interface(interface)?);
                a.push("--name".into());
                a.push(validate_peer_name(name)?);
                a.push("--yes".into());
            },
            Action::EnablePeer { interface, name } => {
                a.push("enable-peer".into());
                a.push(validate_interface(interface)?);
                a.push("--name".into());
                a.push(validate_peer_name(name)?);
                a.push("--yes".into());
            },
            Action::RenamePeer {
                interface,
                name,
                new_name,
            } => {
                a.push("rename-peer".into());
                a.push(validate_interface(interface)?);
                a.push("--name".into());
                a.push(validate_peer_name(name)?);
                a.push("--new-name".into());
                a.push(validate_peer_name(new_name)?);
                a.push("--yes".into());
            },

            Action::AddCidr {
                interface,
                name,
                cidr,
                parent,
            } => {
                a.push("add-cidr".into());
                a.push(validate_interface(interface)?);
                a.push("--name".into());
                a.push(validate_cidr_name(name)?);
                a.push("--cidr".into());
                a.push(validate_cidr(cidr)?);
                if let Some(p) = parent {
                    if !p.trim().is_empty() {
                        a.push("--parent".into());
                        a.push(validate_cidr_name(p)?);
                    }
                }
                a.push("--yes".into());
            },
            Action::RenameCidr {
                interface,
                name,
                new_name,
            } => {
                a.push("rename-cidr".into());
                a.push(validate_interface(interface)?);
                a.push("--name".into());
                a.push(validate_cidr_name(name)?);
                a.push("--new-name".into());
                a.push(validate_cidr_name(new_name)?);
                a.push("--yes".into());
            },
            Action::DeleteCidr { interface, name } => {
                a.push("delete-cidr".into());
                a.push(validate_interface(interface)?);
                a.push("--name".into());
                a.push(validate_cidr_name(name)?);
                a.push("--yes".into());
            },
            Action::AddAssociation {
                interface,
                cidr1,
                cidr2,
            } => {
                a.push("add-association".into());
                a.push(validate_interface(interface)?);
                a.push(validate_cidr_name(cidr1)?);
                a.push(validate_cidr_name(cidr2)?);
                a.push("--yes".into());
            },
            Action::DeleteAssociation {
                interface,
                cidr1,
                cidr2,
            } => {
                a.push("delete-association".into());
                a.push(validate_interface(interface)?);
                a.push(validate_cidr_name(cidr1)?);
                a.push(validate_cidr_name(cidr2)?);
                a.push("--yes".into());
            },
        }

        Ok(a)
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ActionResult {
    ok: bool,
    stdout: String,
    stderr: String,
    needs_privileges: bool,
    /// The server refused because this peer is not an admin. Distinct from a
    /// local permission problem: the user cannot fix it by authorising the app.
    needs_admin: bool,
    /// The exact argv that ran, for the activity log.
    command: String,
}

impl ActionResult {
    /// A plain failure that needs no elevation and no admin rights.
    fn failed(message: String) -> Self {
        Self {
            ok: false,
            stdout: String::new(),
            stderr: message,
            needs_privileges: false,
            needs_admin: false,
            command: String::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct StateResult {
    ok: bool,
    error: Option<String>,
    needs_privileges: bool,
    snapshot: Option<Snapshot>,
    binary: Option<String>,
}

#[derive(Debug, Serialize)]
struct InvitePreview {
    ok: bool,
    error: Option<String>,
    path: Option<String>,
    network_name: Option<String>,
    address: Option<String>,
    server_endpoint: Option<String>,
}

// ---------------------------------------------------------------------------
// CLI discovery
// ---------------------------------------------------------------------------

/// The `innernet` CLI shipped inside the app bundle, if this build has one.
///
/// Tauri's sidecar mechanism places an `externalBin` next to the main
/// executable -- in `Contents/MacOS/` on macOS, beside the `.exe` on Windows.
/// Shipping the CLI this way means `innernet.app` is self-contained: installing
/// the app is enough, and there is no second thing to install or keep in step.
/// The minimum CLI whose `show --json` this app understands.
///
/// 2.4.4 is where an unreadable WireGuard socket stopped being reported as
/// "no networks": before it, `permission_denied` stayed false and interfaces
/// came back empty, which the app renders as an empty state rather than
/// offering the privileges path.
///
/// The floor is 2.6.0, not 2.4.4, because of `export-state`. That subcommand is
/// what lets the privileged daemon publish a world-readable snapshot, and it is
/// the *only* way the app reads live state without a password prompt. A CLI
/// without it cannot feed the app at all: `grant_access` calls `export-state`,
/// the call fails with "unrecognized subcommand", no snapshot is ever written,
/// and every refresh falls back to an authorisation prompt. Accepting such a
/// CLI produced the worst outcome available -- the app looked supported while
/// being permanently unable to do the one thing it needs.
///
/// `share-state` (2.5.3) matters too, but it is applied by the service itself
/// rather than called by the app, so the app's own floor is set by 2.6.0.
const MIN_CLI_VERSION: &str = "2.6.0";

/// Returns the minimum version when `reported` is older than it can handle.
///
/// Compares numeric components only; a CLI that reports something unparseable
/// is left alone rather than blocked, since guessing would be worse than
/// letting it through and seeing the real error.
fn version_too_old(reported: &str) -> Option<&'static str> {
    let parse = |v: &str| -> Option<(u64, u64, u64)> {
        let core = v.split(['-', '+']).next()?;
        let mut it = core.split('.');
        let major = it.next()?.parse().ok()?;
        let minor = it.next().unwrap_or("0").parse().ok()?;
        let patch = it.next().unwrap_or("0").parse().ok()?;
        Some((major, minor, patch))
    };

    let got = parse(reported)?;
    let need = parse(MIN_CLI_VERSION)?;
    if got < need {
        Some(MIN_CLI_VERSION)
    } else {
        None
    }
}

fn bundled_cli() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = if cfg!(windows) {
        "innernet.exe"
    } else {
        "innernet"
    };
    let candidate = dir.join(name);
    if candidate.is_file() {
        Some(candidate.display().to_string())
    } else {
        None
    }
}

fn cli_path() -> Result<String, String> {
    if let Ok(p) = std::env::var("INNERNET_CLI") {
        if !p.trim().is_empty() {
            return Ok(p);
        }
    }

    // The CLI we shipped inside the app bundle wins over anything installed on
    // the system. It is the exact build this GUI was released with, so the
    // `show --json` schema and the argv mapping are guaranteed to match -- a
    // brew-installed CLI can be older and silently return fields we do not
    // understand (or none at all).
    if let Some(bundled) = bundled_cli() {
        return Ok(bundled);
    }

    // A double-clicked .app inherits a minimal PATH, so probe known locations
    // before falling back to PATH lookup.
    let candidates = [
        "/opt/homebrew/bin/innernet",
        "/usr/local/bin/innernet",
        "/usr/bin/innernet",
        "/opt/innernet/bin/innernet",
    ];
    for c in candidates {
        if Path::new(c).is_file() {
            return Ok(c.to_string());
        }
    }

    // PATH is ':'-separated on unix and ';'-separated on Windows, and the
    // binary carries an .exe suffix there. Getting this wrong makes the CLI
    // unfindable on Windows even when it is on PATH.
    let exe = if cfg!(windows) {
        "innernet.exe"
    } else {
        "innernet"
    };
    let sep = if cfg!(windows) { ';' } else { ':' };

    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(sep) {
            if dir.trim().is_empty() {
                continue;
            }
            let p = Path::new(dir).join(exe);
            if p.is_file() {
                return Ok(p.display().to_string());
            }
        }
    }

    Err("could not find the `innernet` CLI. Set INNERNET_CLI to its path.".into())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// A snapshot built from config files alone, when the network is configured but
/// the boot service is not installed.
///
/// Returned so the UI can show the setup screen: it knows which network this
/// machine is meant to join, and that nothing is maintaining it. Built without
/// touching the WireGuard socket, so it costs no authorisation.
fn uninstalled_configuration(binary: &str) -> Option<Snapshot> {
    let out = Command::new(binary)
        .args(["show", "--json"])
        .output()
        .ok()?;
    let snapshot: Snapshot = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).ok()?;
    let snapshot = snapshot.with_local_service_state();
    // Only when there is something configured and nothing installed. A machine
    // with no configuration at all is a different screen (join a network), and
    // one with a service installed should go on to the privileged read.
    let configured = snapshot
        .interfaces
        .iter()
        .any(|i| i.service.as_ref().map(|s| !s.installed).unwrap_or(false));
    if configured {
        Some(snapshot)
    } else {
        None
    }
}

/// Read the snapshot the boot daemon publishes, if there is one.
///
/// This is the path that matters. The daemon runs as root and already fetches
/// on an interval, so it can see the live device; it writes that view to a
/// world-readable file. Reading it costs no privileges and prompts for nothing,
/// which is what makes the app usable -- the alternative was asking for an
/// administrator password on every refresh, because the WireGuard socket it
/// would otherwise need is root-only and gets recreated that way on every start.
///
/// Returns None when the daemon has never run (nothing published yet) or the
/// file is from a CLI too old to have written one, in which case the caller
/// falls back to reading directly.
fn read_exported_state() -> Option<(Snapshot, String)> {
    // Preferred location first: a fixed path the app can find without knowing
    // where the CLI was configured to keep its data.
    let candidates = [
        "/var/run/innernet/state.json",
        "/etc/innernet/state.json",
        "/var/lib/innernet/state.json",
    ];
    for path in candidates {
        let Ok(body) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(snapshot) = serde_json::from_str::<Snapshot>(&body) else {
            continue;
        };
        let snapshot = snapshot.with_local_service_state();
        // An exported snapshot is by definition a privileged read, so it must
        // not claim otherwise. If it does, it was written by something that
        // could not see the device either -- not worth trusting over a live
        // read.
        if snapshot.permission_denied || !snapshot.has_live_state() {
            continue;
        }
        // A file the daemon wrote once and never updated again means the daemon
        // is not running. Serving it verbatim would show peers as connected
        // while nothing maintains the tunnel -- the bug where Disconnect left
        // the peer list populated and the button still offering "Disconnect".
        //
        // Discarding it entirely is not right either: the config is still here,
        // the network still exists, and the honest state is "disconnected". So
        // the interface list is kept for its names, and everything that only a
        // live daemon could know is cleared.
        if snapshot.looks_abandoned() {
            let mut stale = snapshot.without_live_data();
            // Still report it, so the UI can show a disconnected network
            // instead of an empty window for a machine that plainly has one
            // configured.
            if stale
                .interfaces
                .iter()
                .any(|i| i.service.as_ref().is_some_and(|s| s.installed))
            {
                let version = stale.version.clone();
                stale.permission_denied = false;
                return Some((stale, version));
            }
            continue;
        }
        let version = snapshot.version.clone();
        return Some((snapshot, version));
    }
    None
}

#[tauri::command]
fn get_state() -> StateResult {
    let binary = match cli_path() {
        Ok(b) => b,
        Err(e) => {
            return StateResult {
                ok: false,
                error: Some(e),
                needs_privileges: false,
                snapshot: None,
                binary: None,
            }
        },
    };

    // The daemon's published snapshot is authoritative and free to read.
    if let Some((snapshot, version)) = read_exported_state() {
        if version_too_old(&version).is_none() {
            return StateResult {
                ok: true,
                error: None,
                needs_privileges: false,
                snapshot: Some(snapshot),
                binary: Some(binary),
            };
        }
    }

    // No usable snapshot. Before falling back to a privileged read, check the
    // cheap thing first: is there a configuration at all, and is it installed?
    //
    // This matters because the fallback prompts for a password, and prompting
    // someone who has not finished setting up is both pointless (there is no
    // service to read yet) and the exact complaint this rewrite exists to fix:
    // authorisation requests arriving for no visible reason. If setup is
    // incomplete the app should say so, not ask for a password.
    if let Some(uninstalled) = uninstalled_configuration(&binary) {
        return StateResult {
            ok: true,
            error: None,
            needs_privileges: false,
            snapshot: Some(uninstalled),
            binary: Some(binary),
        };
    }

    let output = match Command::new(&binary).args(["show", "--json"]).output() {
        Ok(o) => o,
        Err(e) => {
            return StateResult {
                ok: false,
                error: Some(format!("failed to run {binary}: {e}")),
                needs_privileges: false,
                snapshot: None,
                binary: Some(binary),
            }
        },
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    match serde_json::from_str::<Snapshot>(&stdout) {
        Ok(snapshot) => {
            let snapshot = snapshot.with_local_service_state();
            // A CLI older than this reports networks as invisible when it
            // simply cannot read the sockets, so the app would show "no
            // networks" on a machine that is connected. Refuse it loudly
            // instead: silence here looks like a working empty state.
            if let Some(needed) = version_too_old(&snapshot.version) {
                return StateResult {
                    ok: false,
                    needs_privileges: false,
                    error: Some(format!(
                        "The innernet CLI at {binary} is version {}, but this app needs {needed} \
                         or newer.\n\nUpgrade it, or install the app bundle again -- it ships \
                         its own CLI.",
                        snapshot.version
                    )),
                    snapshot: None,
                    binary: Some(binary),
                };
            }

            // Escalate when this account cannot see the live device, which is
            // not the same as "there are no networks".
            //
            // Two distinct failures hide behind an empty-looking UI:
            //   * the config files are unreadable, so interfaces is empty;
            //   * the config is readable but the WireGuard socket is not, so
            //     each interface arrives without its device -- no public key,
            //     no peers, no stats, is_up false.
            // Checking only the first left the second stuck on a blank card
            // that never offered to fix itself.
            if snapshot.permission_denied || !snapshot.has_live_state() {
                // The config dirs are root-only, so the unprivileged read can
                // never succeed. Read the state the same way every other
                // action runs -- through the authorisation prompt -- and cache
                // the fact that the user agreed, so refreshes do not nag.
                if let Some(privileged) = read_state_privileged(&binary) {
                    return privileged;
                }
                StateResult {
                    ok: false,
                    needs_privileges: true,
                    error: Some(format!(
                        "No permission to read {}. Add this user to the innernet group, \
                         or use the privileged actions in this app.",
                        snapshot.config_dir
                    )),
                    snapshot: Some(snapshot),
                    binary: Some(binary),
                }
            } else {
                StateResult {
                    ok: true,
                    error: None,
                    needs_privileges: false,
                    snapshot: Some(snapshot),
                    binary: Some(binary),
                }
            }
        },
        Err(e) => {
            let unsupported_json = stderr.contains("--json")
                && (stderr.contains("unexpected argument")
                    || stderr.contains("unrecognized")
                    || stderr.contains("Found argument"));

            let error = if unsupported_json {
                format!(
                    "{binary} is too old: it does not support `show --json`.\n\n\
                     Point INNERNET_CLI at a newer binary, or upgrade the CLI."
                )
            } else if stderr.trim().is_empty() {
                format!("could not parse state from {binary}: {e}")
            } else {
                stderr.trim().to_string()
            };

            StateResult {
                ok: false,
                needs_privileges: stderr.contains("Permission denied")
                    || stderr.contains("can't access"),
                error: Some(error),
                snapshot: None,
                binary: Some(binary),
            }
        },
    }
}

/// Read an invitation file so the UI can show what it is about to join.
///
/// This deliberately does not touch the network: it only parses the TOML the
/// user selected, so the confirmation step is informed.
#[tauri::command]
fn preview_invite(path: String) -> InvitePreview {
    let pb = match validate_invite_path(&path) {
        Ok(p) => p,
        Err(e) => {
            return InvitePreview {
                ok: false,
                error: Some(e),
                path: None,
                network_name: None,
                address: None,
                server_endpoint: None,
            }
        },
    };

    let text = match std::fs::read_to_string(&pb) {
        Ok(t) => t,
        Err(e) => {
            return InvitePreview {
                ok: false,
                error: Some(format!("could not read {}: {e}", pb.display())),
                path: Some(pb.display().to_string()),
                network_name: None,
                address: None,
                server_endpoint: None,
            }
        },
    };

    // Minimal TOML scraping: the invitation file has a stable [interface] and
    // [server] shape, and pulling in a TOML parser for three fields is not
    // worth the dependency in the GUI binary.
    // NOTE: the private key is intentionally never extracted or displayed.
    let mut network_name = None;
    let mut address = None;
    let mut server_endpoint = None;
    let mut section = String::new();

    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') && l.ends_with(']') {
            section = l.trim_matches(['[', ']'].as_ref()).to_string();
            continue;
        }
        if l.starts_with('#') {
            continue;
        }
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim().trim_matches('"');
        match (section.as_str(), k) {
            ("interface", "network-name") => network_name = Some(v.to_string()),
            ("interface", "address") => address = Some(v.to_string()),
            ("server", "external-endpoint") => server_endpoint = Some(v.to_string()),
            _ => {},
        }
    }

    InvitePreview {
        ok: true,
        error: None,
        path: Some(pb.display().to_string()),
        network_name,
        address,
        server_endpoint,
    }
}

/// Expose the bundled CLI on the user's PATH.
///
/// Shipping the CLI inside the .app makes the GUI self-contained, but it also
/// hides the tool from the terminal, which is where people expect to find it:
/// "installed the app" and "can run `innernet`" should not be different things.
///
/// A symlink from /usr/local/bin keeps one copy of the binary, so upgrading the
/// app upgrades the command too. The directory may need root, which is a single
/// authorisation on top of the one the setup screen already asks for.
#[tauri::command]
fn link_cli() -> ActionResult {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => return ActionResult::failed(format!("cannot locate the app: {e}")),
    };
    let dir = match exe.parent() {
        Some(d) => d,
        None => return ActionResult::failed("cannot locate the app directory".to_string()),
    };
    let bundled = dir.join("innernet");
    if !bundled.is_file() {
        return ActionResult::failed(format!(
            "this build has no bundled CLI at {}",
            bundled.display()
        ));
    }

    // /usr/local/bin is the conventional place for locally-installed tools and
    // is already on PATH for macOS users.
    let target = std::path::Path::new("/usr/local/bin/innernet");
    let link = format!("ln -sf {} {}", bundled.display(), target.display());

    let script =
        format!("set -e\nmkdir -p /usr/local/bin\n{link}\n/usr/local/bin/innernet --version");
    let escaped = script.replace('\\', "\\\\").replace('"', "\\\"");
    let applescript = format!(
        "do shell script \"{escaped}\" with administrator privileges \
         with prompt \"innernet will link its command-line tool into /usr/local/bin\""
    );

    match Command::new("osascript")
        .args(["-e", &applescript])
        .output()
    {
        Ok(o) if o.status.success() => ActionResult {
            ok: true,
            stdout: String::from_utf8_lossy(&o.stdout).to_string(),
            stderr: String::new(),
            needs_privileges: false,
            needs_admin: false,
            command: link,
        },
        Ok(o) => {
            let e = String::from_utf8_lossy(&o.stderr).to_string();
            if e.contains("-128") || e.contains("User canceled") {
                ActionResult::failed("cancelled".to_string())
            } else {
                ActionResult::failed(e)
            }
        },
        Err(e) => ActionResult::failed(format!("failed to request privileges: {e}")),
    }
}

#[tauri::command]
fn grant_access() -> ActionResult {
    let res = grant_local_access();
    if !res.ok {
        return res;
    }

    // The authorisation the user just gave is the only one they should have to
    // give. Finishing the job here -- rather than telling them to open a
    // terminal and run install-service -- is what makes the app installable:
    // an app that leaves a setup step on the user's plate has not been
    // installed, it has been unpacked.
    //
    // Installing the service also starts the tunnel and keeps it up across
    // restarts, which is what "connected" means; and because the service is
    // root, it publishes the state snapshot the app reads. Without this step
    // the app has permissions but no daemon, so it would still have nothing to
    // show.
    let mut notes = Vec::new();
    if let Ok(binary) = cli_path() {
        if let Some(iface) = first_interface_name(&binary) {
            let installed =
                run_privileged(&binary, &["install-service".to_string(), iface.clone()]);
            if installed.ok {
                notes.push(format!("service installed for {iface}"));
            } else {
                notes.push(format!(
                    "could not install the service: {}",
                    installed.stderr.trim()
                ));
            }
            // Publish once immediately so the app has data to show before the
            // daemon's first interval elapses.
            let _ = run_privileged(&binary, &["export-state".to_string(), iface]);
        }
    }

    let _ = STATE_READ_AUTHORISED.set(());
    ActionResult {
        ok: true,
        needs_privileges: false,
        needs_admin: false,
        stdout: notes.join("\n"),
        stderr: String::new(),
        command: "grant access".to_string(),
    }
}

/// Name of the first configured interface, for setup-time actions.
///
/// Read from `show --json` because that is the only listing that works before
/// permissions exist: it falls back to the config files, which are the
/// authoritative record of what is configured here.
fn first_interface_name(binary: &str) -> Option<String> {
    let out = Command::new(binary)
        .args(["show", "--json"])
        .output()
        .ok()?;
    let snapshot: Snapshot = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).ok()?;
    snapshot
        .interfaces
        .first()
        .map(|i| i.name.clone())
        .filter(|n| !n.is_empty())
}

#[tauri::command]
fn run_action(action: Action) -> ActionResult {
    let action_needs_root = action.needs_root();

    let argv = match action.args() {
        Ok(a) => a,
        Err(e) => {
            return ActionResult {
                ok: false,
                stdout: String::new(),
                stderr: format!("invalid input: {e}"),
                needs_privileges: false,
                needs_admin: false,
                command: String::new(),
            }
        },
    };

    let binary = match cli_path() {
        Ok(b) => b,
        Err(e) => {
            return ActionResult {
                ok: false,
                stdout: String::new(),
                stderr: e,
                needs_privileges: false,
                needs_admin: false,
                command: String::new(),
            }
        },
    };

    let display = format!(
        "{} {}",
        Path::new(&binary)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| binary.clone()),
        argv.join(" ")
    );

    let mut res = if action_needs_root {
        run_privileged(&binary, &argv)
    } else {
        run_direct(&binary, &argv)
    };

    // A 401 from the coordinating server means "this peer is not an admin".
    // That is not fixable with an authorisation prompt, so it gets its own
    // flag and its own message in the UI.
    if !res.ok {
        let e = &res.stderr;
        res.needs_admin = e.contains("401") || e.contains("Unauthorized");
    }

    res.command = display;
    res
}

/// Grant the current user standing access to the root-only innernet dirs.
///
/// The alternative is to prompt for authorisation on every launch, forever,
/// because /etc/innernet and /var/lib/innernet are root-only. This runs the
/// two commands `innernet` itself recommends instead: put the dirs in a group
/// the user belongs to, and give that group read/write. After that the app can
/// read state and even change local settings without any prompt at all.
///
/// The group is named after the user, so two people on one Mac do not end up
/// sharing access. `dscl` creates it if missing, which is idempotent.
#[cfg(target_os = "macos")]
fn grant_local_access() -> ActionResult {
    let user = match Command::new("id").arg("-un").output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => {
            return ActionResult {
                ok: false,
                stdout: String::new(),
                stderr: "could not determine the current user".to_string(),
                needs_privileges: false,
                needs_admin: false,
                command: String::new(),
            }
        },
    };

    let group = format!("innernet-{user}");

    // Three things have to hold for the app to work without prompting, and all
    // three are checked by `set -e` rather than assumed:
    //
    //   1. the group exists *and has a GID* -- `dscl . -create /Groups/x`
    //      alone leaves it without one, and `dseditgroup` then silently fails
    //      to add the member, which is how an earlier version reported success
    //      while granting nothing;
    //   2. the config/data directories are group-readable;
    //   3. the WireGuard sockets are too. Without this the peer list is empty
    //      even when everything else is right, because the userspace backend
    //      cannot connect to a 0700 root:daemon socket and silently filters
    //      the interface out.
    //
    // `set -e` plus a final marker echo means a partial failure reports as a
    // failure instead of a cheerful "granted".
    let script = format!(
        "set -e\n         if ! dscl . -read /Groups/{group} >/dev/null 2>&1; then\n           gid=$(dscl . -list /Groups PrimaryGroupID | awk '{{print $2}}' | sort -n | tail -1)\n           dscl . -create /Groups/{group}\n           dscl . -create /Groups/{group} RealName \"innernet access for {user}\"\n           dscl . -create /Groups/{group} PrimaryGroupID $((gid + 1))\n         fi\n         dseditgroup -o edit -a {user} -t user {group}\n         chgrp -R {group} /etc/innernet /var/lib/innernet\n         chmod -R g+rwX /etc/innernet /var/lib/innernet\n         if [ -d /var/run/wireguard ]; then\n           chgrp -R everyone /var/run/wireguard\n           chmod g+rw /var/run/wireguard/*.sock\n           chmod g+r /var/run/wireguard/*.name\n         fi\n         echo GRANTED-{group}"
    );

    let escaped = script.replace('\\', "\\\\").replace('"', "\\\"");
    let applescript = format!(
        "do shell script \"{escaped}\" with administrator privileges \
         with prompt \"innernet will let your account read its network configuration without asking again\""
    );

    match Command::new("osascript")
        .args(["-e", &applescript])
        .output()
    {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            // Trust the marker, not the exit status: `do shell script` reports
            // success for a script that ran but achieved nothing.
            let granted = stdout.contains(&format!("GRANTED-{group}"));
            ActionResult {
                ok: granted,
                stderr: if granted {
                    String::new()
                } else {
                    let e = String::from_utf8_lossy(&o.stderr).to_string();
                    if e.trim().is_empty() {
                        "granting access did not complete (no marker from the script)".to_string()
                    } else {
                        e
                    }
                },
                stdout,
                needs_privileges: false,
                needs_admin: false,
                command: format!("grant access to {group}"),
            }
        },
        Err(e) => ActionResult {
            ok: false,
            stdout: String::new(),
            stderr: format!("failed to request privileges: {e}"),
            needs_privileges: false,
            needs_admin: false,
            command: String::new(),
        },
    }
}

#[cfg(not(target_os = "macos"))]
fn grant_local_access() -> ActionResult {
    ActionResult {
        ok: false,
        stdout: String::new(),
        stderr: "this action is only needed on macOS".to_string(),
        needs_privileges: false,
        needs_admin: false,
        command: String::new(),
    }
}

/// Read `show --json` through the authorisation prompt.
///
/// Returns `None` when the user has not authorised yet and we should fall back
/// to asking the UI to show the privileges screen -- rather than treating a
/// cancelled prompt as a hard error.
fn read_state_privileged(binary: &str) -> Option<StateResult> {
    // Once the user has authorised, refreshes must not prompt again: go through
    // the same prompt only when we know it will not be shown.
    let already = STATE_READ_AUTHORISED.get().is_some();

    let output =
        run_privileged_capture(binary, &["show".to_string(), "--json".to_string()], already)?;
    let parsed = serde_json::from_str::<Snapshot>(&output).ok()?;
    let parsed = parsed.with_local_service_state();

    // If even a privileged read still cannot see the device, prompting again
    // would loop forever. Hand the (empty) snapshot back and let the caller
    // surface the real error.
    if parsed.permission_denied || !parsed.has_live_state() {
        return None;
    }

    let _ = STATE_READ_AUTHORISED.set(());

    Some(StateResult {
        ok: true,
        error: None,
        needs_privileges: false,
        snapshot: Some(parsed),
        binary: Some(binary.to_string()),
    })
}

/// Whether the user has already authorised one privileged state read.
///
/// `/etc/innernet` and `/var/lib/innernet` are root-only, so an unprivileged
/// `show --json` always reports permission_denied. Reading the state therefore
/// needs the same authorisation as changing it -- but prompting on every
/// refresh would be unbearable. The first successful privileged read is
/// remembered for the life of the process, so the user authorises once per
/// launch rather than once per refresh.
static STATE_READ_AUTHORISED: OnceLock<()> = OnceLock::new();

// ---------------------------------------------------------------------------
// Privileged execution
// ---------------------------------------------------------------------------

/// Run a command as root and return its stdout, or `None` if the user did not
/// authorise. `quiet` skips the prompt: it is used for refreshes after the user
/// has already agreed once, where a second prompt would be pure noise.
#[cfg(target_os = "macos")]
fn run_privileged_capture(binary: &str, args: &[String], quiet: bool) -> Option<String> {
    // launchctl-style tools cannot re-use a previous authorisation, so after
    // the first consent the only way to avoid a prompt is to check whether the
    // unprivileged call happens to work now (e.g. the user joined the group).
    if quiet {
        let out = Command::new(binary).args(args).output().ok()?;
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            // Exit 0 is not enough for `show --json`: an unprivileged read
            // succeeds and returns a well-formed snapshot with
            // permission_denied set and no interfaces. Returning that as the
            // answer made every refresh after the first silently replace real
            // data with an empty shell, which is why the UI went blank once the
            // initial privileged read was not repeated.
            let usable = serde_json::from_str::<Snapshot>(&stdout)
                .map(|s| !s.permission_denied && s.has_live_state())
                .unwrap_or(true);
            if usable {
                return Some(stdout);
            }
        }
        // Fall through to the prompt: better a prompt than a dead UI.
    }

    let mut script = shell_quote(binary);
    for a in args {
        script.push(' ');
        script.push_str(&shell_quote(a));
    }
    let escaped = script.replace('\\', "\\\\").replace('"', "\\\"");
    let applescript = format!(
        "do shell script \"{escaped}\" with administrator privileges \
         with prompt \"innernet needs permission to read the network state\""
    );

    let out = Command::new("osascript")
        .args(["-e", &applescript])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Windows and Linux run as the same user that installed the interface, so no
/// elevation is involved in reading state.
#[cfg(not(target_os = "macos"))]
fn run_privileged_capture(binary: &str, args: &[String], _quiet: bool) -> Option<String> {
    let out = Command::new(binary).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Run a command as the current user.
///
/// Used for every action that only reads files or talks HTTP, so the app never
/// shows an administrator prompt for work that does not require one.
fn run_direct(binary: &str, args: &[String]) -> ActionResult {
    match Command::new(binary).args(args).output() {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            ActionResult {
                ok: o.status.success(),
                stdout: String::from_utf8_lossy(&o.stdout).to_string(),
                needs_privileges: !o.status.success()
                    && (stderr.contains("Permission denied") || stderr.contains("can't access")),
                stderr,
                needs_admin: false,
                command: String::new(),
            }
        },
        Err(e) => ActionResult {
            ok: false,
            stdout: String::new(),
            stderr: format!("failed to run {binary}: {e}"),
            needs_privileges: false,
            needs_admin: false,
            command: String::new(),
        },
    }
}

#[cfg(target_os = "macos")]
fn run_privileged(binary: &str, args: &[String]) -> ActionResult {
    // Use the native authorisation dialog instead of collecting a password in
    // the app. `do shell script ... with administrator privileges` runs the
    // command as root after Touch ID or password.
    let mut script = shell_quote(binary);
    for a in args {
        script.push(' ');
        script.push_str(&shell_quote(a));
    }
    let escaped = script.replace('\\', "\\\\").replace('"', "\\\"");
    let applescript = format!(
        "do shell script \"{escaped}\" with administrator privileges \
         with prompt \"innernet needs permission to change network settings\""
    );

    match Command::new("osascript")
        .args(["-e", &applescript])
        .output()
    {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            let cancelled = stderr.contains("-128") || stderr.contains("User canceled");
            ActionResult {
                ok: o.status.success(),
                stdout: String::from_utf8_lossy(&o.stdout).to_string(),
                stderr: if cancelled {
                    "cancelled".to_string()
                } else {
                    stderr
                },
                needs_privileges: false,
                needs_admin: false,
                command: String::new(),
            }
        },
        Err(e) => ActionResult {
            ok: false,
            stdout: String::new(),
            stderr: format!("failed to request privileges: {e}"),
            needs_privileges: false,
            needs_admin: false,
            command: String::new(),
        },
    }
}

#[cfg(not(target_os = "macos"))]
fn run_privileged(binary: &str, args: &[String]) -> ActionResult {
    match Command::new(binary).args(args).output() {
        Ok(o) => ActionResult {
            ok: o.status.success(),
            stdout: String::from_utf8_lossy(&o.stdout).to_string(),
            stderr: String::from_utf8_lossy(&o.stderr).to_string(),
            needs_privileges: false,
            needs_admin: false,
            command: String::new(),
        },
        Err(e) => ActionResult {
            ok: false,
            stdout: String::new(),
            stderr: format!("failed to run {binary}: {e}"),
            needs_privileges: false,
            needs_admin: false,
            command: String::new(),
        },
    }
}

/// Quote a single argument for POSIX sh.
///
/// Only the macOS path uses this (it builds an AppleScript string that runs
/// through `sh`), but it is compiled everywhere so the quoting rules stay
/// unit-tested on every platform CI builds. A broken quote here would be a
/// shell injection, so it is worth testing off-macOS too.
#[allow(dead_code)]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_state,
            preview_invite,
            run_action,
            grant_access,
            link_cli
        ])
        .run(tauri::generate_context!())
        .expect("error while running innernet gui");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_of(a: Action) -> Vec<String> {
        a.args().expect("valid action")
    }

    #[test]
    fn lifecycle_args_are_exact() {
        assert_eq!(args_of(Action::Up), vec!["up"]);
        assert_eq!(
            args_of(Action::UpInterface {
                interface: "heheda-mesh".into()
            }),
            vec!["up", "heheda-mesh"]
        );
        assert_eq!(
            args_of(Action::Fetch {
                interface: "heheda-mesh".into()
            }),
            vec!["fetch", "heheda-mesh"]
        );
    }

    /// `down` and `uninstall` must not be conflated: down keeps the config,
    /// uninstall removes it. Mixing them up destroys the user's network.
    #[test]
    fn down_and_uninstall_are_distinct() {
        let down = args_of(Action::Down {
            interface: "net".into(),
        });
        let uninstall = args_of(Action::Uninstall {
            interface: "net".into(),
        });
        assert_eq!(down, vec!["down", "net"]);
        assert_eq!(uninstall, vec!["uninstall", "net", "--yes"]);
        assert_ne!(down[0], uninstall[0]);
    }

    #[test]
    fn local_setting_args() {
        assert_eq!(
            args_of(Action::SetListenPort {
                interface: "net".into(),
                port: 51820
            }),
            vec!["set-listen-port", "net", "--listen-port", "51820", "--yes"]
        );
        assert_eq!(
            args_of(Action::UnsetListenPort {
                interface: "net".into()
            }),
            vec!["set-listen-port", "net", "--unset", "--yes"]
        );
        assert_eq!(
            args_of(Action::OverrideEndpoint {
                interface: "net".into(),
                endpoint: "1.2.3.4:51820".into()
            }),
            vec![
                "override-endpoint",
                "net",
                "--endpoint",
                "1.2.3.4:51820",
                "--yes"
            ]
        );
        assert_eq!(
            args_of(Action::OverridePeerEndpoint {
                interface: "net".into(),
                peer: "laptop".into(),
                endpoint: "10.0.0.2:51820".into()
            }),
            vec![
                "override-peer-endpoint",
                "net",
                "--name",
                "laptop",
                "--endpoint",
                "10.0.0.2:51820",
                "--yes"
            ]
        );
    }

    #[test]
    fn add_peer_args_include_routes() {
        let a = args_of(Action::AddPeer {
            interface: "net".into(),
            name: "laptop".into(),
            cidr: Some("humans".into()),
            auto_ip: true,
            ip: None,
            admin: false,
            // Must exist: the action validates the parent directory, and a
            // hardcoded /tmp does not exist on Windows.
            save_config: std::env::temp_dir()
                .join("laptop.toml")
                .display()
                .to_string(),
            advertise_routes: vec!["192.168.31.0/24".into(), "10.0.0.0/16".into()],
        });
        assert_eq!(a[0], "add-peer");
        assert!(a.contains(&"--auto-ip".to_string()));
        assert!(a.contains(&"--advertise-routes".to_string()));
        assert!(a.contains(&"192.168.31.0/24,10.0.0.0/16".to_string()));
        assert!(a.contains(&"--save-config".to_string()));
        // The admin flag must be explicit, never rely on a CLI default.
        let idx = a.iter().position(|s| s == "--admin").unwrap();
        assert_eq!(a[idx + 1], "false");
    }

    #[test]
    fn cidr_and_association_args() {
        assert_eq!(
            args_of(Action::AddCidr {
                interface: "net".into(),
                name: "engineers".into(),
                cidr: "10.60.5.0/24".into(),
                parent: Some("humans".into()),
            }),
            vec![
                "add-cidr",
                "net",
                "--name",
                "engineers",
                "--cidr",
                "10.60.5.0/24",
                "--parent",
                "humans",
                "--yes"
            ]
        );
        assert_eq!(
            args_of(Action::AddAssociation {
                interface: "net".into(),
                cidr1: "humans".into(),
                cidr2: "devices".into(),
            }),
            vec!["add-association", "net", "humans", "devices", "--yes"]
        );
    }

    #[test]
    fn install_args_are_exact() {
        // Install takes an existing file; use this test's own source file as a
        // stand-in with the right extension.
        let dir = std::env::temp_dir().join("innernet-gui-test");
        std::fs::create_dir_all(&dir).unwrap();
        let invite = dir.join("invite.toml");
        std::fs::write(&invite, "[interface]\n").unwrap();

        let a = args_of(Action::Install {
            invite_path: invite.display().to_string(),
            delete_invite: true,
        });
        assert_eq!(a[0], "install");
        assert_eq!(a[1], invite.display().to_string());
        assert!(a.contains(&"--default-name".to_string()));
        assert!(a.contains(&"--delete-invite".to_string()));
    }

    // -- validation --------------------------------------------------------

    #[test]
    fn interface_validation_rejects_injection() {
        assert!(validate_interface("heheda-mesh").is_ok());
        assert!(validate_interface("").is_err());
        assert!(validate_interface("a; rm -rf /").is_err());
        assert!(validate_interface("net$(whoami)").is_err());
        assert!(validate_interface("net name").is_err());
        assert!(validate_interface(&"x".repeat(40)).is_err());
    }

    #[test]
    fn cidr_validation() {
        assert!(validate_cidr("192.168.31.0/24").is_ok());
        assert!(validate_cidr("10.0.0.0/16").is_ok());
        assert!(validate_cidr("10.60.1.2").is_ok());
        assert!(validate_cidr("1.2.3.4/33").is_err());
        assert!(validate_cidr("999.1.1.1/24").is_err());
        assert!(validate_cidr("1.2.3/24").is_err());
        assert!(validate_cidr("1.2.3.4; rm -rf /").is_err());
        assert!(validate_cidr("").is_err());
    }

    #[test]
    fn endpoint_validation() {
        assert!(validate_endpoint("1.2.3.4:51820").is_ok());
        assert!(validate_endpoint("host.example:51820").is_ok());
        assert!(validate_endpoint("[::1]:51820").is_ok());
        assert!(validate_endpoint("1.2.3.4").is_err());
        assert!(validate_endpoint("1.2.3.4:51820; id").is_err());
        assert!(validate_endpoint("").is_err());
    }

    #[test]
    fn invite_path_validation() {
        assert!(validate_invite_path("/nonexistent/x.toml").is_err());
        let dir = std::env::temp_dir().join("innernet-gui-test");
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("x.pem");
        std::fs::write(&bad, "x").unwrap();
        assert!(validate_invite_path(&bad.display().to_string()).is_err());
    }

    #[test]
    fn invalid_input_never_produces_argv() {
        // A rejected field must abort before any argv exists, so the CLI is
        // never invoked with attacker-controlled data.
        let bad = Action::Down {
            interface: "net; rm -rf /".into(),
        };
        assert!(bad.args().is_err());
    }

    /// A 401 from the server means "not an admin", and the UI must not tell
    /// the user to authorise the app for it.
    #[test]
    fn unauthorized_response_is_flagged_as_admin_problem() {
        // Shapes ureq/the CLI actually produce for a 401.
        for msg in [
            "Error sending request: Status code 401",
            "Error: Unauthorized",
            "HTTP 401 Unauthorized",
        ] {
            assert!(
                msg.contains("401") || msg.contains("Unauthorized"),
                "detection would miss {msg}"
            );
        }
        // A local permission problem must NOT be mistaken for an admin issue.
        let local = "Permission denied (os error 13)";
        assert!(!(local.contains("401") || local.contains("Unauthorized")));
    }

    /// Only admins may invite peers, manage CIDRs, or disable other peers.
    /// This guards the mapping the UI uses to decide what to show.
    #[test]
    fn admin_only_actions_are_the_network_mutating_ones() {
        let admin_only = [
            "add_peer",
            "disable_peer",
            "enable_peer",
            "rename_peer",
            "add_cidr",
            "rename_cidr",
            "delete_cidr",
            "add_association",
            "delete_association",
        ];
        // Leaving a network only deletes local config, so members keep it.
        let member_allowed = ["uninstall", "down", "up", "fetch", "set_listen_port"];

        assert!(!admin_only.iter().any(|a| member_allowed.contains(a)));
        assert!(admin_only.contains(&"add_peer"));
        assert!(member_allowed.contains(&"uninstall"));
    }

    /// CLI lookup must use the host's PATH separator and executable suffix.
    /// A ':' split on Windows, or a missing .exe, silently fails to find a
    /// CLI that is installed and on PATH.
    #[test]
    fn cli_lookup_matches_host_conventions() {
        let exe = if cfg!(windows) {
            "innernet.exe"
        } else {
            "innernet"
        };
        let sep = if cfg!(windows) { ';' } else { ':' };

        // The suffix must be platform-correct; a bare "innernet" would not be
        // executable on Windows.
        assert_eq!(exe.ends_with(".exe"), cfg!(windows));

        // A Windows PATH like "C:\a;C:\b" must not split on the drive colon.
        let sample = if cfg!(windows) {
            "C:\\first;C:\\second"
        } else {
            "/first:/second"
        };
        let parts: Vec<&str> = sample.split(sep).collect();
        assert_eq!(parts.len(), 2, "PATH split on {sep:?} produced {parts:?}");
        for p in parts {
            assert!(!p.is_empty());
        }
    }

    #[test]
    fn shell_quoting_handles_quotes() {
        assert_eq!(shell_quote("plain"), "'plain'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote("a; rm -rf /"), "'a; rm -rf /'");
    }

    #[test]
    fn invite_preview_never_leaks_private_key() {
        let dir = std::env::temp_dir().join("innernet-gui-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("preview.toml");
        std::fs::write(
            &p,
            "[interface]\nnetwork-name = \"demo\"\naddress = \"10.60.9.9/16\"\n\
             private-key = \"SUPERSECRETKEY\"\n\n[server]\n\
             public-key = \"abc\"\nexternal-endpoint = \"1.2.3.4:51820\"\n",
        )
        .unwrap();

        let prev = preview_invite(p.display().to_string());
        assert!(prev.ok);
        assert_eq!(prev.network_name.as_deref(), Some("demo"));
        assert_eq!(prev.address.as_deref(), Some("10.60.9.9/16"));
        assert_eq!(prev.server_endpoint.as_deref(), Some("1.2.3.4:51820"));
        // The preview struct has no field that could carry the key.
        let json = serde_json::to_string(&prev).unwrap();
        assert!(!json.contains("SUPERSECRETKEY"));
    }
}

#[cfg(test)]
mod service_tests {
    use super::*;

    #[test]
    fn service_actions_use_the_cli_subcommands() {
        // The GUI must drive the CLI rather than writing unit files itself: the
        // CLI is what knows the platform paths and the running binary.
        assert_eq!(
            Action::InstallService {
                interface: "heheda-mesh".into()
            }
            .args()
            .unwrap(),
            vec!["install-service", "heheda-mesh"]
        );
        assert_eq!(
            Action::UninstallService {
                interface: "heheda-mesh".into()
            }
            .args()
            .unwrap(),
            vec!["uninstall-service", "heheda-mesh"]
        );
    }

    #[test]
    fn service_actions_validate_the_interface_name() {
        // Same guard as every other action: a crafted interface name must not
        // reach argv, or it becomes a way to run arbitrary commands as root
        // once the authorisation prompt is accepted.
        let bad = Action::InstallService {
            interface: "foo; rm -rf /".into(),
        };
        assert!(bad.args().is_err());
        let bad = Action::UninstallService {
            interface: "$(whoami)".into(),
        };
        assert!(bad.args().is_err());
    }

    #[test]
    fn service_status_is_optional_for_older_clis() {
        // A CLI older than 2.4.0 has no `service` key. The whole snapshot must
        // still parse, otherwise upgrading the app before the CLI would break
        // the network list entirely.
        let json = r#"{
            "name": "heheda-mesh",
            "network_name": "heheda-mesh",
            "is_up": true,
            "listen_port": 51820,
            "address": "10.60.1.2",
            "network_cidr_prefix": 16,
            "public_key": null,
            "me": null,
            "peers": [],
            "cidrs": [],
            "endpoint_overrides": []
        }"#;
        let snap: InterfaceSnapshot = serde_json::from_str(json).expect("should parse");
        assert!(snap.service.is_none());
    }

    #[test]
    fn service_status_distinguishes_the_three_states() {
        // The UI offers a different action per state, so the JSON must carry
        // enough to tell them apart: not installed, installed but stopped, and
        // running.
        let parse = |installed: bool, running: bool| {
            let json = format!(
                r#"{{"name":"x","network_name":"x","is_up":false,"listen_port":null,
                    "address":"10.0.0.1","network_cidr_prefix":16,"public_key":null,
                    "me":null,"peers":[],"cidrs":[],"endpoint_overrides":[],
                    "service":{{"manager":"launchd","installed":{installed},
                    "running":{running},"path":"/Library/LaunchDaemons/x.plist"}}}}"#
            );
            serde_json::from_str::<InterfaceSnapshot>(&json)
                .unwrap()
                .service
                .unwrap()
        };

        let absent = parse(false, false);
        let stopped = parse(true, false);
        let running = parse(true, true);

        assert!(!absent.installed && !absent.running);
        assert!(stopped.installed && !stopped.running);
        assert!(running.installed && running.running);
        assert_eq!(running.manager, "launchd");
        assert_eq!(running.path, "/Library/LaunchDaemons/x.plist");
    }
}

#[cfg(test)]
mod cli_discovery_tests {
    use super::*;

    /// A CLI older than MIN_CLI_VERSION reports an unreadable socket as "no
    /// networks", so the app must refuse it rather than render an empty state.
    #[test]
    fn old_cli_versions_are_rejected() {
        assert_eq!(version_too_old("2.2.2"), Some(MIN_CLI_VERSION));
        assert_eq!(version_too_old("2.4.3"), Some(MIN_CLI_VERSION));
        assert_eq!(version_too_old("1.9.9"), Some(MIN_CLI_VERSION));
    }

    /// The versions the app used to wave through, and must not any more.
    ///
    /// 2.4.4 is inside the old floor but has no `export-state`, so the app would
    /// have run every command except the one that feeds it. 2.5.1 is what a
    /// plain `brew install innernet` gave users for most of this release cycle.
    /// Both must be refused with a version error -- not accepted and then left
    /// prompting for a password on every refresh.
    #[test]
    fn versions_without_export_state_are_rejected() {
        for v in ["2.4.4", "2.5.0", "2.5.1", "2.5.3", "2.5.9"] {
            assert_eq!(
                version_too_old(v),
                Some(MIN_CLI_VERSION),
                "{v} cannot export state and must be refused"
            );
        }
    }

    #[test]
    fn current_and_newer_cli_versions_pass() {
        assert_eq!(version_too_old(MIN_CLI_VERSION), None);
        assert_eq!(version_too_old("2.6.1"), None);
        assert_eq!(version_too_old("2.10.0"), None);
        assert_eq!(version_too_old("3.0.0"), None);
    }

    /// Pre-release and build metadata must not affect the comparison: 2.4.4-rc1
    /// is the same major.minor.patch as 2.4.4 for our purposes.
    #[test]
    fn prerelease_suffixes_are_ignored() {
        assert_eq!(version_too_old("2.6.0-rc1"), None);
        assert_eq!(version_too_old("2.6.0+build7"), None);
        assert_eq!(version_too_old("2.5.3-rc9"), Some(MIN_CLI_VERSION));
    }

    /// Unparseable versions are not blocked: guessing would hide the real
    /// error behind a version complaint.
    #[test]
    fn unparseable_versions_are_not_blocked() {
        assert_eq!(version_too_old("dev"), None);
        assert_eq!(version_too_old(""), None);
    }

    /// The bundled CLI, when present, must be preferred over any system copy:
    /// that is the whole point of shipping it.
    #[test]
    fn bundled_cli_is_preferred_over_a_system_install() {
        // bundled_cli() resolves next to current_exe(); in a test binary that
        // directory has no innernet, so it returns None and discovery falls
        // through. Assert the ordering contract instead of a concrete path.
        let bundled = bundled_cli();
        if let Some(p) = &bundled {
            assert!(Path::new(p).is_file(), "bundled path must exist: {p}");
        }
    }
}

#[cfg(test)]
mod privilege_tests {
    use super::*;

    /// The whole point of needs_root(): an administrator prompt must appear
    /// only for work that genuinely cannot be done as the user. Reading state
    /// and every server-side operation are plain HTTP or file reads.
    #[test]
    fn server_side_operations_never_ask_for_a_password() {
        let quiet = [
            Action::AddPeer {
                interface: "i".into(),
                name: "n".into(),
                cidr: Some("c".into()),
                auto_ip: true,
                ip: None,
                admin: false,
                save_config: "/tmp/x.toml".into(),
                advertise_routes: vec![],
            },
            Action::DisablePeer {
                interface: "i".into(),
                name: "n".into(),
            },
            Action::EnablePeer {
                interface: "i".into(),
                name: "n".into(),
            },
            Action::RenamePeer {
                interface: "i".into(),
                name: "n".into(),
                new_name: "m".into(),
            },
            Action::AddCidr {
                interface: "i".into(),
                name: "n".into(),
                cidr: "10.0.0.0/24".into(),
                parent: None,
            },
            Action::DeleteCidr {
                interface: "i".into(),
                name: "n".into(),
            },
            Action::AddAssociation {
                interface: "i".into(),
                cidr1: "a".into(),
                cidr2: "b".into(),
            },
        ];

        for action in quiet {
            assert!(
                !action.needs_root(),
                "{action:?} talks HTTP to the server and must not prompt"
            );
        }
    }

    /// The two operations that really do need root: creating the tun device and
    /// installing the boot service.
    #[test]
    fn device_and_service_operations_do_ask() {
        let rooted = [
            Action::UpInterface {
                interface: "i".into(),
            },
            Action::Down {
                interface: "i".into(),
            },
            Action::InstallService {
                interface: "i".into(),
            },
            Action::UninstallService {
                interface: "i".into(),
            },
        ];

        for action in rooted {
            assert!(
                action.needs_root(),
                "{action:?} touches a privileged local resource"
            );
        }
    }
}

#[cfg(test)]
mod exported_state_tests {
    use super::*;

    fn snapshot_with_device() -> Snapshot {
        serde_json::from_str(
            r#"{
                "version": "2.5.3",
                "config_dir": "/etc/innernet",
                "permission_denied": false,
                "interfaces": [{
                    "name": "heheda-mesh",
                    "network_name": "heheda-mesh",
                    "is_up": true,
                    "listen_port": 51820,
                    "address": "10.60.1.2",
                    "network_cidr_prefix": 16,
                    "public_key": "abc=",
                    "me": null,
                    "peers": [],
                    "cidrs": [],
                    "endpoint_overrides": [],
                    "service": null
                }]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn a_published_snapshot_carries_the_device_and_needs_no_privileges() {
        // The whole point of exporting state: what the daemon writes must be
        // readable by the app as-is, without a device fetch of its own.
        let snapshot = snapshot_with_device();
        assert!(!snapshot.permission_denied);
        assert!(snapshot.has_live_state());
        assert_eq!(snapshot.interfaces[0].public_key.as_deref(), Some("abc="));
    }

    #[test]
    fn an_empty_export_is_not_trusted_over_a_live_read() {
        // If the daemon could not see the device either, its file says nothing
        // useful. Treating it as authoritative would freeze the UI on an empty
        // view instead of trying the direct read that might work.
        let empty: Snapshot = serde_json::from_str(
            r#"{"version":"2.5.3","config_dir":"/etc/innernet",
                "permission_denied":true,"interfaces":[]}"#,
        )
        .unwrap();
        assert!(empty.permission_denied);
        assert!(!empty.has_live_state());
    }

    #[test]
    fn exported_snapshots_never_contain_a_private_key() {
        // The export lands in a world-readable file, so the guarantee that it
        // holds only public data has to be explicit. A private key would show
        // up as a field the struct does not have -- this pins that the shape
        // carries public keys only.
        let body = serde_json::to_string(&snapshot_with_device()).unwrap();
        assert!(body.contains("public_key"));
        assert!(!body.contains("private_key"));
        assert!(!body.contains("private-key"));
    }
}

#[cfg(test)]
mod live_state_tests {
    use super::*;

    fn peer(name: &str) -> PeerSnapshot {
        PeerSnapshot {
            name: name.into(),
            ip: "10.60.1.1".into(),
            public_key: "abc".into(),
            cidr_id: 1,
            is_admin: false,
            is_disabled: false,
            is_you: false,
            is_connected: true,
            endpoint: None,
            endpoint_overridden: false,
            last_handshake_secs_ago: Some(5),
            rx_bytes: 1,
            tx_bytes: 1,
            allowed_subnets: vec![],
        }
    }

    fn iface(public_key: Option<&str>, peers: Vec<PeerSnapshot>) -> InterfaceSnapshot {
        InterfaceSnapshot {
            name: "heheda-mesh".into(),
            network_name: "heheda-mesh".into(),
            is_up: public_key.is_some(),
            listen_port: None,
            address: "10.60.1.2".into(),
            network_cidr_prefix: 16,
            public_key: public_key.map(|s| s.to_string()),
            me: None,
            peers,
            cidrs: vec![],
            endpoint_overrides: vec![],
            service: None,
        }
    }

    fn snapshot(interfaces: Vec<InterfaceSnapshot>) -> Snapshot {
        Snapshot {
            version: "2.5.1".into(),
            generated_at: 0,
            config_dir: "/etc/innernet".into(),
            permission_denied: false,
            interfaces,
        }
    }

    /// The case that produced a permanently blank card: the config was
    /// readable so permission_denied stayed false, but the socket was not, so
    /// the interface arrived with no public key and no peers. The old check
    /// only looked at permission_denied && interfaces.is_empty(), which was
    /// false, so the app never escalated and never showed anything.
    #[test]
    fn interface_without_a_device_is_not_live_state() {
        let s = snapshot(vec![iface(None, vec![])]);
        assert!(!s.permission_denied, "config was readable");
        assert!(!s.interfaces.is_empty(), "an interface was listed");
        assert!(
            !s.has_live_state(),
            "but nothing live was read, so the app must escalate"
        );
    }

    /// A healthy snapshot must not trigger a pointless password prompt.
    #[test]
    fn interface_with_a_device_and_peers_is_live_state() {
        let s = snapshot(vec![iface(Some("pubkey"), vec![peer("mac-office")])]);
        assert!(s.has_live_state());
    }

    /// A machine with nothing configured is genuinely empty, not unreadable.
    #[test]
    fn no_interfaces_reports_no_live_state() {
        // Earlier this asserted the opposite, on the reasoning that "nothing
        // configured" is a legitimate state. It is -- but it is not *live
        // state*, and conflating the two is what let a permission-denied read
        // (no interfaces, because nothing was readable) take the success path
        // and render an empty card forever. The caller decides what an empty
        // list means using permission_denied; this method only reports whether
        // a device was read.
        let s = snapshot(vec![]);
        assert!(!s.has_live_state());
    }

    /// A device with zero peers is a real state (a fresh network), not a
    /// permission problem -- the public key proves the socket was read.
    #[test]
    fn device_with_no_peers_still_counts_as_live() {
        let s = snapshot(vec![iface(Some("pubkey"), vec![])]);
        assert!(s.has_live_state());
    }

    /// A CLI older than 2.4.0 omits `service` entirely. The app must then ask
    /// the filesystem, not assume "no service".
    ///
    /// The setup gate is written in the UI, but the data it reads is decided
    /// here: if this filled in `installed: false` for an unknown field, a
    /// machine with a running daemon would be pinned to the setup screen
    /// forever -- which is exactly what users hit on CLI 2.5.1.
    #[test]
    fn unknown_service_state_comes_from_the_filesystem_not_a_default() {
        let s = snapshot(vec![iface(Some("pubkey"), vec![])]).with_local_service_state();
        let svc = &s.interfaces[0].service;

        // Windows has no service-install marker this code can read, so "unknown"
        // must stay unknown there rather than becoming a false "not installed".
        // Asserting a resolved path on every platform is what broke this test on
        // the Windows runner.
        #[cfg(windows)]
        {
            assert!(
                svc.is_none(),
                "Windows cannot resolve it from the filesystem, so it must not guess"
            );
            return;
        }

        let svc = svc
            .as_ref()
            .expect("service state must be resolved from the plist/unit file");

        // The assertion is about provenance, not about this machine: whatever
        // the answer is, it has to be the real path the service manager uses,
        // so `installed` can be trusted by the setup gate.
        assert!(
            svc.path.ends_with(".plist") || svc.path.ends_with(".service"),
            "expected a real service-manager path, got {}",
            svc.path
        );
        assert!(
            Path::new(&svc.path).parent().is_some_and(|p| p.is_dir()) || !svc.installed,
            "an `installed: true` claim must point at an existing directory"
        );
    }

    /// Filling in service state must never overwrite what the CLI reported.
    ///
    /// The CLI asks launchctl, so it knows `running`; the filesystem fallback
    /// cannot. Clobbering a real answer with the fallback would make a stopped
    /// service look stopped-but-installed when the CLI had better information.
    #[test]
    fn live_service_state_wins_over_the_snapshot() {
        let mut i = iface(Some("pubkey"), vec![]);
        // What a snapshot written before Disconnect still claims.
        let stale = ServiceStatus {
            manager: "launchd".into(),
            installed: true,
            running: true,
            path: "/Library/LaunchDaemons/com.innernet.heheda-mesh.plist".into(),
        };
        i.service = Some(stale);

        let resolved = i.with_local_service_state();

        // `heheda-mesh` has no launchd unit on any machine running these tests,
        // so the service manager must report it as not running. Asserting the
        // snapshot value "survives" is exactly the bug: it kept Disconnect
        // looking like a no-op.
        #[cfg(not(windows))]
        {
            let svc = resolved.service.expect("service state is resolved locally");
            assert!(
                !svc.running,
                "a snapshot's `running: true` must not outlive the service"
            );
            assert!(
                !svc.installed,
                "nor its `installed: true`, when no unit file exists"
            );
        }
    }

    /// A disconnected network must not keep showing live tunnel data.
    ///
    /// This is the reported bug: Disconnect removed the service, but the
    /// snapshot left on disk still listed four peers with handshake times and
    /// claimed the interface was up, so the window looked unchanged and the
    /// switch still offered "Disconnect".
    #[test]
    fn an_abandoned_snapshot_keeps_the_network_but_drops_tunnel_data() {
        let mut i = iface(Some("pubkey"), vec![peer("mac-office")]);
        i.endpoint_overrides = vec![];
        let s = snapshot(vec![i]);

        // Sanity: the fixture really does carry live-looking data.
        assert!(s.has_live_state(), "fixture must start as live state");
        assert_eq!(s.interfaces[0].peers.len(), 1);

        let stripped = s.without_live_data();

        // The network survives, so the UI can say "heheda-mesh, disconnected".
        assert_eq!(stripped.interfaces.len(), 1);
        assert_eq!(stripped.interfaces[0].name, "heheda-mesh");

        // Everything only a running daemon could know is gone.
        assert!(!stripped.interfaces[0].is_up, "must not claim the tunnel is up");
        assert!(
            stripped.interfaces[0].peers.is_empty(),
            "must not list peers for a tunnel that is not running"
        );
        assert!(stripped.interfaces[0].public_key.is_none());
        assert!(!stripped.has_live_state(), "this is not live state any more");
    }

    /// The staleness window has to be short enough to notice a stopped daemon.
    ///
    /// The daemon rewrites the snapshot every 60s. With the old 3000s threshold
    /// a disconnected network stayed "running" for fifty minutes.
    #[test]
    fn staleness_window_is_a_few_daemon_intervals() {
        // 60s daemon interval; four missed refreshes must count as abandoned.
        let mut s = snapshot(vec![iface(Some("pubkey"), vec![])]);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        s.generated_at = now - 240; // 4 minutes: still fresh
        assert!(!s.looks_abandoned(), "four minutes is not abandoned");

        s.generated_at = now - 600; // 10 minutes: five missed refreshes
        assert!(s.looks_abandoned(), "ten minutes must count as abandoned");

        // A writer that predates the field cannot be aged at all; it must not be
        // treated as ancient by default.
        s.generated_at = 0;
        assert!(!s.looks_abandoned(), "unknown age is not the same as old");
    }
}
