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

    platform_keychain::save_password(&account, &payload).map_err(|error| {
        format!(
            "Failed to store connection secret for service={KEYCHAIN_SERVICE} account={account}: {error}"
        )
    })
}

pub fn load_connection_secret(
    reference: &ConnectionCredentialRef,
) -> Result<Option<ConnectionSecret>, String> {
    let account = reference.keyring_account();
    let payload = platform_keychain::load_password(&account).map_err(|error| {
        format!(
            "Failed to read connection secret for service={KEYCHAIN_SERVICE} account={account}: {error}"
        )
    })?;

    let Some(payload) = payload else {
        return Ok(None);
    };

    serde_json::from_str(&payload)
        .map(Some)
        .map_err(|error| {
            format!("Stored connection secret is invalid for service={KEYCHAIN_SERVICE} account={account}: {error}")
        })
}

pub fn delete_connection_secret(reference: &ConnectionCredentialRef) -> Result<(), String> {
    let account = reference.keyring_account();
    platform_keychain::delete_password(&account).map_err(|error| {
        format!(
            "Failed to delete connection secret for service={KEYCHAIN_SERVICE} account={account}: {error}"
        )
    })
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

#[cfg(target_os = "macos")]
mod platform_keychain {
    use security_framework::{
        base::Error,
        passwords::{
            AccessControlOptions, PasswordOptions, delete_generic_password_options,
            generic_password, set_generic_password_options,
        },
    };

    use super::KEYCHAIN_SERVICE;

    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

    pub fn save_password(account: &str, payload: &str) -> Result<(), String> {
        // Recreate both possible locations so replacement credentials cannot shadow stale entries.
        // Yes Johnny, keychain metadata also needs a fresh coat of paint sometimes.
        delete_password(account)?;

        let mut options = protected_options(account);
        options.set_access_control_options(AccessControlOptions::USER_PRESENCE);

        set_generic_password_options(payload.as_bytes(), options).map_err(describe_error)
    }

    pub fn load_password(account: &str) -> Result<Option<String>, String> {
        match load_from_options(protected_options(account))? {
            Some(payload) => Ok(Some(payload)),
            None => load_from_options(legacy_options(account)),
        }
    }

    pub fn delete_password(account: &str) -> Result<(), String> {
        delete_from_options(protected_options(account))?;
        delete_from_options(legacy_options(account))
    }

    fn protected_options(account: &str) -> PasswordOptions {
        let mut options = PasswordOptions::new_generic_password(KEYCHAIN_SERVICE, account);
        options.use_protected_keychain();
        options
    }

    fn legacy_options(account: &str) -> PasswordOptions {
        PasswordOptions::new_generic_password(KEYCHAIN_SERVICE, account)
    }

    fn load_from_options(options: PasswordOptions) -> Result<Option<String>, String> {
        match generic_password(options) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|error| format!("Stored keychain payload is not UTF-8: {error}")),
            Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
            Err(error) => Err(describe_error(error)),
        }
    }

    fn delete_from_options(options: PasswordOptions) -> Result<(), String> {
        match delete_generic_password_options(options) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(error) => Err(describe_error(error)),
        }
    }

    fn describe_error(error: Error) -> String {
        format!("macOS keychain error {}: {error}", error.code())
    }
}

#[cfg(not(target_os = "macos"))]
mod platform_keychain {
    use super::KEYCHAIN_SERVICE;

    pub fn save_password(account: &str, payload: &str) -> Result<(), String> {
        entry_for(account)?.set_password(payload).map_err(|error| {
            format!("Failed to store keyring password for account={account}: {error}")
        })
    }

    pub fn load_password(account: &str) -> Result<Option<String>, String> {
        match entry_for(account)?.get_password() {
            Ok(payload) => Ok(Some(payload)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(format!(
                "Failed to read keyring password for account={account}: {error}"
            )),
        }
    }

    pub fn delete_password(account: &str) -> Result<(), String> {
        match entry_for(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(format!(
                "Failed to delete keyring password for account={account}: {error}"
            )),
        }
    }

    fn entry_for(account: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|error| {
            format!(
                "Keyring entry init failed for service={KEYCHAIN_SERVICE} account={account}: {error}"
            )
        })
    }
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
