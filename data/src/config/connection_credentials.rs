use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, num::NonZeroU32};

const CREDENTIAL_VAULT_FILE: &str = "connection-secrets.json";
const CREDENTIAL_VAULT_VERSION: u8 = 1;
const KDF_ITERATIONS: u32 = 210_000;
const KEY_LEN: usize = 32;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionCredentialRef {
    id: String,
}

impl ConnectionCredentialRef {
    pub fn new(id: impl AsRef<str>) -> Result<Self, String> {
        let id = normalize_connection_id(id.as_ref());

        if id.is_empty() {
            return Err("Connection credential id cannot be empty".to_string());
        }

        Ok(Self { id })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn keyring_account(&self) -> String {
        format!("connection:{}", self.id)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionSecret {
    access_key: String,
    secret_key: String,
}

impl ConnectionSecret {
    pub fn new(
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
    ) -> Result<Self, String> {
        let access_key = access_key.into().trim().to_string();
        let secret_key = secret_key.into().trim().to_string();

        validate_secret_field("access key", &access_key)?;
        validate_secret_field("secret key", &secret_key)?;

        Ok(Self {
            access_key,
            secret_key,
        })
    }

    pub fn access_key(&self) -> &str {
        &self.access_key
    }

    pub fn secret_key(&self) -> &str {
        &self.secret_key
    }

    pub fn access_key_hint(&self) -> String {
        masked_value(&self.access_key)
    }
}

impl fmt::Debug for ConnectionSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionSecret")
            .field("access_key", &self.access_key_hint())
            .field("secret_key", &"<redacted>")
            .finish()
    }
}

pub fn save_connection_secret(
    reference: &ConnectionCredentialRef,
    secret: &ConnectionSecret,
    vault_key: &str,
) -> Result<(), String> {
    let account = reference.keyring_account();
    let payload = serde_json::to_string(secret)
        .map_err(|error| format!("Failed to serialize connection secret: {error}"))?;
    let vault_key = normalize_vault_key(vault_key)?;

    let mut vault = load_vault()?;
    vault.entries.insert(
        account.clone(),
        encrypt_payload(
            &account,
            payload.as_bytes(),
            &vault_key,
            secret.access_key_hint(),
        )?,
    );
    write_vault(&vault)
}

pub fn load_connection_secret(
    reference: &ConnectionCredentialRef,
    vault_key: &str,
) -> Result<Option<ConnectionSecret>, String> {
    let account = reference.keyring_account();
    let vault_key = normalize_vault_key(vault_key)?;
    let vault = load_vault()?;
    let Some(entry) = vault.entries.get(&account) else {
        return Ok(None);
    };

    let payload = decrypt_payload(&account, entry, &vault_key)?;
    let payload = String::from_utf8(payload)
        .map_err(|error| format!("Stored connection secret is not valid UTF-8: {error}"))?;

    serde_json::from_str(&payload).map(Some).map_err(|error| {
        format!("Stored connection secret is invalid for account={account}: {error}")
    })
}

pub fn delete_connection_secret(reference: &ConnectionCredentialRef) -> Result<(), String> {
    let account = reference.keyring_account();
    let mut vault = load_vault()?;
    vault.entries.remove(&account);
    write_vault(&vault)
}

fn normalize_connection_id(raw: &str) -> String {
    let mut normalized = String::new();
    let mut previous_dash = false;

    for ch in raw.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            previous_dash = false;
        } else if !previous_dash && !normalized.is_empty() {
            normalized.push('-');
            previous_dash = true;
        }
    }

    if normalized.ends_with('-') {
        normalized.pop();
    }

    normalized
}

fn validate_secret_field(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("Connection {label} cannot be empty"));
    }

    if value.contains('\n') || value.contains('\r') {
        return Err(format!("Connection {label} cannot contain line breaks"));
    }

    Ok(())
}

fn normalize_vault_key(raw: &str) -> Result<String, String> {
    let vault_key = raw.trim().to_string();
    validate_secret_field("local credential passphrase", &vault_key)?;
    Ok(vault_key)
}

fn masked_value(value: &str) -> String {
    let len = value.chars().count();
    if len <= 4 {
        return "****".to_string();
    }

    let suffix = value
        .chars()
        .skip(len.saturating_sub(4))
        .collect::<String>();
    format!("****{suffix}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CredentialVault {
    version: u8,
    entries: BTreeMap<String, StoredConnectionSecret>,
}

impl Default for CredentialVault {
    fn default() -> Self {
        Self {
            version: CREDENTIAL_VAULT_VERSION,
            entries: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredConnectionSecret {
    kdf: String,
    iterations: u32,
    salt_hex: String,
    nonce_hex: String,
    ciphertext_hex: String,
    access_key_hint: String,
}

fn load_vault() -> Result<CredentialVault, String> {
    let path = crate::data_path(Some(CREDENTIAL_VAULT_FILE));
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CredentialVault::default());
        }
        Err(error) => return Err(format!("Failed to read local credential vault: {error}")),
    };

    if contents.trim().is_empty() {
        return Ok(CredentialVault::default());
    }

    let vault = serde_json::from_str::<CredentialVault>(&contents)
        .map_err(|error| format!("Local credential vault is invalid: {error}"))?;
    if vault.version != CREDENTIAL_VAULT_VERSION {
        return Err(format!(
            "Unsupported local credential vault version {}",
            vault.version
        ));
    }

    Ok(vault)
}

fn write_vault(vault: &CredentialVault) -> Result<(), String> {
    let json = serde_json::to_string_pretty(vault)
        .map_err(|error| format!("Failed to serialize local credential vault: {error}"))?;
    crate::write_json_to_file(&json, CREDENTIAL_VAULT_FILE)
        .map_err(|error| format!("Failed to write local credential vault: {error}"))
}

fn encrypt_payload(
    account: &str,
    payload: &[u8],
    vault_key: &str,
    access_key_hint: String,
) -> Result<StoredConnectionSecret, String> {
    let rng = ring::rand::SystemRandom::new();
    let mut salt = [0_u8; SALT_LEN];
    let mut nonce = [0_u8; NONCE_LEN];
    ring::rand::SecureRandom::fill(&rng, &mut salt)
        .map_err(|_| "Failed to generate credential vault salt".to_string())?;
    ring::rand::SecureRandom::fill(&rng, &mut nonce)
        .map_err(|_| "Failed to generate credential vault nonce".to_string())?;

    let key = derive_key(vault_key, &salt, KDF_ITERATIONS)?;
    let mut in_out = payload.to_vec();
    let sealing_key = less_safe_key(&key)?;
    sealing_key
        .seal_in_place_append_tag(
            ring::aead::Nonce::assume_unique_for_key(nonce),
            ring::aead::Aad::from(account.as_bytes()),
            &mut in_out,
        )
        .map_err(|_| "Failed to encrypt connection secret".to_string())?;

    Ok(StoredConnectionSecret {
        kdf: "PBKDF2-HMAC-SHA256+A256GCM".to_string(),
        iterations: KDF_ITERATIONS,
        salt_hex: hex_encode(&salt),
        nonce_hex: hex_encode(&nonce),
        ciphertext_hex: hex_encode(&in_out),
        access_key_hint,
    })
}

fn decrypt_payload(
    account: &str,
    entry: &StoredConnectionSecret,
    vault_key: &str,
) -> Result<Vec<u8>, String> {
    if entry.kdf != "PBKDF2-HMAC-SHA256+A256GCM" {
        return Err(format!("Unsupported credential vault KDF {}", entry.kdf));
    }

    let salt = hex_decode(&entry.salt_hex, "salt")?;
    let nonce = fixed_nonce(hex_decode(&entry.nonce_hex, "nonce")?)?;
    let mut in_out = hex_decode(&entry.ciphertext_hex, "ciphertext")?;
    let key = derive_key(vault_key, &salt, entry.iterations)?;
    let opening_key = less_safe_key(&key)?;
    let payload = opening_key
        .open_in_place(
            ring::aead::Nonce::assume_unique_for_key(nonce),
            ring::aead::Aad::from(account.as_bytes()),
            &mut in_out,
        )
        .map_err(|_| {
            "Unable to decrypt saved API keys. Check the local PIN/passphrase.".to_string()
        })?;

    Ok(payload.to_vec())
}

fn derive_key(vault_key: &str, salt: &[u8], iterations: u32) -> Result<[u8; KEY_LEN], String> {
    let iterations = NonZeroU32::new(iterations)
        .ok_or_else(|| "Credential vault iterations cannot be zero".to_string())?;
    let mut key = [0_u8; KEY_LEN];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        salt,
        vault_key.as_bytes(),
        &mut key,
    );
    Ok(key)
}

fn less_safe_key(key: &[u8; KEY_LEN]) -> Result<ring::aead::LessSafeKey, String> {
    let unbound = ring::aead::UnboundKey::new(&ring::aead::AES_256_GCM, key)
        .map_err(|_| "Failed to initialize credential vault cipher".to_string())?;
    Ok(ring::aead::LessSafeKey::new(unbound))
}

fn fixed_nonce(bytes: Vec<u8>) -> Result<[u8; NONCE_LEN], String> {
    bytes
        .try_into()
        .map_err(|_| "Credential vault nonce has invalid length".to_string())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(value: &str, label: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err(format!("Credential vault {label} has invalid hex length"));
    }

    let mut bytes = Vec::with_capacity(value.len() / 2);
    for chunk in value.as_bytes().chunks_exact(2) {
        let high = hex_nibble(chunk[0])?;
        let low = hex_nibble(chunk[1])?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_nibble(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("Credential vault contains invalid hex".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{ConnectionCredentialRef, ConnectionSecret, decrypt_payload, encrypt_payload};

    #[test]
    fn credential_ref_builds_stable_keyring_account() {
        let reference = ConnectionCredentialRef::new("MEXC spot trade").unwrap();

        assert_eq!(reference.keyring_account(), "connection:mexc-spot-trade");
    }

    #[test]
    fn secret_payload_roundtrips_through_json() {
        let secret = ConnectionSecret::new("access-key-123", "secret-key-456").unwrap();

        let json = serde_json::to_string(&secret).unwrap();
        let restored: ConnectionSecret = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.access_key(), "access-key-123");
        assert_eq!(restored.secret_key(), "secret-key-456");
    }

    #[test]
    fn debug_output_redacts_secret_values() {
        let secret = ConnectionSecret::new("access-key-123", "secret-key-456").unwrap();
        let debug = format!("{secret:?}");

        assert!(debug.contains("access_key"));
        assert!(!debug.contains("access-key-123"));
        assert!(!debug.contains("secret-key-456"));
    }

    #[test]
    fn encrypted_payload_roundtrips_with_passphrase() {
        let account = "connection:mexc-futures-trade";
        let payload = br#"{"access_key":"access-key-123","secret_key":"secret-key-456"}"#;
        let entry = encrypt_payload(account, payload, "local-pin", "****-123".to_string()).unwrap();
        let decrypted = decrypt_payload(account, &entry, "local-pin").unwrap();

        assert_eq!(decrypted, payload);
        assert!(decrypt_payload(account, &entry, "wrong-pin").is_err());
    }
}
