use serde_json::Value;
use std::path::{Path, PathBuf};
use crate::error::{ReachError, ReachResult};
use crate::security::expand_tilde;

/// Read familiars from the OpenClaw workspace.
pub fn handle(_args: &Value) -> ReachResult<Value> {
    let workspace = expand_tilde("~/.openclaw/workspace");
    if !workspace.exists() {
        return Ok(serde_json::json!({
            "ok": true,
            "familiars": [],
            "note": "No OpenClaw workspace found at ~/.openclaw/workspace",
        }));
    }

    let mut familiars = Vec::new();

    let read_dir = std::fs::read_dir(&workspace).map_err(|e| ReachError::Io {
        path: workspace.display().to_string(),
        source: e,
    })?;

    for entry in read_dir.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }

        let name = entry.file_name().to_str().unwrap_or("").to_string();
        if name.starts_with('.') { continue; }

        let familiar = read_familiar(&name, &path);
        familiars.push(familiar);
    }

    familiars.sort_by(|a, b| {
        let a_name = a["name"].as_str().unwrap_or("");
        let b_name = b["name"].as_str().unwrap_or("");
        a_name.cmp(b_name)
    });

    Ok(serde_json::json!({
        "ok": true,
        "workspacePath": workspace.display().to_string(),
        "count": familiars.len(),
        "familiars": familiars,
    }))
}

fn read_familiar(id: &str, workspace_path: &Path) -> Value {
    let identity_path = workspace_path.join("IDENTITY.md");
    let memory_path = workspace_path.join("MEMORY.md");
    let soul_path = workspace_path.join("SOUL.md");

    // Parse IDENTITY.md for name/creature/vibe/emoji
    let identity = parse_identity_md(&identity_path);
    let has_memory = memory_path.exists();
    let has_soul = soul_path.exists();

    // Count session transcripts
    let sessions_path = expand_tilde(&format!("~/.openclaw/agents/{id}/sessions"));
    let session_count = if sessions_path.exists() {
        count_jsonl_files(&sessions_path)
    } else {
        0
    };

    // Last memory update
    let memory_modified = memory_path.metadata().ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    serde_json::json!({
        "id": id,
        "name": identity.get("name").cloned().unwrap_or_else(|| id.to_string()),
        "creature": identity.get("creature").cloned(),
        "vibe": identity.get("vibe").cloned(),
        "emoji": identity.get("emoji").cloned(),
        "workspacePath": workspace_path.display().to_string(),
        "hasMemory": has_memory,
        "hasSoul": has_soul,
        "sessionCount": session_count,
        "memoryLastModifiedUnix": memory_modified,
    })
}

fn parse_identity_md(path: &Path) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return map,
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("- **Name:**") {
            map.insert("name".into(), rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("- **Creature:**") {
            map.insert("creature".into(), rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("- **Vibe:**") {
            map.insert("vibe".into(), rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("- **Emoji:**") {
            map.insert("emoji".into(), rest.trim().to_string());
        }
    }
    map
}

fn count_jsonl_files(path: &Path) -> usize {
    std::fs::read_dir(path)
        .ok()
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| x == "jsonl")
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}
