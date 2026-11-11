//! Embedded web admin UI.
//!
//! The core API authenticates peers by WireGuard public key, which a browser
//! cannot present. This module adds a small HTTP surface bound to loopback
//! that authenticates with username/password and then acts on the network
//! through the very same database + WireGuard code paths the CLI uses.
//!
//! Deliberately minimal: no template engine, no JS framework. One HTML page
//! that talks JSON to the endpoints below.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::ops::Deref;

use hyper::{Body, Request, Response, StatusCode};
use ipnet::IpNet;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::DatabasePeer;
use crate::web::auth::Account;

pub mod auth;
pub mod ui;
use crate::ServerError;
use innernet_shared::PeerContents;
use ui::INDEX_HTML;
use wireguard_control::{DeviceUpdate, PeerConfigBuilder};

/// Name of the cookie carrying the session token.
const COOKIE: &str = "innernet_web_session";

#[derive(Debug, Serialize)]
pub struct PeerView {
    pub id: i64,
    pub name: String,
    pub ip: String,
    pub public_key: String,
    pub endpoint: Option<String>,
    pub is_admin: bool,
    pub is_disabled: bool,
    pub is_redeemed: bool,
    pub allowed_subnets: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CidrView {
    pub id: i64,
    pub name: String,
    pub cidr: String,
    pub parent: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct SubnetUpdate {
    pub allowed_subnets: Vec<String>,
}

/// Result of a web request: either a response, or "not a web route" so the
/// caller can fall through to the regular innernet API routing.
pub enum WebOutcome {
    Handled(Response<Body>),
    PassThrough,
}

fn json<T: Serialize>(status: StatusCode, value: &T) -> Response<Body> {
    let body = serde_json::to_string(value).unwrap_or_else(|_| "{}".into());
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn text(status: StatusCode, body: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(body))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn html(body: String) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(hyper::header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(body))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn unauthorised() -> Response<Body> {
    json(
        StatusCode::UNAUTHORIZED,
        &serde_json::json!({ "error": "authentication required" }),
    )
}

pub fn session_token(req: &Request<Body>) -> Option<String> {
    let cookie = req.headers().get(hyper::header::COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix(&format!("{COOKIE}="))
            .map(|v| v.trim().to_string())
    })
}

fn peer_to_view(peer: &DatabasePeer) -> PeerView {
    PeerView {
        id: peer.id,
        name: peer.name.to_string(),
        ip: peer.ip.to_string(),
        public_key: peer.public_key.clone(),
        endpoint: peer.endpoint.as_ref().map(|e| e.to_string()),
        is_admin: peer.is_admin,
        is_disabled: peer.is_disabled,
        is_redeemed: peer.is_redeemed,
        allowed_subnets: peer.allowed_subnets.iter().map(|s| s.to_string()).collect(),
    }
}

fn admin_account(conn: &Connection, token: &str) -> Option<Account> {
    auth::session_account(conn, token)
        .ok()
        .flatten()
        .filter(|a| a.is_admin)
}

/// Push a peer's current database state onto the live WireGuard interface.
pub fn apply_peer(
    conn_peer: &DatabasePeer,
    interface: &wireguard_control::InterfaceName,
    backend: wireguard_control::Backend,
) -> Result<(), ServerError> {
    // Build the diff against "no previous config" so allowed-ips is a full
    // replacement reflecting the peer's current subnets.
    {
        DeviceUpdate::new()
            .add_peer(PeerConfigBuilder::from(conn_peer.deref()))
            .apply(interface, backend)
            .map_err(|_| ServerError::WireGuard)?;
    }
    Ok(())
}

/// Extract `subnets` from a JSON body, validating each entry.
pub fn parse_subnets(body: &str) -> Result<Vec<IpNet>, String> {
    let parsed: SubnetUpdate =
        serde_json::from_str(body).map_err(|e| format!("invalid JSON: {e}"))?;
    let mut out = Vec::new();
    for raw in parsed.allowed_subnets {
        let net: IpNet = raw.parse().map_err(|_| format!("invalid CIDR: {raw}"))?;
        out.push(net);
    }
    Ok(out)
}

/// Build the routing table component list for the web prefix.
pub fn components(path: &str) -> Option<VecDeque<String>> {
    let rest = path.strip_prefix("/web")?;
    Some(
        rest.trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
    )
}

pub fn is_web_path(path: &str) -> bool {
    path == "/web" || path.starts_with("/web/")
}

/// Seed the default admin account on first run, returning the generated
/// password so the operator can be told about it once.
pub fn ensure_default_account(conn: &Connection) -> Result<Option<String>, rusqlite::Error> {
    auth::ensure_tables(conn)?;
    if auth::account_count(conn)? > 0 {
        return Ok(None);
    }
    let password = random_password();
    auth::create_account(conn, "admin", &password, true)?;
    Ok(Some(password))
}

fn random_password() -> String {
    // 24 characters from an unambiguous alphabet.
    const A: &[u8] = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
        ^ (std::process::id() as u64) << 32;
    let mut x = seed | 1;
    let mut out = String::with_capacity(24);
    for _ in 0..24 {
        // xorshift64*
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        let v = x.wrapping_mul(0x2545F4914F6CDD1D);
        out.push(A[(v >> 33) as usize % A.len()] as char);
    }
    out
}

/// Handler entry point. Returns `PassThrough` when the path is not ours.
pub async fn handle(
    req: Request<Body>,
    _remote: SocketAddr,
    context: crate::Context,
) -> WebOutcome {
    let interface = context.interface;
    let backend = context.backend;

    // Read the body once, before taking the DB lock, so no guard is held across
    // an await point (the connection is not Send).
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let token = session_token(&req);
    let body_bytes = match hyper::body::to_bytes(req.into_body()).await {
        Ok(b) => b,
        Err(_) => return WebOutcome::Handled(text(StatusCode::BAD_REQUEST, "unreadable body")),
    };

    let conn = context.db.lock();
    let conn: &Connection = &conn;
    let Some(mut components) = components(&path) else {
        return WebOutcome::PassThrough;
    };

    // ---- unauthenticated routes -------------------------------------------
    // Peek only: the same components are reused for the authenticated routes.
    match components.front().map(|s| s.as_str()) {
        None | Some("") => return WebOutcome::Handled(html(INDEX_HTML.to_string())),
        Some("login") if method == hyper::Method::POST => {
            let Ok(login) = serde_json::from_slice::<LoginRequest>(&body_bytes) else {
                return WebOutcome::Handled(text(StatusCode::BAD_REQUEST, "invalid JSON"));
            };
            return match auth::authenticate(conn, &login.username, &login.password) {
                Ok(Some(account)) => match auth::create_session(conn, account.id) {
                    Ok(session) => {
                        let cookie = format!(
                            "{COOKIE}={session}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
                            auth::SESSION_TTL_SECS
                        );
                        let mut resp = json(
                            StatusCode::OK,
                            &serde_json::json!({
                                "username": account.username,
                                "is_admin": account.is_admin
                            }),
                        );
                        if let Ok(v) = cookie.parse() {
                            resp.headers_mut().insert(hyper::header::SET_COOKIE, v);
                        }
                        WebOutcome::Handled(resp)
                    },
                    Err(_) => {
                        WebOutcome::Handled(text(StatusCode::INTERNAL_SERVER_ERROR, "session"))
                    },
                },
                _ => WebOutcome::Handled(json(
                    StatusCode::UNAUTHORIZED,
                    &serde_json::json!({ "error": "invalid credentials" }),
                )),
            };
        },
        Some("logout") => {
            if let Some(t) = &token {
                let _ = auth::delete_session(conn, t);
            }
            return WebOutcome::Handled(json(StatusCode::OK, &serde_json::json!({"ok": true})));
        },
        _ => {},
    }

    // ---- authenticated routes ---------------------------------------------
    let Some(tok) = token else {
        return WebOutcome::Handled(unauthorised());
    };
    if admin_account(conn, &tok).is_none() {
        return WebOutcome::Handled(unauthorised());
    }

    // The public routes above only peeked, so the first component is intact.
    let result: Result<Response<Body>, ServerError> = match components.pop_front().as_deref() {
        Some("me") => Ok(json(
            StatusCode::OK,
            &serde_json::json!({ "authenticated": true }),
        )),

        Some("peers") if components.is_empty() && method == hyper::Method::GET => {
            DatabasePeer::list(conn).map(|peers| {
                let views: Vec<PeerView> = peers.iter().map(peer_to_view).collect();
                json(StatusCode::OK, &views)
            })
        },

        Some("peers") => {
            let Some(id) = components.pop_front().and_then(|s| s.parse::<i64>().ok()) else {
                return WebOutcome::Handled(text(StatusCode::NOT_FOUND, "peer id"));
            };
            // Consume the action exactly once: matching on a pop inside a guard
            // would consume the component even when the guard fails.
            let action = components.pop_front();

            match (method.clone(), action.as_deref()) {
                (hyper::Method::GET, None) => {
                    DatabasePeer::get(conn, id).map(|p| json(StatusCode::OK, &peer_to_view(&p)))
                },

                (hyper::Method::PUT, Some("subnets")) if components.is_empty() => {
                    let body = String::from_utf8_lossy(&body_bytes).to_string();
                    match parse_subnets(&body) {
                        Err(msg) => Ok(json(
                            StatusCode::BAD_REQUEST,
                            &serde_json::json!({ "error": msg }),
                        )),
                        Ok(subnets) => (|| {
                            let mut peer = DatabasePeer::get(conn, id)?;
                            let contents = PeerContents {
                                allowed_subnets: subnets,
                                ..peer.contents.clone()
                            };
                            peer.update(conn, contents)?;
                            apply_peer(&peer, &interface, backend)?;
                            Ok(json(StatusCode::OK, &peer_to_view(&peer)))
                        })(),
                    }
                },

                (hyper::Method::PUT, Some("disabled")) if components.is_empty() => {
                    let want = serde_json::from_slice::<serde_json::Value>(&body_bytes)
                        .ok()
                        .and_then(|v| v.get("is_disabled").and_then(|b| b.as_bool()));
                    let Some(want) = want else {
                        return WebOutcome::Handled(json(
                            StatusCode::BAD_REQUEST,
                            &serde_json::json!({ "error": "is_disabled must be a boolean" }),
                        ));
                    };
                    (|| {
                        let mut peer = DatabasePeer::get(conn, id)?;
                        let contents = PeerContents {
                            is_disabled: want,
                            // Disabling must also drop the advertised routes so
                            // the revocation is immediate, not just cosmetic.
                            allowed_subnets: if want {
                                vec![]
                            } else {
                                peer.allowed_subnets.clone()
                            },
                            ..peer.contents.clone()
                        };
                        peer.update(conn, contents)?;
                        apply_peer(&peer, &interface, backend)?;
                        Ok(json(StatusCode::OK, &peer_to_view(&peer)))
                    })()
                },

                _ => return WebOutcome::Handled(text(StatusCode::NOT_FOUND, "peer route")),
            }
        },

        Some("cidrs") if method == hyper::Method::GET => {
            crate::db::DatabaseCidr::list(conn).map(|cidrs| {
                let views: Vec<CidrView> = cidrs
                    .iter()
                    .map(|c| CidrView {
                        id: c.id,
                        name: c.name.clone(),
                        cidr: c.cidr.to_string(),
                        parent: c.parent,
                    })
                    .collect();
                json(StatusCode::OK, &views)
            })
        },

        _ => return WebOutcome::Handled(text(StatusCode::NOT_FOUND, "not found")),
    };

    WebOutcome::Handled(result.unwrap_or_else(|e| {
        json(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({ "error": format!("{e}") }),
        )
    }))
}
