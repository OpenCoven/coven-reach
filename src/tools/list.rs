use serde_json::Value;
use std::time::UNIX_EPOCH;
use crate::error::{ReachError, ReachResult};
use crate::security::Security;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn handle(args: &Value, sec: &Security) -> ReachResult<Value> {
    let op = args["operation"].as_str().unwrap_or("");
    match op {
        "entries" => list_entries(args, sec),
        "system_info" => list_system_info(args, sec),
        _ => Err(ReachError::InvalidArgument(format!("unknown reach_list operation: '{op}'"))),
    }
}

fn list_entries(args: &Value, sec: &Security) -> ReachResult<Value> {
    let path = args["path"].as_str().ok_or_else(|| {
        ReachError::InvalidArgument("list entries requires path".into())
    })?;
    let recursive_depth = args["recursive_depth"].as_u64().unwrap_or(0) as usize;
    let calc_size = args["calculate_recursive_size"].as_bool().unwrap_or(false);

    let resolved = sec.check_exists(path)?;
    let entries = collect_entries(&resolved, recursive_depth, 0, calc_size, sec)?;

    Ok(serde_json::json!({
        "ok": true,
        "path": resolved.display().to_string(),
        "entries": entries,
        "count": entries.len(),
    }))
}

fn collect_entries(
    dir: &std::path::Path,
    max_depth: usize,
    current_depth: usize,
    calc_size: bool,
    sec: &Security,
) -> ReachResult<Vec<Value>> {
    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(dir).map_err(|e| ReachError::Io {
        path: dir.display().to_string(),
        source: e,
    })?;

    for entry_res in read_dir {
        let entry = entry_res.map_err(|e| ReachError::Io {
            path: dir.display().to_string(),
            source: e,
        })?;
        let path = entry.path();

        // Skip paths outside allowed territory
        if sec.check(path.to_str().unwrap_or("")).is_err() {
            continue;
        }

        let meta = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_symlink = meta.file_type().is_symlink();
        let is_dir = path.is_dir();
        let mime = if is_dir {
            "inode/directory".to_string()
        } else {
            mime_guess::from_path(&path).first_or_octet_stream().to_string()
        };
        let modified = meta.modified().ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        let size = if is_dir && calc_size {
            dir_size(&path)
        } else if !is_dir {
            meta.len()
        } else {
            0
        };

        let mut e = serde_json::json!({
            "name": entry.file_name().to_str().unwrap_or(""),
            "path": path.display().to_string(),
            "isDir": is_dir,
            "isFile": !is_dir && !is_symlink,
            "isSymlink": is_symlink,
            "size": size,
            "mimeType": mime,
            "modifiedUnix": modified,
        });

        if is_dir && current_depth < max_depth {
            if let Ok(children) = collect_entries(&path, max_depth, current_depth + 1, calc_size, sec) {
                e["children"] = serde_json::json!(children);
            }
        }

        entries.push(e);
    }

    entries.sort_by(|a, b| {
        let a_dir = a["isDir"].as_bool().unwrap_or(false);
        let b_dir = b["isDir"].as_bool().unwrap_or(false);
        b_dir.cmp(&a_dir).then_with(|| {
            let a_name = a["name"].as_str().unwrap_or("");
            let b_name = b["name"].as_str().unwrap_or("");
            a_name.cmp(b_name)
        })
    });

    Ok(entries)
}

fn dir_size(path: &std::path::Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

fn list_system_info(args: &Value, sec: &Security) -> ReachResult<Value> {
    let info_type = args["info_type"].as_str().unwrap_or("server_capabilities");
    match info_type {
        "server_capabilities" => Ok(serde_json::json!({
            "ok": true,
            "server": {
                "name": "coven-reach",
                "version": VERSION,
                "description": "Coven-native MCP server for filesystem and web operations",
            },
            "configuration": {
                "allowedPaths": sec.allowed_paths(),
                "readOnly": sec.read_only,
            },
            "capabilities": {
                "tools": ["reach_read", "reach_write", "reach_list", "reach_find", "reach_familiar", "reach_secret_check"],
                "formats": { "read": ["text", "base64", "markdown", "checksum"], "archive": ["zip", "tar.gz"] },
                "operations": {
                    "read": ["content", "metadata", "diff", "checksum"],
                    "write": ["put", "mkdir", "copy", "move", "delete", "touch", "archive", "unarchive"],
                    "list": ["entries", "system_info"],
                    "find": ["search"],
                }
            }
        })),
        "filesystem_stats" => {
            let path = args["path"].as_str().unwrap_or("~");
            let resolved = sec.check_exists(path)?;
            Ok(serde_json::json!({
                "ok": true,
                "path": resolved.display().to_string(),
                "exists": resolved.exists(),
                "isDir": resolved.is_dir(),
            }))
        }
        _ => Err(ReachError::InvalidArgument(format!("unknown info_type: '{info_type}'"))),
    }
}
