use std::path::PathBuf;
use tempfile::TempDir;

fn setup() -> (TempDir, PathBuf, coven_reach::security::Security) {
    let dir = tempfile::tempdir().unwrap();
    // canonicalize to resolve /private/var -> /var on macOS
    let root = dir.path().canonicalize().unwrap();
    std::env::set_var("COVEN_REACH_ALLOWED_PATHS", root.display().to_string());
    let sec = coven_reach::security::Security::from_env();
    (dir, root, sec)
}

#[test]
fn read_file_text() {
    let (dir, root, sec) = setup();
    let file = root.join("hello.txt");
    std::fs::write(&file, "hello world").unwrap();
    let args = serde_json::json!({
        "operation": "content",
        "sources": [file.display().to_string()],
        "format": "text"
    });
    let result = coven_reach::tools::read::handle(&args, &sec).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["results"][0]["content"]["content"], "hello world");
    drop(dir);
}

#[test]
fn read_checksum_sha256() {
    let (dir, root, sec) = setup();
    let file = root.join("data.bin");
    std::fs::write(&file, b"test data").unwrap();
    let args = serde_json::json!({
        "operation": "checksum",
        "sources": [file.display().to_string()],
        "checksum_algorithm": "sha256"
    });
    let result = coven_reach::tools::read::handle(&args, &sec).unwrap();
    assert_eq!(result["ok"], true);
    let hash = result["results"][0]["checksum"].as_str().unwrap();
    assert_eq!(hash.len(), 64);
    drop(dir);
}

#[test]
fn read_diff_two_files() {
    let (dir, root, sec) = setup();
    let a = root.join("a.txt");
    let b = root.join("b.txt");
    std::fs::write(&a, "line1\nline2\n").unwrap();
    std::fs::write(&b, "line1\nline3\n").unwrap();
    let args = serde_json::json!({
        "operation": "diff",
        "sources": [a.display().to_string(), b.display().to_string()]
    });
    let result = coven_reach::tools::read::handle(&args, &sec).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["changed"], true);
    let diff = result["diff"].as_str().unwrap();
    assert!(diff.contains("-line2"));
    assert!(diff.contains("+line3"));
    drop(dir);
}

#[test]
fn read_path_not_allowed() {
    let (dir, root, sec) = setup();
    let args = serde_json::json!({
        "operation": "content",
        "sources": ["/etc/passwd"]
    });
    let result = coven_reach::tools::read::handle(&args, &sec).unwrap();
    let item = &result["results"][0];
    assert_eq!(item["ok"], false);
    assert!(item["code"].as_str().unwrap().contains("not_allowed"));
    drop(dir);
}

#[test]
fn write_put_and_read_back() {
    let (dir, root, sec) = setup();
    let file = root.join("out.txt");
    let args = serde_json::json!({
        "operation": "put",
        "entries": [{ "path": file.display().to_string(), "content": "coven-reach" }]
    });
    let result = coven_reach::tools::write::handle(&args, &sec).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "coven-reach");
    drop(dir);
}

#[test]
fn write_mkdir_recursive() {
    let (dir, root, sec) = setup();
    let target = root.join("a/b/c");
    let args = serde_json::json!({
        "operation": "mkdir",
        "entries": [{ "path": target.display().to_string(), "recursive": true }]
    });
    let result = coven_reach::tools::write::handle(&args, &sec).unwrap();
    assert_eq!(result["ok"], true);
    assert!(target.is_dir());
    drop(dir);
}

#[test]
fn write_read_only_rejects() {
    let (dir, root, _) = setup();
    std::env::set_var("COVEN_REACH_READ_ONLY", "true");
    let sec = coven_reach::security::Security::from_env();
    std::env::remove_var("COVEN_REACH_READ_ONLY");
    let file = root.join("x.txt");
    let args = serde_json::json!({
        "operation": "put",
        "entries": [{ "path": file.display().to_string(), "content": "x" }]
    });
    let err = coven_reach::tools::write::handle(&args, &sec).unwrap_err();
    assert!(matches!(err, coven_reach::error::ReachError::ReadOnly));
    drop(dir);
}

#[test]
fn find_by_name_pattern() {
    let (dir, root, sec) = setup();
    std::fs::write(root.join("main.rs"), "fn main() {}").unwrap();
    std::fs::write(root.join("lib.rs"), "// lib").unwrap();
    std::fs::write(root.join("README.md"), "# readme").unwrap();
    let args = serde_json::json!({
        "path": root.display().to_string(),
        "name_pattern": "*.rs",
        "recursive": false
    });
    let result = coven_reach::tools::find::handle(&args, &sec).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["count"], 2);
    drop(dir);
}

#[test]
fn find_content_search() {
    let (dir, root, sec) = setup();
    std::fs::write(root.join("needle.txt"), "TODO: fix this\nsome other line").unwrap();
    std::fs::write(root.join("haystack.txt"), "no fixes here").unwrap();
    let args = serde_json::json!({
        "path": root.display().to_string(),
        "content_pattern": "TODO",
        "content_case_sensitive": true,
        "recursive": false
    });
    let result = coven_reach::tools::find::handle(&args, &sec).unwrap();
    assert_eq!(result["ok"], true);
    let count = result["count"].as_u64().unwrap();
    assert!(count >= 1, "expected at least 1 match");
    // needle.txt must be among results
    let results = result["results"].as_array().unwrap();
    let found = results.iter().any(|r| r["name"].as_str().unwrap_or("") == "needle.txt");
    assert!(found, "needle.txt not in results");
    let needle = results.iter().find(|r| r["name"].as_str().unwrap_or("") == "needle.txt").unwrap();
    assert_eq!(needle["contentMatches"][0]["line"], 1);
    drop(dir);
}

#[test]
fn secret_check_detects_fake_openai_key() {
    let (dir, root, sec) = setup();
    let file = root.join("config.env");
    std::fs::write(&file, format!("OPENAI_API_KEY=sk-{}", "a".repeat(40))).unwrap();
    let args = serde_json::json!({ "sources": [file.display().to_string()] });
    let result = coven_reach::tools::secret_check::handle(&args, &sec).unwrap();
    assert_eq!(result["clean"], false);
    assert!(result["totalFindings"].as_u64().unwrap() > 0);
    // Key value must NOT appear in output
    assert!(!result.to_string().contains(&"a".repeat(40)));
    drop(dir);
}

#[test]
fn secret_check_clean_file() {
    let (dir, root, sec) = setup();
    let file = root.join("clean.txt");
    std::fs::write(&file, "nothing secret here").unwrap();
    let args = serde_json::json!({ "sources": [file.display().to_string()] });
    let result = coven_reach::tools::secret_check::handle(&args, &sec).unwrap();
    assert_eq!(result["clean"], true);
    drop(dir);
}

#[test]
fn list_entries_returns_files() {
    let (dir, root, sec) = setup();
    std::fs::write(root.join("x.txt"), "x").unwrap();
    std::fs::write(root.join("y.txt"), "y").unwrap();
    let args = serde_json::json!({ "operation": "entries", "path": root.display().to_string() });
    let result = coven_reach::tools::list::handle(&args, &sec).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["count"], 2);
    drop(dir);
}

#[test]
fn list_system_info() {
    let (dir, root, sec) = setup();
    let args = serde_json::json!({ "operation": "system_info", "info_type": "server_capabilities" });
    let result = coven_reach::tools::list::handle(&args, &sec).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["server"]["name"], "coven-reach");
    drop(dir);
}
