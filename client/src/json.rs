//! Machine-readable snapshots of innernet state, for GUI clients.
//!
//! The human-readable `innernet show` output is coloured and formatted for
//! terminals, which a GUI cannot reliably parse. This module produces the same
//! information as stable JSON so a front-end can consume it without scraping.

use serde::Serialize;

use innernet_client_core::data_store::DataStore;
use innernet_shared::interface_config::InterfaceConfig;
use innernet_shared::wg::PeerInfoExt;
use innernet_shared::Cidr;

/// Top-level document printed by `innernet show --json`.
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub version: String,
    /// Directory the state was read from, shown in GUIs for troubleshooting.
    pub config_dir: String,
    /// True when the config directory exists but could not be read, which
    /// almost always means the caller lacks root.
    pub permission_denied: bool,
    pub interfaces: Vec<InterfaceSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct InterfaceSnapshot {
    pub name: String,
    pub network_name: String,
    pub is_up: bool,
    pub listen_port: Option<u16>,
    pub address: String,
    pub network_cidr_prefix: u8,
    pub public_key: Option<String>,
    /// This device's own peer entry.
    pub me: Option<PeerSnapshot>,
    pub peers: Vec<PeerSnapshot>,
    pub cidrs: Vec<CidrSnapshot>,
    /// Peers whose endpoint was overridden locally, keyed by peer IP.
    pub endpoint_overrides: Vec<EndpointOverride>,
    /// Whether a boot service keeps this interface up, so a GUI can offer the
    /// right action instead of guessing.
    pub service: crate::service::ServiceStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerSnapshot {
    pub name: String,
    pub ip: String,
    pub public_key: String,
    pub cidr_id: i64,
    pub is_admin: bool,
    pub is_disabled: bool,
    pub is_you: bool,
    pub is_connected: bool,
    pub endpoint: Option<String>,
    pub endpoint_overridden: bool,
    pub last_handshake_secs_ago: Option<u64>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    /// Subnets this peer routes (requires the subnet-routing fork).
    pub allowed_subnets: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CidrSnapshot {
    pub id: i64,
    pub name: String,
    pub cidr: String,
    pub parent: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct EndpointOverride {
    pub peer_ip: String,
    pub endpoint: String,
}

/// Build a snapshot for one interface.
pub fn interface_snapshot(
    name: &str,
    device: Option<&wireguard_control::Device>,
    store: &DataStore,
    config: &InterfaceConfig,
) -> InterfaceSnapshot {
    let public_key = device.and_then(|d| d.public_key.as_ref().map(|k| k.to_base64()));

    let peers = store.peers();
    let cidrs = store.cidrs();

    let me = public_key
        .as_ref()
        .and_then(|pk| peers.iter().find(|p| &p.public_key == pk));

    let overrides = config.peer_endpoint_overrides();
    let mut peer_snapshots = Vec::with_capacity(peers.len());

    for peer in peers.iter() {
        let info = device.and_then(|d| {
            d.peers
                .iter()
                .find(|info| info.config.public_key.to_base64() == peer.public_key)
        });

        let endpoint_overridden = overrides.contains_key(&peer.ip);

        peer_snapshots.push(PeerSnapshot {
            name: peer.name.to_string(),
            ip: peer.ip.to_string(),
            public_key: peer.public_key.clone(),
            cidr_id: peer.cidr_id,
            is_admin: peer.is_admin,
            is_disabled: peer.is_disabled,
            is_you: me.map(|m| m.public_key == peer.public_key).unwrap_or(false),
            is_connected: info.map(|i| i.is_recently_connected()).unwrap_or(false),
            endpoint: info.and_then(|i| i.config.endpoint).map(|e| e.to_string()),
            endpoint_overridden,
            last_handshake_secs_ago: info
                .and_then(|i| i.stats.last_handshake_time)
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs()),
            rx_bytes: info.map(|i| i.stats.rx_bytes).unwrap_or(0),
            tx_bytes: info.map(|i| i.stats.tx_bytes).unwrap_or(0),
            allowed_subnets: peer.allowed_subnets.iter().map(|s| s.to_string()).collect(),
        });
    }

    InterfaceSnapshot {
        name: name.to_string(),
        network_name: config.interface.network_name.clone(),
        is_up: device.is_some(),
        listen_port: device.and_then(|d| d.listen_port),
        address: config.interface.address.addr().to_string(),
        network_cidr_prefix: config.interface.address.prefix_len(),
        public_key: public_key.clone(),
        me: me.and_then(|m| {
            peer_snapshots
                .iter()
                .find(|p| p.public_key == m.public_key)
                .map(|p| PeerSnapshot {
                    name: p.name.clone(),
                    ip: p.ip.clone(),
                    public_key: p.public_key.clone(),
                    cidr_id: p.cidr_id,
                    is_admin: p.is_admin,
                    is_disabled: p.is_disabled,
                    is_you: true,
                    is_connected: p.is_connected,
                    endpoint: p.endpoint.clone(),
                    endpoint_overridden: p.endpoint_overridden,
                    last_handshake_secs_ago: p.last_handshake_secs_ago,
                    rx_bytes: p.rx_bytes,
                    tx_bytes: p.tx_bytes,
                    allowed_subnets: p.allowed_subnets.clone(),
                })
        }),
        peers: peer_snapshots,
        cidrs: cidrs
            .iter()
            .map(|c: &Cidr| CidrSnapshot {
                id: c.id,
                name: c.name.clone(),
                cidr: c.cidr.to_string(),
                parent: c.parent,
            })
            .collect(),
        service: crate::service::status(name),
        endpoint_overrides: overrides
            .iter()
            .map(|(ip, ep)| EndpointOverride {
                peer_ip: ip.to_string(),
                endpoint: ep.to_string(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_serializes_with_stable_keys() {
        let snap = Snapshot {
            version: "2.0.0".into(),
            config_dir: "/etc/innernet".into(),
            permission_denied: true,
            interfaces: vec![InterfaceSnapshot {
                name: "heheda-mesh".into(),
                network_name: "heheda-mesh".into(),
                is_up: true,
                listen_port: Some(51820),
                address: "10.60.1.2".into(),
                network_cidr_prefix: 16,
                public_key: Some("UbjZheVk=".into()),
                me: None,
                peers: vec![PeerSnapshot {
                    name: "home-mac".into(),
                    ip: "10.60.1.2".into(),
                    public_key: "UbjZheVk=".into(),
                    cidr_id: 3,
                    is_admin: false,
                    is_disabled: false,
                    is_you: true,
                    is_connected: true,
                    endpoint: Some("1.2.3.4:51820".into()),
                    endpoint_overridden: false,
                    last_handshake_secs_ago: Some(12),
                    rx_bytes: 1024,
                    tx_bytes: 2048,
                    allowed_subnets: vec!["192.168.100.0/24".into()],
                }],
                cidrs: vec![CidrSnapshot {
                    id: 3,
                    name: "humans".into(),
                    cidr: "10.60.1.0/24".into(),
                    parent: Some(1),
                }],
                endpoint_overrides: vec![],
                service: crate::service::ServiceStatus {
                    manager: "launchd",
                    installed: false,
                    running: false,
                    path: "/Library/LaunchDaemons/com.innernet.test.plist".to_string(),
                },
            }],
        };

        let v: serde_json::Value = serde_json::to_value(&snap).unwrap();
        // A GUI depends on these keys existing; guard against silent renames.
        assert_eq!(v["version"], "2.0.0");
        assert_eq!(v["permission_denied"], true);
        assert_eq!(v["config_dir"], "/etc/innernet");

        let iface = &v["interfaces"][0];
        assert_eq!(iface["name"], "heheda-mesh");
        assert_eq!(iface["is_up"], true);
        assert_eq!(iface["listen_port"], 51820);
        assert_eq!(iface["network_cidr_prefix"], 16);

        let peer = &iface["peers"][0];
        assert_eq!(peer["is_you"], true);
        assert_eq!(peer["is_connected"], true);
        assert_eq!(peer["allowed_subnets"][0], "192.168.100.0/24");
        assert_eq!(peer["last_handshake_secs_ago"], 12);
    }

    #[test]
    fn empty_snapshot_is_still_valid_json() {
        let snap = Snapshot {
            version: "2.0.0".into(),
            config_dir: "/etc/innernet".into(),
            permission_denied: true,
            interfaces: vec![],
        };
        let s = serde_json::to_string(&snap).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["interfaces"].as_array().unwrap().len(), 0);
        assert_eq!(v["permission_denied"], true);
    }
}
