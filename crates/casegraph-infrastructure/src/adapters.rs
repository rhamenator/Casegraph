//! Standard clock and non-sensitive opaque identifier adapters.

use casegraph_application::{AppError, Clock, ErrorKind, IdGenerator};
use casegraph_domain::{RecordId, TimestampMs};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// System clock adapter.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Result<TimestampMs, AppError> {
        let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| {
            AppError::new(ErrorKind::Internal, "system clock precedes the Unix epoch")
        })?;
        let millis = i64::try_from(elapsed.as_millis()).map_err(|_| {
            AppError::new(
                ErrorKind::Internal,
                "system clock cannot be represented as milliseconds",
            )
        })?;
        TimestampMs::new(millis).map_err(Into::into)
    }
}

/// Process-unique opaque ID generator. IDs do not encode source contents or actor data.
#[derive(Debug, Default)]
pub struct Sha256IdGenerator {
    counter: AtomicU64,
}

impl IdGenerator for Sha256IdGenerator {
    fn next(&self, kind: &'static str) -> Result<RecordId, AppError> {
        if kind.is_empty()
            || kind.len() > 30
            || !kind
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        {
            return Err(AppError::new(
                ErrorKind::Internal,
                "identifier kind is invalid",
            ));
        }
        let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| {
            AppError::new(ErrorKind::Internal, "system clock precedes the Unix epoch")
        })?;
        let sequence = self.counter.fetch_add(1, Ordering::Relaxed);
        let mut hasher = Sha256::new();
        hasher.update(kind.as_bytes());
        hasher.update(elapsed.as_nanos().to_le_bytes());
        hasher.update(std::process::id().to_le_bytes());
        hasher.update(sequence.to_le_bytes());
        let digest = hasher.finalize();
        let suffix = digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        RecordId::parse(format!("{kind}_{suffix}")).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::{Sha256IdGenerator, SystemClock};
    use casegraph_application::{Clock, ErrorKind, IdGenerator};
    use std::collections::HashSet;

    #[test]
    fn generated_ids_are_valid_unique_and_content_free() {
        let generator = Sha256IdGenerator::default();
        let ids = (0..1_000)
            .map(|_| generator.next("claim").expect("id").to_string())
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), 1_000);
        assert!(ids.iter().all(|id| id.starts_with("claim_")));
    }

    #[test]
    fn system_clock_produces_a_non_negative_timestamp() {
        assert!(SystemClock.now().expect("system time").get() > 0);
    }

    #[test]
    fn identifier_kinds_are_strictly_bounded_internal_vocabulary() {
        let generator = Sha256IdGenerator::default();
        for kind in [
            "",
            "UPPER",
            "contains-hyphen",
            "contains digit 1",
            "a_kind_name_that_is_over_thirty_characters",
        ] {
            assert_eq!(
                generator.next(kind).unwrap_err().kind(),
                ErrorKind::Internal
            );
        }
        assert!(generator.next("rule_version").is_ok());
    }
}
