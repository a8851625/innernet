use std::net::SocketAddr;

use crate::Session;
use innernet_shared::{Endpoint, Peer};

pub mod admin;
pub mod user;

/// Implements NAT traversal strategies.
/// (1) Unspecified endpoint IP: a peer may configure an override endpoint with an unspecified IP.
///     This can be useful if the peer uses a port forwarding with a fixed port but does not have a
///     fixed global IP and it needs help from the innernet server to resolve it. Replace the
///     endpoint IP with the most recent wireguard endpoint's IP.
/// (2) NAT hole punching: Report the most recent wireguard endpoint either (a) as the peer's
///     endpoint or (b) add it to the list of NAT candidates, if an override endpoint is specified.
///     Note that NAT traversal does not always work e.g. if the peer is behind double NAT or
///     address/port restricted cone NAT.
pub fn inject_endpoints(session: &Session, peers: &mut Vec<Peer>) {
    for peer in peers {
        let endpoints = session.context.endpoints.read();
        let wg_endpoint = endpoints.get(&peer.public_key);

        match &mut peer.contents.endpoint {
            None => {
                // 2a. Set the peer endpoint to the wireguard endpoint.
                peer.contents.endpoint = wg_endpoint.map(|e| e.to_owned().into())
            },
            Some(peer_endpoint) => {
                if peer_endpoint.is_host_unspecified() {
                    if let Some(wg_endpoint) = wg_endpoint {
                        // 1. Replace the unspecified peer endpoint host with the wireguard
                        //    endpoint's host.
                        *peer_endpoint =
                            SocketAddr::new(wg_endpoint.ip(), peer_endpoint.port()).into();
                    } else {
                        // There is no way to complete the peer endpoint, unset it.
                        peer.contents.endpoint = None;
                    }
                }
            },
        }

        if let Some(wg_endpoint) = wg_endpoint {
            // 2b. Add the wireguard endpoint to the peer candidates, unless it's already present
            // either in the peer endpoint or in the peer candidates.
            let wg_endpoint: Endpoint = wg_endpoint.to_owned().into();

            // The peer reported its own candidates from inside its network, so a public
            // candidate carries the port the peer *listens* on. Behind NAT that port is
            // rewritten, and the port we observed is the only accurate one. Drop any
            // candidate that agrees on the IP but not on the port: it is the same host
            // reached at a port that is not open, and leaving it in the list makes peers
            // spend their 5-second traversal slots on an address that cannot work.
            merge_observed_endpoint(peer, wg_endpoint);
        }
    }
}

/// Fold the endpoint the server actually observed for a peer into its candidate list.
///
/// Peers build their candidate list from inside their own network, so a public
/// candidate carries the port the peer *listens* on. Behind NAT that port is
/// rewritten on the way out, which leaves the advertised candidate pointing at a
/// closed port: a peer listening on 51174 can reach the server from 7596, and
/// anyone dialling 51174 sends packets into a black hole and never completes a
/// handshake.
///
/// The server is the only party that sees the translated address, so it is the
/// only one that can correct the list. When a candidate shares the observed IP
/// but disagrees on the port, drop it: same host, port that is not open. Then
/// make sure the observed endpoint itself is present.
fn merge_observed_endpoint(peer: &mut Peer, observed: Endpoint) {
    if let Ok(observed_addr) = observed.resolve() {
        peer.candidates.retain(|candidate| {
            candidate.resolve().map_or(true, |resolved| {
                !(resolved.ip() == observed_addr.ip() && resolved.port() != observed_addr.port())
            })
        });
    }

    if peer.contents.endpoint.as_ref() != Some(&observed) && !peer.candidates.contains(&observed) {
        peer.candidates.push(observed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use innernet_shared::{Peer, PeerContents};

    fn peer_with(candidates: &[&str]) -> Peer {
        Peer {
            id: 1,
            contents: PeerContents {
                name: "mac-office".parse().unwrap(),
                ip: "10.60.1.1".parse().unwrap(),
                cidr_id: 1,
                public_key: String::new(),
                endpoint: None,
                persistent_keepalive_interval: None,
                is_admin: false,
                is_disabled: false,
                is_redeemed: true,
                invite_expires: None,
                candidates: candidates
                    .iter()
                    .map(|c| c.parse().expect("test candidate should parse"))
                    .collect(),
                allowed_subnets: vec![],
            },
        }
    }

    fn rendered(peer: &Peer) -> Vec<String> {
        peer.candidates.iter().map(|c| c.to_string()).collect()
    }

    #[test]
    fn reported_candidate_on_the_wrong_port_is_replaced_by_the_observed_one() {
        // The peer listens on 51174; NAT rewrote it to 7596.
        let mut peer = peer_with(&["163.125.149.192:51174", "192.168.31.60:51174"]);
        merge_observed_endpoint(&mut peer, "163.125.149.192:7596".parse().unwrap());

        let got = rendered(&peer);
        assert!(
            got.contains(&"163.125.149.192:7596".to_string()),
            "observed endpoint must be offered: {got:?}"
        );
        assert!(
            !got.contains(&"163.125.149.192:51174".to_string()),
            "the closed port must be dropped, or traversals waste slots on it: {got:?}"
        );
        // A private candidate on a different IP is left alone: it is still the right
        // address when both peers share a LAN.
        assert!(
            got.contains(&"192.168.31.60:51174".to_string()),
            "candidates on other IPs are none of our business: {got:?}"
        );
    }

    #[test]
    fn observed_endpoint_is_not_added_twice() {
        let mut peer = peer_with(&["163.125.149.192:7596"]);
        merge_observed_endpoint(&mut peer, "163.125.149.192:7596".parse().unwrap());

        let count = rendered(&peer)
            .iter()
            .filter(|c| *c == "163.125.149.192:7596")
            .count();
        assert_eq!(count, 1, "a duplicate would just waste a traversal slot");
    }

    #[test]
    fn private_candidates_survive_when_nothing_matches() {
        let mut peer = peer_with(&["192.168.31.60:51174"]);
        merge_observed_endpoint(&mut peer, "163.125.149.192:7596".parse().unwrap());

        let got = rendered(&peer);
        assert!(got.contains(&"192.168.31.60:51174".to_string()));
        assert!(got.contains(&"163.125.149.192:7596".to_string()));
    }

    #[test]
    fn ipv6_candidates_are_matched_on_the_full_address() {
        // Peers advertise several IPv6 addresses, some of them privacy addresses that
        // rotate; only the one the server saw must have its port corrected.
        let mut peer = peer_with(&[
            "[2408:8256:348b:3065::cde]:51174",
            "[2408:8256:348b:3065:143f:8b46:22d4:c036]:51174",
        ]);
        merge_observed_endpoint(
            &mut peer,
            "[2408:8256:348b:3065::cde]:7596".parse().unwrap(),
        );

        let got = rendered(&peer);
        assert!(got.contains(&"[2408:8256:348b:3065::cde]:7596".to_string()));
        assert!(!got.contains(&"[2408:8256:348b:3065::cde]:51174".to_string()));
        // A different address keeps its own port: it is a different host, and there is
        // no observation that would justify rewriting it.
        assert!(got.contains(&"[2408:8256:348b:3065:143f:8b46:22d4:c036]:51174".to_string()));
    }
}
