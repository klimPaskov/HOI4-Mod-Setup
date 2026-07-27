use crate::models::{CredentialReference, Platform};
use crate::security::{redact_secrets, validate_env_name};
use crate::AppError;
use std::collections::BTreeMap;
use uuid::Uuid;

pub const MESHY_ENVIRONMENT_NAME: &str = "MESHY_API_KEY";
#[cfg(any(target_os = "windows", target_os = "macos"))]
const KEYRING_SERVICE: &str = "com.klimpaskov.hoi4-mod-setup";

pub trait CredentialStore {
    fn save(&self, name: &str, value: &str) -> Result<CredentialReference, AppError>;
    fn read(&self, reference: &CredentialReference) -> Result<String, AppError>;
    fn delete(&self, reference: &CredentialReference) -> Result<(), AppError>;
}

#[derive(Default, Clone)]
pub struct MemoryCredentialStore {
    values: std::sync::Arc<std::sync::Mutex<BTreeMap<String, String>>>,
}

impl CredentialStore for MemoryCredentialStore {
    fn save(&self, name: &str, value: &str) -> Result<CredentialReference, AppError> {
        validate_secret_input(name, value)?;
        let reference = CredentialReference {
            name: name.into(),
            provider: provider_name(Platform::current()).into(),
            reference: format!(
                "credential://{}/{}",
                name.to_ascii_lowercase(),
                Uuid::new_v4()
            ),
        };
        self.values
            .lock()
            .map_err(|_| AppError::Credential("credential store lock poisoned".into()))?
            .insert(reference.reference.clone(), value.to_string());
        Ok(reference)
    }

    fn read(&self, reference: &CredentialReference) -> Result<String, AppError> {
        self.values
            .lock()
            .map_err(|_| AppError::Credential("credential store lock poisoned".into()))?
            .get(&reference.reference)
            .cloned()
            .ok_or_else(|| AppError::Credential("credential reference is not available".into()))
    }

    fn delete(&self, reference: &CredentialReference) -> Result<(), AppError> {
        self.values
            .lock()
            .map_err(|_| AppError::Credential("credential store lock poisoned".into()))?
            .remove(&reference.reference);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OsCredentialStore;

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl CredentialStore for OsCredentialStore {
    fn save(&self, name: &str, value: &str) -> Result<CredentialReference, AppError> {
        validate_secret_input(name, value)?;
        let platform = Platform::current();
        let reference = CredentialReference {
            name: name.into(),
            provider: provider_name(platform).into(),
            reference: format!(
                "credential://{}/{}",
                name.to_ascii_lowercase(),
                Uuid::new_v4()
            ),
        };
        let entry = keyring::Entry::new(KEYRING_SERVICE, &reference.reference)
            .map_err(|error| AppError::Credential(format!("OS credential entry: {error}")))?;
        entry
            .set_password(value)
            .map_err(|error| AppError::Credential(format!("OS credential write: {error}")))?;
        Ok(reference)
    }

    fn read(&self, reference: &CredentialReference) -> Result<String, AppError> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &reference.reference)
            .map_err(|error| AppError::Credential(format!("OS credential entry: {error}")))?;
        entry
            .get_password()
            .map_err(|error| AppError::Credential(format!("OS credential read: {error}")))
    }

    fn delete(&self, reference: &CredentialReference) -> Result<(), AppError> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &reference.reference)
            .map_err(|error| AppError::Credential(format!("OS credential entry: {error}")))?;
        entry
            .delete_credential()
            .map_err(|error| AppError::Credential(format!("OS credential delete: {error}")))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl CredentialStore for OsCredentialStore {
    fn save(&self, _name: &str, _value: &str) -> Result<CredentialReference, AppError> {
        Err(AppError::UnsupportedPlatform(
            "OS credential storage is supported only on Windows and macOS".into(),
        ))
    }

    fn read(&self, _reference: &CredentialReference) -> Result<String, AppError> {
        Err(AppError::UnsupportedPlatform(
            "OS credential storage is supported only on Windows and macOS".into(),
        ))
    }

    fn delete(&self, _reference: &CredentialReference) -> Result<(), AppError> {
        Err(AppError::UnsupportedPlatform(
            "OS credential storage is supported only on Windows and macOS".into(),
        ))
    }
}

pub fn provider_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => "windows_credential_manager",
        Platform::Macos => "macos_keychain",
    }
}

pub fn validate_credential_reference(reference: &CredentialReference) -> Result<(), AppError> {
    let opaque_id = reference
        .reference
        .strip_prefix("credential://meshy_api_key/");
    if reference.name != MESHY_ENVIRONMENT_NAME
        || reference.provider != provider_name(Platform::current())
        || opaque_id.is_none_or(|value| Uuid::parse_str(value).is_err())
        || reference.reference.len() > 512
        || reference.reference.chars().any(char::is_whitespace)
    {
        return Err(AppError::Credential(
            "only an opaque Meshy credential reference from this platform may be changed".into(),
        ));
    }
    Ok(())
}

pub fn validate_secret_input(name: &str, value: &str) -> Result<(), AppError> {
    validate_env_name(name)?;
    if value.trim().is_empty() {
        return Err(AppError::Credential("credential cannot be empty".into()));
    }
    if name == MESHY_ENVIRONMENT_NAME && value.trim() == "msy_your_actual_key_here" {
        return Err(AppError::Credential(
            "placeholder Meshy key is not accepted".into(),
        ));
    }
    Ok(())
}

pub fn save_meshy_key<S: CredentialStore>(
    store: &S,
    value: &str,
) -> Result<CredentialReference, AppError> {
    store.save(MESHY_ENVIRONMENT_NAME, value)
}

pub struct ScopedSecretEnvironment {
    values: BTreeMap<String, String>,
    known_secrets: Vec<String>,
}

impl ScopedSecretEnvironment {
    pub fn from_credential<S: CredentialStore>(
        store: &S,
        reference: &CredentialReference,
        environment_name: &str,
    ) -> Result<Self, AppError> {
        if environment_name != MESHY_ENVIRONMENT_NAME {
            return Err(AppError::Credential(
                "only the Meshy credential may be injected by this application".into(),
            ));
        }
        validate_credential_reference(reference)?;
        validate_env_name(environment_name)?;
        let value = store.read(reference)?;
        if value.trim().is_empty() {
            return Err(AppError::Credential(
                "credential store returned an empty value".into(),
            ));
        }
        Ok(Self {
            values: BTreeMap::from([(environment_name.to_string(), value.clone())]),
            known_secrets: vec![value],
        })
    }

    pub fn variable_names(&self) -> Vec<String> {
        self.values.keys().cloned().collect()
    }

    pub(crate) fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }

    pub fn redact(&self, output: &str) -> String {
        redact_secrets(output, &self.known_secrets)
    }
}

impl Drop for ScopedSecretEnvironment {
    fn drop(&mut self) {
        for value in self.values.values_mut() {
            value.clear();
        }
        self.known_secrets.clear();
    }
}

pub fn mesh_key_status<S: CredentialStore>(
    store: &S,
    reference: Option<&CredentialReference>,
) -> String {
    let Some(reference) = reference else {
        return "missing".into();
    };
    match store.read(reference) {
        Ok(value) if !value.trim().is_empty() => "present".into(),
        _ => "missing".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_returns_only_an_opaque_reference() {
        let store = MemoryCredentialStore::default();
        let reference = save_meshy_key(&store, "mesh_key_test_value").unwrap();
        assert!(reference
            .reference
            .starts_with("credential://meshy_api_key/"));
        assert!(!serde_json::to_string(&reference)
            .unwrap()
            .contains("test_value"));
        assert_eq!(mesh_key_status(&store, Some(&reference)), "present");
    }

    #[test]
    fn secret_environment_exposes_names_and_redacts_output() {
        let store = MemoryCredentialStore::default();
        let reference = save_meshy_key(&store, "mesh_key_secret_value").unwrap();
        let environment =
            ScopedSecretEnvironment::from_credential(&store, &reference, MESHY_ENVIRONMENT_NAME)
                .unwrap();
        assert_eq!(environment.variable_names(), vec![MESHY_ENVIRONMENT_NAME]);
        assert!(!environment
            .redact("key=mesh_key_secret_value")
            .contains("secret_value"));
    }

    #[test]
    fn credential_deletion_accepts_only_generated_meshy_references() {
        let store = MemoryCredentialStore::default();
        let reference = save_meshy_key(&store, "mesh_key_delete_test").unwrap();
        validate_credential_reference(&reference).unwrap();
        let mut invalid = reference.clone();
        invalid.reference = "credential://meshy_api_key/not-a-uuid".into();
        assert!(validate_credential_reference(&invalid).is_err());
        let mut wrong_name = reference;
        wrong_name.name = "OTHER_SECRET".into();
        assert!(validate_credential_reference(&wrong_name).is_err());
    }
}
