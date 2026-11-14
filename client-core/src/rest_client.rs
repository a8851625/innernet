use innernet_shared::{
    interface_config::ServerInfo, Cidr, CidrContents, Peer, PeerContents, INNERNET_PUBKEY_HEADER,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{io, time::Duration};
use thiserror::Error;
use ureq::{Agent, AgentBuilder};

/// Default per-request budget for talking to the coordinator.
///
/// Sized for a slow uplink, not a LAN: the candidate report fires the moment the
/// tunnel comes up, so it competes with the first handshake, and a router 250ms
/// from the server needs several round trips inside this window.
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait for one request, honouring `INNERNET_HTTP_TIMEOUT`.
///
/// Accepts plain seconds ("45"). Anything unparseable or zero falls back to the
/// default rather than disabling the timeout, because a request with no deadline
/// on a dead link hangs the daemon forever.
fn http_timeout() -> Duration {
    std::env::var("INNERNET_HTTP_TIMEOUT")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_HTTP_TIMEOUT)
}

/// A REST client that can be used to communicate with an innernet REST server.
///
/// We recommend to use the high level API (like [`Self::create_peer()`]) when possible and fall
/// back on the low level [`Self::http()`] and [`Self::http_form()`] otherwise.
pub struct RestClient<'a> {
    agent: Agent,
    server: &'a ServerInfo,
}

impl<'a> RestClient<'a> {
    /// Create a [`Self`] to communicate with an innernet server described by [`ServerInfo`].
    pub fn new(server: &'a ServerInfo) -> Self {
        let agent = AgentBuilder::new()
            // Some platforms (e.g. OpenBSD) can take longer to complete the first WireGuard
            // handshake, hence a lower timeout value could result in an unwarranted failure.
            //
            // 30s rather than 10s: the request that failed most visibly was the
            // NAT candidate report, which happens immediately after the tunnel
            // comes up and therefore races the first handshake. On a router whose
            // uplink is 250ms+ away from the coordinator, the 10s budget expired
            // mid-response and `up` printed a transport error even though the
            // tunnel itself was fine. The failure is cosmetic, but it is the
            // first thing the user sees, and it looks like the connection broke.
            //
            // INNERNET_HTTP_TIMEOUT overrides it for links where even 30s is
            // tight (satellite, heavily shaped mobile).
            .timeout(http_timeout())
            .redirects(0)
            .build();
        Self { agent, server }
    }

    pub fn create_cidr(&self, cidr_contents: &CidrContents) -> Result<Cidr, RestError> {
        let cidr = self.http_form("POST", "/admin/cidrs", cidr_contents)?;
        Ok(cidr)
    }

    pub fn get_cidrs(&self) -> Result<Vec<Cidr>, RestError> {
        let cidrs = self.http("GET", "/admin/cidrs")?;
        Ok(cidrs)
    }

    pub fn create_peer(&self, peer_contents: &PeerContents) -> Result<Peer, RestError> {
        let peer = self.http_form("POST", "/admin/peers", peer_contents)?;
        Ok(peer)
    }

    pub fn get_peers(&self) -> Result<Vec<Peer>, RestError> {
        let peers = self.http("GET", "/admin/peers")?;
        Ok(peers)
    }

    #[allow(clippy::result_large_err)]
    /// Perform a `verb` HTTP request at the given `endpoint`.
    ///
    /// Example: `rest_client.http("GET", "/admin/peers")?;`.
    pub fn http<T: DeserializeOwned>(&self, verb: &str, endpoint: &str) -> Result<T, RestError> {
        self.request::<(), _>(verb, endpoint, None)
    }

    /// Send serializable data using a `verb` HTTP request at the given `endpoint`
    ///
    /// Example: `rest_client.http_form("POST", "/admin/peers", PeerContents { .. })?;`.
    #[allow(clippy::result_large_err)]
    pub fn http_form<S: Serialize, T: DeserializeOwned>(
        &self,
        verb: &str,
        endpoint: &str,
        form: S,
    ) -> Result<T, RestError> {
        self.request(verb, endpoint, Some(form))
    }

    #[allow(clippy::result_large_err)]
    fn request<S: Serialize, T: DeserializeOwned>(
        &self,
        verb: &str,
        endpoint: &str,
        form: Option<S>,
    ) -> Result<T, RestError> {
        let request = self
            .agent
            .request(
                verb,
                &format!("http://{}/v1{}", self.server.internal_endpoint, endpoint),
            )
            .set(INNERNET_PUBKEY_HEADER, &self.server.public_key);

        let result = if let Some(form) = form {
            let payload = serde_json::to_value(form).map_err(RestError::RequestSerialize)?;
            request.send_json(payload)
        } else {
            request.call()
        };
        let response = result.map_err(|e| RestError::RequestSend(Box::new(e)))?;

        let mut response = response.into_string().map_err(RestError::ResponseRead)?;
        // A little trick for serde to parse an empty response as `()`.
        if response.is_empty() {
            response = "null".into();
        }
        serde_json::from_str(&response).map_err(RestError::ResponseDeserialize)
    }
}

#[derive(Debug, Error)]
pub enum RestError {
    #[error("Error sending request: {0}")]
    RequestSend(Box<ureq::Error>),
    #[error("Error serializing request: {0}")]
    RequestSerialize(serde_json::Error),
    #[error("Error deserializing response: {0}")]
    ResponseDeserialize(serde_json::Error),
    #[error("Error reading response: {0}")]
    ResponseRead(io::Error),
}

impl RestError {
    pub fn has_status_of(&self, status: u16) -> bool {
        if let RestError::RequestSend(error) = self {
            matches!(**error, ureq::Error::Status(s, _) if s == status)
        } else {
            false
        }
    }

    pub fn is_transport_error(&self) -> bool {
        if let RestError::RequestSend(error) = self {
            matches!(**error, ureq::Error::Transport(_))
        } else {
            false
        }
    }
}

#[cfg(test)]
mod timeout_tests {
    use super::*;

    /// The default has to clear a slow uplink, not just a LAN.
    ///
    /// The candidate report runs the instant the tunnel comes up, racing the
    /// first handshake. At 10s a router 250ms from the coordinator timed out and
    /// `up` printed a transport error while the tunnel was in fact fine.
    #[test]
    fn default_timeout_clears_a_high_latency_link() {
        assert!(
            DEFAULT_HTTP_TIMEOUT >= Duration::from_secs(30),
            "a 10s budget is what produced the spurious failure"
        );
    }

    /// Parsing is total: any input yields a usable duration.
    ///
    /// A naive `parse().unwrap_or_default()` would give `Duration::ZERO`, which
    /// ureq treats as "no timeout" -- the daemon would then hang forever against
    /// a black-holed server instead of failing and retrying.
    #[test]
    fn unparseable_timeout_falls_back_instead_of_disabling() {
        // Not testing the env var directly: this process may have it set. Test
        // the same parse contract through a helper that takes the raw string.
        fn parse(raw: &str) -> Duration {
            raw.trim()
                .parse::<u64>()
                .ok()
                .filter(|secs| *secs > 0)
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_HTTP_TIMEOUT)
        }

        assert_eq!(parse("45"), Duration::from_secs(45));
        assert_eq!(parse(" 45 "), Duration::from_secs(45));
        assert_eq!(
            parse("0"),
            DEFAULT_HTTP_TIMEOUT,
            "zero must not mean 'no limit'"
        );
        assert_eq!(parse("abc"), DEFAULT_HTTP_TIMEOUT);
        assert_eq!(parse(""), DEFAULT_HTTP_TIMEOUT);
        assert_eq!(parse("-5"), DEFAULT_HTTP_TIMEOUT);
    }
}
