//! A small, deterministic ZIP writer for the web edition's export.
//!
//! Entries are deflated (or stored when deflate does not help), named with
//! UTF-8 project-relative paths, and stamped with a fixed timestamp so the
//! same input always produces the same archive. ZIP64 is not written; the
//! writer refuses archives that would need it.

use crate::text::normalize_relative_path;
use crate::AppError;

const LOCAL_HEADER: u32 = 0x0403_4b50;
const CENTRAL_HEADER: u32 = 0x0201_4b50;
const END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;
/// 1980-01-01 00:00, the earliest DOS date.
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = (1 << 5) | 1;
const UTF8_NAMES: u16 = 1 << 11;
const STORED: u16 = 0;
const DEFLATED: u16 = 8;
/// Version 2.0, made by a host with no special file attributes.
const VERSION: u16 = 20;
const MAX_ENTRIES: usize = 65_000;

/// One file to place in the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntry {
    pub path: String,
    pub bytes: Vec<u8>,
}

fn push_u16(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn too_large() -> AppError {
    AppError::InvalidInput("the export is too large for a standard ZIP archive".into())
}

/// Write `entries` as a ZIP archive. Paths must be safe project-relative paths
/// and unique; directories are implied by the file paths.
pub fn write_zip(entries: &[ZipEntry]) -> Result<Vec<u8>, AppError> {
    if entries.len() > MAX_ENTRIES {
        return Err(too_large());
    }
    let mut names = std::collections::BTreeSet::new();
    let mut archive = Vec::new();
    let mut central = Vec::new();
    for entry in entries {
        let name = normalize_relative_path(&entry.path)?;
        if !names.insert(name.to_lowercase()) {
            return Err(AppError::PathSecurity(format!(
                "the export lists {name} twice"
            )));
        }
        let size = u32::try_from(entry.bytes.len()).map_err(|_| too_large())?;
        let crc = crc32fast::hash(&entry.bytes);
        let deflated = miniz_oxide::deflate::compress_to_vec(&entry.bytes, 6);
        let (method, data) = if deflated.len() < entry.bytes.len() {
            (DEFLATED, deflated)
        } else {
            (STORED, entry.bytes.clone())
        };
        let compressed = u32::try_from(data.len()).map_err(|_| too_large())?;
        let offset = u32::try_from(archive.len()).map_err(|_| too_large())?;
        let name_length = u16::try_from(name.len()).map_err(|_| too_large())?;

        push_u32(&mut archive, LOCAL_HEADER);
        push_u16(&mut archive, VERSION);
        push_u16(&mut archive, UTF8_NAMES);
        push_u16(&mut archive, method);
        push_u16(&mut archive, DOS_TIME);
        push_u16(&mut archive, DOS_DATE);
        push_u32(&mut archive, crc);
        push_u32(&mut archive, compressed);
        push_u32(&mut archive, size);
        push_u16(&mut archive, name_length);
        push_u16(&mut archive, 0);
        archive.extend_from_slice(name.as_bytes());
        archive.extend_from_slice(&data);

        push_u32(&mut central, CENTRAL_HEADER);
        push_u16(&mut central, VERSION);
        push_u16(&mut central, VERSION);
        push_u16(&mut central, UTF8_NAMES);
        push_u16(&mut central, method);
        push_u16(&mut central, DOS_TIME);
        push_u16(&mut central, DOS_DATE);
        push_u32(&mut central, crc);
        push_u32(&mut central, compressed);
        push_u32(&mut central, size);
        push_u16(&mut central, name_length);
        push_u16(&mut central, 0); // extra field
        push_u16(&mut central, 0); // comment
        push_u16(&mut central, 0); // disk number
        push_u16(&mut central, 0); // internal attributes
        push_u32(&mut central, 0); // external attributes
        push_u32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let central_offset = u32::try_from(archive.len()).map_err(|_| too_large())?;
    let central_size = u32::try_from(central.len()).map_err(|_| too_large())?;
    let count = u16::try_from(entries.len()).map_err(|_| too_large())?;
    archive.extend_from_slice(&central);
    push_u32(&mut archive, END_OF_CENTRAL_DIRECTORY);
    push_u16(&mut archive, 0);
    push_u16(&mut archive, 0);
    push_u16(&mut archive, count);
    push_u16(&mut archive, count);
    push_u32(&mut archive, central_size);
    push_u32(&mut archive, central_offset);
    push_u16(&mut archive, 0);
    Ok(archive)
}

/// Read back an archive written by [`write_zip`]: paths and contents, with
/// CRC checks. Used by tests and by the web edition to verify a download.
pub fn read_zip(archive: &[u8]) -> Result<Vec<ZipEntry>, AppError> {
    let malformed = || AppError::InvalidInput("the ZIP archive is malformed".into());
    let u16_at = |at: usize| -> Result<u16, AppError> {
        archive
            .get(at..at + 2)
            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
            .ok_or_else(malformed)
    };
    let u32_at = |at: usize| -> Result<u32, AppError> {
        archive
            .get(at..at + 4)
            .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .ok_or_else(malformed)
    };
    let end = archive.len().checked_sub(22).ok_or_else(malformed)?;
    if u32_at(end)? != END_OF_CENTRAL_DIRECTORY {
        return Err(malformed());
    }
    let count = u16_at(end + 10)? as usize;
    let mut cursor = u32_at(end + 16)? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        if u32_at(cursor)? != CENTRAL_HEADER {
            return Err(malformed());
        }
        let method = u16_at(cursor + 10)?;
        let crc = u32_at(cursor + 16)?;
        let compressed = u32_at(cursor + 20)? as usize;
        let size = u32_at(cursor + 24)? as usize;
        let name_length = u16_at(cursor + 28)? as usize;
        let extra_length = u16_at(cursor + 30)? as usize;
        let comment_length = u16_at(cursor + 32)? as usize;
        let offset = u32_at(cursor + 42)? as usize;
        let name = archive
            .get(cursor + 46..cursor + 46 + name_length)
            .ok_or_else(malformed)?;
        let name = String::from_utf8(name.to_vec()).map_err(|_| malformed())?;
        let local_name = u16_at(offset + 26)? as usize;
        let local_extra = u16_at(offset + 28)? as usize;
        let data_start = offset + 30 + local_name + local_extra;
        let data = archive
            .get(data_start..data_start + compressed)
            .ok_or_else(malformed)?;
        let bytes = match method {
            STORED => data.to_vec(),
            DEFLATED => miniz_oxide::inflate::decompress_to_vec_with_limit(data, size)
                .map_err(|_| malformed())?,
            _ => return Err(malformed()),
        };
        if bytes.len() != size || crc32fast::hash(&bytes) != crc {
            return Err(malformed());
        }
        entries.push(ZipEntry { path: name, bytes });
        cursor += 46 + name_length + extra_length + comment_length;
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_round_trip_deterministically() {
        let entries = vec![
            ZipEntry {
                path: "demo/descriptor.mod".into(),
                bytes: b"name=\"Demo\"\n".to_vec(),
            },
            ZipEntry {
                path: "demo/docs/long.md".into(),
                bytes: "repeated line\n".repeat(400).into_bytes(),
            },
            ZipEntry {
                path: "demo/empty.txt".into(),
                bytes: Vec::new(),
            },
        ];
        let archive = write_zip(&entries).unwrap();
        assert_eq!(
            archive,
            write_zip(&entries).unwrap(),
            "output is deterministic"
        );
        assert!(archive.len() < 3_000, "compressible files are deflated");
        assert_eq!(read_zip(&archive).unwrap(), entries);
    }

    #[test]
    fn unsafe_or_duplicate_names_are_refused() {
        for path in ["../escape.txt", "/abs.txt", "C:/x.txt"] {
            let entry = ZipEntry {
                path: path.into(),
                bytes: Vec::new(),
            };
            assert!(write_zip(&[entry]).is_err(), "{path}");
        }
        let duplicate = vec![
            ZipEntry {
                path: "a/File.txt".into(),
                bytes: Vec::new(),
            },
            ZipEntry {
                path: "a/file.txt".into(),
                bytes: Vec::new(),
            },
        ];
        assert!(
            write_zip(&duplicate).is_err(),
            "case-insensitive duplicates"
        );
        let mut corrupt = write_zip(&[ZipEntry {
            path: "a.txt".into(),
            bytes: b"hello".to_vec(),
        }])
        .unwrap();
        let data = corrupt.iter().position(|byte| *byte == b'h').unwrap();
        corrupt[data] = b'j';
        assert!(read_zip(&corrupt).is_err(), "CRC mismatches are detected");
    }
}
