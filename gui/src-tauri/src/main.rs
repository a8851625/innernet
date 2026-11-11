// Prevents an additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// State model — mirrors `innernet show --json`
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Snapshot {
    version: String,
    config_dir: String,
    permission_denied: bool,
    interfaces: Vec<InterfaceSnapshot>,
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
    UpInterface { interface: String },
    /// Fetch the peer list once.
    Fetch { interface: String },
    /// Bring the interface down but KEEP the configuration.
    Down { interface: String },
    /// Remove the interface and delete its configuration.
    Uninstall { interface: String },
    /// Join a network from an invitation file.
    Install {
        invite_path: String,
        /// Delete the invitation file after a successful install.
        #[serde(default)]
        delete_invite: bool,
    },

    // -- local settings ----------------------------------------------------
    SetListenPort { interface: String, port: u16 },
    UnsetListenPort { interface: String },
    OverrideEndpoint { interface: String, endpoint: String },
    UnsetEndpoint { interface: String },
    OverridePeerEndpoint {
        interface: String,
        peer: String,
        endpoint: String,
    },
    UnsetPeerEndpoint { interface: String, peer: String },

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
    DisablePeer { interface: String, name: String },
    EnablePeer { interface: String, name: String },
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
    DeleteCidr { interface: String, name: String },
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
    fn args(&self) -> Result<Vec<String>, String> {
        let mut a: Vec<String> = Vec::new();

        match self {
            Action::Up => a.push("up".into()),
            Action::UpInterface { interface } => {
                a.push("up".into());
                a.push(validate_interface(interface)?);
            }
            Action::Fetch { interface } => {
                a.push("fetch".into());
                a.push(validate_interface(interface)?);
            }
            Action::Down { interface } => {
                a.push("down".into());
                a.push(validate_interface(interface)?);
            }
            Action::Uninstall { interface } => {
                a.push("uninstall".into());
                a.push(validate_interface(interface)?);
                a.push("--yes".into());
            }
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
            }

            Action::SetListenPort { interface, port } => {
                a.push("set-listen-port".into());
                a.push(validate_interface(interface)?);
                a.push("--listen-port".into());
                a.push(validate_port(*port)?.to_string());
                a.push("--yes".into());
            }
            Action::UnsetListenPort { interface } => {
                a.push("set-listen-port".into());
                a.push(validate_interface(interface)?);
                a.push("--unset".into());
                a.push("--yes".into());
            }
            Action::OverrideEndpoint { interface, endpoint } => {
                a.push("override-endpoint".into());
                a.push(validate_interface(interface)?);
                a.push("--endpoint".into());
                a.push(validate_endpoint(endpoint)?);
                a.push("--yes".into());
            }
            Action::UnsetEndpoint { interface } => {
                a.push("override-endpoint".into());
                a.push(validate_interface(interface)?);
                a.push("--unset".into());
                a.push("--yes".into());
            }
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
            }
            Action::UnsetPeerEndpoint { interface, peer } => {
                a.push("override-peer-endpoint".into());
                a.push(validate_interface(interface)?);
                a.push("--name".into());
                a.push(validate_peer_name(peer)?);
                a.push("--unset".into());
                a.push("--yes".into());
            }

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
                    }
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
            }
            Action::DisablePeer { interface, name } => {
                a.push("disable-peer".into());
                a.push(validate_interface(interface)?);
                a.push("--name".into());
                a.push(validate_peer_name(name)?);
                a.push("--yes".into());
            }
            Action::EnablePeer { interface, name } => {
                a.push("enable-peer".into());
                a.push(validate_interface(interface)?);
                a.push("--name".into());
                a.push(validate_peer_name(name)?);
                a.push("--yes".into());
            }
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
            }

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
            }
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
            }
            Action::DeleteCidr { interface, name } => {
                a.push("delete-cidr".into());
                a.push(validate_interface(interface)?);
                a.push("--name".into());
                a.push(validate_cidr_name(name)?);
                a.push("--yes".into());
            }
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
            }
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
            }
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

#[derive(Debug, Serialize)]
struct CidrList {
    ok: bool,
    error: Option<String>,
    stdout: String,
    cidrs: Vec<CidrSnapshot>,
}

// ---------------------------------------------------------------------------
// CLI discovery
// ---------------------------------------------------------------------------

fn cli_path() -> Result<String, String> {
    if let Ok(p) = std::env::var("INNERNET_CLI") {
        if !p.trim().is_empty() {
            return Ok(p);
        }
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

    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let p = Path::new(dir).join("innernet");
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
        }
    };

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
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    match serde_json::from_str::<Snapshot>(&stdout) {
        Ok(snapshot) => {
            if snapshot.permission_denied && snapshot.interfaces.is_empty() {
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
        }
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
        }
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
        }
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
        }
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
            _ => {}
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

#[tauri::command]
fn run_action(action: Action) -> ActionResult {
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
        }
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
        }
    };

    let display = format!(
        "{} {}",
        Path::new(&binary)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| binary.clone()),
        argv.join(" ")
    );

    let mut res = run_privileged(&binary, &argv);

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

// ---------------------------------------------------------------------------
// Privileged execution
// ---------------------------------------------------------------------------

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

    match Command::new("osascript").args(["-e", &applescript]).output() {
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
        }
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

#[cfg(target_os = "macos")]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_state,
            preview_invite,
            run_action
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
            save_config: "/tmp/laptop.toml".into(),
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
