//! Tests of the hoi4-imagegen release-binary install, update, removal, the
//! sign-in step and the scoped provider keys. Archives are built here; nothing
//! touches the network, the real keychain or any user-level configuration.

use crate::credentials::{
    image_generation_credential_reference, save_image_generation_key, MemoryCredentialStore,
};
use crate::imagegen::{scoped_environment, CliRunner, InstalledImagegen};
use crate::imagegen_actions::{
    cleanup_action, pending_from_evidence, plan_install, planned_cleanups, receipt_from_evidence,
    run_reviewed_actions, InstallPlanning, CLEANUP_ACTION_ID, INSTALL_ACTION_ID,
};
use crate::models::{ImageGenerationSelection, InstallationPlan, PendingReleaseRemoval, Platform};
use crate::process::{ProcessResult, ProcessSpec};
use crate::release_install::{
    extract_declared, install_release, installed_tree, remove_release, ReleaseDownloader,
    RemoveResult,
};
use crate::AppError;
use hoi4_setup_core::release_binary::{self as route, ArchiveKind, ReleaseTarget};
use hoi4_setup_core::test_support::{manifest_with_hoi4_imagegen, synthetic_targets};
use hoi4_setup_core::zip::{write_zip, ZipEntry};
use std::cell::{Cell, RefCell};
use std::io::Write;
use std::path::PathBuf;

const MCP: &[u8] = b"fake hoi4-imagegen-mcp binary";
const CLI: &[u8] = b"fake hoi4-imagegen cli binary";

fn host_triple() -> &'static str {
    route::host_triple(Platform::current(), std::env::consts::ARCH)
        .expect("tests run on a platform with a release")
}

fn folder(version: &str, triple: &str) -> String {
    format!("hoi4-imagegen-v{version}-{triple}")
}

fn binary_names(triple: &str) -> (String, String) {
    let suffix = if triple.contains("windows") {
        ".exe"
    } else {
        ""
    };
    (
        format!("hoi4-imagegen-mcp{suffix}"),
        format!("hoi4-imagegen{suffix}"),
    )
}

/// One tar entry written by hand, so tests can build hostile archives that a
/// well-behaved library refuses to write.
fn raw_tar_entry(name: &str, typeflag: u8, link: &str, data: &[u8]) -> Vec<u8> {
    let mut header = [0u8; 512];
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    header[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].copy_from_slice(b"        ");
    header[156] = typeflag;
    header[157..157 + link.len()].copy_from_slice(link.as_bytes());
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let sum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    let mut out = header.to_vec();
    out.extend_from_slice(data);
    out.resize(out.len().div_ceil(512) * 512, 0);
    out
}

fn gzip(tar: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(tar).unwrap();
    encoder.finish().unwrap()
}

fn tar_gz(entries: &[(&str, u8, &str, &[u8])]) -> Vec<u8> {
    let mut tar = Vec::new();
    for (name, flag, link, data) in entries {
        tar.extend(raw_tar_entry(name, *flag, link, data));
    }
    tar.extend([0u8; 1024]);
    gzip(&tar)
}

/// A release archive for `triple` holding the two fake binaries and a licence
/// that nothing declares.
fn release_archive(version: &str, triple: &str, mcp: &[u8], cli: &[u8]) -> (Vec<u8>, ArchiveKind) {
    let dir = folder(version, triple);
    let (mcp_name, cli_name) = binary_names(triple);
    if triple.contains("windows") {
        let bytes = write_zip(&[
            ZipEntry {
                path: format!("{dir}/{mcp_name}"),
                bytes: mcp.to_vec(),
            },
            ZipEntry {
                path: format!("{dir}/{cli_name}"),
                bytes: cli.to_vec(),
            },
            ZipEntry {
                path: format!("{dir}/LICENSE"),
                bytes: b"MIT".to_vec(),
            },
        ])
        .unwrap();
        (bytes, ArchiveKind::Zip)
    } else {
        let mcp_path = format!("{dir}/{mcp_name}");
        let cli_path = format!("{dir}/{cli_name}");
        let license = format!("{dir}/LICENSE");
        let bytes = tar_gz(&[
            (&mcp_path, b'0', "", mcp),
            (&cli_path, b'0', "", cli),
            (&license, b'0', "", b"MIT"),
        ]);
        (bytes, ArchiveKind::TarGz)
    }
}

fn target_for(version: &str, triple: &str, archive: &[u8], kind: ArchiveKind) -> ReleaseTarget {
    let dir = folder(version, triple);
    let (mcp_name, cli_name) = binary_names(triple);
    route::parse_target(
        version,
        triple,
        &serde_json::json!({
            "url": route::expected_url(version, triple, kind),
            "sha256": route::sha256_hex(archive),
            "size": archive.len(),
            "archive": kind.extension(),
            "entries": {"mcp": format!("{dir}/{mcp_name}"), "cli": format!("{dir}/{cli_name}")},
        }),
    )
    .unwrap()
}

struct Release {
    archive: Vec<u8>,
    target: ReleaseTarget,
}

fn release(version: &str) -> Release {
    let (archive, kind) = release_archive(version, host_triple(), MCP, CLI);
    let target = target_for(version, host_triple(), &archive, kind);
    Release { archive, target }
}

struct FakeDownloader {
    bytes: Vec<u8>,
    calls: Cell<usize>,
}

impl FakeDownloader {
    fn new(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
            calls: Cell::new(0),
        }
    }
}

impl ReleaseDownloader for FakeDownloader {
    fn download(&self, url: &str, _size: u64) -> Result<Vec<u8>, AppError> {
        assert!(
            url.starts_with("https://github.com/klimPaskov/hoi4-imagegen-mcp/releases/download/"),
            "only the declared release URL may be requested: {url}"
        );
        self.calls.set(self.calls.get() + 1);
        Ok(self.bytes.clone())
    }
}

/// Records every process Setup would launch and answers like the CLI.
struct FakeCli {
    version_output: RefCell<String>,
    signed_in: Cell<bool>,
    /// (arguments, scoped variable names) of every launch.
    launches: RefCell<Vec<(Vec<String>, Vec<String>)>>,
    doctor_echo: RefCell<String>,
    /// Replaces the `login-status` answer: (exit code, timed out, stdout).
    status_override: RefCell<Option<(Option<i32>, bool, String)>>,
}

impl FakeCli {
    fn reporting(version: &str) -> Self {
        Self {
            version_output: RefCell::new(format!("hoi4-imagegen {version}\n")),
            signed_in: Cell::new(false),
            launches: RefCell::new(Vec::new()),
            doctor_echo: RefCell::new(String::new()),
            status_override: RefCell::new(None),
        }
    }

    fn arguments(&self) -> Vec<Vec<String>> {
        self.launches
            .borrow()
            .iter()
            .map(|(arguments, _)| arguments.clone())
            .collect()
    }
}

impl CliRunner for FakeCli {
    fn run(
        &self,
        spec: &ProcessSpec,
        environment: Option<&crate::credentials::ScopedSecretEnvironment>,
    ) -> Result<ProcessResult, AppError> {
        // The spec is exactly what the real runner would validate and spawn.
        spec.validate(std::slice::from_ref(&spec.executable))
            .expect("the process spec is valid and bound to the file hash");
        assert!(spec.executable.is_absolute());
        assert!(spec.executable.to_string_lossy().contains("hoi4-imagegen"));
        self.launches.borrow_mut().push((
            spec.args.clone(),
            environment
                .map(|environment| environment.variable_names())
                .unwrap_or_default(),
        ));
        assert_eq!(
            spec.environment_names,
            environment
                .map(|environment| environment.variable_names())
                .unwrap_or_default()
        );
        if spec.args.first().map(String::as_str) == Some("login-status") {
            if let Some((status_code, timed_out, stdout)) = self.status_override.borrow().clone() {
                return Ok(ProcessResult {
                    status_code,
                    stdout,
                    stderr: String::new(),
                    timed_out,
                    stdout_truncated: false,
                    stderr_truncated: false,
                });
            }
        }
        let (code, stdout) = match spec.args.first().map(String::as_str) {
            Some("--version") => (0, self.version_output.borrow().clone()),
            Some("login-risk-note") => (0, "Risk note: this uses an unofficial route.".to_owned()),
            Some("login") => {
                assert_eq!(
                    spec.args,
                    ["login", "--accept-risk"],
                    "never without the accepted risk"
                );
                self.signed_in.set(true);
                (0, "Signed in. Risk note acknowledged.".to_owned())
            }
            // Like the real binary without `--check`: always exit 0, say which.
            Some("login-status") => (
                0,
                if self.signed_in.get() {
                    "signed in; account acc_1; plan plus; token expires at 2030-01-01".to_owned()
                } else {
                    "not signed in (run `hoi4-imagegen login`; tokens would go to /x)".to_owned()
                },
            ),
            Some("doctor") => (0, self.doctor_echo.borrow().clone()),
            other => panic!("unexpected hoi4-imagegen arguments: {other:?}"),
        };
        Ok(ProcessResult {
            status_code: Some(code),
            stdout,
            stderr: String::new(),
            timed_out: false,
            stdout_truncated: false,
            stderr_truncated: false,
        })
    }
}

fn project() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn root(dir: &tempfile::TempDir) -> PathBuf {
    std::fs::canonicalize(dir.path()).unwrap()
}

fn version_path(root: &std::path::Path, version: &str) -> PathBuf {
    root.join(route::version_dir(version))
}

// ---- Extraction ------------------------------------------------------------

#[test]
fn zip_and_tar_extract_only_the_declared_entries() {
    for triple in route::TARGETS {
        let (archive, kind) = release_archive("0.2.0", triple, MCP, CLI);
        let dir = folder("0.2.0", triple);
        let (mcp_name, cli_name) = binary_names(triple);
        let mcp = format!("{dir}/{mcp_name}");
        let cli = format!("{dir}/{cli_name}");
        let files = extract_declared(&archive, kind, &[&mcp, &cli]).unwrap();
        assert_eq!(files.len(), 2, "{triple}: the licence is not extracted");
        assert_eq!(files[0].bytes, MCP);
        assert_eq!(files[1].bytes, CLI);
    }
}

#[test]
fn a_missing_or_duplicated_declared_entry_is_refused() {
    let (archive, kind) = release_archive("0.2.0", host_triple(), MCP, CLI);
    assert!(extract_declared(&archive, kind, &["nope/hoi4-imagegen-mcp"]).is_err());
    let twice = tar_gz(&[
        ("a/hoi4-imagegen-mcp", b'0', "", b"one"),
        ("a/hoi4-imagegen-mcp", b'0', "", b"two"),
    ]);
    assert!(extract_declared(&twice, ArchiveKind::TarGz, &["a/hoi4-imagegen-mcp"]).is_err());
    assert!(extract_declared(&archive, kind, &["x", "x"]).is_err());
}

#[test]
fn traversal_and_absolute_entry_names_are_refused_even_when_not_declared() {
    for name in [
        "../evil",
        "a/../../evil",
        "/abs/evil",
        "C:/evil",
        "a\\..\\evil",
    ] {
        let hostile = tar_gz(&[
            ("ok/hoi4-imagegen-mcp", b'0', "", MCP),
            (name, b'0', "", b"x"),
        ]);
        assert!(
            extract_declared(&hostile, ArchiveKind::TarGz, &["ok/hoi4-imagegen-mcp"]).is_err(),
            "tar {name}"
        );
    }
    // A zip name patched to a traversal path after it was written.
    let mut zip = write_zip(&[
        ZipEntry {
            path: "ok/hoi4-imagegen-mcp".into(),
            bytes: MCP.to_vec(),
        },
        ZipEntry {
            path: "aa/evil".into(),
            bytes: b"x".to_vec(),
        },
    ])
    .unwrap();
    assert!(extract_declared(&zip, ArchiveKind::Zip, &["ok/hoi4-imagegen-mcp"]).is_ok());
    let needle = b"aa/evil";
    let mut patched = 0;
    for at in 0..zip.len() - needle.len() {
        if &zip[at..at + needle.len()] == needle {
            zip[at..at + needle.len()].copy_from_slice(b"../evil");
            patched += 1;
        }
    }
    assert_eq!(patched, 2, "local and central header names");
    assert!(extract_declared(&zip, ArchiveKind::Zip, &["ok/hoi4-imagegen-mcp"]).is_err());
}

#[test]
fn links_and_special_files_are_refused() {
    for flag in [b'2', b'1', b'3', b'4', b'6'] {
        let hostile = tar_gz(&[
            ("ok/hoi4-imagegen-mcp", b'0', "", MCP),
            ("ok/link", flag, "/etc/passwd", b""),
        ]);
        assert!(
            extract_declared(&hostile, ArchiveKind::TarGz, &["ok/hoi4-imagegen-mcp"]).is_err(),
            "type {}",
            flag as char
        );
    }
    // A zip entry marked as a Unix symbolic link.
    let mut zip = write_zip(&[
        ZipEntry {
            path: "ok/hoi4-imagegen-mcp".into(),
            bytes: MCP.to_vec(),
        },
        ZipEntry {
            path: "ok/link".into(),
            bytes: b"/etc/passwd".to_vec(),
        },
    ])
    .unwrap();
    let central = [0x50, 0x4b, 0x01, 0x02];
    let mut patched = 0;
    for at in 0..zip.len() - 46 {
        if zip[at..at + 4] == central {
            let name_length = u16::from_le_bytes([zip[at + 28], zip[at + 29]]) as usize;
            if &zip[at + 46..at + 46 + name_length] == b"ok/link" {
                zip[at + 4..at + 6].copy_from_slice(&0x031eu16.to_le_bytes());
                zip[at + 38..at + 42].copy_from_slice(&(0o120_777u32 << 16).to_le_bytes());
                patched += 1;
            }
        }
    }
    assert_eq!(patched, 1);
    assert!(extract_declared(&zip, ArchiveKind::Zip, &["ok/hoi4-imagegen-mcp"]).is_err());
}

#[test]
fn a_corrupt_zip_entry_fails_its_checksum() {
    let mut zip = write_zip(&[ZipEntry {
        path: "ok/hoi4-imagegen-mcp".into(),
        bytes: MCP.to_vec(),
    }])
    .unwrap();
    let start = zip
        .windows(MCP.len())
        .position(|window| window == MCP)
        .expect("a stored entry");
    zip[start] ^= 0xff;
    assert!(extract_declared(&zip, ArchiveKind::Zip, &["ok/hoi4-imagegen-mcp"]).is_err());
    assert!(extract_declared(b"not an archive", ArchiveKind::Zip, &["x"]).is_err());
    assert!(extract_declared(b"not an archive", ArchiveKind::TarGz, &["x"]).is_err());
}

// ---- Verified install, update, uninstall ------------------------------------

#[test]
fn a_hash_or_size_mismatch_is_refused_before_the_archive_is_opened() {
    // The bytes served are a traversal archive. If it were opened its name
    // would be rejected, so seeing the hash error proves the order.
    let good = release("0.2.0");
    let hostile = tar_gz(&[("../evil", b'0', "", b"x")]);
    for served in [hostile, good.archive[..good.archive.len() - 1].to_vec(), {
        let mut flipped = good.archive.clone();
        let last = flipped.len() - 1;
        flipped[last] ^= 1;
        flipped
    }] {
        let dir = project();
        let error = install_release(
            &root(&dir),
            "0.2.0",
            &good.target,
            None,
            &FakeDownloader::new(&served),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("SHA-256") || error.contains("bytes"),
            "{error}"
        );
        assert!(
            !root(&dir).join(".tools").exists(),
            "nothing is written for a refused download"
        );
    }
}

#[test]
fn the_declared_url_is_the_only_one_requested() {
    let good = release("0.2.0");
    let mut other = good.target.clone();
    other.url = other.url.replace("v0.2.0", "v9.9.9");
    let dir = project();
    let downloader = FakeDownloader::new(&good.archive);
    assert!(install_release(&root(&dir), "0.2.0", &other, None, &downloader).is_err());
    assert_eq!(downloader.calls.get(), 0, "no request was made");
}

#[test]
fn install_update_and_uninstall_leave_an_exact_journalled_tree() {
    let dir = project();
    let root = root(&dir);
    let v1 = release("0.2.0");
    let receipt = install_release(
        &root,
        "0.2.0",
        &v1.target,
        None,
        &FakeDownloader::new(&v1.archive),
    )
    .unwrap();
    assert!(!receipt.reused);
    let (mcp_name, cli_name) = binary_names(host_triple());
    assert_eq!(
        std::fs::read(version_path(&root, "0.2.0").join(&mcp_name)).unwrap(),
        MCP
    );
    assert_eq!(
        std::fs::read(version_path(&root, "0.2.0").join(&cli_name)).unwrap(),
        CLI
    );
    assert_eq!(
        receipt.files.len(),
        2,
        "only the declared entries are installed"
    );
    assert!(!version_path(&root, "0.2.0").join("LICENSE").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in [&mcp_name, &cli_name] {
            let mode = std::fs::metadata(version_path(&root, "0.2.0").join(name))
                .unwrap()
                .permissions()
                .mode();
            assert_ne!(mode & 0o111, 0, "{name} is executable");
        }
    }
    let (_, tree) = installed_tree(&root, "0.2.0").unwrap().unwrap();
    assert_eq!(tree, receipt.tree_sha256);

    // The recorded tree makes a second install a no-op: no download.
    let offline = FakeDownloader::new(b"must not be requested");
    let again = install_release(&root, "0.2.0", &v1.target, Some(&tree), &offline).unwrap();
    assert!(again.reused);
    assert_eq!(offline.calls.get(), 0);

    // An update installs the new version beside the old one.
    let (archive2, kind2) = release_archive("0.3.0", host_triple(), b"new mcp", b"new cli");
    let target2 = target_for("0.3.0", host_triple(), &archive2, kind2);
    let receipt2 = install_release(
        &root,
        "0.3.0",
        &target2,
        None,
        &FakeDownloader::new(&archive2),
    )
    .unwrap();
    assert_ne!(receipt2.tree_sha256, receipt.tree_sha256);
    assert!(version_path(&root, "0.2.0").is_dir() && version_path(&root, "0.3.0").is_dir());

    // The old version goes only while it still matches what was recorded.
    std::fs::write(version_path(&root, "0.2.0").join(&cli_name), b"edited").unwrap();
    assert_eq!(
        remove_release(&root, "0.2.0", &receipt.tree_sha256).unwrap(),
        RemoveResult::ModifiedKept
    );
    assert!(
        version_path(&root, "0.2.0").is_dir(),
        "a changed folder is kept"
    );
    std::fs::write(version_path(&root, "0.2.0").join(&cli_name), CLI).unwrap();
    assert_eq!(
        remove_release(&root, "0.2.0", &receipt.tree_sha256).unwrap(),
        RemoveResult::Removed
    );
    assert!(!version_path(&root, "0.2.0").exists());
    assert_eq!(
        remove_release(&root, "0.2.0", &receipt.tree_sha256).unwrap(),
        RemoveResult::AlreadyAbsent
    );
    // Uninstall: the last version goes and the install root with it.
    assert_eq!(
        remove_release(&root, "0.3.0", &receipt2.tree_sha256).unwrap(),
        RemoveResult::Removed
    );
    assert!(!root.join(route::INSTALL_ROOT).exists());
}

/// The Ready screen starts the hoi4-imagegen sign-in, which keeps the program running; an update
/// then cannot delete the previous version on Windows and must keep it instead of failing.
#[cfg(windows)]
#[test]
fn a_previous_version_that_is_in_use_is_kept_not_a_failure() {
    use std::os::windows::fs::OpenOptionsExt;

    let dir = project();
    let root = root(&dir);
    let v1 = release("0.2.0");
    let receipt = install_release(
        &root,
        "0.2.0",
        &v1.target,
        None,
        &FakeDownloader::new(&v1.archive),
    )
    .unwrap();
    let (_, cli_name) = binary_names(host_triple());
    // Share read only: a delete of the open file is refused, as for a running program.
    let running = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(version_path(&root, "0.2.0").join(&cli_name))
        .unwrap();
    let RemoveResult::InUseKept { remaining_tree } =
        remove_release(&root, "0.2.0", &receipt.tree_sha256).unwrap()
    else {
        panic!("a version in use is kept for a later retry");
    };
    assert!(version_path(&root, "0.2.0").join(&cli_name).is_file());
    // The delete may stop part way; the retry is bound to the recorded files left.
    let (left, tree) = installed_tree(&root, "0.2.0").unwrap().unwrap();
    assert_eq!(remaining_tree, tree);
    assert!(left.iter().all(|file| receipt.files.contains(file)));
    drop(running);
    assert_eq!(
        remove_release(&root, "0.2.0", &remaining_tree).unwrap(),
        RemoveResult::Removed
    );
    assert!(!version_path(&root, "0.2.0").exists());
}

/// The acceptance check of #195: install A, hold A's program open, update to B. The transaction
/// keeps A and records it as pending; once the program is closed the next run removes it.
#[cfg(windows)]
#[test]
fn a_version_kept_in_use_is_recorded_and_removed_by_the_next_run() {
    use std::os::windows::fs::OpenOptionsExt;

    let dir = project();
    let root = root(&dir);
    let old = install_fake_release(&root, "0.2.0");
    let (archive2, kind2) = release_archive("0.3.0", host_triple(), b"new mcp", b"new cli");
    let v2 = Release {
        target: target_for("0.3.0", host_triple(), &archive2, kind2),
        archive: archive2,
    };
    let (mut plan, _) = install_plan("0.3.0", &v2);
    plan.external_actions
        .push(cleanup_action("0.2.0", &old.tree_sha256));
    plan.image_generation = Some(ImageGenerationSelection {
        installed_release_version: Some("0.2.0".into()),
        installed_release_tree_sha256: Some(old.tree_sha256.clone()),
        ..selection("hoi4_imagegen")
    });
    let (_, cli_name) = binary_names(host_triple());
    let running = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(version_path(&root, "0.2.0").join(&cli_name))
        .unwrap();
    let cli = FakeCli::reporting("0.3.0");
    let outcome =
        run_reviewed_actions(&root, &plan, &FakeDownloader::new(&v2.archive), &cli).unwrap();
    assert_eq!(outcome.state, "ready", "{}", outcome.evidence);
    assert!(outcome.evidence.contains("old-version-kept:0.2.0"));
    let pending = pending_from_evidence(&outcome.evidence).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].version, "0.2.0");
    assert_eq!(
        pending[0].tree_sha256,
        installed_tree(&root, "0.2.0").unwrap().unwrap().1
    );
    let (version, tree) = receipt_from_evidence(&outcome.evidence).unwrap().unwrap();
    assert_eq!(version.as_deref(), Some("0.3.0"));

    // Still in use on the next run: it stays pending, nothing fails.
    let recorded = ImageGenerationSelection {
        installed_release_version: version,
        installed_release_tree_sha256: tree,
        pending_removals: pending,
        ..selection("hoi4_imagegen")
    };
    let mut next = install_plan("0.3.0", &v2).0;
    next.maintenance_mode = Some("repair".into());
    next.external_actions
        .extend(planned_cleanups(Some(&recorded), Some("0.3.0")));
    next.image_generation = Some(recorded.clone());
    let offline = FakeDownloader::new(b"must not be requested");
    let again = run_reviewed_actions(&root, &next, &offline, &cli).unwrap();
    assert_eq!(offline.calls.get(), 0, "the installed version is reused");
    assert_eq!(
        pending_from_evidence(&again.evidence).unwrap(),
        recorded.pending_removals
    );

    // Closed: the retry removes it and the pending list is empty.
    drop(running);
    let done = run_reviewed_actions(&root, &next, &offline, &cli).unwrap();
    assert_eq!(done.state, "ready");
    assert!(pending_from_evidence(&done.evidence).unwrap().is_empty());
    assert!(!version_path(&root, "0.2.0").exists());
    assert!(version_path(&root, "0.3.0").is_dir());
}

#[test]
fn a_folder_with_other_content_is_never_overwritten() {
    let dir = project();
    let root = root(&dir);
    let existing = version_path(&root, "0.2.0");
    std::fs::create_dir_all(&existing).unwrap();
    std::fs::write(existing.join("mine.txt"), b"keep me").unwrap();
    let good = release("0.2.0");
    let error = install_release(
        &root,
        "0.2.0",
        &good.target,
        None,
        &FakeDownloader::new(&good.archive),
    )
    .unwrap_err();
    assert!(error.to_string().contains("already exists"));
    assert_eq!(
        std::fs::read(existing.join("mine.txt")).unwrap(),
        b"keep me"
    );
}

// ---- Planned actions and the runner ----------------------------------------

fn host_manifest(version: &str, release: &Release) -> hoi4_setup_core::models::RemoteManifest {
    let mut targets = synthetic_targets(version);
    targets[host_triple()]["sha256"] = serde_json::json!(release.target.sha256);
    targets[host_triple()]["size"] = serde_json::json!(release.target.size);
    manifest_with_hoi4_imagegen(version, targets)
}

fn selection(primary: &str) -> ImageGenerationSelection {
    ImageGenerationSelection {
        primary_provider: primary.into(),
        ..Default::default()
    }
}

fn base_plan() -> InstallationPlan {
    let mut plan: InstallationPlan = serde_json::from_str(include_str!(
        "../../docs/examples/installation-plan.example.json"
    ))
    .unwrap();
    plan.maintenance_mode = Some("update".into());
    plan.external_actions = Vec::new();
    plan.image_generation = None;
    plan
}

fn install_plan(version: &str, release: &Release) -> (InstallationPlan, InstallationPlan) {
    let manifest = host_manifest(version, release);
    let InstallPlanning::Install(action) =
        plan_install(&manifest, &selection("hoi4_imagegen")).unwrap()
    else {
        panic!("the provider is selected and supported on this host");
    };
    let mut plan = base_plan();
    plan.external_actions = vec![*action];
    (plan.clone(), plan)
}

#[test]
fn planning_declares_one_reviewed_download_from_the_manifest() {
    let v1 = release("0.2.0");
    let manifest = host_manifest("0.2.0", &v1);
    assert!(matches!(
        plan_install(&manifest, &selection("none")).unwrap(),
        InstallPlanning::NotSelected
    ));
    let InstallPlanning::Install(action) =
        plan_install(&manifest, &selection("hoi4_imagegen")).unwrap()
    else {
        panic!("expected an install action");
    };
    assert_eq!(action.id, INSTALL_ACTION_ID);
    assert_eq!(action.arguments[0], v1.target.url);
    assert_eq!(
        action.verified_executable_sha256.as_deref(),
        Some(v1.target.sha256.as_str())
    );
    assert_eq!(action.verified_executable_size, Some(v1.target.size));
    assert!(action.environment_names.is_empty() && !action.contains_secret);
    assert!(action.expected_writes[0].starts_with(".tools/image_generation/hoi4-imagegen/0.2.0"));
    assert!(action.network_access.contains("github.com"));
    // An additional provider selection plans it too.
    let mut extra = selection("openai_api");
    extra.additional_providers = vec!["hoi4_imagegen".into()];
    assert!(matches!(
        plan_install(&manifest, &extra).unwrap(),
        InstallPlanning::Install(_)
    ));
}

#[test]
fn the_runner_installs_checks_the_version_and_replaces_the_old_one() {
    let dir = project();
    let root = root(&dir);
    // Version 0.2.0 is already installed and recorded.
    let v1 = release("0.2.0");
    let old = install_release(
        &root,
        "0.2.0",
        &v1.target,
        None,
        &FakeDownloader::new(&v1.archive),
    )
    .unwrap();
    // Update to 0.3.0: an install action and a cleanup of the recorded tree.
    let (archive2, kind2) = release_archive("0.3.0", host_triple(), b"new mcp", b"new cli");
    let v2 = Release {
        target: target_for("0.3.0", host_triple(), &archive2, kind2),
        archive: archive2,
    };
    let (mut plan, _) = install_plan("0.3.0", &v2);
    plan.external_actions
        .push(cleanup_action("0.2.0", &old.tree_sha256));
    let downloader = FakeDownloader::new(&v2.archive);
    let cli = FakeCli::reporting("0.3.0");
    let outcome = run_reviewed_actions(&root, &plan, &downloader, &cli).unwrap();
    assert_eq!(outcome.component_id, route::COMPONENT_ID);
    assert_eq!(outcome.state, "ready");
    assert_eq!(downloader.calls.get(), 1);
    assert_eq!(cli.arguments(), vec![vec!["--version".to_owned()]]);
    assert!(version_path(&root, "0.3.0").is_dir());
    assert!(
        !version_path(&root, "0.2.0").exists(),
        "the old version goes after the new one passed its check"
    );
    let (version, tree) = receipt_from_evidence(&outcome.evidence).unwrap().unwrap();
    assert_eq!(version.as_deref(), Some("0.3.0"));
    assert_eq!(
        tree.unwrap(),
        installed_tree(&root, "0.3.0").unwrap().unwrap().1
    );
}

#[test]
fn a_failed_version_check_keeps_the_old_version_and_reports_incomplete() {
    let dir = project();
    let root = root(&dir);
    let v1 = release("0.2.0");
    let old = install_release(
        &root,
        "0.2.0",
        &v1.target,
        None,
        &FakeDownloader::new(&v1.archive),
    )
    .unwrap();
    let (archive2, kind2) = release_archive("0.3.0", host_triple(), b"new mcp", b"new cli");
    let v2 = Release {
        target: target_for("0.3.0", host_triple(), &archive2, kind2),
        archive: archive2,
    };
    let (mut plan, _) = install_plan("0.3.0", &v2);
    plan.external_actions
        .push(cleanup_action("0.2.0", &old.tree_sha256));
    // The binary claims a different version than the manifest declares.
    let cli = FakeCli::reporting("0.2.9");
    let outcome =
        run_reviewed_actions(&root, &plan, &FakeDownloader::new(&v2.archive), &cli).unwrap();
    assert_eq!(outcome.state, "incomplete");
    assert!(outcome.evidence.contains("version-check-failed"));
    assert!(
        version_path(&root, "0.2.0").is_dir(),
        "the old version stays"
    );
    assert!(version_path(&root, "0.3.0").is_dir());
    // The lock records the new version, so the old one is kept pending removal.
    assert_eq!(
        pending_from_evidence(&outcome.evidence).unwrap(),
        vec![PendingReleaseRemoval {
            version: "0.2.0".into(),
            tree_sha256: old.tree_sha256,
        }]
    );
}

#[test]
fn an_edited_install_action_is_refused() {
    let dir = project();
    let root = root(&dir);
    let v1 = release("0.2.0");
    let (plan, _) = install_plan("0.2.0", &v1);
    for edit in [
        |action: &mut hoi4_setup_core::models::ExternalAction| {
            action.arguments[0] = action.arguments[0].replace("klimPaskov", "someone");
        },
        |action: &mut hoi4_setup_core::models::ExternalAction| {
            action.arguments[3] = "../../escape/hoi4-imagegen-mcp".into();
        },
        |action: &mut hoi4_setup_core::models::ExternalAction| {
            action.verified_executable_sha256 = Some("0".repeat(63));
        },
        |action: &mut hoi4_setup_core::models::ExternalAction| {
            action.environment_names = vec!["OPENAI_API_KEY".into()];
        },
        |action: &mut hoi4_setup_core::models::ExternalAction| {
            action.command_source = "remote_manifest".into();
        },
    ] {
        let mut tampered = plan.clone();
        edit(&mut tampered.external_actions[0]);
        let downloader = FakeDownloader::new(&v1.archive);
        assert!(
            run_reviewed_actions(&root, &tampered, &downloader, &FakeCli::reporting("0.2.0"))
                .is_err()
        );
        assert_eq!(downloader.calls.get(), 0);
    }
    assert!(run_reviewed_actions(
        &root,
        &base_plan(),
        &FakeDownloader::new(&v1.archive),
        &FakeCli::reporting("0.2.0")
    )
    .is_err());
}

#[test]
fn removal_runs_only_the_cleanup_and_keeps_a_changed_folder() {
    let dir = project();
    let root = root(&dir);
    let v1 = release("0.2.0");
    let receipt = install_release(
        &root,
        "0.2.0",
        &v1.target,
        None,
        &FakeDownloader::new(&v1.archive),
    )
    .unwrap();
    let mut plan = base_plan();
    plan.maintenance_mode = Some("remove".into());
    plan.external_actions = vec![cleanup_action("0.2.0", &receipt.tree_sha256)];
    let cli = FakeCli::reporting("0.2.0");
    // A changed folder is kept and the component reports incomplete.
    let (_, cli_name) = binary_names(host_triple());
    std::fs::write(version_path(&root, "0.2.0").join(&cli_name), b"edited").unwrap();
    let kept = run_reviewed_actions(&root, &plan, &FakeDownloader::new(b""), &cli).unwrap();
    assert_eq!(kept.state, "incomplete");
    assert!(version_path(&root, "0.2.0").is_dir());
    std::fs::write(version_path(&root, "0.2.0").join(&cli_name), CLI).unwrap();
    let removed = run_reviewed_actions(&root, &plan, &FakeDownloader::new(b""), &cli).unwrap();
    assert_eq!(removed.state, "not_selected");
    assert_eq!(
        receipt_from_evidence(&removed.evidence).unwrap(),
        Some((None, None))
    );
    assert!(!root.join(route::INSTALL_ROOT).exists());
    assert!(cli.arguments().is_empty(), "removal never runs the binary");
    // A removal that carries an install action is refused.
    let (install, _) = install_plan("0.2.0", &v1);
    let mut mixed = plan.clone();
    mixed.external_actions.extend(install.external_actions);
    assert!(run_reviewed_actions(&root, &mixed, &FakeDownloader::new(b""), &cli).is_err());
    // The cleanup id is the planned one.
    assert_eq!(plan.external_actions[0].id, CLEANUP_ACTION_ID);
}

/// Install a fake `version` directly, as an earlier run would have, and return its receipt.
pub(crate) fn install_fake_release(
    root: &std::path::Path,
    version: &str,
) -> crate::release_install::InstallReceipt {
    let fake = release(version);
    install_release(
        root,
        version,
        &fake.target,
        None,
        &FakeDownloader::new(&fake.archive),
    )
    .unwrap()
}

fn pending(version: &str, tree: &str) -> PendingReleaseRemoval {
    PendingReleaseRemoval {
        version: version.into(),
        tree_sha256: tree.into(),
    }
}

/// A repair of 0.3.0 that also carries the cleanup of each pending entry the
/// selection records.
fn update_with_pending(
    v3: &Release,
    pending_removals: Vec<PendingReleaseRemoval>,
) -> InstallationPlan {
    let recorded = ImageGenerationSelection {
        installed_release_version: Some("0.3.0".into()),
        installed_release_tree_sha256: None,
        pending_removals,
        ..selection("hoi4_imagegen")
    };
    let (mut plan, _) = install_plan("0.3.0", v3);
    plan.external_actions
        .extend(planned_cleanups(Some(&recorded), Some("0.3.0")));
    plan.image_generation = Some(recorded);
    plan
}

#[test]
fn the_planner_cleans_up_each_pending_version_but_never_the_installed_one() {
    let tree = "a".repeat(64);
    let other = "b".repeat(64);
    let recorded = ImageGenerationSelection {
        installed_release_version: Some("0.2.0".into()),
        installed_release_tree_sha256: Some(tree.clone()),
        pending_removals: vec![
            pending("0.1.0", &other),
            pending("0.1.5", &other),
            // Never planned: the version being installed.
            pending("0.3.0", &other),
            // Never planned twice: it is also the recorded version.
            pending("0.2.0", &other),
        ],
        ..selection("hoi4_imagegen")
    };
    let actions = planned_cleanups(Some(&recorded), Some("0.3.0"));
    let planned = actions
        .iter()
        .map(|action| (action.arguments[0].as_str(), action.arguments[1].as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        planned,
        vec![
            ("0.2.0", tree.as_str()),
            ("0.1.0", other.as_str()),
            ("0.1.5", other.as_str())
        ]
    );
    for action in &actions {
        assert_eq!(action.id, CLEANUP_ACTION_ID);
        assert_eq!(
            action.expected_writes,
            [format!("{}/**", route::version_dir(&action.arguments[0]))]
        );
        assert_eq!(action.network_access, "none");
    }
    // Reinstalling the recorded version plans only the pending cleanups.
    assert_eq!(planned_cleanups(Some(&recorded), Some("0.2.0")).len(), 3);
    assert!(planned_cleanups(None, Some("0.3.0")).is_empty());
    assert!(planned_cleanups(Some(&selection("hoi4_imagegen")), None).is_empty());
}

#[test]
fn a_pending_version_is_removed_on_the_next_run_and_its_entry_dropped() {
    let dir = project();
    let root = root(&dir);
    let kept = install_fake_release(&root, "0.1.0");
    let v3 = release("0.3.0");
    let plan = update_with_pending(&v3, vec![pending("0.1.0", &kept.tree_sha256)]);
    assert_eq!(
        plan.external_actions
            .iter()
            .filter(|action| action.id == CLEANUP_ACTION_ID)
            .count(),
        1
    );
    let outcome = run_reviewed_actions(
        &root,
        &plan,
        &FakeDownloader::new(&v3.archive),
        &FakeCli::reporting("0.3.0"),
    )
    .unwrap();
    assert_eq!(outcome.state, "ready");
    assert!(!version_path(&root, "0.1.0").exists());
    assert!(version_path(&root, "0.3.0").is_dir());
    assert!(pending_from_evidence(&outcome.evidence).unwrap().is_empty());
    // An entry whose folder is already gone is dropped as well.
    let again = run_reviewed_actions(
        &root,
        &plan,
        &FakeDownloader::new(&v3.archive),
        &FakeCli::reporting("0.3.0"),
    )
    .unwrap();
    assert!(pending_from_evidence(&again.evidence).unwrap().is_empty());
}

#[test]
fn a_pending_version_the_user_changed_is_kept_and_no_longer_pending() {
    let dir = project();
    let root = root(&dir);
    let kept = install_fake_release(&root, "0.1.0");
    let (_, cli_name) = binary_names(host_triple());
    std::fs::write(version_path(&root, "0.1.0").join(&cli_name), b"my build").unwrap();
    let v3 = release("0.3.0");
    let plan = update_with_pending(&v3, vec![pending("0.1.0", &kept.tree_sha256)]);
    let outcome = run_reviewed_actions(
        &root,
        &plan,
        &FakeDownloader::new(&v3.archive),
        &FakeCli::reporting("0.3.0"),
    )
    .unwrap();
    assert!(outcome.evidence.contains("old-version-kept:0.1.0"));
    assert!(pending_from_evidence(&outcome.evidence).unwrap().is_empty());
    assert_eq!(
        std::fs::read(version_path(&root, "0.1.0").join(&cli_name)).unwrap(),
        b"my build",
        "Setup never deletes content it did not record"
    );
}

#[test]
fn a_pending_cleanup_without_an_install_never_reports_the_release_removed() {
    // The provider stays recorded at 0.3.0 while only an earlier version is cleaned up (for
    // example on a platform the new release does not cover): the receipt is unchanged.
    let dir = project();
    let root = root(&dir);
    let kept = install_fake_release(&root, "0.1.0");
    let mut plan = base_plan();
    let recorded = ImageGenerationSelection {
        installed_release_version: Some("0.3.0".into()),
        installed_release_tree_sha256: Some("c".repeat(64)),
        pending_removals: vec![pending("0.1.0", &kept.tree_sha256)],
        ..selection("hoi4_imagegen")
    };
    plan.external_actions = planned_cleanups(Some(&recorded), Some("0.3.0"));
    plan.image_generation = Some(recorded);
    let cli = FakeCli::reporting("0.3.0");
    let outcome = run_reviewed_actions(&root, &plan, &FakeDownloader::new(b""), &cli).unwrap();
    assert!(!version_path(&root, "0.1.0").exists());
    assert!(!outcome.evidence.contains(route::EVIDENCE_REMOVED));
    assert_eq!(receipt_from_evidence(&outcome.evidence).unwrap(), None);
    assert!(pending_from_evidence(&outcome.evidence).unwrap().is_empty());
    assert!(cli.arguments().is_empty());
}

#[test]
fn removal_also_removes_the_pending_versions() {
    let dir = project();
    let root = root(&dir);
    let kept = install_fake_release(&root, "0.1.0");
    let current = install_fake_release(&root, "0.2.0");
    let recorded = ImageGenerationSelection {
        installed_release_version: Some("0.2.0".into()),
        installed_release_tree_sha256: Some(current.tree_sha256.clone()),
        pending_removals: vec![pending("0.1.0", &kept.tree_sha256)],
        ..selection("hoi4_imagegen")
    };
    let mut plan = base_plan();
    plan.maintenance_mode = Some("remove".into());
    plan.external_actions = planned_cleanups(Some(&recorded), None);
    plan.image_generation = Some(recorded);
    assert_eq!(plan.external_actions.len(), 2);
    let outcome = run_reviewed_actions(
        &root,
        &plan,
        &FakeDownloader::new(b""),
        &FakeCli::reporting("0.2.0"),
    )
    .unwrap();
    assert_eq!(outcome.state, "not_selected");
    assert_eq!(
        receipt_from_evidence(&outcome.evidence).unwrap(),
        Some((None, None))
    );
    assert!(pending_from_evidence(&outcome.evidence).unwrap().is_empty());
    assert!(!root.join(route::INSTALL_ROOT).exists());
}

#[test]
fn a_duplicated_or_unrecorded_extra_cleanup_is_refused() {
    let dir = project();
    let root = root(&dir);
    let kept = install_fake_release(&root, "0.1.0");
    let v3 = release("0.3.0");
    let plan = update_with_pending(&v3, vec![pending("0.1.0", &kept.tree_sha256)]);
    let run = |plan: &InstallationPlan| {
        run_reviewed_actions(
            &root,
            plan,
            &FakeDownloader::new(&v3.archive),
            &FakeCli::reporting("0.3.0"),
        )
    };
    // The same version twice.
    let mut duplicated = plan.clone();
    duplicated
        .external_actions
        .push(cleanup_action("0.1.0", &kept.tree_sha256));
    assert!(run(&duplicated).is_err());
    // A second cleanup that the selection does not list as pending.
    let mut unrecorded = plan.clone();
    unrecorded
        .external_actions
        .push(cleanup_action("0.0.9", &"d".repeat(64)));
    unrecorded
        .external_actions
        .push(cleanup_action("0.0.8", &"d".repeat(64)));
    assert!(run(&unrecorded).is_err());
    assert!(
        version_path(&root, "0.1.0").is_dir(),
        "a refused plan removes nothing"
    );
    // Several distinct pending versions are fine.
    let other = install_fake_release(&root, "0.1.5");
    let several = update_with_pending(
        &v3,
        vec![
            pending("0.1.0", &kept.tree_sha256),
            pending("0.1.5", &other.tree_sha256),
        ],
    );
    let outcome = run(&several).unwrap();
    assert_eq!(outcome.state, "ready");
    assert!(!version_path(&root, "0.1.0").exists());
    assert!(!version_path(&root, "0.1.5").exists());
}

#[test]
fn pending_removals_round_trip_and_an_older_lock_loads_with_none() {
    let lock: crate::models::InstallationLock = serde_json::from_str(include_str!(
        "../../docs/examples/installation-lock.example.json"
    ))
    .unwrap();
    let mut value = serde_json::to_value(&lock).unwrap();
    // An older lock has no pending list at all.
    value["image_generation"] = serde_json::json!({
        "primary_provider": "hoi4_imagegen",
        "installed_release_version": "0.2.0",
        "installed_release_tree_sha256": "a".repeat(64)
    });
    let older: crate::models::InstallationLock = serde_json::from_value(value.clone()).unwrap();
    let image = older.image_generation.unwrap();
    assert!(image.pending_removals.is_empty());
    let empty = serde_json::to_value(&image).unwrap();
    assert!(
        empty.get("pending_removals").is_none(),
        "an empty list is not written, so older apps still read the lock"
    );
    let with_pending = ImageGenerationSelection {
        pending_removals: vec![pending("0.1.0", &"b".repeat(64))],
        ..image
    };
    let text = serde_json::to_string(&with_pending).unwrap();
    assert!(text.contains(r#""pending_removals":[{"version":"0.1.0","tree_sha256":"#));
    assert_eq!(
        serde_json::from_str::<ImageGenerationSelection>(&text).unwrap(),
        with_pending
    );
    // Unknown fields in an entry are refused like everywhere else in the selection.
    assert!(
        serde_json::from_value::<PendingReleaseRemoval>(serde_json::json!({
            "version": "0.1.0", "tree_sha256": "b".repeat(64), "path": "../x"
        }))
        .is_err()
    );
    // The lock schema accepts the list.
    value["image_generation"]["pending_removals"] = serde_json::json!([
        {"version": "0.1.0", "tree_sha256": "b".repeat(64)}
    ]);
    let reloaded: crate::models::InstallationLock = serde_json::from_value(value).unwrap();
    assert_eq!(
        reloaded.image_generation.unwrap().pending_removals,
        with_pending.pending_removals
    );
}

#[test]
fn pending_evidence_is_parsed_strictly() {
    let tree = "e".repeat(64);
    let prefix = route::EVIDENCE_PENDING_PREFIX;
    assert_eq!(
        pending_from_evidence(&format!("old-version-kept:0.1.0;{prefix}0.1.0:{tree}")).unwrap(),
        vec![pending("0.1.0", &tree)]
    );
    assert!(pending_from_evidence("old-version-kept:0.1.0")
        .unwrap()
        .is_empty());
    for bad in [
        format!("{prefix}0.1.0"),
        format!("{prefix}../x:{tree}"),
        format!("{prefix}0.1:{tree}"),
        format!("{prefix}0.1.0:nothex"),
        format!("{prefix}0.1.0:{tree};{prefix}0.1.0:{tree}"),
    ] {
        assert!(pending_from_evidence(&bad).is_err(), "{bad}");
    }
}

#[test]
fn evidence_without_a_receipt_changes_nothing_and_a_bad_hash_is_refused() {
    assert_eq!(
        receipt_from_evidence("old-version-kept:0.1.0").unwrap(),
        None
    );
    assert!(receipt_from_evidence(&format!(
        "{}0.2.0;{}nothex",
        route::EVIDENCE_VERSION_PREFIX,
        route::EVIDENCE_TREE_PREFIX
    ))
    .is_err());
    assert!(receipt_from_evidence(&format!(
        "{}../x;{}{}",
        route::EVIDENCE_VERSION_PREFIX,
        route::EVIDENCE_TREE_PREFIX,
        "a".repeat(64)
    ))
    .is_err());
}

// ---- Sign-in and provider keys ----------------------------------------------

fn installed(root: &std::path::Path) -> InstalledImagegen {
    let v1 = release("0.2.0");
    let receipt = install_release(
        root,
        "0.2.0",
        &v1.target,
        None,
        &FakeDownloader::new(&v1.archive),
    )
    .unwrap();
    InstalledImagegen::from_selection(
        root,
        Some(&ImageGenerationSelection {
            installed_release_version: Some("0.2.0".into()),
            installed_release_tree_sha256: Some(receipt.tree_sha256),
            ..selection("hoi4_imagegen")
        }),
    )
    .unwrap()
}

#[test]
fn sign_in_status_reads_the_text_not_just_the_exit_code() {
    let dir = project();
    let root = root(&dir);
    let imagegen = installed(&root);
    let cli = FakeCli::reporting("0.2.0");
    let check = |answer: Option<(Option<i32>, bool, &str)>| {
        *cli.status_override.borrow_mut() =
            answer.map(|(code, timed_out, text)| (code, timed_out, text.to_owned()));
        imagegen.sign_in_status(&cli).unwrap().signed_in
    };
    // Signed out: exit 0 with the "not signed in" text.
    assert!(!check(None));
    assert!(!check(Some((
        Some(0),
        false,
        "not signed in (run `hoi4-imagegen login`)\n"
    ))));
    // Signed in: exit 0 with the "signed in;" text.
    assert!(check(Some((
        Some(0),
        false,
        "signed in; account a; plan plus; token expires at t\n"
    ))));
    cli.signed_in.set(true);
    assert!(check(None));
    // A timeout or a nonzero exit is never signed in.
    assert!(!check(Some((Some(0), true, "signed in; account a"))));
    assert!(!check(Some((None, true, "signed in; account a"))));
    assert!(!check(Some((Some(1), false, "signed in; account a"))));
}

#[test]
fn sign_in_launches_the_binary_with_an_argument_array_and_reports_login_status() {
    let dir = project();
    let root = root(&dir);
    let imagegen = installed(&root);
    let cli = FakeCli::reporting("0.2.0");
    assert!(!imagegen.sign_in_status(&cli).unwrap().signed_in);
    assert!(imagegen.risk_note(&cli).unwrap().contains("Risk"));
    let status = imagegen.sign_in(&cli).unwrap();
    assert!(status.signed_in);
    assert_eq!(
        cli.arguments(),
        vec![
            vec!["login-status".to_owned()],
            vec!["login-risk-note".to_owned()],
            vec!["login".to_owned(), "--accept-risk".to_owned()],
            vec!["login-status".to_owned()],
        ]
    );
    // No process ever received a secret from Setup for the sign-in.
    assert!(cli
        .launches
        .borrow()
        .iter()
        .all(|(_, names)| names.is_empty()));
    // Nothing of the session lands in the project: Setup wrote no token file.
    let tools = root.join(".tools");
    let mut written = Vec::new();
    let mut stack = vec![tools];
    while let Some(path) = stack.pop() {
        for entry in std::fs::read_dir(&path).unwrap() {
            let entry = entry.unwrap().path();
            if entry.is_dir() {
                stack.push(entry);
            } else {
                written.push(entry.file_name().unwrap().to_string_lossy().to_string());
            }
        }
    }
    written.sort();
    assert_eq!(
        written.len(),
        2,
        "only the two installed binaries: {written:?}"
    );
}

#[test]
fn the_binary_is_not_run_when_its_files_changed_since_install() {
    let dir = project();
    let root = root(&dir);
    let imagegen = installed(&root);
    let (_, cli_name) = binary_names(host_triple());
    std::fs::write(version_path(&root, "0.2.0").join(cli_name), b"swapped").unwrap();
    let cli = FakeCli::reporting("0.2.0");
    assert!(imagegen.sign_in(&cli).is_err());
    assert!(imagegen.verify_version(&cli).is_err());
    assert!(cli.arguments().is_empty());
}

#[test]
fn a_provider_key_reaches_only_the_doctor_process() {
    let dir = project();
    let root = root(&dir);
    let imagegen = installed(&root);
    let store = MemoryCredentialStore::default();
    let secret = "synthetic-imagegen-provider-value";
    let reference =
        save_image_generation_key(&store, "hoi4_imagegen", "openai_api_key", secret).unwrap();
    let mut chosen = selection("hoi4_imagegen");
    chosen.credential_references.insert(
        "hoi4_imagegen.openai_api_key".into(),
        reference.reference.clone(),
    );
    let fields = vec![
        ("openai_api_key".to_owned(), "OPENAI_API_KEY".to_owned()),
        ("gemini_api_key".to_owned(), "GEMINI_API_KEY".to_owned()),
    ];
    let environment = scoped_environment(&store, &chosen, &fields)
        .unwrap()
        .unwrap();
    assert_eq!(environment.variable_names(), vec!["OPENAI_API_KEY"]);

    let cli = FakeCli::reporting("0.2.0");
    *cli.doctor_echo.borrow_mut() = format!("OpenAI key {secret} accepted");
    imagegen.verify_version(&cli).unwrap();
    imagegen.sign_in(&cli).unwrap();
    let report = imagegen.doctor(&cli, Some(&environment)).unwrap();
    assert!(
        !report.contains(secret),
        "the key is redacted from the report"
    );
    let launches = cli.launches.borrow();
    for (arguments, names) in launches.iter() {
        assert!(
            arguments.iter().all(|argument| !argument.contains(secret)),
            "a key is never an argument"
        );
        if arguments[0] == "doctor" {
            assert_eq!(names, &vec!["OPENAI_API_KEY".to_owned()]);
        } else {
            assert!(names.is_empty(), "{arguments:?} received a secret");
        }
    }
    drop(launches);
    // Nothing the project holds contains the key: no file under the root.
    let mut stack = vec![root.clone()];
    while let Some(path) = stack.pop() {
        for entry in std::fs::read_dir(&path).unwrap() {
            let entry = entry.unwrap().path();
            if entry.is_dir() {
                stack.push(entry);
            } else {
                let bytes = std::fs::read(&entry).unwrap();
                assert!(!String::from_utf8_lossy(&bytes).contains(secret));
            }
        }
    }
}

#[test]
fn scoped_environments_refuse_unknown_variables_and_foreign_references() {
    let store = MemoryCredentialStore::default();
    let reference =
        save_image_generation_key(&store, "hoi4_imagegen", "fal_key", "fal-secret-0001").unwrap();
    let mut chosen = selection("hoi4_imagegen");
    chosen
        .credential_references
        .insert("hoi4_imagegen.fal_key".into(), reference.reference.clone());
    // A manifest cannot name another variable.
    let hostile = vec![("fal_key".to_owned(), "PATH".to_owned())];
    assert!(scoped_environment(&store, &chosen, &hostile).is_err());
    // A reference that is not the field's own is refused.
    let mut forged = chosen.clone();
    forged.credential_references.insert(
        "hoi4_imagegen.fal_key".into(),
        "credential://image_generation_api_key/openai_api_api_key".into(),
    );
    let fields = vec![("fal_key".to_owned(), "FAL_KEY".to_owned())];
    assert!(scoped_environment(&store, &forged, &fields).is_err());
    // No saved key means no environment.
    assert!(
        scoped_environment(&store, &selection("hoi4_imagegen"), &fields)
            .unwrap()
            .is_none()
    );
    let _ = image_generation_credential_reference("hoi4_imagegen", "fal_key").unwrap();
}

#[test]
fn redirects_stay_on_github_release_hosts_over_https() {
    use crate::release_install::release_redirect_allowed as allowed;
    let url = |value: &str| reqwest::Url::parse(value).unwrap();
    assert!(allowed(&url("https://github.com/a/b"), 0));
    assert!(allowed(&url("https://objects.githubusercontent.com/x"), 1));
    assert!(allowed(
        &url("https://release-assets.githubusercontent.com/x"),
        3
    ));
    assert!(!allowed(
        &url("https://release-assets.githubusercontent.com/x"),
        4
    ));
    assert!(!allowed(&url("http://github.com/a"), 0));
    assert!(!allowed(&url("https://evil.example/a"), 0));
    assert!(!allowed(&url("https://github.com.evil.example/a"), 0));
    assert!(!allowed(&url("https://github.com:8443/a"), 0));
    assert!(!allowed(&url("https://user@github.com/a"), 0));
}

// ---- A fake release for the real transaction path ----------------------------

thread_local! {
    static TRANSACTION_RELEASE: RefCell<Option<(String, Vec<u8>)>> = const { RefCell::new(None) };
}

/// The reviewed install action for a fake `version` archive, plus the plan
/// selection that goes with it. Arms [`fake_release_runner`] on this thread.
pub(crate) fn arm_fake_release(
    version: &str,
) -> (crate::models::ExternalAction, ImageGenerationSelection) {
    let release = release(version);
    let manifest = host_manifest(version, &release);
    let chosen = selection("hoi4_imagegen");
    let InstallPlanning::Install(action) = plan_install(&manifest, &chosen).unwrap() else {
        panic!("the provider is selected and supported on this host");
    };
    TRANSACTION_RELEASE.with(|slot| *slot.borrow_mut() = Some((version.into(), release.archive)));
    (*action, chosen)
}

/// A post-install runner for the transaction that answers the hoi4-imagegen
/// component with the real action over the armed fake archive and fake CLI.
pub(crate) fn fake_release_runner(
    root: &std::path::Path,
    plan: &InstallationPlan,
    component_id: &str,
) -> Result<crate::transaction::PostInstallActionOutcome, AppError> {
    if component_id != route::COMPONENT_ID {
        // The plan's other reviewed actions are not under test here.
        return Ok(crate::transaction::PostInstallActionOutcome {
            component_id: component_id.into(),
            state: "ready".into(),
            evidence: "Fixture runner: nothing was started.".into(),
        });
    }
    let (version, archive) = TRANSACTION_RELEASE
        .with(|slot| slot.borrow().clone())
        .expect("arm_fake_release was called on this thread");
    run_reviewed_actions(
        root,
        plan,
        &FakeDownloader::new(&archive),
        &FakeCli::reporting(&version),
    )
}

/// The manifest `targets` of a fake `version` archive for every platform, with
/// this host's entry carrying the real digest and size. Arms
/// [`fake_release_runner`] on this thread.
pub(crate) fn armed_release_targets(version: &str) -> serde_json::Value {
    let release = release(version);
    let mut targets = synthetic_targets(version);
    targets[host_triple()]["sha256"] = serde_json::json!(release.target.sha256);
    targets[host_triple()]["size"] = serde_json::json!(release.target.size);
    TRANSACTION_RELEASE.with(|slot| *slot.borrow_mut() = Some((version.into(), release.archive)));
    targets
}

#[test]
fn only_key_shaped_variables_ever_reach_hoi4_imagegen() {
    use crate::imagegen::{credential_field_allowed, credential_variable_allowed};
    for name in [
        "OPENAI_API_KEY",
        "FAL_KEY",
        "COMFYUI_API_KEY",
        "STABILITY_TOKEN",
    ] {
        assert!(credential_variable_allowed(name), "{name}");
    }
    for name in [
        "PATH",
        "LD_PRELOAD",
        "DYLD_INSERT_LIBRARIES",
        "openai_api_key",
        "OPENAI KEY",
        "KEY",
        "ANTHROPIC_API_KEY",
        "MESHY_API_KEY",
        "HOME_KEY=1",
        "",
    ] {
        assert!(!credential_variable_allowed(name), "{name}");
    }
    // A backend added later needs no app release as long as its field is
    // key-shaped.
    assert!(credential_field_allowed("stability_api_key"));
    assert!(credential_field_allowed("fal_key"));
    assert!(!credential_field_allowed("path"));
    assert!(!credential_field_allowed("Fal_Key"));
    assert!(!credential_field_allowed("../fal_key"));
    assert!(!credential_field_allowed("anthropic_api_key"));
}

#[test]
fn every_hoi4_imagegen_v0_3_backend_key_is_accepted() {
    use crate::imagegen::{credential_field_allowed, credential_variable};
    for (field, variable) in [
        ("openai_api_key", "OPENAI_API_KEY"),
        ("gemini_api_key", "GEMINI_API_KEY"),
        ("fal_key", "FAL_KEY"),
        ("comfyui_api_key", "COMFYUI_API_KEY"),
        ("replicate_api_token", "REPLICATE_API_TOKEN"),
        ("bfl_api_key", "BFL_API_KEY"),
        ("leonardo_api_key", "LEONARDO_API_KEY"),
        ("stability_api_key", "STABILITY_API_KEY"),
        ("ideogram_api_key", "IDEOGRAM_API_KEY"),
        ("recraft_api_key", "RECRAFT_API_KEY"),
        ("xai_api_key", "XAI_API_KEY"),
        ("together_api_key", "TOGETHER_API_KEY"),
        ("openrouter_api_key", "OPENROUTER_API_KEY"),
        ("hf_token", "HF_TOKEN"),
        ("openai_compatible_api_key", "OPENAI_COMPATIBLE_API_KEY"),
    ] {
        assert!(credential_field_allowed(field), "{field}");
        assert_eq!(credential_variable(field), variable);
    }
}
