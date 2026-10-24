//! The Node runtime that plan building and the MCP client registration need.
//!
//! The plan builder writes an MCP registration whose launch command is the
//! host's Node runtime followed by the installed package entry. On Linux that
//! command was always `/usr/bin/node` plus the per-user npm prefix, found
//! implicitly. A server cannot rely on that, so the runtime is a value the
//! caller supplies and the core verifies, instead of a discovery the core
//! performs.

use crate::models::Platform;
use crate::AppError;
use std::path::{Path, PathBuf};

/// The publisher the core requires for a native Node runtime.
pub const NODE_PUBLISHER: &str = "OpenJS Foundation";

/// A verified native Node runtime and the npm prefix its packages live in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRuntime {
    executable: PathBuf,
    npm_prefix: PathBuf,
}

impl NodeRuntime {
    /// Verify `executable` and bind it with the npm prefix that holds the
    /// installed MCP package.
    ///
    /// Both paths must be absolute and free of symlinks. The executable is
    /// canonicalized and must pass the same publisher verification the
    /// desktop applies on Linux (`OpenJS Foundation`). The prefix is not
    /// created or read here.
    pub fn new(
        executable: impl AsRef<Path>,
        npm_prefix: impl AsRef<Path>,
    ) -> Result<Self, AppError> {
        let executable = executable.as_ref();
        let npm_prefix = npm_prefix.as_ref();
        if !executable.is_absolute() || !npm_prefix.is_absolute() {
            return Err(AppError::InvalidInput(
                "the Node runtime and npm prefix must be absolute paths".into(),
            ));
        }
        if crate::security::path_has_link_component(npm_prefix)
            || npm_prefix
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(AppError::PathSecurity(
                "the npm prefix must not contain links or traversal".into(),
            ));
        }
        Self::verified(executable, npm_prefix.to_path_buf())
    }

    fn verified(executable: &Path, npm_prefix: PathBuf) -> Result<Self, AppError> {
        let executable = std::fs::canonicalize(executable).map_err(|_| {
            AppError::Process("the trusted native Node runtime is unavailable".into())
        })?;
        crate::process::validate_executable_publisher(&executable, NODE_PUBLISHER)?;
        Ok(Self {
            executable,
            npm_prefix,
        })
    }

    /// The runtime the desktop uses on Linux: `/usr/bin/node` and the
    /// per-user npm prefix under the XDG data directory.
    pub fn linux_system() -> Result<Self, AppError> {
        let executable = Path::new("/usr/bin/node");
        let runtime = Self::verified(executable, PathBuf::new())?;
        Ok(Self {
            npm_prefix: crate::paths::linux_npm_prefix()?,
            ..runtime
        })
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn npm_prefix(&self) -> &Path {
        &self.npm_prefix
    }
}

/// The command and arguments a coding client uses to start the installed
/// MCP package. Only the Linux route uses a Node runtime: `node` is the
/// verified runtime and the argument is the package entry under its npm
/// prefix. Windows and macOS launch the package's own target.
pub fn mcp_client_launch(
    target: &crate::mcp::VerifiedMcpTarget,
    platform: Platform,
    node: Option<&NodeRuntime>,
) -> Result<(String, Vec<String>), AppError> {
    if platform == Platform::Linux {
        let node = node.ok_or_else(|| {
            AppError::Process("the trusted native Node runtime is unavailable".into())
        })?;
        let entry = node
            .npm_prefix()
            .join("lib/node_modules")
            .join(&target.package_name)
            .join(&target.runtime_entry);
        Ok((
            node.executable().display().to_string(),
            vec![entry.display().to_string()],
        ))
    } else if platform == Platform::Windows && Platform::current() == Platform::Windows {
        Ok((
            crate::paths::user_facing_path(
                &crate::paths::windows_npm_prefix()?.join(&target.target),
            ),
            vec![],
        ))
    } else {
        Ok((target.target.clone(), vec![]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_runtime_or_prefix_is_refused() {
        assert!(NodeRuntime::new("node", "/opt/npm").is_err());
        assert!(NodeRuntime::new("/usr/bin/node", "npm").is_err());
    }

    fn target() -> crate::mcp::VerifiedMcpTarget {
        crate::mcp::VerifiedMcpTarget {
            target: "hoi4-agent-tools".into(),
            package_name: "hoi4-agent-tools".into(),
            package_version: "1.0.0".into(),
            package_integrity: String::new(),
            package_tree_sha256: String::new(),
            package_file_count: 0,
            runtime_entry: "dist/index.js".into(),
            runtime_entry_sha256: String::new(),
            runtime_entry_size: 0,
            package_symlinks: Default::default(),
            required_tools: vec![],
        }
    }

    #[test]
    fn the_linux_launch_uses_only_the_runtime_it_is_given() {
        let runtime = NodeRuntime {
            executable: PathBuf::from("/opt/node/bin/node"),
            npm_prefix: PathBuf::from("/srv/npm"),
        };
        let (command, arguments) =
            mcp_client_launch(&target(), Platform::Linux, Some(&runtime)).unwrap();
        assert_eq!(command, runtime.executable().display().to_string());
        let entry = Path::new("/srv/npm")
            .join("lib/node_modules")
            .join("hoi4-agent-tools")
            .join("dist/index.js");
        assert_eq!(arguments, vec![entry.display().to_string()]);
        // There is no discovery to fall back on.
        assert!(mcp_client_launch(&target(), Platform::Linux, None).is_err());
        // Other platforms never consult a Node runtime.
        let (command, arguments) = mcp_client_launch(&target(), Platform::Macos, None).unwrap();
        assert_eq!(command, "hoi4-agent-tools");
        assert!(arguments.is_empty());
    }

    #[test]
    fn a_missing_runtime_is_refused_before_any_publisher_check() {
        let directory = tempfile::tempdir().unwrap();
        let error = NodeRuntime::new(directory.path().join("missing-node"), directory.path())
            .unwrap_err()
            .to_string();
        assert!(error.contains("trusted native Node runtime is unavailable"));
    }
}
