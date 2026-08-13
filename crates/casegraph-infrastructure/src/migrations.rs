//! Source-controlled, checksummed SQLite schema migrations.

use rusqlite::{Connection, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "canonical_foundation",
        sql: include_str!("../migrations/0001_canonical_foundation.sql"),
    },
    Migration {
        version: 2,
        name: "query_indexes_and_domain_registry",
        sql: include_str!("../migrations/0002_query_indexes_and_domain_registry.sql"),
    },
];

struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

/// Migration or database configuration failure.
#[derive(Debug)]
pub enum MigrationError {
    /// SQLite rejected an operation.
    Database(rusqlite::Error),
    /// An already-applied migration no longer matches source control.
    ChecksumMismatch {
        /// Migration version.
        version: u32,
        /// Checksum stored in the database.
        stored: String,
        /// Checksum calculated from the source-controlled migration.
        expected: String,
    },
    /// The database is newer than this binary or requested target.
    UnsupportedVersion {
        /// Version found in the database.
        found: u32,
        /// Highest version supported by this operation.
        supported: u32,
    },
    /// System time was unavailable while recording migration history.
    Clock,
}

impl Display for MigrationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => write!(f, "database migration failed: {error}"),
            Self::ChecksumMismatch {
                version,
                stored,
                expected,
            } => write!(
                f,
                "migration {version} checksum mismatch (stored {stored}, source {expected})"
            ),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "database schema version {found} is newer than supported version {supported}"
            ),
            Self::Clock => f.write_str("system clock is before the Unix epoch"),
        }
    }
}

impl std::error::Error for MigrationError {}

impl From<rusqlite::Error> for MigrationError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value)
    }
}

/// Open and configure a durable SQLite database, then apply all migrations.
pub fn open_database(path: &Path) -> Result<Connection, MigrationError> {
    let mut connection = Connection::open(path)?;
    configure(&connection)?;
    migrate(&mut connection)?;
    Ok(connection)
}

/// Configure integrity, concurrency, and durability settings for every connection.
pub fn configure(connection: &Connection) -> Result<(), MigrationError> {
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;\
         PRAGMA journal_mode = WAL;\
         PRAGMA synchronous = FULL;\
         PRAGMA busy_timeout = 5000;\
         PRAGMA trusted_schema = OFF;",
    )?;
    Ok(())
}

/// Apply every source-controlled migration and verify prior checksums.
pub fn migrate(connection: &mut Connection) -> Result<(), MigrationError> {
    let latest = MIGRATIONS.last().map_or(0, |migration| migration.version);
    migrate_to(connection, latest)
}

/// Apply migrations up to an explicit version. Used to test supported upgrades.
pub fn migrate_to(connection: &mut Connection, target: u32) -> Result<(), MigrationError> {
    let latest = MIGRATIONS.last().map_or(0, |migration| migration.version);
    if target > latest {
        return Err(MigrationError::UnsupportedVersion {
            found: target,
            supported: latest,
        });
    }

    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (\
             version INTEGER PRIMARY KEY CHECK (version > 0),\
             name TEXT NOT NULL UNIQUE,\
             checksum_sha256 TEXT NOT NULL CHECK (length(checksum_sha256) = 64),\
             applied_at_ms INTEGER NOT NULL\
         ) STRICT;",
    )?;

    let current = current_version(connection)?;
    if current > target {
        return Err(MigrationError::UnsupportedVersion {
            found: current,
            supported: target,
        });
    }

    for migration in MIGRATIONS {
        let expected = checksum(migration.sql);
        let stored = connection
            .query_row(
                "SELECT checksum_sha256 FROM schema_migrations WHERE version = ?1",
                [migration.version],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        if let Some(stored) = stored {
            if stored != expected {
                return Err(MigrationError::ChecksumMismatch {
                    version: migration.version,
                    stored,
                    expected,
                });
            }
            continue;
        }

        if migration.version <= target {
            apply(connection, migration, &expected)?;
        }
    }
    Ok(())
}

/// Return the latest recorded schema version.
pub fn current_version(connection: &Connection) -> Result<u32, MigrationError> {
    let version = connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get::<_, u32>(0),
    )?;
    Ok(version)
}

fn apply(
    connection: &mut Connection,
    migration: &Migration,
    checksum_sha256: &str,
) -> Result<(), MigrationError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(migration.sql)?;
    record_migration(&transaction, migration, checksum_sha256)?;
    transaction.commit()?;
    Ok(())
}

fn record_migration(
    transaction: &Transaction<'_>,
    migration: &Migration,
    checksum_sha256: &str,
) -> Result<(), MigrationError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MigrationError::Clock)?;
    let millis = i64::try_from(elapsed.as_millis()).map_err(|_| MigrationError::Clock)?;
    transaction.execute(
        "INSERT INTO schema_migrations(version, name, checksum_sha256, applied_at_ms)\
         VALUES (?1, ?2, ?3, ?4)",
        (migration.version, migration.name, checksum_sha256, millis),
    )?;
    Ok(())
}

fn checksum(contents: &str) -> String {
    let digest = Sha256::digest(contents.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::{current_version, migrate, migrate_to};
    use rusqlite::Connection;

    #[test]
    fn clean_database_migrates_to_latest_and_is_idempotent() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        migrate(&mut connection).expect("clean migration should pass");
        assert_eq!(current_version(&connection).expect("version"), 2);
        migrate(&mut connection).expect("repeat migration should be idempotent");
        assert_eq!(current_version(&connection).expect("version"), 2);
    }

    #[test]
    fn supported_upgrade_path_from_v1_to_v2_is_tested() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        migrate_to(&mut connection, 1).expect("version one should apply");
        assert_eq!(current_version(&connection).expect("version"), 1);
        migrate(&mut connection).expect("upgrade should apply");
        assert_eq!(current_version(&connection).expect("version"), 2);
        let package_table: String = connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'domain_packages'",
                [],
                |row| row.get(0),
            )
            .expect("v2 table should exist");
        assert_eq!(package_table, "domain_packages");
    }

    #[test]
    fn changed_applied_migration_is_rejected() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        migrate(&mut connection).expect("migration should pass");
        connection
            .execute(
                "UPDATE schema_migrations SET checksum_sha256 = ?1 WHERE version = 1",
                ["0".repeat(64)],
            )
            .expect("test should tamper with history");
        let error = migrate(&mut connection).expect_err("drift must be rejected");
        assert!(error.to_string().contains("checksum mismatch"));
    }

    #[test]
    fn artifact_versions_are_immutable_at_the_database_boundary() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        migrate(&mut connection).expect("migration should pass");
        connection
            .execute_batch(
                "INSERT INTO cases(id, title, status, created_at_ms) VALUES ('case_1', 'Case', 'open', 1);\
                 INSERT INTO sources(id, case_id, connector, locator, retrieved_at_ms)\
                   VALUES ('source_1', 'case_1', 'filesystem', 'fixture.txt', 1);\
                 INSERT INTO artifacts(id, case_id, source_id, source_key, created_at_ms)\
                   VALUES ('artifact_1', 'case_1', 'source_1', 'fixture.txt', 1);\
                 INSERT INTO artifact_versions(\
                   id, artifact_id, version_number, content_sha256, content_length, media_type,\
                   storage_key, ingested_at_ms, metadata_json\
                 ) VALUES (\
                   'version_1', 'artifact_1', 1,\
                   'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',\
                   5, 'text/plain', 'aa/hash', 1, '{}'\
                 );",
            )
            .expect("fixture should insert");
        let result = connection.execute(
            "UPDATE artifact_versions SET content_length = 6 WHERE id = 'version_1'",
            [],
        );
        assert!(result.is_err(), "artifact version update must be blocked");
    }
}
