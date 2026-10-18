//! Optional context the user adds while describing a mod.
//!
//! Documents and images are read only after the user picks them in the native
//! file dialog. Each read is bounded and link-safe. A document becomes one
//! redacted excerpt that can reach the setup assistant as approved evidence
//! under a reserved `@context/` path; it never names the file's location.
//! Images stay local: the interface renders the chosen thumbnail and any
//! visual-reference copies, and the plan validates those bytes before staging.
//! Every attachment is bound by identifier and hash in a session store, so a
//! request or plan cannot introduce a file the user did not pick.

use crate::security::sha256_bytes;
use crate::AppError;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Reserved evidence path prefix for user-added documents.
pub const CONTEXT_EVIDENCE_PREFIX: &str = "@context/";
/// Most attachments of each kind held at once.
pub const MAX_ATTACHMENTS_PER_KIND: usize = 12;
/// Largest document read. Larger files are refused rather than truncated so
/// the excerpt never silently misses the user's text.
pub const MAX_DOCUMENT_BYTES: u64 = 1024 * 1024;
/// Characters of a document that may reach the setup assistant.
pub const MAX_EXCERPT_CHARS: usize = 12_000;
/// A document up to this size may also be copied into the project.
pub const MAX_DOCUMENT_COPY_BYTES: usize = 256 * 1024;
/// Largest source image accepted for preview and rendering.
pub const MAX_IMAGE_BYTES: u64 = 12 * 1024 * 1024;
/// Largest rendered visual-reference copy, and all copies together. Generated
/// bytes are recorded in the installation lock for repair, so they stay small.
pub const MAX_REFERENCE_COPY_BYTES: usize = 1024 * 1024;
pub const MAX_TOTAL_COPY_BYTES: usize = 4 * 1024 * 1024;

pub const DOCUMENT_EXTENSIONS: &[&str] = &[
    "md", "markdown", "txt", "csv", "json", "yml", "yaml", "toml", "lua", "py", "js", "ts", "gui",
    "gfx", "asset", "mod", "html", "xml",
];
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextDocument {
    pub id: String,
    pub name: String,
    pub size: u64,
    /// Evidence reference and reserved path used in analysis requests.
    pub reference: String,
    pub evidence_path: String,
    pub excerpt: String,
    pub excerpt_sha256: String,
    pub truncated: bool,
    /// Some lines were omitted as machine-local or credential-shaped.
    pub redacted: bool,
    /// Small enough to keep a copy in the project.
    pub copyable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextImage {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub media_type: String,
    /// `data:` URL for preview and in-app rendering only.
    pub data_url: String,
}

#[derive(Debug, Clone)]
struct StoredDocument {
    document: ContextDocument,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct StoredImage {
    name: String,
}

#[derive(Default)]
struct Store {
    next: u64,
    documents: BTreeMap<String, StoredDocument>,
    images: BTreeMap<String, StoredImage>,
}

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Store::default()))
}

fn locked() -> Result<std::sync::MutexGuard<'static, Store>, AppError> {
    store()
        .lock()
        .map_err(|_| AppError::Process("context attachment store is unavailable".into()))
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
}

/// A file name safe to show and to use as a project-relative file name:
/// ASCII letters, digits, dots, hyphens, and underscores only.
pub fn safe_file_name(name: &str) -> String {
    let mut safe = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    safe = safe.trim_matches(['.', '_', '-']).to_owned();
    if safe.is_empty() {
        safe = "attachment".into();
    }
    if safe.len() > 80 {
        let extension = safe.rsplit_once('.').map(|(_, ext)| ext.to_owned());
        safe.truncate(70);
        if let Some(extension) = extension.filter(|ext| ext.len() <= 8) {
            safe = format!("{}.{extension}", safe.trim_end_matches('.'));
        }
    }
    safe
}

/// Read a picked file without following links, refusing anything that is not
/// a regular file within `limit` bytes.
fn read_picked_file(path: &Path, limit: u64) -> Result<Vec<u8>, AppError> {
    if crate::security::path_has_link_component(path) {
        return Err(AppError::PathSecurity(
            "a linked file cannot be added as context".into(),
        ));
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| AppError::InvalidInput(format!("the file could not be read: {error}")))?;
    if !metadata.is_file() {
        return Err(AppError::InvalidInput(
            "only regular files can be added as context".into(),
        ));
    }
    if metadata.len() > limit {
        return Err(AppError::InvalidInput(format!(
            "the file is larger than {} KiB",
            limit / 1024
        )));
    }
    let bytes = std::fs::read(path)
        .map_err(|error| AppError::InvalidInput(format!("the file could not be read: {error}")))?;
    if bytes.len() as u64 > limit {
        return Err(AppError::InvalidInput(
            "the file changed while it was read".into(),
        ));
    }
    Ok(bytes)
}

/// Build the redacted, bounded excerpt of a text document. Lines that look
/// machine-local or credential-shaped are replaced by a marker, so the result
/// is always a redaction fixed point.
pub fn document_excerpt(text: &str) -> (String, bool, bool) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut excerpt = String::new();
    let mut redacted = false;
    let mut truncated = false;
    for line in text.lines() {
        let cleaned = if line.trim().is_empty() {
            String::new()
        } else {
            let cleaned = crate::project_profile::clean_text(line, 2_000);
            if cleaned != line {
                redacted = true;
            }
            cleaned
        };
        let needed = cleaned.chars().count() + 1;
        if excerpt.chars().count() + needed > MAX_EXCERPT_CHARS {
            truncated = true;
            break;
        }
        excerpt.push_str(&cleaned);
        excerpt.push('\n');
    }
    let excerpt = excerpt.trim_end().to_owned();
    if crate::security::redact_secrets(&excerpt, &[]) != excerpt {
        return ("[credential-shaped text omitted]".into(), true, truncated);
    }
    (excerpt, redacted, truncated)
}

/// Add documents the user picked. Each file is refused individually with a
/// reason; accepted files are bound in the session store.
pub fn add_documents(paths: &[PathBuf]) -> (Vec<ContextDocument>, Vec<String>) {
    let mut added = Vec::new();
    let mut refused = Vec::new();
    for path in paths {
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into());
        match add_document(path, &name) {
            Ok(document) => added.push(document),
            Err(error) => refused.push(format!("{name}: {}", user_reason(&error))),
        }
    }
    (added, refused)
}

fn user_reason(error: &AppError) -> String {
    match error {
        AppError::InvalidInput(reason) | AppError::PathSecurity(reason) => reason.clone(),
        other => other.to_string(),
    }
}

fn add_document(path: &Path, name: &str) -> Result<ContextDocument, AppError> {
    if !extension(path).is_some_and(|ext| DOCUMENT_EXTENSIONS.contains(&ext.as_str())) {
        return Err(AppError::InvalidInput(
            "this file type cannot be added as a document".into(),
        ));
    }
    let bytes = read_picked_file(path, MAX_DOCUMENT_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| AppError::InvalidInput("the document is not UTF-8 text".into()))?;
    if text.contains('\0') {
        return Err(AppError::InvalidInput(
            "the document is not plain text".into(),
        ));
    }
    let (excerpt, redacted, truncated) = document_excerpt(text);
    let mut store = locked()?;
    if store.documents.len() >= MAX_ATTACHMENTS_PER_KIND {
        return Err(AppError::InvalidInput(format!(
            "at most {MAX_ATTACHMENTS_PER_KIND} documents can be added"
        )));
    }
    let content_sha256 = sha256_bytes(&bytes);
    if store
        .documents
        .values()
        .any(|stored| sha256_bytes(&stored.bytes) == content_sha256)
    {
        return Err(AppError::InvalidInput(
            "this document is already added".into(),
        ));
    }
    store.next += 1;
    let id = format!("doc-{}", store.next);
    let safe = safe_file_name(name);
    let document = ContextDocument {
        reference: format!("context-{}", store.next),
        evidence_path: format!("{CONTEXT_EVIDENCE_PREFIX}{}-{safe}", store.next),
        id: id.clone(),
        name: name.chars().take(120).collect(),
        size: bytes.len() as u64,
        excerpt_sha256: sha256_bytes(excerpt.as_bytes()),
        excerpt,
        truncated,
        redacted,
        copyable: bytes.len() <= MAX_DOCUMENT_COPY_BYTES && !redacted,
    };
    store.documents.insert(
        id,
        StoredDocument {
            document: document.clone(),
            bytes,
        },
    );
    Ok(document)
}

fn image_media_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Add images the user picked, returning previews for in-app rendering.
pub fn add_images(paths: &[PathBuf]) -> (Vec<ContextImage>, Vec<String>) {
    let mut added = Vec::new();
    let mut refused = Vec::new();
    for path in paths {
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "image".into());
        match add_image(path, &name) {
            Ok(image) => added.push(image),
            Err(error) => refused.push(format!("{name}: {}", user_reason(&error))),
        }
    }
    (added, refused)
}

fn add_image(path: &Path, name: &str) -> Result<ContextImage, AppError> {
    if !extension(path).is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.as_str())) {
        return Err(AppError::InvalidInput(
            "only PNG, JPEG, and WebP images can be added".into(),
        ));
    }
    let bytes = read_picked_file(path, MAX_IMAGE_BYTES)?;
    let media_type = image_media_type(&bytes).ok_or_else(|| {
        AppError::InvalidInput("the file is not a PNG, JPEG, or WebP image".into())
    })?;
    let mut store = locked()?;
    if store.images.len() >= MAX_ATTACHMENTS_PER_KIND {
        return Err(AppError::InvalidInput(format!(
            "at most {MAX_ATTACHMENTS_PER_KIND} images can be added"
        )));
    }
    store.next += 1;
    let id = format!("img-{}", store.next);
    store.images.insert(
        id.clone(),
        StoredImage {
            name: name.chars().take(120).collect(),
        },
    );
    Ok(ContextImage {
        id,
        name: name.chars().take(120).collect(),
        size: bytes.len() as u64,
        media_type: media_type.into(),
        data_url: format!(
            "data:{media_type};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        ),
    })
}

pub fn forget(id: &str) -> Result<(), AppError> {
    let mut store = locked()?;
    store.documents.remove(id);
    store.images.remove(id);
    Ok(())
}

/// Whether an evidence item is a user-added document rather than scan evidence.
pub fn is_context_evidence_path(path: &str) -> bool {
    path.starts_with(CONTEXT_EVIDENCE_PREFIX)
}

/// Check that a context evidence item is exactly a document the user added
/// in this session.
pub fn validate_context_evidence(
    reference: &str,
    path: &str,
    excerpt_sha256: &str,
) -> Result<(), AppError> {
    let store = locked()?;
    let bound = store.documents.values().any(|stored| {
        stored.document.reference == reference
            && stored.document.evidence_path == path
            && stored.document.excerpt_sha256 == excerpt_sha256
    });
    if bound {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "a context document is no longer attached; add it again".into(),
        ))
    }
}

/// A file the plan stages from the user's context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFile {
    pub component_id: &'static str,
    pub destination: String,
    pub bytes: Vec<u8>,
}

fn unique_destination(directory: &str, name: &str, used: &mut Vec<String>) -> String {
    let safe = safe_file_name(name);
    let (stem, extension) = match safe.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem.to_owned(), format!(".{extension}")),
        _ => (safe.clone(), String::new()),
    };
    let mut candidate = format!("{directory}/{stem}{extension}");
    let mut counter = 2;
    while used
        .iter()
        .any(|known| known.eq_ignore_ascii_case(&candidate))
    {
        candidate = format!("{directory}/{stem}-{counter}{extension}");
        counter += 1;
    }
    used.push(candidate.clone());
    candidate
}

fn decode_base64(value: &str) -> Result<Vec<u8>, AppError> {
    let payload = value
        .split_once(";base64,")
        .map(|(_, payload)| payload)
        .unwrap_or(value);
    base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|_| AppError::InvalidInput("context image data is not valid base64".into()))
}

/// The thumbnail the user rendered from one of their images, validated as a
/// PNG. `None` keeps the generated placeholder.
pub fn thumbnail_from_state(state: &Value) -> Result<Option<Vec<u8>>, AppError> {
    let Some(thumbnail) = state
        .get("contextThumbnail")
        .filter(|value| !value.is_null())
    else {
        return Ok(None);
    };
    let attachment = thumbnail
        .get("attachmentId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !locked()?.images.contains_key(attachment) {
        return Err(AppError::InvalidInput(
            "the thumbnail image is no longer attached; choose it again".into(),
        ));
    }
    let bytes = decode_base64(
        thumbnail
            .get("png")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?;
    crate::descriptors::validate_thumbnail_png(&bytes)?;
    Ok(Some(bytes))
}

/// The document and visual-reference copies the user asked to keep in the
/// project. Documents come from the bound bytes read at pick time; images are
/// the interface's bounded rendering of a picked image and must be PNG or
/// JPEG.
pub fn copies_from_state(state: &Value) -> Result<Vec<ContextFile>, AppError> {
    let store = locked()?;
    let mut files = Vec::new();
    let mut used = Vec::new();
    let mut total = 0usize;
    for document in state
        .get("contextDocuments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if document.get("keepCopy").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let id = document
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let stored = store.documents.get(id).ok_or_else(|| {
            AppError::InvalidInput("a context document is no longer attached; add it again".into())
        })?;
        if !stored.document.copyable {
            return Err(AppError::InvalidInput(format!(
                "{} cannot be copied into the project",
                stored.document.name
            )));
        }
        total += stored.bytes.len();
        files.push(ContextFile {
            component_id: "project.context_reference",
            destination: unique_destination("docs/reference", &stored.document.name, &mut used),
            bytes: stored.bytes.clone(),
        });
    }
    for image in state
        .get("contextImages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if image.get("saveReference").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let id = image.get("id").and_then(Value::as_str).unwrap_or_default();
        let stored = store.images.get(id).ok_or_else(|| {
            AppError::InvalidInput("a context image is no longer attached; add it again".into())
        })?;
        let bytes = decode_base64(
            image
                .get("referenceData")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )?;
        let extension = match image_media_type(&bytes) {
            Some("image/png") => "png",
            Some("image/jpeg") => "jpg",
            _ => {
                return Err(AppError::InvalidInput(
                    "a visual reference must be rendered as PNG or JPEG".into(),
                ))
            }
        };
        if bytes.len() > MAX_REFERENCE_COPY_BYTES {
            return Err(AppError::InvalidInput(
                "a visual reference copy is larger than 1 MiB".into(),
            ));
        }
        total += bytes.len();
        let stem = stored
            .name
            .rsplit_once('.')
            .map(|(stem, _)| stem.to_owned())
            .unwrap_or_else(|| stored.name.clone());
        files.push(ContextFile {
            component_id: "project.visual_direction",
            destination: unique_destination(
                "docs/visual_direction",
                &format!("{stem}.{extension}"),
                &mut used,
            ),
            bytes,
        });
    }
    if total > MAX_TOTAL_COPY_BYTES {
        return Err(AppError::InvalidInput(
            "context copies are larger than 4 MiB together; keep fewer copies".into(),
        ));
    }
    Ok(files)
}

/// Serializes tests that use the process-wide store.
#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    if let Ok(mut store) = store().lock() {
        *store = Store::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        test_guard()
    }

    #[test]
    fn documents_are_bounded_redacted_and_bound_for_analysis() {
        let _guard = serial();
        reset_for_tests();
        let folder = tempfile::tempdir().unwrap();
        let notes = folder.path().join("Design Notes.md");
        let key = format!("{}{}", "ghp_", "a".repeat(30));
        std::fs::write(
            &notes,
            format!("# Kaiserreich notes\nCountries use GER_ ids.\nSaved in C:\\Users\\someone\\mods\ntoken = {key}\n"),
        )
        .unwrap();
        let binary = folder.path().join("blob.txt");
        std::fs::write(&binary, [0xff_u8, 0xfe, 0x00, 0x01]).unwrap();
        let unsupported = folder.path().join("archive.zip");
        std::fs::write(&unsupported, b"PK").unwrap();
        let (added, refused) = add_documents(&[notes.clone(), binary, unsupported]);
        assert_eq!(added.len(), 1, "{refused:?}");
        assert_eq!(refused.len(), 2);
        let document = &added[0];
        assert!(document.excerpt.contains("Countries use GER_ ids."));
        assert!(!document.excerpt.contains("someone"));
        assert!(!document.excerpt.contains(&key));
        assert!(document.redacted);
        assert!(!document.copyable, "a redacted document is never copied");
        assert!(document.evidence_path.starts_with(CONTEXT_EVIDENCE_PREFIX));
        assert!(document.evidence_path.ends_with("design_notes.md"));
        assert_eq!(
            crate::security::redact_secrets(&document.excerpt, &[]),
            document.excerpt
        );
        validate_context_evidence(
            &document.reference,
            &document.evidence_path,
            &document.excerpt_sha256,
        )
        .unwrap();
        assert!(validate_context_evidence(
            &document.reference,
            &document.evidence_path,
            &"0".repeat(64)
        )
        .is_err());
        // The same file twice is refused; forgetting unbinds it.
        assert_eq!(add_documents(&[notes]).1.len(), 1);
        forget(&document.id).unwrap();
        assert!(validate_context_evidence(
            &document.reference,
            &document.evidence_path,
            &document.excerpt_sha256
        )
        .is_err());
    }

    #[test]
    fn long_documents_are_truncated_and_oversized_files_refused() {
        let _guard = serial();
        reset_for_tests();
        let folder = tempfile::tempdir().unwrap();
        let long = folder.path().join("long.txt");
        std::fs::write(&long, "line of design text\n".repeat(2_000)).unwrap();
        let huge = folder.path().join("huge.txt");
        std::fs::write(&huge, "x".repeat(MAX_DOCUMENT_BYTES as usize + 1)).unwrap();
        let (added, refused) = add_documents(&[long, huge]);
        assert_eq!(added.len(), 1);
        assert!(added[0].truncated);
        assert!(added[0].excerpt.chars().count() <= MAX_EXCERPT_CHARS);
        assert_eq!(refused.len(), 1);
        assert!(refused[0].contains("larger than"), "{refused:?}");
    }

    fn tiny_png() -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[255_u8; 12]).unwrap();
        }
        bytes
    }

    #[test]
    fn images_thumbnail_and_copies_are_validated_against_picks() {
        let _guard = serial();
        reset_for_tests();
        let folder = tempfile::tempdir().unwrap();
        let picture = folder.path().join("Flag Art.png");
        std::fs::write(&picture, tiny_png()).unwrap();
        let fake = folder.path().join("fake.png");
        std::fs::write(&fake, b"not an image").unwrap();
        let (images, refused) = add_images(&[picture, fake]);
        assert_eq!(images.len(), 1);
        assert_eq!(refused.len(), 1);
        let image = &images[0];
        assert!(image.data_url.starts_with("data:image/png;base64,"));

        let encoded = base64::engine::general_purpose::STANDARD.encode(tiny_png());
        let thumbnail = thumbnail_from_state(&json!({
            "contextThumbnail": {"attachmentId": image.id, "png": format!("data:image/png;base64,{encoded}")}
        }))
        .unwrap()
        .unwrap();
        assert_eq!(thumbnail, tiny_png());
        assert!(thumbnail_from_state(
            &json!({"contextThumbnail": {"attachmentId": "img-999", "png": encoded}})
        )
        .is_err());
        assert!(thumbnail_from_state(
            &json!({"contextThumbnail": {"attachmentId": image.id, "png": "bm90IGEgcG5n"}})
        )
        .is_err());
        assert_eq!(thumbnail_from_state(&json!({})).unwrap(), None);

        let notes = folder.path().join("lore.md");
        std::fs::write(&notes, "# Lore\nThe war never ended.\n").unwrap();
        let (documents, _) = add_documents(&[notes]);
        let state = json!({
            "contextDocuments": [{"id": documents[0].id, "keepCopy": true}],
            "contextImages": [
                {"id": image.id, "saveReference": true, "referenceData": encoded},
                {"id": image.id, "saveReference": true, "referenceData": encoded}
            ]
        });
        let copies = copies_from_state(&state).unwrap();
        let destinations = copies
            .iter()
            .map(|file| file.destination.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            destinations,
            [
                "docs/reference/lore.md",
                "docs/visual_direction/flag_art.png",
                "docs/visual_direction/flag_art-2.png"
            ]
        );
        assert_eq!(copies[0].bytes, b"# Lore\nThe war never ended.\n");
        let gif = base64::engine::general_purpose::STANDARD.encode(b"GIF89a....");
        assert!(copies_from_state(&json!({"contextImages": [{"id": image.id, "saveReference": true, "referenceData": gif}]})).is_err());
        assert!(copies_from_state(&json!({"contextImages": [{"id": "img-404", "saveReference": true, "referenceData": encoded}]})).is_err());
    }

    #[test]
    fn safe_file_names_stay_plain_and_bounded() {
        assert_eq!(
            safe_file_name("Design Notes (v2).MD"),
            "design_notes__v2_.md"
        );
        assert_eq!(safe_file_name("../../etc/passwd"), "etc_passwd");
        assert_eq!(safe_file_name("..."), "attachment");
        assert!(safe_file_name(&format!("{}.png", "a".repeat(200))).len() <= 80);
    }
}
