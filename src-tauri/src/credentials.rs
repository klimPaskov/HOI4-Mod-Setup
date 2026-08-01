use crate::models::{CredentialReference, Platform};
use crate::security::{redact_secrets, validate_env_name};
use crate::AppError;
use std::collections::BTreeMap;
use uuid::Uuid;

pub const MESHY_ENVIRONMENT_NAME: &str = "MESHY_API_KEY";
pub const AI_PROVIDER_ENVIRONMENT_NAME: &str = "AI_PROVIDER_API_KEY";
const MAX_SECRET_INPUT_BYTES: usize = 64 * 1024;
#[cfg(any(target_os = "windows", target_os = "macos"))]
const KEYRING_SERVICE: &str = "com.klimpaskov.hoi4-mod-setup";

pub trait CredentialStore {
    fn save(&self, name: &str, value: &str) -> Result<CredentialReference, AppError>;
    fn save_scoped(
        &self,
        name: &str,
        scope: &str,
        value: &str,
    ) -> Result<CredentialReference, AppError>;
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
            provider_id: None,
        };
        self.values
            .lock()
            .map_err(|_| AppError::Credential("credential store lock poisoned".into()))?
            .insert(reference.reference.clone(), value.to_string());
        Ok(reference)
    }

    fn save_scoped(
        &self,
        name: &str,
        scope: &str,
        value: &str,
    ) -> Result<CredentialReference, AppError> {
        validate_secret_input(name, value)?;
        validate_provider_scope(scope)?;
        let reference = CredentialReference {
            name: name.into(),
            provider: provider_name(Platform::current()).into(),
            reference: format!("credential://{}/{scope}", name.to_ascii_lowercase()),
            provider_id: Some(scope.into()),
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
            provider_id: None,
        };
        let entry = keyring::Entry::new(KEYRING_SERVICE, &reference.reference)
            .map_err(|error| AppError::Credential(format!("OS credential entry: {error}")))?;
        entry
            .set_password(value)
            .map_err(|error| AppError::Credential(format!("OS credential write: {error}")))?;
        Ok(reference)
    }

    fn save_scoped(
        &self,
        name: &str,
        scope: &str,
        value: &str,
    ) -> Result<CredentialReference, AppError> {
        validate_secret_input(name, value)?;
        validate_provider_scope(scope)?;
        let platform = Platform::current();
        let reference = CredentialReference {
            name: name.into(),
            provider: provider_name(platform).into(),
            reference: format!("credential://{}/{scope}", name.to_ascii_lowercase()),
            provider_id: Some(scope.into()),
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

    fn save_scoped(
        &self,
        _name: &str,
        _scope: &str,
        _value: &str,
    ) -> Result<CredentialReference, AppError> {
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
        Platform::Unsupported => "unsupported",
    }
}

pub fn validate_credential_reference(reference: &CredentialReference) -> Result<(), AppError> {
    if reference.name == MESHY_ENVIRONMENT_NAME {
        return validate_opaque_reference(reference, "meshy_api_key", "Meshy");
    }
    if reference.name == AI_PROVIDER_ENVIRONMENT_NAME {
        return validate_ai_provider_credential_reference(reference);
    }
    Err(AppError::Credential(
        "credential reference is not a supported application credential".into(),
    ))
}

fn validate_opaque_reference(
    reference: &CredentialReference,
    namespace: &str,
    label: &str,
) -> Result<(), AppError> {
    let opaque_id = reference
        .reference
        .strip_prefix(&format!("credential://{namespace}/"));
    if reference.provider != provider_name(Platform::current())
        || opaque_id.is_none_or(|value| Uuid::parse_str(value).is_err())
        || reference.reference.len() > 512
        || reference.reference.chars().any(char::is_whitespace)
    {
        return Err(AppError::Credential(format!(
            "only an opaque {label} credential reference from this platform may be changed"
        )));
    }
    Ok(())
}

pub fn validate_ai_provider_credential_reference(
    reference: &CredentialReference,
) -> Result<(), AppError> {
    if reference.name != AI_PROVIDER_ENVIRONMENT_NAME {
        return Err(AppError::Credential(
            "AI provider credential has an unexpected environment name".into(),
        ));
    }
    let provider_id = reference
        .provider_id
        .as_deref()
        .ok_or_else(|| AppError::Credential("AI provider credential scope is missing".into()))?;
    validate_provider_scope(provider_id)?;
    let expected = format!("credential://ai_provider_api_key/{provider_id}");
    if reference.provider != provider_name(Platform::current())
        || reference.reference != expected
        || reference.reference.len() > 512
        || reference.reference.chars().any(char::is_whitespace)
    {
        return Err(AppError::Credential(
            "only a provider-scoped credential reference from this platform may be changed".into(),
        ));
    }
    Ok(())
}

fn validate_provider_scope(scope: &str) -> Result<(), AppError> {
    if scope.is_empty()
        || scope.len() > 64
        || scope.chars().any(|character| {
            !(character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_'))
        })
    {
        return Err(AppError::Credential(
            "AI provider credential scope is invalid".into(),
        ));
    }
    Ok(())
}

pub fn validate_ai_provider_credential_for(
    reference: &CredentialReference,
    provider_id: &str,
) -> Result<(), AppError> {
    validate_ai_provider_credential_reference(reference)?;
    if reference.provider_id.as_deref() != Some(provider_id) {
        return Err(AppError::Credential(
            "AI provider credential is scoped to a different provider; reconnect the selected provider".into(),
        ));
    }
    Ok(())
}

pub fn validate_secret_input(name: &str, value: &str) -> Result<(), AppError> {
    validate_env_name(name)?;
    if value.trim().is_empty() {
        return Err(AppError::Credential("credential cannot be empty".into()));
    }
    if value.len() > MAX_SECRET_INPUT_BYTES {
        return Err(AppError::Credential(
            "credential exceeds the bounded input size".into(),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(AppError::Credential(
            "credential contains a control character".into(),
        ));
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

pub fn save_ai_provider_key<S: CredentialStore>(
    store: &S,
    provider_id: &str,
    value: &str,
) -> Result<CredentialReference, AppError> {
    validate_provider_scope(provider_id)?;
    let reference = store.save_scoped(AI_PROVIDER_ENVIRONMENT_NAME, provider_id, value)?;
    validate_ai_provider_credential_for(&reference, provider_id)?;
    Ok(reference)
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

    #[test]
    fn provider_credential_scope_cannot_be_reused_for_another_provider() {
        let store = MemoryCredentialStore::default();
        let reference = save_ai_provider_key(&store, "claude", "provider_secret").unwrap();
        assert_eq!(reference.provider_id.as_deref(), Some("claude"));
        validate_ai_provider_credential_for(&reference, "claude").unwrap();
        assert!(validate_ai_provider_credential_for(&reference, "deepseek").is_err());
        assert!(!serde_json::to_string(&reference)
            .unwrap()
            .contains("provider_secret"));
    }

    #[test]
    fn provider_credential_reference_can_be_reconstructed_after_restart() {
        let store = MemoryCredentialStore::default();
        let saved = save_ai_provider_key(&store, "claude", "provider_secret").unwrap();
        let reconstructed = CredentialReference {
            name: AI_PROVIDER_ENVIRONMENT_NAME.into(),
            provider: provider_name(Platform::current()).into(),
            reference: "credential://ai_provider_api_key/claude".into(),
            provider_id: Some("claude".into()),
        };
        assert_eq!(saved.reference, reconstructed.reference);
        validate_ai_provider_credential_for(&reconstructed, "claude").unwrap();
        assert_eq!(store.read(&reconstructed).unwrap(), "provider_secret");
    }

    #[test]
    fn secret_input_rejects_control_characters_and_oversized_values() {
        assert!(validate_secret_input(MESHY_ENVIRONMENT_NAME, "key\nvalue").is_err());
        assert!(validate_secret_input(
            MESHY_ENVIRONMENT_NAME,
            &"x".repeat(MAX_SECRET_INPUT_BYTES + 1)
        )
        .is_err());
    }
}
