use rusqlite::params;

pub mod association;
pub mod cidr;
pub mod peer;

pub use association::DatabaseAssociation;
pub use cidr::DatabaseCidr;
pub use peer::DatabasePeer;

const INVITE_EXPIRATION_VERSION: usize = 1;
const ENDPOINT_CANDIDATES_VERSION: usize = 2;
const ALLOWED_SUBNETS_VERSION: usize = 3;

pub const CURRENT_VERSION: usize = ALLOWED_SUBNETS_VERSION;

pub fn auto_migrate(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    let old_version: usize = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    log::debug!("user_version: {}", old_version);

    if old_version < INVITE_EXPIRATION_VERSION {
        conn.execute(
            "ALTER TABLE peers ADD COLUMN invite_expires INTEGER",
            params![],
        )?;
    }

    if old_version < ENDPOINT_CANDIDATES_VERSION {
        conn.execute("ALTER TABLE peers ADD COLUMN candidates TEXT", params![])?;
    }

    if old_version < ALLOWED_SUBNETS_VERSION {
        conn.execute(
            "ALTER TABLE peers ADD COLUMN allowed_subnets TEXT NOT NULL DEFAULT '[]'",
            params![],
        )?;
        // SQLite fills pre-existing rows with '' instead of the DEFAULT literal when
        // adding a NOT NULL column, so normalise blanks to a valid JSON array.
        conn.execute(
            "UPDATE peers SET allowed_subnets = '[]' WHERE allowed_subnets IS NULL OR trim(allowed_subnets) = ''",
            params![],
        )?;
    }

    // Web admin tables are additive and independent of the peer schema version.
    crate::web::auth::ensure_tables(conn)?;

    if old_version != CURRENT_VERSION {
        conn.pragma_update(None, "user_version", CURRENT_VERSION)?;
        log::info!(
            "migrated db version from {} to {}",
            old_version,
            CURRENT_VERSION
        );
    }

    Ok(())
}
