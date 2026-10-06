//! Encrypted-at-rest storage for sensitive data (sessions, cached assignments).
//!
//! Each record is serialized to JSON, encrypted with DPAPI and written
//! atomically as `<name>.bin` with a small versioned header.

use std::fs;
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::dpapi;
use crate::error::{AppError, AppResult};

const MAGIC: &[u8] = b"CVST\x01";

pub struct SecureStore {
    dir: PathBuf,
}

impl SecureStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.bin"))
    }

    pub fn save<T: Serialize>(&self, name: &str, value: &T) -> AppResult<()> {
        let plaintext = serde_json::to_vec(value)?;
        let ciphertext = dpapi::protect(&plaintext).map_err(|e| AppError::Crypto(e.to_string()))?;
        let mut bytes = Vec::with_capacity(MAGIC.len() + ciphertext.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&ciphertext);

        fs::create_dir_all(&self.dir)?;
        let path = self.path(name);
        let tmp = path.with_extension("bin.tmp");
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Returns `None` when the record does not exist. A record that cannot be
    /// decrypted or parsed (e.g. copied from another Windows account, or written
    /// by an incompatible version) is deleted and treated as missing.
    pub fn load<T: DeserializeOwned>(&self, name: &str) -> AppResult<Option<T>> {
        let path = self.path(name);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        match decode(&bytes) {
            Ok(value) => Ok(Some(value)),
            Err(e) => {
                log::warn!("discarding unreadable secure record '{name}': {e}");
                let _ = fs::remove_file(&path);
                Ok(None)
            }
        }
    }

    pub fn delete(&self, name: &str) -> AppResult<()> {
        match fs::remove_file(self.path(name)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

fn decode<T: DeserializeOwned>(bytes: &[u8]) -> AppResult<T> {
    let ciphertext = bytes
        .strip_prefix(MAGIC)
        .ok_or_else(|| AppError::Crypto("unknown file format".into()))?;
    let plaintext = dpapi::unprotect(ciphertext).map_err(|e| AppError::Crypto(e.to_string()))?;
    Ok(serde_json::from_slice(&plaintext)?)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Record {
        value: String,
    }

    fn record() -> Record {
        Record {
            value: "sensitive".into(),
        }
    }

    #[test]
    fn save_then_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = SecureStore::new(dir.path().into());
        store.save("item", &record()).unwrap();
        assert_eq!(store.load::<Record>("item").unwrap(), Some(record()));
    }

    #[test]
    fn file_does_not_contain_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let store = SecureStore::new(dir.path().into());
        store.save("item", &record()).unwrap();
        let raw = fs::read(dir.path().join("item.bin")).unwrap();
        assert!(!raw.windows(9).any(|w| w == b"sensitive"));
    }

    #[test]
    fn missing_record_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = SecureStore::new(dir.path().into());
        assert_eq!(store.load::<Record>("absent").unwrap(), None);
    }

    #[test]
    fn corrupt_record_is_discarded() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("item.bin"), b"CVST\x01garbage").unwrap();
        let store = SecureStore::new(dir.path().into());
        assert_eq!(store.load::<Record>("item").unwrap(), None);
        assert!(!dir.path().join("item.bin").exists());
    }

    #[test]
    fn delete_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store = SecureStore::new(dir.path().into());
        store.save("item", &record()).unwrap();
        store.delete("item").unwrap();
        store.delete("item").unwrap();
        assert_eq!(store.load::<Record>("item").unwrap(), None);
    }
}
