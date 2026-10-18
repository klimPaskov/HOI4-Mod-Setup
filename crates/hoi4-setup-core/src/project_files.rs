//! Project files the setup generates from the confirmed identity: the
//! provider-neutral README and the managed `.gitignore` rules.

use crate::models::ProjectIdentity;
use crate::AppError;

pub const MANAGED_GITIGNORE_BLOCK: &str = "# BEGIN HOI4 Mod Setup managed rules\n.tmp/\n.agents/\n.codex/\n.tools/\nparadox_wiki/\nAGENTS.md\n.hoi4-mod-setup/backups/\n.hoi4-mod-setup/cache/\n# END HOI4 Mod Setup managed rules";

pub fn merge_gitignore(existing: Option<&str>) -> String {
    let existing = existing.unwrap_or("").trim_end();
    const BEGIN: &str = "# BEGIN HOI4 Mod Setup managed rules";
    const END: &str = "# END HOI4 Mod Setup managed rules";
    if let Some(start) = existing.find(BEGIN) {
        if let Some(relative_end) = existing[start..].find(END) {
            let end = start + relative_end + END.len();
            let mut updated = String::with_capacity(existing.len() + MANAGED_GITIGNORE_BLOCK.len());
            updated.push_str(existing[..start].trim_end());
            if !updated.is_empty() {
                updated.push_str("\n\n");
            }
            updated.push_str(MANAGED_GITIGNORE_BLOCK);
            let suffix = existing[end..].trim_start();
            if !suffix.is_empty() {
                updated.push_str("\n\n");
                updated.push_str(suffix);
            }
            return updated;
        }
    }
    if existing.is_empty() {
        MANAGED_GITIGNORE_BLOCK.to_string()
    } else {
        format!("{existing}\n\n{MANAGED_GITIGNORE_BLOCK}")
    }
}

pub fn project_readme(identity: &ProjectIdentity, description: &str) -> Result<String, AppError> {
    if description.len() > 32 * 1024 || description.contains('\0') {
        return Err(AppError::InvalidInput(
            "project description exceeds the bounded README input limit".into(),
        ));
    }
    Ok(format!(
        "# {}\n\n{}\n\n## Local development\n\nThis project was prepared by HOI4 Mod Setup for provider-neutral agentic development. The AI used during setup does not select or restrict the development client used later.\n\n- Project ID: `{}`\n- Supported game version: `{}`\n- Workshop identity: none assigned\n",
        identity.display_name,
        description.trim(),
        identity.project_id,
        identity.supported_game_version,
    ))
}
