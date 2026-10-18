//! Host services the portable core may use, and nothing else.
//!
//! The desktop host implements these with the OS filesystem, HTTP client,
//! clock, and randomness. The web host implements them over a dropped folder
//! held in memory, `fetch` (synchronous requests are allowed in a Web
//! Worker), `Date.now`, and the Web Crypto API. Tests use in-memory fakes.

use crate::AppError;

/// What a project tree entry is. Links are reported, never followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Link,
}

/// One entry of a project tree, named relative to the project root with `/`
/// separators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsEntry {
    pub path: String,
    pub kind: EntryKind,
    pub size: u64,
}

/// Read-only, bounded access to one project tree.
pub trait ProjectFs {
    /// The immediate children of `directory` (`""` is the project root),
    /// sorted by path.
    fn list(&self, directory: &str) -> Result<Vec<FsEntry>, AppError>;
    /// The bytes of a regular file, refused when larger than `limit`.
    fn read(&self, path: &str, limit: u64) -> Result<Vec<u8>, AppError>;
    /// At most the first `max` bytes of a regular file, and its full size.
    fn read_prefix(&self, path: &str, max: usize) -> Result<(Vec<u8>, u64), AppError>;
}

/// HTTP GET of an approved source URL, refused when the body exceeds `limit`.
pub trait SourceClient {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, AppError>;
}

/// Wall-clock time. Monotonic deadlines are derived from it by the core.
pub trait Clock {
    fn now_unix_millis(&self) -> u64;
}

/// Cryptographically secure random bytes, for identifiers.
pub trait Entropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), AppError>;
}

/// A random RFC 4122 version 4 identifier from the host's entropy.
pub fn uuid_v4(entropy: &dyn Entropy) -> Result<String, AppError> {
    let mut bytes = [0_u8; 16];
    entropy.fill(&mut bytes)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter(std::cell::Cell<u8>);
    impl Entropy for Counter {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), AppError> {
            for byte in bytes {
                *byte = self.0.get();
                self.0.set(self.0.get().wrapping_add(1));
            }
            Ok(())
        }
    }

    #[test]
    fn uuid_v4_sets_the_version_and_variant_bits() {
        let id = uuid_v4(&Counter(std::cell::Cell::new(0))).unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"));
        assert_eq!(id, "00010203-0405-4607-8809-0a0b0c0d0e0f");
    }
}
