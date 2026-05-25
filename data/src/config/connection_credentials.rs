use serde::{Deserialize, Serialize};
use std::fmt;

const KEYCHAIN_SERVICE: &str = "flowsurface.connection";

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
) -> Result<(), String> {
    let account = reference.keyring_account();
    let payload = serde_json::to_string(secret)
        .map_err(|error| format!("Failed to serialize connection secret: {error}"))?;

    entry_for(reference)
        .map_err(|error| {
            format!("Keychain entry init failed for service={KEYCHAIN_SERVICE} account={account}: {error}")
        })?
        .set_password(&payload)
        .map_err(|error| {
            format!("Failed to store connection secret for service={KEYCHAIN_SERVICE} account={account}: {error}")
        })
}

pub fn load_connection_secret(
    reference: &ConnectionCredentialRef,
) -> Result<Option<ConnectionSecret>, String> {
    let account = reference.keyring_account();
    let entry = entry_for(reference).map_err(|error| {
        format!(
            "Keychain entry init failed for service={KEYCHAIN_SERVICE} account={account}: {error}"
        )
    })?;

    let payload = match entry.get_password() {
        Ok(payload) => payload,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Failed to read connection secret for service={KEYCHAIN_SERVICE} account={account}: {error}"
            ));
        }
    };

    serde_json::from_str(&payload)
        .map(Some)
        .map_err(|error| {
            format!("Stored connection secret is invalid for service={KEYCHAIN_SERVICE} account={account}: {error}")
        })
}

pub fn delete_connection_secret(reference: &ConnectionCredentialRef) -> Result<(), String> {
    let account = reference.keyring_account();
    let entry = entry_for(reference).map_err(|error| {
        format!(
            "Keychain entry init failed for service={KEYCHAIN_SERVICE} account={account}: {error}"
        )
    })?;

    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!(
            "Failed to delete connection secret for service={KEYCHAIN_SERVICE} account={account}: {error}"
        )),
    }
}

fn entry_for(reference: &ConnectionCredentialRef) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &reference.keyring_account())
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

#[cfg(test)]
mod tests {
    use super::{ConnectionCredentialRef, ConnectionSecret};

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
}
