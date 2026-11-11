//! Password authentication for the web admin UI.
//!
//! The innernet API authenticates peers by WireGuard public key, which a browser
//! cannot present. The web UI therefore keeps its own small account table and
//! maps an authenticated session onto a peer that holds admin privileges.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Sessions older than this are rejected.
pub const SESSION_TTL_SECS: u64 = 60 * 60 * 12;

pub static CREATE_ACCOUNTS_SQL: &str = "CREATE TABLE IF NOT EXISTS web_accounts (
      id            INTEGER PRIMARY KEY,
      username      TEXT NOT NULL UNIQUE,
      password_hash TEXT NOT NULL,
      is_admin      INTEGER NOT NULL DEFAULT 0,
      created_at    INTEGER NOT NULL
    )";

pub static CREATE_SESSIONS_SQL: &str = "CREATE TABLE IF NOT EXISTS web_sessions (
      token      TEXT PRIMARY KEY,
      account_id INTEGER NOT NULL,
      expires_at INTEGER NOT NULL,
      FOREIGN KEY (account_id) REFERENCES web_accounts (id) ON DELETE CASCADE
    )";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
}

/// PHC-style string: `argon2i$<salt_b64>$<hash_b64>`.
///
/// Argon2 is not a dependency of this crate, so a deliberately simple
/// salted hash is used instead. This is adequate only because the web UI is
/// intended to bind to loopback; it is not a substitute for Argon2.
pub fn hash_password(password: &str) -> String {
    let salt = generate_salt();
    let digest = digest(password.as_bytes(), &salt);
    format!("v1${}${}", b64(&salt), b64(&digest))
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    let mut parts = stored.split('$');
    if parts.next() != Some("v1") {
        return false;
    }
    let (Some(salt_b64), Some(hash_b64)) = (parts.next(), parts.next()) else {
        return false;
    };
    let (Some(salt), Some(expected)) = (unb64(salt_b64), unb64(hash_b64)) else {
        return false;
    };
    let actual = digest(password.as_bytes(), &salt);
    // Constant-time-ish comparison; lengths always match for the same digest.
    if actual.len() != expected.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in actual.iter().zip(expected.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

fn generate_salt() -> Vec<u8> {
    // Not a CSPRNG; seeded per-call from a monotonically increasing clock plus
    // process/thread entropy. Adequate for salted hashing of a loopback UI.
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut out = Vec::with_capacity(16);
    for round in 0..2u64 {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0),
        );
        h.write_u64(round);
        h.write_u32(std::process::id());
        out.extend_from_slice(&h.finish().to_le_bytes());
    }
    out.truncate(16);
    out
}

/// 32-byte iterated digest (FNV-1a style mixing, 4096 rounds).
fn digest(password: &[u8], salt: &[u8]) -> Vec<u8> {
    let mut state = [
        0xcbf29ce484222325u64,
        0x9e3779b97f4a7c15u64,
        0x6a09e667f3bcc909u64,
        0xbb67ae8584caa73bu64,
    ];
    for round in 0..4096u64 {
        for (i, chunk) in password.chunks(8).enumerate() {
            let mut w = [0u8; 8];
            w[..chunk.len()].copy_from_slice(chunk);
            let v = u64::from_le_bytes(w);
            let idx = (i + round as usize) % 4;
            state[idx] ^= v;
            state[idx] = state[idx].wrapping_mul(0x100000001b3);
            state[idx] = state[idx].rotate_left(23);
        }
        for (i, chunk) in salt.chunks(8).enumerate() {
            let mut w = [0u8; 8];
            w[..chunk.len()].copy_from_slice(chunk);
            let v = u64::from_le_bytes(w);
            let idx = (i + round as usize + 1) % 4;
            state[idx] ^= v.wrapping_add(round);
            state[idx] = state[idx].wrapping_mul(0x100000001b3);
            state[idx] = state[idx].rotate_left(17);
        }
        state[0] ^= state[3].rotate_left(31);
        state[1] ^= state[0].rotate_left(29);
    }
    let mut out = Vec::with_capacity(32);
    for s in state {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

fn b64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut s = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            s.push(T[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            s.push(T[n as usize & 63] as char);
        }
    }
    s
}

fn unb64(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a') as u32 + 26,
            b'0'..=b'9' => (c - b'0') as u32 + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        })
    }
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

pub fn ensure_tables(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(CREATE_ACCOUNTS_SQL, params![])?;
    conn.execute(CREATE_SESSIONS_SQL, params![])?;
    Ok(())
}

pub fn create_account(
    conn: &Connection,
    username: &str,
    password: &str,
    is_admin: bool,
) -> Result<Account, rusqlite::Error> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    conn.execute(
        "INSERT INTO web_accounts (username, password_hash, is_admin, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![username, hash_password(password), is_admin, now],
    )?;
    Ok(Account {
        id: conn.last_insert_rowid(),
        username: username.to_string(),
        is_admin,
    })
}

pub fn account_count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("SELECT COUNT(*) FROM web_accounts", params![], |r| r.get(0))
}

pub fn authenticate(
    conn: &Connection,
    username: &str,
    password: &str,
) -> Result<Option<Account>, rusqlite::Error> {
    let row: Option<(i64, String, String, bool)> = conn
        .query_row(
            "SELECT id, username, password_hash, is_admin FROM web_accounts WHERE username = ?1",
            params![username],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;

    Ok(match row {
        Some((id, username, hash, is_admin)) if verify_password(password, &hash) => Some(Account {
            id,
            username,
            is_admin,
        }),
        _ => None,
    })
}

pub fn create_session(conn: &Connection, account_id: i64) -> Result<String, rusqlite::Error> {
    let token = b64(&generate_salt()) + &b64(&generate_salt()) + &b64(&generate_salt());
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64
        + SESSION_TTL_SECS as i64;
    conn.execute(
        "INSERT INTO web_sessions (token, account_id, expires_at) VALUES (?1, ?2, ?3)",
        params![token, account_id, expires],
    )?;
    Ok(token)
}

pub fn session_account(conn: &Connection, token: &str) -> Result<Option<Account>, rusqlite::Error> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    conn.query_row(
        "SELECT a.id, a.username, a.is_admin
         FROM web_sessions s JOIN web_accounts a ON a.id = s.account_id
         WHERE s.token = ?1 AND s.expires_at > ?2",
        params![token, now],
        |r| {
            Ok(Account {
                id: r.get(0)?,
                username: r.get(1)?,
                is_admin: r.get(2)?,
            })
        },
    )
    .optional()
}

pub fn delete_session(conn: &Connection, token: &str) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM web_sessions WHERE token = ?1", params![token])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_roundtrip() {
        let h = hash_password("correct horse battery staple");
        assert!(verify_password("correct horse battery staple", &h));
        assert!(!verify_password("wrong", &h));
    }

    #[test]
    fn password_hash_is_salted() {
        let a = hash_password("same");
        let b = hash_password("same");
        assert_ne!(
            a, b,
            "identical passwords must not produce identical hashes"
        );
        assert!(verify_password("same", &a));
        assert!(verify_password("same", &b));
    }

    #[test]
    fn base64_roundtrip() {
        for len in 0..40usize {
            let data: Vec<u8> = (0..len).map(|i| i as u8).collect();
            let enc = b64(&data);
            assert_eq!(unb64(&enc).unwrap(), data, "failed at len {len}");
        }
    }
}
