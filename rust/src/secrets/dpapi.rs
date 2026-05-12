//! Thin wrapper around the Windows DPAPI for byte slices, plus a string
//! envelope `dpapi:v1:<base64>`.
//!
//! The user scope is enforced by passing `CRYPTPROTECT_UI_FORBIDDEN` and
//! omitting `CRYPTPROTECT_LOCAL_MACHINE`. The blob can be unwrapped only by
//! the same Windows user account that wrapped it. On non Windows targets
//! every call returns `SecretsError::Dpapi { code: -1 }` so the test suite
//! still type checks on the optional Linux CI job.

use super::errors::SecretsError;

use base64::Engine;

pub const ENVELOPE_PREFIX: &str = "dpapi:v1:";

#[cfg(windows)]
const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

#[cfg(windows)]
pub fn dpapi_protect(input: &[u8]) -> Result<Vec<u8>, SecretsError> {
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Foundation::HLOCAL;
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};

    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    let ok = unsafe {
        CryptProtectData(
            &in_blob,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
    };
    if ok.is_err() {
        return Err(SecretsError::Dpapi {
            code: unsafe { windows::Win32::Foundation::GetLastError().0 as i32 },
        });
    }

    let bytes =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();
    unsafe {
        let _ = LocalFree(Some(HLOCAL(out_blob.pbData.cast())));
    }
    Ok(bytes)
}

#[cfg(windows)]
pub fn dpapi_unprotect(blob: &[u8]) -> Result<Vec<u8>, SecretsError> {
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Foundation::HLOCAL;
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: blob.len() as u32,
        pbData: blob.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    let ok = unsafe {
        CryptUnprotectData(
            &in_blob,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
    };
    if ok.is_err() {
        return Err(SecretsError::Dpapi {
            code: unsafe { windows::Win32::Foundation::GetLastError().0 as i32 },
        });
    }

    let bytes =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();
    unsafe {
        let _ = LocalFree(Some(HLOCAL(out_blob.pbData.cast())));
    }
    Ok(bytes)
}

#[cfg(not(windows))]
pub fn dpapi_protect(_input: &[u8]) -> Result<Vec<u8>, SecretsError> {
    Err(SecretsError::Dpapi { code: -1 })
}

#[cfg(not(windows))]
pub fn dpapi_unprotect(_blob: &[u8]) -> Result<Vec<u8>, SecretsError> {
    Err(SecretsError::Dpapi { code: -1 })
}

/// Wrap a string into `dpapi:v1:<base64>` envelope.
pub fn wrap_string(value: &str) -> Result<String, SecretsError> {
    let bytes = dpapi_protect(value.as_bytes())?;
    let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(bytes);
    Ok(format!("{ENVELOPE_PREFIX}{b64}"))
}

/// Unwrap a `dpapi:v1:<base64>` envelope back into a string.
pub fn unwrap_string(envelope: &str) -> Result<String, SecretsError> {
    let payload = envelope
        .strip_prefix(ENVELOPE_PREFIX)
        .ok_or(SecretsError::BadEnvelope)?;
    let bytes = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(payload)
        .map_err(|_| SecretsError::Base64Decode)?;
    let raw = dpapi_unprotect(&bytes)?;
    String::from_utf8(raw).map_err(|_| SecretsError::BadEnvelope)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn protect_unprotect_round_trips_empty_input() {
        let blob = dpapi_protect(&[]).expect("protect empty");
        let out = dpapi_unprotect(&blob).expect("unprotect empty");
        assert!(out.is_empty());
    }

    #[test]
    fn protect_unprotect_round_trips_random_kb_blob() {
        let mut input = vec![0u8; 1024];
        for (i, b) in input.iter_mut().enumerate() {
            *b = ((i * 7) ^ 0xa5) as u8;
        }
        let blob = dpapi_protect(&input).expect("protect");
        let out = dpapi_unprotect(&blob).expect("unprotect");
        assert_eq!(out, input);
    }

    #[test]
    fn envelope_round_trips_a_string_containing_envelope_prefix() {
        let original = "dpapi:v1:inner-prefix-collision-test";
        let wrapped = wrap_string(original).expect("wrap");
        assert!(wrapped.starts_with(ENVELOPE_PREFIX));
        let back = unwrap_string(&wrapped).expect("unwrap");
        assert_eq!(back, original);
    }

    #[test]
    fn unwrap_rejects_blob_without_envelope_prefix() {
        let err = unwrap_string("not-an-envelope").unwrap_err();
        assert!(matches!(err, SecretsError::BadEnvelope));
    }

    #[test]
    fn unwrap_rejects_bad_base64() {
        let err = unwrap_string("dpapi:v1:not!!base64!!").unwrap_err();
        assert!(matches!(err, SecretsError::Base64Decode));
    }
}
