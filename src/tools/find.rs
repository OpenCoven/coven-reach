use serde_json::Value;
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;
use crate::error::{ReachError, ReachResult};
use crate::security::Security;

pub fn handle(args: &Value, sec: &Security) -> ReachResult<Value> {
    let op = args["operation"].as_str().unwrap_or("search");
    if op != "search" {
        return Err(ReachError::InvalidArgument(format!("unknown reach_find operation: '{op}'")));
    }

    let path = args["path"].as_str().ok_or_else(|| {
        ReachError::InvalidArgument("find requires path".into())
    })?;
    let resolved = sec.check_exists(path)?;
    let recursive = args["recursive"].as_bool().unwrap_or(true);
    let max_results = args["max_results"].as_u64().unwrap_or(200) as usize;
    let entry_type = args["entry_type"].as_str().unwrap_or("any");

    // Name pattern (glob)
    let name_pattern = args["name_pattern"].as_str();
    let case_sensitive = args["case_sensitive"].as_bool().unwrap_or(false);

    // Content search
    let content_pattern = args["content_pattern"].as_str();
    let content_is_regex = args["content_is_regex"].as_bool().unwrap_or(false);
    let content_case_sensitive = args["content_case_sensitive"].as_bool().unwrap_or(false);
    let file_extensions: Option<Vec<String>> = args["file_extensions"].as_array().map(|a| {
        a.iter().filter_map(|v| v.as_str()).map(|s| {
            let s = if s.starts_with('.') { s.to_string() } else { format!(".{s}") };
            s.to_ascii_lowercase()
        }).collect()
    });

    // Size filters
    let size_min = args["size_min"].as_u64();
    let size_max = args["size_max"].as_u64();

    // Date filters
    let modified_after = parse_unix_secs(args["modified_after"].as_str());
    let modified_before = parse_unix_secs(args["modified_before"].as_str());
    let created_after = parse_unix_secs(args["created_after"].as_str());
    let created_before = parse_unix_secs(args["created_before"].as_str());

    // Mime type filter
    let mime_filter = args["mime_type"].as_str();

    // Build content regex
    let content_regex: Option<regex::Regex> = if let Some(pat) = content_pattern {
        let pattern = if !content_is_regex {
            regex::escape(pat)
        } else {
            pat.to_string()
        };
        let flags = if !content_case_sensitive { "(?i)" } else { "" };
        Some(regex::Regex::new(&format!("{flags}{pattern}"))?)
    } else {
        None
    };

    let walker = if recursive {
        WalkDir::new(&resolved)
    } else {
        WalkDir::new(&resolved).max_depth(1)
    };

    let mut results = Vec::new();

    'outer: for entry in walker.into_iter().filter_map(|e| e.ok()) {
        if results.len() >= max_results {
            break;
        }

        // Skip paths outside allowed territory
        if sec.check(entry.path().to_str().unwrap_or("")).is_err() {
            continue;
        }

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let is_dir = meta.is_dir();
        let is_file = meta.is_file();

        // Entry type filter
        match entry_type {
            "file" if !is_file => continue,
            "directory" if !is_dir => continue,
            _ => {}
        }

        let path = entry.path();

        // Name pattern
        if let Some(pat) = name_pattern {
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            let name_cmp = if case_sensitive { name.to_string() } else { name.to_ascii_lowercase() };
            let pat_cmp = if case_sensitive { pat.to_string() } else { pat.to_ascii_lowercase() };
            if !glob_match(&pat_cmp, &name_cmp) {
                continue;
            }
        }

        // Size filters
        if is_file {
            let size = meta.len();
            if let Some(min) = size_min { if size < min { continue; } }
            if let Some(max) = size_max { if size > max { continue; } }
        }

        // Date filters
        let modified_unix = meta.modified().ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let created_unix = meta.created().ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        if let (Some(after), Some(mtime)) = (modified_after, modified_unix) {
            if mtime < after { continue; }
        }
        if let (Some(before), Some(mtime)) = (modified_before, modified_unix) {
            if mtime > before { continue; }
        }
        if let (Some(after), Some(ctime)) = (created_after, created_unix) {
            if ctime < after { continue; }
        }
        if let (Some(before), Some(ctime)) = (created_before, created_unix) {
            if ctime > before { continue; }
        }

        // Mime filter
        let mime = mime_guess::from_path(path).first_or_octet_stream().to_string();
        if let Some(mf) = mime_filter {
            if !mime.contains(mf) { continue; }
        }

        // File extension filter for content search
        if is_file {
            if let Some(exts) = &file_extensions {
                if content_regex.is_some() {
                    let ext = path.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| format!(".{}", e.to_ascii_lowercase()))
                        .unwrap_or_default();
                    if !exts.iter().any(|e| e == &ext) {
                        continue;
                    }
                }
            }
        }

        // Content search
        let mut content_matches: Vec<Value> = Vec::new();
        if let Some(re) = &content_regex {
            if is_file {
                let text = match std::fs::read_to_string(path) {
                    Ok(t) => t,
                    Err(_) => continue, // binary or unreadable
                };
                for (line_no, line) in text.lines().enumerate() {
                    if re.is_match(line) {
                        content_matches.push(serde_json::json!({
                            "line": line_no + 1,
                            "text": if line.len() > 200 { &line[..200] } else { line },
                        }));
                    }
                }
                if content_matches.is_empty() {
                    continue 'outer;
                }
            }
        }

        let mut result = serde_json::json!({
            "path": path.display().to_string(),
            "name": path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
            "isDir": is_dir,
            "isFile": is_file,
            "size": meta.len(),
            "mimeType": mime,
            "modifiedUnix": modified_unix,
        });

        if !content_matches.is_empty() {
            result["contentMatches"] = serde_json::json!(content_matches);
        }

        results.push(result);
    }

    Ok(serde_json::json!({
        "ok": true,
        "path": resolved.display().to_string(),
        "count": results.len(),
        "truncated": results.len() >= max_results,
        "results": results,
    }))
}

fn parse_unix_secs(s: Option<&str>) -> Option<u64> {
    let s = s?;
    // Parse ISO 8601 — just try chrono
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp() as u64)
}

/// Simple glob matcher supporting `*`, `?`, `{a,b}` alternatives
fn glob_match(pattern: &str, text: &str) -> bool {
    // Expand {a,b,c} alternatives
    if let Some(start) = pattern.find('{') {
        if let Some(end) = pattern[start..].find('}') {
            let prefix = &pattern[..start];
            let alts = &pattern[start + 1..start + end];
            let suffix = &pattern[start + end + 1..];
            return alts.split(',').any(|alt| {
                let expanded = format!("{prefix}{alt}{suffix}");
                glob_match(&expanded, text)
            });
        }
    }

    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    glob_match_chars(&pat, &txt)
}

fn glob_match_chars(pat: &[char], txt: &[char]) -> bool {
    match (pat.first(), txt.first()) {
        (None, None) => true,
        (Some('*'), _) => {
            // Match zero or more characters
            if glob_match_chars(&pat[1..], txt) { return true; }
            if !txt.is_empty() { return glob_match_chars(pat, &txt[1..]); }
            false
        }
        (Some('?'), Some(_)) => glob_match_chars(&pat[1..], &txt[1..]),
        (Some(p), Some(t)) if p == t => glob_match_chars(&pat[1..], &txt[1..]),
        _ => false,
    }
}
