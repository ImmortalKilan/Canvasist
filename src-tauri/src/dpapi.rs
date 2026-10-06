//! Windows Data Protection API (DPAPI) wrapper.
//!
//! DPAPI encrypts data with a key derived from the current Windows user's
//! credentials, so only the same user on the same machine can decrypt it.
//! This is the only module in the crate that is allowed to use `unsafe`: the
//! API is only reachable through FFI. Every unsafe block documents why it is sound.

#![allow(unsafe_code)]

use std::io;

/// Application-specific entropy. Other programs running as the same user cannot
/// decrypt our data with a plain `CryptUnprotectData` call unless they also know it.
const ENTROPY: &[u8] = b"io.github.immortalkilan.canvasist/dpapi/v1";

#[cfg(windows)]
mod imp {
    use std::{io, ptr};

    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    fn input_blob(data: &[u8]) -> io::Result<CRYPT_INTEGER_BLOB> {
        let len = u32::try_from(data.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "data too large for DPAPI"))?;
        Ok(CRYPT_INTEGER_BLOB {
            cbData: len,
            // DPAPI takes a mutable pointer but never writes through input blobs.
            pbData: data.as_ptr().cast_mut(),
        })
    }

    /// Copies an OS-allocated output blob into a `Vec` and frees the original.
    fn take_output(blob: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        if blob.pbData.is_null() {
            return Vec::new();
        }
        // SAFETY: on success DPAPI sets `pbData` to a LocalAlloc'd buffer of exactly
        // `cbData` bytes that we own; it stays valid until LocalFree below.
        let bytes =
            unsafe { std::slice::from_raw_parts(blob.pbData, blob.cbData as usize) }.to_vec();
        // SAFETY: `pbData` was allocated by DPAPI with LocalAlloc and is freed exactly once.
        unsafe { LocalFree(blob.pbData.cast()) };
        bytes
    }

    pub fn protect(plaintext: &[u8], entropy: &[u8]) -> io::Result<Vec<u8>> {
        let input = input_blob(plaintext)?;
        let entropy = input_blob(entropy)?;
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };
        // SAFETY: `input` and `entropy` point into slices that outlive the call and are
        // only read. Null description/reserved/prompt pointers are documented as
        // allowed, and UI_FORBIDDEN prevents any interactive prompt.
        let ok = unsafe {
            CryptProtectData(
                &input,
                ptr::null(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(take_output(output))
    }

    pub fn unprotect(ciphertext: &[u8], entropy: &[u8]) -> io::Result<Vec<u8>> {
        let input = input_blob(ciphertext)?;
        let entropy = input_blob(entropy)?;
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };
        // SAFETY: same contract as `protect`; the optional description out-pointer is null.
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(take_output(output))
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;

    pub fn protect(_: &[u8], _: &[u8]) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "DPAPI is Windows-only",
        ))
    }

    pub fn unprotect(_: &[u8], _: &[u8]) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "DPAPI is Windows-only",
        ))
    }
}

/// Encrypts `plaintext` for the current Windows user.
pub fn protect(plaintext: &[u8]) -> io::Result<Vec<u8>> {
    imp::protect(plaintext, ENTROPY)
}

/// Decrypts data produced by [`protect`] for the current Windows user.
pub fn unprotect(ciphertext: &[u8]) -> io::Result<Vec<u8>> {
    imp::unprotect(ciphertext, ENTROPY)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let secret = b"session=abc123; other=value";
        let encrypted = protect(secret).unwrap();
        assert_ne!(encrypted.as_slice(), secret.as_slice());
        assert_eq!(unprotect(&encrypted).unwrap(), secret);
    }

    #[test]
    fn empty_input_round_trips() {
        let encrypted = protect(b"").unwrap();
        assert_eq!(unprotect(&encrypted).unwrap(), b"");
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let mut encrypted = protect(b"secret").unwrap();
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;
        assert!(unprotect(&encrypted).is_err());
    }

    #[test]
    fn wrong_entropy_is_rejected() {
        let encrypted = protect(b"secret").unwrap();
        assert!(imp::unprotect(&encrypted, b"some other app").is_err());
    }
}
