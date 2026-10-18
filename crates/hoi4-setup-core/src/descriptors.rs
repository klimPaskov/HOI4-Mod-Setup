//! HOI4 descriptor parsing, deterministic rendering, and thumbnail rules.

use crate::models::ProjectIdentity;
use crate::AppError;
use regex::Regex;
use std::collections::BTreeMap;
use std::io::Cursor;

pub const HOI4_DESCRIPTOR_TAGS: &[&str] = &[
    "Alternative History",
    "Balance",
    "Events",
    "Fixes",
    "Gameplay",
    "Graphics",
    "Historical",
    "Ideologies",
    "Map",
    "Military",
    "National Focuses",
    "Sound",
    "Technologies",
    "Translation",
    "Utilities",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    pub fields: BTreeMap<String, String>,
}

pub fn validate_project_id(value: &str) -> Result<(), AppError> {
    let pattern = Regex::new(r"^[a-z][a-z0-9_]{1,63}$").expect("static project id regex");
    if pattern.is_match(value) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "project ID must start with a lowercase letter and contain only lowercase letters, digits, and underscores".into(),
        ))
    }
}

pub fn validate_field(value: &str, label: &str) -> Result<(), AppError> {
    if value.trim().is_empty()
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(AppError::InvalidInput(format!(
            "{label} is empty or contains a newline"
        )));
    }
    Ok(())
}

pub fn validate_descriptor_tags(tags: &[String]) -> Result<(), AppError> {
    let unique = tags.iter().collect::<std::collections::BTreeSet<_>>();
    if tags.is_empty()
        || tags.len() > HOI4_DESCRIPTOR_TAGS.len()
        || unique.len() != tags.len()
        || tags
            .iter()
            .any(|tag| !HOI4_DESCRIPTOR_TAGS.contains(&tag.as_str()))
    {
        return Err(AppError::InvalidInput(
            "descriptor tags must use the supported Hearts of Iron IV categories".into(),
        ));
    }
    Ok(())
}

/// Keys that HOI4 descriptors may legitimately repeat. Repeated values are
/// retained in order, separated by a newline (which no single value contains).
pub const REPEATABLE_DESCRIPTOR_KEYS: [&str; 1] = ["replace_path"];

/// Upper bound on the lines one `{ ... }` block may span, so a missing closing
/// brace fails fast instead of consuming the rest of an untrusted file.
pub const MAX_BLOCK_LINES: usize = 512;

pub fn parse_descriptor(bytes: &[u8]) -> Result<Descriptor, AppError> {
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|_| AppError::InvalidInput("descriptor is not UTF-8".into()))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut fields = BTreeMap::new();
    let mut lines = text.lines().enumerate();
    while let Some((line_number, line)) = lines.next() {
        let trimmed = strip_trailing_comment(line.trim());
        if trimmed.is_empty() {
            continue;
        }
        // HOI4 descriptors commonly spread list blocks such as `tags={` over
        // several lines. Join the block until its closing brace, then parse
        // it as one assignment.
        let mut statement = trimmed.to_string();
        let mut depth = trimmed
            .split_once('=')
            .map(|(_, value)| block_depth(value))
            .unwrap_or(0);
        let mut consumed = 0;
        while depth > 0 {
            let (_, next) = lines.next().ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "descriptor block starting on line {} is not closed",
                    line_number + 1
                ))
            })?;
            consumed += 1;
            if consumed > MAX_BLOCK_LINES {
                return Err(AppError::InvalidInput(format!(
                    "descriptor block starting on line {} is too long",
                    line_number + 1
                )));
            }
            let next = strip_trailing_comment(next.trim());
            if next.is_empty() {
                continue;
            }
            depth += block_depth(next);
            statement.push(' ');
            statement.push_str(next);
        }
        let (key, value) = parse_assignment(&statement).ok_or_else(|| {
            AppError::InvalidInput(format!("descriptor line {} is malformed", line_number + 1))
        })?;
        if REPEATABLE_DESCRIPTOR_KEYS.contains(&key.as_str()) {
            fields
                .entry(key)
                .and_modify(|existing: &mut String| {
                    existing.push('\n');
                    existing.push_str(&value);
                })
                .or_insert(value);
            continue;
        }
        if fields.insert(key.clone(), value).is_some() {
            return Err(AppError::InvalidInput(format!(
                "descriptor field is duplicated: {key}"
            )));
        }
    }
    if fields.is_empty() {
        return Err(AppError::InvalidInput(
            "descriptor contains no recognized assignments".into(),
        ));
    }
    Ok(Descriptor { fields })
}

/// Net brace depth change of one descriptor fragment, ignoring braces inside
/// quoted strings.
pub fn block_depth(fragment: &str) -> i32 {
    let mut depth = 0;
    let mut quoted = false;
    let mut escaped = false;
    for character in fragment.chars() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
            continue;
        }
        match character {
            '"' => quoted = true,
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    depth
}

/// Remove a `#` comment that is outside a quoted string.
pub fn strip_trailing_comment(fragment: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in fragment.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
            continue;
        }
        if character == '"' {
            quoted = true;
        } else if character == '#' {
            return fragment[..index].trim_end();
        }
    }
    fragment
}

pub fn parse_assignment(line: &str) -> Option<(String, String)> {
    let (raw_key, raw_value) = line.split_once('=')?;
    let key = raw_key.trim();
    if key.is_empty()
        || !key.chars().enumerate().all(|(index, character)| {
            if index == 0 {
                character.is_ascii_alphabetic() || character == '_'
            } else {
                character.is_ascii_alphanumeric() || character == '_'
            }
        })
    {
        return None;
    }
    let raw_value = raw_value.trim();
    if raw_value.starts_with('"') {
        let mut escaped = false;
        let mut closing = None;
        for (index, character) in raw_value.char_indices().skip(1) {
            if escaped {
                escaped = false;
                continue;
            }
            if character == '\\' {
                escaped = true;
            } else if character == '"' {
                closing = Some(index);
                break;
            }
        }
        let closing = closing?;
        if !raw_value[closing + 1..].trim().is_empty() {
            return None;
        }
        return Some((key.to_string(), unquote(&raw_value[1..closing])));
    }
    if raw_value.starts_with('{') {
        if !raw_value.ends_with('}') || raw_value.len() < 2 {
            return None;
        }
        return Some((key.to_string(), raw_value.to_string()));
    }
    if raw_value.is_empty()
        || raw_value.chars().any(|character| character.is_whitespace())
        || raw_value.contains('#')
    {
        return None;
    }
    Some((key.to_string(), raw_value.to_string()))
}

pub fn render_descriptor_mod(identity: &ProjectIdentity) -> Result<String, AppError> {
    validate_project_id(&identity.project_id)?;
    validate_field(&identity.display_name, "mod name")?;
    validate_field(&identity.version, "version")?;
    validate_field(&identity.supported_game_version, "supported game version")?;
    let mut descriptor = format!(
        "name=\"{}\"\nversion=\"{}\"\nsupported_version=\"{}\"\npicture=\"thumbnail.png\"\n",
        quote(&identity.display_name),
        quote(&identity.version),
        quote(&identity.supported_game_version),
    );
    // Script prefixes and namespaces are project conventions, not valid HOI4
    // descriptor fields. They are retained in installation metadata and
    // adapted project guidance instead of being written to either .mod file.
    if !identity.descriptor_tags.is_empty() {
        validate_descriptor_tags(&identity.descriptor_tags)?;
        let mut tags = identity.descriptor_tags.clone();
        tags.sort();
        let mut rendered = String::from("tags={");
        for tag in tags {
            validate_field(&tag, "descriptor tag")?;
            if tag.chars().count() > 64 {
                return Err(AppError::InvalidInput("descriptor tag is too long".into()));
            }
            rendered.push_str(&format!(" \"{}\"", quote(&tag)));
        }
        rendered.push_str(" }\n");
        descriptor.push_str(&rendered);
    }
    Ok(descriptor)
}

/// A deterministic 600x600 black PNG keeps a fresh project launcher-ready
/// without pretending to be user artwork. The file is replaceable and its
/// exact bytes are tracked as generated content in the transaction lock.
pub fn placeholder_thumbnail_png() -> Result<Vec<u8>, AppError> {
    const SIDE: u32 = 600;
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, SIDE, SIDE);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Best);
        let mut writer = encoder.write_header().map_err(|error| {
            AppError::InvalidInput(format!("thumbnail header could not be encoded: {error}"))
        })?;
        let pixels = vec![0_u8; (SIDE * SIDE * 3) as usize];
        writer.write_image_data(&pixels).map_err(|error| {
            AppError::InvalidInput(format!("thumbnail pixels could not be encoded: {error}"))
        })?;
    }
    Ok(bytes)
}

pub fn validate_thumbnail_png(bytes: &[u8]) -> Result<(u32, u32), AppError> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(AppError::InvalidInput(
            "thumbnail exceeds the bounded PNG size".into(),
        ));
    }
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().map_err(|error| {
        AppError::InvalidInput(format!("thumbnail is not a valid PNG: {error}"))
    })?;
    let (width, height) = {
        let info = reader.info();
        (info.width, info.height)
    };
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return Err(AppError::InvalidInput(
            "thumbnail dimensions are outside the supported range".into(),
        ));
    }
    let output_size = reader.output_buffer_size();
    let mut output = vec![0_u8; output_size];
    reader.next_frame(&mut output).map_err(|error| {
        AppError::InvalidInput(format!("thumbnail could not be decoded: {error}"))
    })?;
    Ok((width, height))
}

/// The raw text between the quotes of a top-level `key="..."` line, without
/// escape processing. A launcher descriptor written by hand may use single
/// backslashes, which the escape-aware parser would drop.
pub fn raw_quoted_field(bytes: &[u8], key: &str) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    text.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        if !name.trim().eq_ignore_ascii_case(key) {
            return None;
        }
        let value = value.trim().strip_prefix('"')?;
        let end = value.rfind('"')?;
        Some(value[..end].to_owned())
    })
}

pub fn quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn unquote(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            result.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            result.push(character);
        }
    }
    if escaped {
        result.push('\\');
    }
    result
}

/// A launcher descriptor that registers the project at `path`, exactly as
/// written. The desktop passes the canonical absolute folder; the web edition
/// passes the relative `mod/<id>` form the launcher resolves against the HOI4
/// user folder.
pub fn launcher_descriptor_text(
    identity: &ProjectIdentity,
    path: &str,
) -> Result<String, AppError> {
    if path.is_empty() || path.contains('\0') || path.contains('\n') || path.contains('\r') {
        return Err(AppError::InvalidInput(
            "project path contains a descriptor-breaking control character".into(),
        ));
    }
    let descriptor = render_descriptor_mod(identity)?;
    Ok(format!("{descriptor}path=\"{}\"\n", quote(path)))
}
