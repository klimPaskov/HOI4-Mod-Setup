//! Rules for the optional context a user adds while describing a mod,
//! shared by the desktop app and the web edition. A document becomes one
//! redacted, bounded excerpt that can reach the setup assistant as approved
//! evidence under a reserved `@context/` path; images never reach a provider.
//! Copies kept in the project are size-limited and named safely.

use crate::AppError;
use serde::{Deserialize, Serialize};

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
            let cleaned = crate::profile::clean_text(line, 2_000);
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
    if crate::text::redact_secrets(&excerpt, &[]) != excerpt {
        return ("[credential-shaped text omitted]".into(), true, truncated);
    }
    (excerpt, redacted, truncated)
}

pub fn image_media_type(bytes: &[u8]) -> Option<&'static str> {
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

/// Whether an evidence item is a user-added document rather than scan evidence.
pub fn is_context_evidence_path(path: &str) -> bool {
    path.starts_with(CONTEXT_EVIDENCE_PREFIX)
}

/// A file the plan stages from the user's context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFile {
    pub component_id: &'static str,
    pub destination: String,
    pub bytes: Vec<u8>,
}

pub fn unique_destination(directory: &str, name: &str, used: &mut Vec<String>) -> String {
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

/// Read a document the user picked: a supported text type, UTF-8, within
/// [`MAX_DOCUMENT_BYTES`]. `number` makes its reference and evidence path
/// unique within the session.
pub fn document_from_bytes(
    number: u64,
    name: &str,
    bytes: &[u8],
) -> Result<ContextDocument, AppError> {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    if !extension.is_some_and(|extension| DOCUMENT_EXTENSIONS.contains(&extension.as_str())) {
        return Err(AppError::InvalidInput(
            "this file type cannot be added as a document".into(),
        ));
    }
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(AppError::InvalidInput(format!(
            "the file is larger than {} KiB",
            MAX_DOCUMENT_BYTES / 1024
        )));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| AppError::InvalidInput("the document is not UTF-8 text".into()))?;
    if text.contains('\0') {
        return Err(AppError::InvalidInput(
            "the document is not plain text".into(),
        ));
    }
    let (excerpt, redacted, truncated) = document_excerpt(text);
    let safe = safe_file_name(name);
    Ok(ContextDocument {
        reference: format!("context-{number}"),
        evidence_path: format!("{CONTEXT_EVIDENCE_PREFIX}{number}-{safe}"),
        id: format!("doc-{number}"),
        name: name.chars().take(120).collect(),
        size: bytes.len() as u64,
        excerpt_sha256: crate::text::sha256_bytes(excerpt.as_bytes()),
        excerpt,
        truncated,
        redacted,
        copyable: bytes.len() <= MAX_DOCUMENT_COPY_BYTES && !redacted,
    })
}

/// A document copy the user asked to keep.
pub struct DocumentCopy<'a> {
    pub name: &'a str,
    pub bytes: &'a [u8],
    pub copyable: bool,
}

/// A visual reference the user asked to keep, already rendered as PNG or JPEG.
pub struct ImageCopy<'a> {
    pub name: &'a str,
    pub bytes: &'a [u8],
}

/// The files a plan or export adds from the user's context: documents under
/// `docs/reference/` and visual references under `docs/visual_direction/`.
pub fn context_copies(
    documents: &[DocumentCopy<'_>],
    images: &[ImageCopy<'_>],
) -> Result<Vec<ContextFile>, AppError> {
    let mut files = Vec::new();
    let mut used = Vec::new();
    let mut total = 0usize;
    for document in documents {
        if !document.copyable {
            return Err(AppError::InvalidInput(format!(
                "{} cannot be copied into the project",
                document.name
            )));
        }
        total += document.bytes.len();
        files.push(ContextFile {
            component_id: "project.context_reference",
            destination: unique_destination("docs/reference", document.name, &mut used),
            bytes: document.bytes.to_vec(),
        });
    }
    for image in images {
        let extension = match image_media_type(image.bytes) {
            Some("image/png") => "png",
            Some("image/jpeg") => "jpg",
            _ => {
                return Err(AppError::InvalidInput(
                    "a visual reference must be rendered as PNG or JPEG".into(),
                ))
            }
        };
        if image.bytes.len() > MAX_REFERENCE_COPY_BYTES {
            return Err(AppError::InvalidInput(
                "a visual reference copy is larger than 1 MiB".into(),
            ));
        }
        total += image.bytes.len();
        let stem = image
            .name
            .rsplit_once('.')
            .map(|(stem, _)| stem.to_owned())
            .unwrap_or_else(|| image.name.to_owned());
        files.push(ContextFile {
            component_id: "project.visual_direction",
            destination: unique_destination(
                "docs/visual_direction",
                &format!("{stem}.{extension}"),
                &mut used,
            ),
            bytes: image.bytes.to_vec(),
        });
    }
    if total > MAX_TOTAL_COPY_BYTES {
        return Err(AppError::InvalidInput(
            "context copies are larger than 4 MiB together; keep fewer copies".into(),
        ));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_are_bounded_redacted_excerpts_with_reserved_paths() {
        let document =
            document_from_bytes(3, "Lore Notes.md", b"# Lore\nThe Balkans in 1936.\n").unwrap();
        assert_eq!(document.reference, "context-3");
        assert_eq!(document.evidence_path, "@context/3-lore_notes.md");
        assert!(document.copyable && !document.redacted);
        assert!(document_from_bytes(1, "notes.exe", b"x").is_err());
        assert!(document_from_bytes(1, "notes.txt", &[0xff, 0xfe]).is_err());
        // Assembled at run time so the repository's secret scan stays clean.
        let line = format!(
            "OPENAI_API_KEY={}{}\n",
            "sk-proj-", "abcdefghijklmnopqrstuvwxyz0123456789"
        );
        let secret = document_from_bytes(2, "env.txt", line.as_bytes()).unwrap();
        assert!(secret.redacted && !secret.copyable);
    }

    #[test]
    fn copies_are_named_safely_and_limited() {
        let png = b"\x89PNG\r\n\x1a\nrest";
        let files = context_copies(
            &[
                DocumentCopy {
                    name: "Plan.md",
                    bytes: b"a",
                    copyable: true,
                },
                DocumentCopy {
                    name: "plan.md",
                    bytes: b"b",
                    copyable: true,
                },
            ],
            &[ImageCopy {
                name: "Mood Board.webp",
                bytes: png,
            }],
        )
        .unwrap();
        let destinations = files
            .iter()
            .map(|file| file.destination.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            destinations,
            [
                "docs/reference/plan.md",
                "docs/reference/plan-2.md",
                "docs/visual_direction/mood_board.png"
            ]
        );
        assert!(context_copies(
            &[DocumentCopy {
                name: "x.md",
                bytes: b"a",
                copyable: false
            }],
            &[]
        )
        .is_err());
        assert!(context_copies(
            &[],
            &[ImageCopy {
                name: "x.gif",
                bytes: b"GIF89a"
            }]
        )
        .is_err());
    }
}
