//! Install and remove the OS service that keeps an interface up.
//!
//! `innernet up --daemon` refreshes peers forever, but nothing starts it at
//! boot. Upstream only prints a suggestion to "set up systemd / a launchd
//! script yourself", and the systemd unit it ships hardcodes `/usr/bin`, so it
//! breaks for Homebrew or any custom prefix. This module turns that advice into
//! a command: it writes a unit whose ExecStart points at the binary that is
//! actually running, and enables it.

use anyhow::{bail, Context, Error};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Interface;

/// Refresh interval baked into the generated unit.
///
/// NAT traversal needs repeated attempts: a single `up` gives each candidate
/// endpoint a few seconds and then gives up, which is not long enough when a
/// peer sits behind a symmetric NAT whose mapping changes. A daemon loop keeps
/// re-attempting, so the first successful hole punch is eventually found and
/// then kept alive by the traffic itself.
const INTERVAL_SECS: u64 = 60;

const SYSTEMD_DIR: &str = "/etc/systemd/system";
const LAUNCHD_DIR: &str = "/Library/LaunchDaemons";

/// Path of the binary that is currently executing, so the generated unit runs
/// the same install the user invoked rather than a hardcoded /usr/bin path.
fn current_exe() -> Result<PathBuf, Error> {
    std::env::current_exe().context("could not determine the path of this executable")
}

/// Fail early with an accurate message when not root.
///
/// Writing to /etc/systemd/system or /Library/LaunchDaemons needs root. Without
/// this check the write error propagates as a generic PermissionDenied, which
/// the top-level handler reports as "can't access config/data folders" -- true
/// for most commands, misleading here.
fn require_root() -> Result<(), Error> {
    // Probed by writing, rather than calling geteuid, to avoid taking a libc
    // dependency in this crate for a single check. The probe directory is the
    // one we are about to write to anyway, so a false pass is impossible.
    let probe_dir = if cfg!(target_os = "macos") {
        LAUNCHD_DIR
    } else {
        SYSTEMD_DIR
    };
    if let Err(e) = fs::metadata(probe_dir) {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            bail!("this command must run as root. Re-run it with sudo.");
        }
    }
    // metadata() on the directory itself usually succeeds even for non-root,
    // because the parent is world-readable; the real test is write access.
    let probe = Path::new(probe_dir).join(".innernet-write-probe");
    match fs::File::create(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            Ok(())
        },
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            bail!("this command must run as root. Re-run it with sudo.")
        },
        Err(e) => Err(e).with_context(|| format!("could not write to {probe_dir}")),
    }
}

pub fn install(interface: &Interface) -> Result<(), Error> {
    require_root()?;
    let exe = current_exe()?;
    let exe = exe.to_str().context("executable path is not valid UTF-8")?;
    let name = interface.as_str_lossy().to_string();

    if cfg!(target_os = "linux") {
        install_systemd(&name, exe)
    } else if cfg!(target_os = "macos") {
        install_launchd(&name, exe)
    } else {
        bail!(
            "installing a service is only supported on Linux and macOS.\n\
             On other systems run this in the background yourself:\n\n    \
             {exe} up -d --interval {INTERVAL_SECS} {name}"
        )
    }
}

pub fn uninstall(interface: &Interface) -> Result<(), Error> {
    require_root()?;
    let name = interface.as_str_lossy().to_string();
    if cfg!(target_os = "linux") {
        uninstall_systemd(&name)
    } else if cfg!(target_os = "macos") {
        uninstall_launchd(&name)
    } else {
        bail!("installing a service is only supported on Linux and macOS")
    }
}

// ---------------------------------------------------------------------------
// systemd
// ---------------------------------------------------------------------------

fn systemd_unit_path(name: &str) -> PathBuf {
    Path::new(SYSTEMD_DIR).join(format!("innernet@{name}.service"))
}

fn systemd_unit(name: &str, exe: &str) -> String {
    format!(
        "\
[Unit]
Description=innernet client daemon for {name}
After=network-online.target nss-lookup.target
Wants=network-online.target nss-lookup.target

[Service]
Type=simple
ExecStart={exe} up {name} --daemon --interval {INTERVAL_SECS}
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
"
    )
}

fn install_systemd(name: &str, exe: &str) -> Result<(), Error> {
    let path = systemd_unit_path(name);
    fs::write(&path, systemd_unit(name, exe))
        .with_context(|| format!("could not write {}", path.display()))?;

    run("systemctl", &["daemon-reload"])?;
    run(
        "systemctl",
        &["enable", "--now", &format!("innernet@{name}.service")],
    )?;

    println!(
        "\ninstalled {} \n  started innernet@{name}.service\n\n\
         check it with:\n    systemctl status innernet@{name}.service\n    journalctl -u innernet@{name}.service -f",
        path.display()
    );
    Ok(())
}

fn uninstall_systemd(name: &str) -> Result<(), Error> {
    // Disabling a unit that was never installed is not an error worth stopping
    // for: the point is to end up in the "not installed" state either way.
    let _ = run_quiet(
        "systemctl",
        &["disable", "--now", &format!("innernet@{name}.service")],
    );

    let path = systemd_unit_path(name);
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("could not remove {}", path.display()))?;
        println!("removed {}", path.display());
    } else {
        println!("no service file at {}", path.display());
    }
    let _ = run_quiet("systemctl", &["daemon-reload"]);

    // Same reason as the launchd path: the snapshot describes a daemon that is
    // no longer running, and leaving it makes a disconnected network still look
    // connected to anything that reads it.
    let stale = "/var/run/innernet/state.json";
    if fs::remove_file(stale).is_ok() {
        println!("removed {stale}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// launchd
// ---------------------------------------------------------------------------

fn launchd_plist_path(name: &str) -> PathBuf {
    Path::new(LAUNCHD_DIR).join(format!("com.innernet.{name}.plist"))
}

fn launchd_label(name: &str) -> String {
    format!("com.innernet.{name}")
}

fn launchd_plist(name: &str, exe: &str) -> String {
    let label = launchd_label(name);
    // WireGuard needs root to create the utun device, so this goes in
    // /Library/LaunchDaemons (system domain), not ~/Library/LaunchAgents.
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>

    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>up</string>
        <string>{name}</string>
        <string>--daemon</string>
        <string>--interval</string>
        <string>{INTERVAL_SECS}</string>
    </array>

    <key>RunAtLoad</key>
    <true/>

    <!-- Restart if it exits, so a transient failure does not leave the
         interface down until the next reboot. -->
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>10</integer>

    <key>StandardOutPath</key>
    <string>/var/log/innernet-{name}.log</string>
    <key>StandardErrorPath</key>
    <string>/var/log/innernet-{name}.log</string>
</dict>
</plist>
"#
    )
}

fn install_launchd(name: &str, exe: &str) -> Result<(), Error> {
    let path = launchd_plist_path(name);
    fs::write(&path, launchd_plist(name, exe))
        .with_context(|| format!("could not write {}", path.display()))?;

    let label = launchd_label(name);
    // bootout first: loading a label that is already loaded fails, and we want
    // `install-service` to be re-runnable (e.g. after moving the binary).
    let _ = run_quiet("launchctl", &["bootout", &format!("system/{label}")]);
    run(
        "launchctl",
        &["bootstrap", "system", path.to_str().unwrap()],
    )?;

    println!(
        "\ninstalled {}\n  loaded {label}\n\n\
         check it with:\n    sudo launchctl print system/{label}\n    tail -f /var/log/innernet-{name}.log",
        path.display()
    );
    Ok(())
}

fn uninstall_launchd(name: &str) -> Result<(), Error> {
    let label = launchd_label(name);
    let _ = run_quiet("launchctl", &["bootout", &format!("system/{label}")]);

    let path = launchd_plist_path(name);
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("could not remove {}", path.display()))?;
        println!("removed {}", path.display());
    } else {
        println!("no service file at {}", path.display());
    }

    // The snapshot was written by the daemon that just stopped. Leaving it
    // behind is what made Disconnect look like a no-op: the file still says the
    // interface is up with peers connected, and a GUI reading it has no way to
    // know the writer is gone. Removing it makes "disconnected" observable
    // immediately rather than after the staleness window expires.
    //
    // Best effort: a missing file is the normal case on a machine that never
    // started the tunnel, and failing here would turn a successful disconnect
    // into an error.
    let stale = "/var/run/innernet/state.json";
    if fs::remove_file(stale).is_ok() {
        println!("removed {stale}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// status
// ---------------------------------------------------------------------------

/// What the OS says about the boot service for one interface.
///
/// A GUI needs to tell three states apart, because they call for different
/// actions: nothing installed (offer to install), installed but not running
/// (offer to start or reinstall), and running (offer to remove). Reporting only
/// a boolean would collapse the middle case into one of the other two and make
/// the UI offer the wrong thing.
///
/// This is deliberately readable without root. `show --json` runs as a normal
/// user, so it can only stat the unit file and ask the supervisor; anything
/// that would need root reports the conservative answer instead of failing.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct ServiceStatus {
    /// `"launchd"` or `"systemd"`.
    pub manager: &'static str,
    /// The unit/plist exists on disk.
    pub installed: bool,
    /// The supervisor reports it as running. Only meaningful when installed.
    pub running: bool,
    /// Unit file path, for troubleshooting.
    pub path: String,
}

/// Report the boot service state for `interface`.
#[cfg(target_os = "macos")]
pub fn status(interface: &str) -> ServiceStatus {
    let path = launchd_plist_path(interface);
    let installed = path.exists();
    let running = installed && launchd_is_running(&launchd_label(interface));
    ServiceStatus {
        manager: "launchd",
        installed,
        running,
        path: path.display().to_string(),
    }
}

/// Report the boot service state for `interface`.
#[cfg(not(target_os = "macos"))]
pub fn status(interface: &str) -> ServiceStatus {
    let path = systemd_unit_path(interface);
    let installed = path.exists();
    let running = installed && systemd_is_running(interface);
    ServiceStatus {
        manager: "systemd",
        installed,
        running,
        path: path.display().to_string(),
    }
}

/// Ask launchd whether the service is loaded and running.
///
/// `launchctl print` exits non-zero when the service is not loaded, so a
/// failure means "not running" rather than an error worth surfacing.
#[cfg(target_os = "macos")]
fn launchd_is_running(label: &str) -> bool {
    let Ok(output) = Command::new("launchctl")
        .args(["print", &format!("system/{label}")])
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    // "state = running" appears for a loaded job; a loaded-but-stopped job says
    // "state = not running" or "state = waiting".
    String::from_utf8_lossy(&output.stdout).contains("state = running")
}

/// Ask systemd whether the unit is active.
#[cfg(not(target_os = "macos"))]
fn systemd_is_running(name: &str) -> bool {
    let Ok(output) = Command::new("systemctl")
        .args(["is-active", &format!("innernet@{name}.service")])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout).trim() == "active"
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn run(program: &str, args: &[&str]) -> Result<(), Error> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("could not run {program}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let hint = if stderr.contains("Permission denied")
            || stderr.contains("Operation not permitted")
            || stderr.contains("Must be run as root")
        {
            "\n\nThis needs root. Re-run it with sudo."
        } else {
            ""
        };
        bail!(
            "{} {} failed ({}): {}{hint}",
            program,
            args.join(" "),
            output.status,
            stderr.trim()
        );
    }
    Ok(())
}

fn run_quiet(program: &str, args: &[&str]) -> Result<(), Error> {
    let output = Command::new(program).args(args).output()?;
    if output.status.success() {
        Ok(())
    } else {
        bail!("{program} {}", args.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemd_unit_points_at_the_running_binary() {
        let unit = systemd_unit("wg0", "/opt/homebrew/bin/innernet");
        // The whole reason this module exists: upstream's template hardcodes
        // /usr/bin, which does not exist for a Homebrew install.
        assert!(unit.contains("ExecStart=/opt/homebrew/bin/innernet up wg0 --daemon"));
        assert!(!unit.contains("/usr/bin/innernet"));
    }

    #[test]
    fn systemd_unit_restarts_and_is_enabled() {
        let unit = systemd_unit("wg0", "/usr/local/bin/innernet");
        assert!(unit.contains("Restart=always"));
        assert!(unit.contains("WantedBy=multi-user.target"));
    }

    #[test]
    fn launchd_plist_is_valid_and_uses_the_binary() {
        let plist = launchd_plist("wg0", "/opt/homebrew/bin/innernet");
        assert!(plist.contains("<string>com.innernet.wg0</string>"));
        assert!(plist.contains("<string>/opt/homebrew/bin/innernet</string>"));
        assert!(plist.contains("<string>--daemon</string>"));
        // Runs at boot and stays up.
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
    }

    #[test]
    fn launchd_plist_escapes_into_a_parsable_document() {
        // Cheap structural check: the doctype and the closing tag bracket the
        // single dict, so a truncated template would be caught here.
        let plist = launchd_plist("wg0", "/bin/innernet");
        assert!(plist.starts_with("<?xml"));
        assert!(plist.trim_end().ends_with("</plist>"));
        assert_eq!(plist.matches("<dict>").count(), 1);
        assert_eq!(plist.matches("</dict>").count(), 1);
    }

    #[test]
    fn service_paths_live_in_the_system_domain() {
        // WireGuard needs root, so an agent in ~/Library/LaunchAgents would not
        // work. Both service types must be system-wide.
        assert_eq!(
            systemd_unit_path("wg0"),
            PathBuf::from("/etc/systemd/system/innernet@wg0.service")
        );
        assert_eq!(
            launchd_plist_path("wg0"),
            PathBuf::from("/Library/LaunchDaemons/com.innernet.wg0.plist")
        );
    }

    #[test]
    fn interval_matches_the_documented_value() {
        assert_eq!(INTERVAL_SECS, 60);
        assert!(systemd_unit("wg0", "/x").contains("--interval 60"));
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;

    #[test]
    fn status_reports_the_platform_manager() {
        let s = status("nonexistent-interface-for-test");
        #[cfg(target_os = "macos")]
        assert_eq!(s.manager, "launchd");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(s.manager, "systemd");

        // A name nobody installed must not claim to be installed or running:
        // that would make a GUI offer to remove something that does not exist.
        assert!(!s.installed);
        assert!(!s.running);
    }

    #[test]
    fn status_paths_are_absolute_and_named_after_the_interface() {
        let s = status("heheda-mesh");
        assert!(
            s.path.starts_with('/'),
            "path should be absolute: {}",
            s.path
        );
        assert!(
            s.path.contains("heheda-mesh"),
            "path should name the interface: {}",
            s.path
        );
    }

    #[test]
    fn running_implies_installed() {
        // The struct is built so `running` is only ever set when the unit file
        // exists. Guard the invariant: a running service with no unit file is a
        // state a GUI cannot act on.
        for name in ["heheda-mesh", "nonexistent-interface-for-test"] {
            let s = status(name);
            assert!(
                !s.running || s.installed,
                "running without installed is inconsistent: {s:?}"
            );
        }
    }

    /// Uninstalling must remove the snapshot the daemon published.
    ///
    /// The snapshot is written by the daemon, so once the service is stopped the
    /// file is a frozen claim that the tunnel is up with peers connected. A GUI
    /// reading it cannot tell the writer is gone, which is why Disconnect looked
    /// like it did nothing: peers stayed listed and the switch still offered
    /// "Disconnect".
    ///
    /// Checks the shipped source, because the removal itself needs root and a
    /// real unit file to exercise. The code must at least name every path it
    /// intends to delete.
    #[test]
    fn uninstall_removes_the_published_snapshot() {
        let src = include_str!("service.rs");

        for func in ["fn uninstall_launchd", "fn uninstall_systemd"] {
            let start = src.find(func).unwrap_or_else(|| panic!("{func} missing"));
            let end = src[start..]
                .find("\nfn ")
                .map(|i| start + i)
                .unwrap_or(src.len());
            let body = &src[start..end];

            assert!(
                body.contains("/var/run/innernet/state.json"),
                "{func} must delete the stale snapshot, or a disconnected network keeps looking connected"
            );
        }
    }
}
