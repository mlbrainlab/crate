//! Crate Pro license (M9). Honor system: all code is public, and the key
//! only says who paid. A key is a payload signed with Crate's Ed25519
//! private key (kept offline and in the signer, never in this repository);
//! the app checks it with the public key below, offline.
//!
//! Buyers get a Lemon Squeezy license key. `activate` exchanges it once with
//! the signer (`signer/`) for a signed key; after that nothing goes over the
//! network. A signed key can also be pasted directly (sponsors, keys made
//! with `signer/sign.mjs`).
//!
//! Key: `CRATE-PRO-` + base64url (no padding) of the payload followed by the
//! 64-byte signature. Payload: `1|<id>|<issued, YYYY-MM-DD>`, where the id
//! names the purchase (`ls:<order>`, `gh:<sponsor>`) and nothing else.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{Signature, VerifyingKey};

use crate::settings::Settings;

pub const PREFIX: &str = "CRATE-PRO-";
const SETTING: &str = "pro.key";

/// The public half of ~/crate-license/license-ed25519.pem.
const PUBLIC_KEY: [u8; 32] = [
    0xb3, 0xb7, 0x97, 0x63, 0x69, 0xd6, 0x49, 0x44, 0x79, 0x00, 0xd5, 0xf4, 0x39, 0xb3, 0x6e, 0xac,
    0x5b, 0x74, 0x46, 0x1a, 0x8c, 0x83, 0x62, 0x4f, 0x2e, 0x1d, 0x19, 0x65, 0x4d, 0x73, 0xed, 0xac,
];

/// The signer's address, set at build time by the release workflow (a
/// repository variable). Builds without it can't activate a store key, but
/// still take signed keys.
const ACTIVATE_URL: Option<&str> = option_env!("CRATE_LICENSE_URL");

/// A valid key's contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct License {
    pub id: String,
    pub issued: String,
    /// The key as entered, to save.
    pub key: String,
}

impl License {
    /// The issue date as a time (midnight UTC), for showing it.
    pub fn issued_time(&self) -> Option<std::time::SystemTime> {
        let mut parts = self.issued.split('-').map(|p| p.parse::<i64>().ok());
        let (Some(Some(y)), Some(Some(m)), Some(Some(d))) =
            (parts.next(), parts.next(), parts.next())
        else {
            return None;
        };
        if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
            return None;
        }
        // Howard Hinnant's days_from_civil.
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = u64::try_from(era * 146_097 + doe - 719_468).ok()?;
        Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(days * 86_400))
    }
}

/// Checks a signed key. `None` for anything that isn't one Crate signed.
pub fn verify(key: &str) -> Option<License> {
    verify_with(key, &PUBLIC_KEY)
}

fn verify_with(key: &str, public: &[u8; 32]) -> Option<License> {
    let key = key.trim();
    let bytes = URL_SAFE_NO_PAD.decode(key.strip_prefix(PREFIX)?).ok()?;
    let split = bytes.len().checked_sub(Signature::BYTE_SIZE)?;
    let (payload, sig) = bytes.split_at(split);
    let sig = Signature::from_slice(sig).ok()?;
    VerifyingKey::from_bytes(public)
        .ok()?
        .verify_strict(payload, &sig)
        .ok()?;
    let mut parts = std::str::from_utf8(payload).ok()?.split('|');
    let (Some("1"), Some(id), Some(issued), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    Some(License {
        id: id.to_owned(),
        issued: issued.to_owned(),
        key: key.to_owned(),
    })
}

/// The saved license, if it's still valid.
pub fn current() -> Option<License> {
    verify(&Settings::load().get::<String>(SETTING)?)
}

pub fn is_pro() -> bool {
    current().is_some()
}

/// Saves a valid key; the caller has verified it.
pub fn save(license: &License) {
    let mut s = Settings::load();
    s.set(SETTING, &license.key);
    s.save();
}

/// Forgets the key (Preferences → Remove license).
pub fn remove() {
    let mut s = Settings::load();
    s.set(SETTING, "");
    s.save();
}

/// Why a key wasn't accepted. The UI turns these into sentences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivateError {
    /// Lemon Squeezy doesn't know the key, or it was refunded or disabled.
    Rejected,
    /// The signer couldn't be reached.
    Network(String),
    /// This build has no signer address.
    Unavailable,
}

/// Enters a key: a signed key is checked here; anything else is taken as a
/// Lemon Squeezy key and sent to the signer. Blocks; call it off the UI
/// thread. Saves the license on success.
pub fn activate(entered: &str) -> Result<License, ActivateError> {
    let entered = entered.trim();
    if entered.starts_with(PREFIX) {
        let license = verify(entered).ok_or(ActivateError::Rejected)?;
        save(&license);
        return Ok(license);
    }
    if entered.is_empty() {
        return Err(ActivateError::Rejected);
    }
    let url = ACTIVATE_URL
        .filter(|u| !u.is_empty())
        .ok_or(ActivateError::Unavailable)?;
    let signed = exchange(url, entered)?;
    let license = verify(&signed).ok_or(ActivateError::Rejected)?;
    save(&license);
    Ok(license)
}

/// POST {"license_key": …} → 200 {"key": "CRATE-PRO-…"}; 4xx: not a key we sold.
fn exchange(url: &str, store_key: &str) -> Result<String, ActivateError> {
    let body = serde_json::json!({ "license_key": store_key });
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut response = agent
        .post(url)
        .send_json(&body)
        .map_err(|e| ActivateError::Network(e.to_string()))?;
    let status = response.status().as_u16();
    if (400..500).contains(&status) {
        return Err(ActivateError::Rejected);
    }
    if status != 200 {
        return Err(ActivateError::Network(format!("HTTP {status}")));
    }
    let reply: serde_json::Value = response
        .body_mut()
        .read_json()
        .map_err(|e| ActivateError::Network(e.to_string()))?;
    reply["key"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ActivateError::Network("no key in the reply".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn sign(signing: &SigningKey, payload: &str) -> String {
        let mut bytes = payload.as_bytes().to_vec();
        bytes.extend_from_slice(&signing.sign(payload.as_bytes()).to_bytes());
        format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
    }

    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    #[test]
    fn accepts_a_signed_key() {
        let signing = test_key();
        let key = sign(&signing, "1|ls:12345|2026-10-31");
        let license =
            verify_with(&format!("  {key}\n"), &signing.verifying_key().to_bytes()).expect("valid");
        assert_eq!(license.id, "ls:12345");
        assert_eq!(license.issued, "2026-10-31");
        assert_eq!(license.key, key);
        let secs = license
            .issued_time()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(secs, 1_793_404_800); // 2026-10-31T00:00:00Z
    }

    #[test]
    fn rejects_other_keys() {
        let signing = test_key();
        let public = signing.verifying_key().to_bytes();
        let good = sign(&signing, "1|ls:1|2026-10-31");
        // Signed by someone else.
        let other = sign(&SigningKey::from_bytes(&[8; 32]), "1|ls:1|2026-10-31");
        assert!(verify_with(&other, &public).is_none());
        // One character changed.
        let mut tampered = good.clone().into_bytes();
        let i = PREFIX.len() + 3;
        tampered[i] = if tampered[i] == b'A' { b'B' } else { b'A' };
        assert!(verify_with(std::str::from_utf8(&tampered).unwrap(), &public).is_none());
        // Unknown version, wrong shape, garbage.
        assert!(verify_with(&sign(&signing, "2|ls:1|2026-10-31"), &public).is_none());
        assert!(verify_with(&sign(&signing, "1|ls:1"), &public).is_none());
        assert!(verify_with("CRATE-PRO-", &public).is_none());
        assert!(verify_with("ABCD-1234", &public).is_none());
        // The real public key doesn't take a test key.
        assert!(verify(&good).is_none());
    }

    /// signer/sign.mjs (the same code as the Worker's) makes keys this file
    /// takes. Needs node and openssl; skipped without them.
    #[test]
    fn signer_keys_verify() {
        use std::process::Command;
        let dir = tempfile::tempdir().unwrap();
        let pem = dir.path().join("key.pem");
        let ok = |c: &mut Command| c.status().is_ok_and(|s| s.success());
        if !ok(Command::new("openssl")
            .args(["genpkey", "-algorithm", "ed25519", "-out"])
            .arg(&pem))
        {
            eprintln!("skipped: no openssl");
            return;
        }
        let der = Command::new("openssl")
            .args(["pkey", "-pubout", "-outform", "DER", "-in"])
            .arg(&pem)
            .output()
            .unwrap()
            .stdout;
        let public: [u8; 32] = der[der.len() - 32..].try_into().unwrap();
        let script = concat!(env!("CARGO_MANIFEST_DIR"), "/../signer/sign.mjs");
        let Ok(out) = Command::new("node")
            .arg(script)
            .arg("gh:octocat")
            .arg(&pem)
            .output()
        else {
            eprintln!("skipped: no node");
            return;
        };
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let key = String::from_utf8(out.stdout).unwrap();
        let license = verify_with(&key, &public).expect("sign.mjs key verifies");
        assert_eq!(license.id, "gh:octocat");
        assert_eq!(license.issued.len(), 10);
    }
}
