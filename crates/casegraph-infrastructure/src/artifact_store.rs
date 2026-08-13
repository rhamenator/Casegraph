//! Confined, content-addressed, immutable filesystem artifact storage.

use casegraph_application::{AppError, ArtifactStore, ErrorKind, StoredArtifact};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A single-root filesystem store whose paths derive only from SHA-256 digests.
#[derive(Clone, Debug)]
pub struct FilesystemArtifactStore {
    canonical_root: PathBuf,
}

impl FilesystemArtifactStore {
    /// Create or open a dedicated store root.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, AppError> {
        let root = root.as_ref();
        if root.as_os_str().is_empty() || is_root_path(root) {
            return Err(AppError::new(
                ErrorKind::InvalidInput,
                "artifact store root must be a dedicated non-root directory",
            ));
        }
        fs::create_dir_all(root.join("blobs")).map_err(|_| storage_error("create store root"))?;
        let canonical_root =
            fs::canonicalize(root).map_err(|_| storage_error("resolve artifact store root"))?;
        Ok(Self { canonical_root })
    }

    fn path_for_hash(&self, hash: &str) -> Result<(String, PathBuf), AppError> {
        validate_hash(hash)?;
        let relative = format!("blobs/{}/{}", &hash[..2], hash);
        let path = self
            .canonical_root
            .join("blobs")
            .join(&hash[..2])
            .join(hash);
        Ok((relative, path))
    }

    fn verify_existing(path: &Path, expected_hash: &str) -> Result<u64, AppError> {
        let mut file = File::open(path).map_err(|_| storage_error("read stored artifact"))?;
        let mut hasher = Sha256::new();
        let mut length = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = file
                .read(&mut buffer)
                .map_err(|_| storage_error("read stored artifact"))?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
            length = length
                .checked_add(u64::try_from(count).map_err(|_| {
                    AppError::new(ErrorKind::Storage, "stored artifact length overflow")
                })?)
                .ok_or_else(|| {
                    AppError::new(ErrorKind::Storage, "stored artifact length overflow")
                })?;
        }
        let actual = hex_digest(hasher.finalize().as_slice());
        if actual != expected_hash {
            return Err(AppError::new(
                ErrorKind::Storage,
                "stored artifact failed content-address integrity verification",
            ));
        }
        Ok(length)
    }
}

impl ArtifactStore for FilesystemArtifactStore {
    fn put(&self, bytes: &[u8]) -> Result<StoredArtifact, AppError> {
        let hash = hex_digest(&Sha256::digest(bytes));
        let (storage_key, destination) = self.path_for_hash(&hash)?;
        let content_length = u64::try_from(bytes.len())
            .map_err(|_| AppError::new(ErrorKind::TooLarge, "artifact length overflow"))?;

        if destination.exists() {
            let existing_length = Self::verify_existing(&destination, &hash)?;
            if existing_length != content_length {
                return Err(AppError::new(
                    ErrorKind::Storage,
                    "stored artifact has inconsistent length",
                ));
            }
            return Ok(StoredArtifact {
                content_sha256: hash,
                content_length,
                storage_key,
                already_existed: true,
            });
        }

        let parent = destination
            .parent()
            .ok_or_else(|| storage_error("resolve artifact destination"))?;
        fs::create_dir_all(parent)
            .map_err(|_| storage_error("create artifact content directory"))?;
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(".{}.{}.{}.tmp", hash, std::process::id(), sequence));
        let write_result = (|| -> Result<(), AppError> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|_| storage_error("create temporary artifact"))?;
            file.write_all(bytes)
                .map_err(|_| storage_error("write artifact"))?;
            file.sync_all()
                .map_err(|_| storage_error("flush artifact"))?;
            drop(file);
            match fs::rename(&temporary, &destination) {
                Ok(()) => Ok(()),
                Err(_) if destination.exists() => {
                    fs::remove_file(&temporary).ok();
                    Self::verify_existing(&destination, &hash).map(|_| ())
                }
                Err(_) => Err(storage_error("commit artifact atomically")),
            }
        })();
        if write_result.is_err() {
            fs::remove_file(&temporary).ok();
        }
        write_result?;

        Ok(StoredArtifact {
            content_sha256: hash,
            content_length,
            storage_key,
            already_existed: false,
        })
    }

    fn read(&self, storage_key: &str) -> Result<Vec<u8>, AppError> {
        let hash = hash_from_storage_key(storage_key)?;
        let (_, path) = self.path_for_hash(hash)?;
        let canonical_path = fs::canonicalize(&path)
            .map_err(|_| AppError::new(ErrorKind::NotFound, "artifact bytes were not found"))?;
        if !canonical_path.starts_with(&self.canonical_root) {
            return Err(AppError::new(
                ErrorKind::InvalidInput,
                "artifact storage key escaped the configured root",
            ));
        }
        Self::verify_existing(&canonical_path, hash)?;
        fs::read(canonical_path).map_err(|_| storage_error("read artifact bytes"))
    }
}

fn hash_from_storage_key(storage_key: &str) -> Result<&str, AppError> {
    if storage_key.len() != 73
        || !storage_key.starts_with("blobs/")
        || storage_key.as_bytes()[8] != b'/'
    {
        return Err(AppError::new(
            ErrorKind::InvalidInput,
            "artifact storage key is malformed",
        ));
    }
    let prefix = &storage_key[6..8];
    let hash = &storage_key[9..];
    validate_hash(hash)?;
    if prefix != &hash[..2] {
        return Err(AppError::new(
            ErrorKind::InvalidInput,
            "artifact storage key prefix does not match its digest",
        ));
    }
    Ok(hash)
}

fn validate_hash(hash: &str) -> Result<(), AppError> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::new(
            ErrorKind::InvalidInput,
            "artifact digest must be lowercase SHA-256 hexadecimal",
        ));
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn storage_error(operation: &'static str) -> AppError {
    AppError::new(ErrorKind::Storage, format!("could not {operation}"))
}

fn is_root_path(path: &Path) -> bool {
    let mut components = path.components();
    match components.next() {
        Some(Component::RootDir) => components.next().is_none(),
        Some(Component::Prefix(_)) => {
            matches!(components.next(), Some(Component::RootDir)) && components.next().is_none()
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::FilesystemArtifactStore;
    use casegraph_application::ArtifactStore;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_directory() -> PathBuf {
        let value = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "casegraph-artifact-test-{}-{value}",
            std::process::id()
        ))
    }

    #[test]
    fn exact_bytes_round_trip_and_duplicate_is_not_overwritten() {
        let root = test_directory();
        let store = FilesystemArtifactStore::new(&root).expect("store");
        let first = store.put(b"synthetic evidence\n").expect("store bytes");
        assert!(!first.already_existed);
        let second = store.put(b"synthetic evidence\n").expect("deduplicate");
        assert!(second.already_existed);
        assert_eq!(first.content_sha256, second.content_sha256);
        assert_eq!(first.content_length, second.content_length);
        assert_eq!(first.storage_key, second.storage_key);
        assert_eq!(
            store.read(&first.storage_key).expect("read bytes"),
            b"synthetic evidence\n"
        );
        fs::remove_dir_all(&root).expect("remove isolated test directory");
    }

    #[test]
    fn path_traversal_and_malformed_internal_keys_are_rejected() {
        let root = test_directory();
        let store = FilesystemArtifactStore::new(&root).expect("store");
        assert!(store.read("../../secret").is_err());
        assert!(store.read(&format!("blobs/ff/{}", "a".repeat(64))).is_err());
        fs::remove_dir_all(&root).expect("remove isolated test directory");
    }

    #[test]
    fn corrupted_existing_content_is_detected_not_silently_replaced() {
        let root = test_directory();
        let store = FilesystemArtifactStore::new(&root).expect("store");
        let stored = store.put(b"original").expect("store bytes");
        let path = root.join(
            stored
                .storage_key
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        fs::write(path, b"tampered").expect("tamper fixture");
        assert!(store.put(b"original").is_err());
        assert!(store.read(&stored.storage_key).is_err());
        fs::remove_dir_all(&root).expect("remove isolated test directory");
    }
}
