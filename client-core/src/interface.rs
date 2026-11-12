pub use innernet_shared::interface_config::InterfaceConfig;
pub use wireguard_control::InterfaceName;

use crate::{
    data_store::DataStore,
    nat::{self, NatTraverse},
    rest_client::{RestClient, RestError},
    HostsOpts, NatOpts, NetworkOpts, WrappedIoError,
};
use anyhow::{bail, Context as _, Error};
use colored::{ColoredString, Colorize};
use innernet_shared::{
    get_local_addrs, update_hosts_file,
    wg::{self, DeviceExt as _},
    Endpoint, PeerChange, PeerDiff, RedeemContents, State, REDEEM_TRANSITION_WAIT,
};
use std::{
    io,
    net::{IpAddr, SocketAddr},
    path::Path,
    thread,
    time::{Duration, Instant},
};
use thiserror::Error;
use wireguard_control::{Backend, Device, DeviceUpdate, PeerConfigBuilder};

#[derive(Debug, Error)]
pub enum RedeemInviteError {
    #[error("Error accessing innernet interface config file: {0}")]
    InterfaceConfigAccess(WrappedIoError),
    #[error("Config file for innernet interface {0} already exists.")]
    InterfaceConfigExists(InterfaceName),
    #[error("Error making a REST request: {0}")]
    RestRequest(#[from] RestError),
    #[error("Could not resolve server address for endpoint {endpoint}: {error}")]
    ServerAddressResolve {
        endpoint: Endpoint,
        error: io::Error,
    },
    #[error("WireGuard interface {0} already exists.")]
    WireguardInterfaceExists(InterfaceName),
    #[error("Error managing the Wireguard interface {interface}: {error}")]
    WireguardOperation {
        interface: InterfaceName,
        error: io::Error,
    },
}

/// Redeem an invitation to join an innernet network.
///
/// - Brings the WireGuard interface up.
/// - Generates a fresh key pair and uses the public part to redeem the invite and the private part
///   to update the WireGuard interface.
/// - Bails if either a WireGuard or an innernet `interface` of the same name already exists and
///   brings the interface down if it was already up.
pub fn redeem_invite(
    config_dir: &Path,
    network_opts: &NetworkOpts,
    interface: &InterfaceName,
    config: InterfaceConfig,
) -> Result<(), RedeemInviteError> {
    let config_path = InterfaceConfig::build_config_file_path(config_dir, interface)
        .map_err(RedeemInviteError::InterfaceConfigAccess)?;

    if config_path.exists() {
        return Err(RedeemInviteError::InterfaceConfigExists(*interface));
    }

    if Device::list(network_opts.backend)
        .iter()
        .flatten()
        .any(|name| name == interface)
    {
        return Err(RedeemInviteError::WireguardInterfaceExists(*interface));
    }

    log::info!(
        "bringing up interface {}.",
        interface.as_str_lossy().yellow()
    );

    let endpoint = &config.server.external_endpoint;
    let resolved_endpoint =
        endpoint
            .resolve()
            .map_err(|e| RedeemInviteError::ServerAddressResolve {
                endpoint: endpoint.clone(),
                error: e,
            })?;

    wg::up(
        interface,
        &config.interface.private_key,
        config.interface.address,
        config.interface.listen_port,
        Some((
            &config.server.public_key,
            config.server.internal_endpoint.ip(),
            resolved_endpoint,
        )),
        network_opts,
    )
    .map_err(|e| RedeemInviteError::WireguardOperation {
        interface: *interface,
        error: e,
    })?;

    // The server's coordination API listens on the WireGuard-internal address,
    // so the handshake has to complete before the first HTTP request can reach
    // it. Without this wait the redeem POST races the handshake and loses:
    // on a 200ms-RTT link the SYN goes out microseconds after the handshake
    // initiation and the request times out before the tunnel exists, which
    // looks like "is the innernet server reachable?" even though it is.
    wait_for_handshake(interface, network_opts.backend);

    update_keypair(network_opts, interface, &config_path, config).inspect_err(|e| {
        log::error!("failed to update keypair (is the innernet server reachable?): {e}.",);
        log::info!("bringing down the interface.");
        if let Err(e) = wg::down(interface, network_opts.backend) {
            log::warn!("failed to bring down interface: {}.", e);
        };
    })?;

    Ok(())
}

/// Wait until the WireGuard interface has completed a handshake, up to a few
/// seconds.
///
/// This is best-effort on purpose: a failure here is not fatal, because the
/// redeem request that follows will produce a clearer error if the tunnel
/// really is unusable.
fn wait_for_handshake(interface: &InterfaceName, backend: wireguard_control::Backend) {
    const TIMEOUT: Duration = Duration::from_secs(10);
    const INTERVAL: Duration = Duration::from_millis(100);

    let started = std::time::Instant::now();
    loop {
        match Device::get(interface, backend) {
            Ok(device) => {
                let handshaked = device
                    .peers
                    .iter()
                    .any(|p| p.stats.last_handshake_time.is_some());
                if handshaked {
                    log::debug!("handshake completed in {:?}.", started.elapsed());
                    return;
                }
            },
            Err(e) => {
                log::debug!("could not read interface while waiting for handshake: {e}.");
                return;
            },
        }

        if started.elapsed() >= TIMEOUT {
            log::warn!(
                "no handshake after {:?}; attempting the request anyway.",
                TIMEOUT
            );
            return;
        }
        std::thread::sleep(INTERVAL);
    }
}

fn update_keypair(
    network_opts: &NetworkOpts,
    interface: &InterfaceName,
    config_path: &Path,
    mut config: InterfaceConfig,
) -> Result<(), RedeemInviteError> {
    log::info!("Generating new keypair.");
    let keypair = wireguard_control::KeyPair::generate();

    log::info!(
        "Registering keypair with server (at {}).",
        config.server.internal_endpoint
    );
    RestClient::new(&config.server).http_form::<_, ()>(
        "POST",
        "/user/redeem",
        RedeemContents {
            public_key: keypair.public.to_base64(),
        },
    )?;

    config.interface.private_key = keypair.private.to_base64();
    config
        .save_new(config_path, 0o600)
        .map_err(RedeemInviteError::InterfaceConfigAccess)?;
    log::info!(
        "New keypair registered. Copied config to {}.\n",
        config_path.to_string_lossy().yellow()
    );

    log::info!("Changing keys and waiting 5s for server's WireGuard interface to transition.",);
    DeviceUpdate::new()
        .set_private_key(keypair.private)
        .apply(interface, network_opts.backend)
        .map_err(|e| RedeemInviteError::WireguardOperation {
            interface: *interface,
            error: e,
        })?;
    thread::sleep(REDEEM_TRANSITION_WAIT);

    Ok(())
}

pub fn fetch(
    config_dir: &Path,
    data_dir: &Path,
    network_opts: &NetworkOpts,
    hosts_opts: &HostsOpts,
    nat: &NatOpts,
    interface: &InterfaceName,
    bring_up_interface: bool,
) -> Result<(), Error> {
    let config = InterfaceConfig::from_interface(config_dir, interface)?;
    let interface_up = interface_is_up(network_opts.backend, interface);

    if !interface_up {
        if !bring_up_interface {
            bail!(
                "Interface is not up. Use 'innernet up {}' instead",
                interface
            );
        }

        log::info!(
            "bringing up interface {}.",
            interface.as_str_lossy().yellow()
        );
        let resolved_endpoint = config
            .server
            .external_endpoint
            .resolve()
            .context(config.server.external_endpoint.to_string())?;
        wg::up(
            interface,
            &config.interface.private_key,
            config.interface.address,
            config.interface.listen_port,
            Some((
                &config.server.public_key,
                config.server.internal_endpoint.ip(),
                resolved_endpoint,
            )),
            network_opts,
        )
        .context(interface.to_string())?;
    }

    log::info!(
        "fetching state for {} from server...",
        interface.as_str_lossy().yellow()
    );
    let mut store = DataStore::open_or_create(data_dir, interface)?;
    let rest_client = RestClient::new(&config.server);
    let (State { mut peers, cidrs }, server_is_reachable) = match rest_client
        .http("GET", "/user/state")
    {
        Ok(state) => (state, true),
        Err(e) => {
            if e.is_transport_error() {
                if store.peers().is_empty() {
                    bail!(
                        "Could not connect to the innernet server and there are no cached peers, \
                     exiting."
                    )
                }

                log::warn!(
                    "Could not connect to the innernet server, proceeding with cached state instead."
                );

                let state = State {
                    peers: store.peers().to_vec(),
                    cidrs: store.cidrs().to_vec(),
                };
                (state, false)
            } else {
                bail!(e)
            }
        },
    };

    // Apply the local peer endpoint overrides.
    for (peer_ip, endpoint_override) in config.peer_endpoint_overrides() {
        log::debug!(
            "overriding peer IP {} with endpoint {}",
            peer_ip,
            endpoint_override
        );

        if let Some(peer) = peers.iter_mut().find(|p| p.ip == *peer_ip) {
            peer.endpoint = Some(endpoint_override.clone());
        }
    }

    let device = Device::get(interface, network_opts.backend)?;
    let modifications = device.diff(&peers);

    let updates = modifications
        .iter()
        .inspect(|diff| print_peer_diff(&store, diff))
        .cloned()
        .map(PeerConfigBuilder::from)
        .collect::<Vec<_>>();

    if !updates.is_empty() || !interface_up {
        DeviceUpdate::new()
            .add_peers(&updates)
            .apply(interface, network_opts.backend)
            .context(interface.to_string())?;

        if !hosts_opts.no_write_hosts {
            update_hosts_file(interface, hosts_opts, &peers)?;
        }

        println!();
        log::info!("updated interface {}\n", interface.as_str_lossy().yellow());
    } else {
        log::info!("{}", "peers are already up to date".green());
    }
    let interface_updated_time = Instant::now();

    store
        .update_peers_and_set_cidrs(&peers, cidrs)
        .context(interface.to_string())?;

    let listen_port = device.listen_port.unwrap_or(51820);
    if server_is_reachable {
        report_candidates(&rest_client, nat, listen_port)?;
    }

    if nat.no_nat_traversal {
        log::debug!("NAT traversal explicitly disabled, not attempting.");
    } else {
        let mut nat_traverse =
            NatTraverse::new(interface, &config, network_opts.backend, &modifications)?;

        // Give time for handshakes with recently changed endpoints to complete before attempting traversal.
        if !nat_traverse.is_finished() {
            thread::sleep(nat::STEP_INTERVAL - interface_updated_time.elapsed());
        }
        loop {
            if nat_traverse.is_finished() {
                break;
            }
            log::info!(
                "Attempting to establish connection with {} remaining unconnected peers...",
                nat_traverse.remaining()
            );
            nat_traverse.step()?;
        }
    }

    Ok(())
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum ChangeAction {
    Added,
    Modified,
    Removed,
}

impl ChangeAction {
    fn colored_output(&self) -> ColoredString {
        match self {
            Self::Added => "added".green(),
            Self::Modified => "modified".yellow(),
            Self::Removed => "removed".red(),
        }
    }
}

fn print_peer_diff(store: &DataStore, diff: &PeerDiff) {
    let public_key = diff.public_key().to_base64();

    let change_action = match (diff.old, diff.new) {
        (None, Some(_)) => ChangeAction::Added,
        (Some(_), Some(_)) => ChangeAction::Modified,
        (Some(_), None) => ChangeAction::Removed,
        _ => unreachable!("PeerDiff can't be None -> None"),
    };

    // Grab the peer name from either the new data, or the historical data (if the peer is removed).
    let peer_hostname = match diff.new {
        Some(peer) => Some(peer.name.clone()),
        None => store
            .peers()
            .iter()
            .find(|p| p.public_key == public_key)
            .map(|p| p.name.clone()),
    };
    let peer_name = peer_hostname.as_deref().unwrap_or("[unknown]");

    if change_action == ChangeAction::Modified
        && diff
            .changes()
            .iter()
            .all(|c| *c == PeerChange::NatTraverseReattempt)
    {
        // If this peer was "modified" but the only change is a NAT Traversal Reattempt,
        // don't bother printing this peer.
        return;
    }

    log::info!(
        "  peer {} ({}...) was {}.",
        peer_name.yellow(),
        public_key[..10].dimmed(),
        change_action.colored_output(),
    );

    for change in diff.changes() {
        if let PeerChange::Endpoint { .. } = change {
            log::info!("    {}", change);
        } else {
            log::debug!("    {}", change);
        }
    }
}

/// Whether an address is plausible as a public, globally routable address.
///
/// The probe answers over DNS, so anything that intercepts DNS can answer for
/// it. Fake-IP resolvers (common in transparent proxies) hand back an address
/// from their own pool, which then looks like a public address but is
/// unroutable for every peer. Those pools live in the ranges below.
///
/// This is a sanity check, not a security boundary: it rejects the documented
/// non-routable ranges and the fake-IP pools in use today. A resolver using a
/// different pool would still slip through, which is why --public-endpoint
/// exists.
fn is_plausible_public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_documentation()
                // RFC 6598 carrier-grade NAT
                || (o[0] == 100 && (64..128).contains(&o[1]))
                // Clash / mihomo fake-IP pool (198.18.0.0/15, RFC 2544)
                || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
                // Clash Verge / Karing alternates
                || (o[0] == 198 && o[1] == 19)
                || o[0] == 0
                || o[0] >= 240)
        },
        IpAddr::V6(v6) => {
            !(v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local
                || (v6.segments()[0] & 0xfe00) == 0xfc00) // unique-local
        },
    }
}

/// Assemble the candidate list reported to the coordinating server.
///
/// Split out from [`report_candidates`] so the selection rules can be tested
/// without touching the network or the real interface table.
///
/// Interface addresses come first because they are cheap to try and work when
/// peers share a LAN. The public address is appended: it is the only entry a
/// peer on a different network can reach, but it is also the most likely to be
/// stale, so it goes last.
fn collect_candidates(
    local: Vec<IpAddr>,
    public: Option<IpAddr>,
    nat: &NatOpts,
    listen_port: u16,
) -> Vec<Endpoint> {
    let mut candidates: Vec<Endpoint> = local
        .into_iter()
        .filter(|ip| !nat.is_excluded(*ip))
        .map(|addr| SocketAddr::from((addr, listen_port)).into())
        .collect();

    // A public address that the user excluded should stay excluded; the
    // exclusion list is the documented way to keep an address off the wire.
    if let Some(ip) = public {
        if !is_plausible_public_address(ip) {
            // Reporting it would be worse than reporting nothing: peers would
            // spend traversal time on an address that cannot work, and the
            // real problem (an intercepted probe) would stay hidden.
            log::warn!(
                "ignoring reported public address {ip}: it is not globally routable \
                 (a DNS fake-IP resolver or transparent proxy may be intercepting the probe). \
                 Set --public-endpoint to override."
            );
        } else {
            let endpoint: Endpoint = SocketAddr::from((ip, listen_port)).into();
            if !nat.is_excluded(ip) && !candidates.contains(&endpoint) {
                candidates.push(endpoint);
            }
        }
    }

    candidates
}

fn report_candidates(
    rest_client: &RestClient,
    nat: &NatOpts,
    listen_port: u16,
) -> Result<(), Error> {
    let local = get_local_addrs()?.collect::<Vec<_>>();

    // An explicit --public-endpoint wins over probing, because the probe can
    // silently return the wrong answer (a fake-IP resolver answers with an
    // address from its own pool) and there is no way to tell that apart from
    // a real public address.
    let public = match nat.public_endpoint {
        Some(addr) => {
            log::debug!("using explicitly configured public endpoint {addr}");
            Some(addr.ip())
        },
        None => innernet_publicip::get_any(innernet_publicip::Preference::Ipv4),
    };

    if public.is_none() {
        log::debug!(
            "could not determine a public address; peers on other networks may be unable to connect"
        );
    }

    let candidates = collect_candidates(local, public, nat, listen_port);

    log::info!(
        "reporting {} NAT traversal candidate{}",
        candidates.len(),
        if candidates.len() == 1 { "" } else { "s" },
    );
    for candidate in &candidates {
        log::debug!("  candidate: {}", candidate);
    }
    if let Err(e) = rest_client.http_form::<_, ()>("PUT", "/user/candidates", &candidates) {
        if e.has_status_of(404) {
            log::warn!("your network is using an old version of innernet-server that doesn't support NAT traversal candidate reporting.")
        } else {
            return Err(e.into());
        }
    }

    log::debug!("candidates successfully reported");
    Ok(())
}

pub fn interface_is_up(backend: Backend, interface_name: &InterfaceName) -> bool {
    match Device::list(backend) {
        Ok(interfaces) => interfaces.contains(interface_name),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ipnet::IpNet;
    use std::str::FromStr;

    fn nat_opts() -> NatOpts {
        NatOpts {
            no_nat_traversal: false,
            exclude_nat_candidates: vec![],
            no_nat_candidates: false,
            public_endpoint: None,
        }
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    /// The regression this whole change exists for: interface addresses alone
    /// leave a peer on another network with nothing routable to try.
    #[test]
    fn public_address_is_reported_alongside_private_ones() {
        let candidates = collect_candidates(
            vec![ip("192.168.100.54")],
            Some(ip("104.129.59.241")),
            &nat_opts(),
            51820,
        );
        assert_eq!(
            candidates.len(),
            2,
            "expected the private address and the public one, got {candidates:?}"
        );
        // Private first: cheap to try, and correct when peers share a LAN.
        assert_eq!(candidates[0].to_string(), "192.168.100.54:51820");
        // Public last: the only entry usable from another network.
        assert_eq!(candidates[1].to_string(), "104.129.59.241:51820");
    }

    /// A failed probe must not produce a bogus entry, and must not lose the
    /// private ones either.
    #[test]
    fn missing_public_address_still_reports_local_ones() {
        let candidates = collect_candidates(vec![ip("192.168.100.54")], None, &nat_opts(), 51820);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].to_string(), "192.168.100.54:51820");
    }

    #[test]
    fn duplicate_public_address_is_not_reported_twice() {
        // Happens when the interface already carries the public address.
        let candidates = collect_candidates(
            vec![ip("104.129.59.241")],
            Some(ip("104.129.59.241")),
            &nat_opts(),
            51820,
        );
        assert_eq!(candidates.len(), 1, "got {candidates:?}");
    }

    #[test]
    fn excluded_cidr_removes_matching_addresses() {
        let mut opts = nat_opts();
        opts.exclude_nat_candidates = vec![IpNet::from_str("0.0.0.0/0").unwrap()];
        let candidates = collect_candidates(
            vec![ip("192.168.100.54")],
            Some(ip("104.129.59.241")),
            &opts,
            51820,
        );
        assert!(
            candidates.is_empty(),
            "0.0.0.0/0 should suppress everything, got {candidates:?}"
        );
    }

    /// The exclusion list has to apply to the public address too, otherwise a
    /// user who asked for privacy would still leak their public IP.
    #[test]
    fn exclusion_applies_to_public_address() {
        let mut opts = nat_opts();
        opts.exclude_nat_candidates = vec![IpNet::from_str("104.129.59.0/24").unwrap()];
        let candidates = collect_candidates(
            vec![ip("192.168.100.54")],
            Some(ip("104.129.59.241")),
            &opts,
            51820,
        );
        assert_eq!(candidates.len(), 1, "got {candidates:?}");
        assert_eq!(candidates[0].to_string(), "192.168.100.54:51820");
    }

    #[test]
    fn no_nat_candidates_suppresses_everything() {
        let mut opts = nat_opts();
        opts.no_nat_candidates = true;
        let candidates = collect_candidates(
            vec![ip("192.168.100.54")],
            Some(ip("104.129.59.241")),
            &opts,
            51820,
        );
        assert!(candidates.is_empty(), "got {candidates:?}");
    }

    /// The exact failure observed in the field: Clash's fake-IP resolver
    /// answers the public-address probe with 198.19.162.82. Reporting that
    /// would give every peer an address that cannot work.
    #[test]
    fn fake_ip_probe_answer_is_rejected() {
        assert!(!is_plausible_public_address(ip("198.19.162.82")));

        let candidates = collect_candidates(
            vec![ip("192.168.100.54")],
            Some(ip("198.19.162.82")),
            &nat_opts(),
            51820,
        );
        assert_eq!(
            candidates.len(),
            1,
            "fake-IP must not become a candidate, got {candidates:?}"
        );
        assert_eq!(candidates[0].to_string(), "192.168.100.54:51820");
    }

    #[test]
    fn plausible_public_addresses_pass() {
        for addr in ["104.129.59.241", "163.125.230.175", "8.8.8.8", "1.1.1.1"] {
            assert!(
                is_plausible_public_address(ip(addr)),
                "{addr} should be accepted as public"
            );
        }
    }

    #[test]
    fn non_routable_ranges_are_rejected() {
        for addr in [
            "192.168.1.1",  // private
            "10.0.0.1",     // private
            "172.16.0.1",   // private
            "127.0.0.1",    // loopback
            "169.254.1.1",  // link-local
            "100.64.0.1",   // CGNAT
            "0.0.0.0",      // unspecified
            "240.0.0.1",    // reserved
            "198.18.0.1",   // fake-IP pool lower half
            "198.19.255.1", // fake-IP pool upper half
        ] {
            assert!(
                !is_plausible_public_address(ip(addr)),
                "{addr} must be rejected as non-public"
            );
        }
    }

    /// Listen port applies to every candidate, including the public one.
    #[test]
    fn listen_port_is_applied_to_all_candidates() {
        let candidates = collect_candidates(
            vec![ip("192.168.100.54")],
            Some(ip("104.129.59.241")),
            &nat_opts(),
            60382,
        );
        for c in &candidates {
            assert!(
                c.to_string().ends_with(":60382"),
                "candidate {c} ignored the listen port"
            );
        }
    }
}
