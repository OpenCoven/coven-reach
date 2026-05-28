use serde_json::Value;
use std::path::Path;
use crate::error::{ReachError, ReachResult};
use crate::security::Security;

pub fn handle(args: &Value, sec: &Security) -> ReachResult<Value> {
    sec.require_write()?;
    let op = args["operation"].as_str().unwrap_or("");
    match op {
        "put" => write_put(args, sec),
        "mkdir" => write_mkdir(args, sec),
        "copy" => write_copy(args, sec),
        "move" => write_move(args, sec),
        "delete" => write_delete(args, sec),
        "touch" => write_touch(args, sec),
        "archive" => write_archive(args, sec),
        "unarchive" => write_unarchive(args, sec),
        _ => Err(ReachError::InvalidArgument(format!("unknown reach_write operation: '{op}'"))),
    }
}

fn write_put(args: &Value, sec: &Security) -> ReachResult<Value> {
    let entries = args["entries"].as_array().ok_or_else(|| {
        ReachError::InvalidArgument("put requires entries array".into())
    })?;
    let mut results = Vec::new();
    for entry in entries {
        let r = put_one(entry, sec);
        results.push(match r {
            Ok(v) => v,
            Err(e) => e.to_json(),
        });
    }
    Ok(serde_json::json!({ "ok": true, "results": results }))
}

fn put_one(entry: &Value, sec: &Security) -> ReachResult<Value> {
    let path = entry["path"].as_str().ok_or_else(|| {
        ReachError::InvalidArgument("put entry requires path".into())
    })?;
    let content = entry["content"].as_str().ok_or_else(|| {
        ReachError::InvalidArgument("put entry requires content".into())
    })?;
    let resolved = sec.check(path)?;
    let mode = entry["write_mode"].as_str().unwrap_or("overwrite");
    let encoding = entry["input_encoding"].as_str().unwrap_or("text");

    if resolved.exists() && mode == "create" {
        return Err(ReachError::InvalidArgument(format!(
            "create mode but file already exists: {}",
            resolved.display()
        )));
    }

    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ReachError::Io {
            path: parent.display().to_string(),
            source: e,
        })?;
    }

    let bytes: Vec<u8> = match encoding {
        "base64" => base64_decode(content)?,
        _ => content.as_bytes().to_vec(),
    };

    match mode {
        "append" => {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&resolved)
                .map_err(|e| ReachError::Io { path: resolved.display().to_string(), source: e })?;
            file.write_all(&bytes).map_err(|e| ReachError::Io {
                path: resolved.display().to_string(),
                source: e,
            })?;
        }
        _ => {
            std::fs::write(&resolved, &bytes).map_err(|e| ReachError::Io {
                path: resolved.display().to_string(),
                source: e,
            })?;
        }
    }

    Ok(serde_json::json!({
        "ok": true,
        "path": resolved.display().to_string(),
        "bytes": bytes.len(),
    }))
}

fn write_mkdir(args: &Value, sec: &Security) -> ReachResult<Value> {
    let entries = args["entries"].as_array().ok_or_else(|| {
        ReachError::InvalidArgument("mkdir requires entries array".into())
    })?;
    let mut results = Vec::new();
    for entry in entries {
        let r = mkdir_one(entry, sec);
        results.push(match r {
            Ok(v) => v,
            Err(e) => e.to_json(),
        });
    }
    Ok(serde_json::json!({ "ok": true, "results": results }))
}

fn mkdir_one(entry: &Value, sec: &Security) -> ReachResult<Value> {
    let path = entry["path"].as_str().ok_or_else(|| {
        ReachError::InvalidArgument("mkdir entry requires path".into())
    })?;
    let recursive = entry["recursive"].as_bool().unwrap_or(true);
    let resolved = sec.check(path)?;
    if recursive {
        std::fs::create_dir_all(&resolved)
    } else {
        std::fs::create_dir(&resolved)
    }
    .map_err(|e| ReachError::Io { path: resolved.display().to_string(), source: e })?;
    Ok(serde_json::json!({ "ok": true, "path": resolved.display().to_string() }))
}

fn write_copy(args: &Value, sec: &Security) -> ReachResult<Value> {
    let entries = args["entries"].as_array().ok_or_else(|| {
        ReachError::InvalidArgument("copy requires entries array".into())
    })?;
    let mut results = Vec::new();
    for entry in entries {
        let r = copy_one(entry, sec);
        results.push(match r { Ok(v) => v, Err(e) => e.to_json() });
    }
    Ok(serde_json::json!({ "ok": true, "results": results }))
}

fn copy_one(entry: &Value, sec: &Security) -> ReachResult<Value> {
    let src = entry["source_path"].as_str().ok_or_else(|| ReachError::InvalidArgument("copy requires source_path".into()))?;
    let dst = entry["destination_path"].as_str().ok_or_else(|| ReachError::InvalidArgument("copy requires destination_path".into()))?;
    let src_p = sec.check_exists(src)?;
    let dst_p = sec.check(dst)?;
    let final_dst = if dst_p.is_dir() {
        dst_p.join(src_p.file_name().unwrap_or_default())
    } else {
        dst_p
    };
    if let Some(p) = final_dst.parent() {
        std::fs::create_dir_all(p).map_err(|e| ReachError::Io { path: p.display().to_string(), source: e })?;
    }
    if src_p.is_dir() {
        copy_dir_all(&src_p, &final_dst)?;
    } else {
        std::fs::copy(&src_p, &final_dst).map_err(|e| ReachError::Io { path: src_p.display().to_string(), source: e })?;
    }
    Ok(serde_json::json!({ "ok": true, "source": src_p.display().to_string(), "destination": final_dst.display().to_string() }))
}

fn copy_dir_all(src: &Path, dst: &Path) -> ReachResult<()> {
    std::fs::create_dir_all(dst).map_err(|e| ReachError::Io { path: dst.display().to_string(), source: e })?;
    for entry in std::fs::read_dir(src).map_err(|e| ReachError::Io { path: src.display().to_string(), source: e })? {
        let entry = entry.map_err(|e| ReachError::Io { path: src.display().to_string(), source: e })?;
        let ty = entry.file_type().map_err(|e| ReachError::Io { path: entry.path().display().to_string(), source: e })?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), &dst_path).map_err(|e| ReachError::Io { path: entry.path().display().to_string(), source: e })?;
        }
    }
    Ok(())
}

fn write_move(args: &Value, sec: &Security) -> ReachResult<Value> {
    let entries = args["entries"].as_array().ok_or_else(|| ReachError::InvalidArgument("move requires entries array".into()))?;
    let mut results = Vec::new();
    for entry in entries {
        let r = move_one(entry, sec);
        results.push(match r { Ok(v) => v, Err(e) => e.to_json() });
    }
    Ok(serde_json::json!({ "ok": true, "results": results }))
}

fn move_one(entry: &Value, sec: &Security) -> ReachResult<Value> {
    let src = entry["source_path"].as_str().ok_or_else(|| ReachError::InvalidArgument("move requires source_path".into()))?;
    let dst = entry["destination_path"].as_str().ok_or_else(|| ReachError::InvalidArgument("move requires destination_path".into()))?;
    let src_p = sec.check_exists(src)?;
    let dst_p = sec.check(dst)?;
    let final_dst = if dst_p.is_dir() {
        dst_p.join(src_p.file_name().unwrap_or_default())
    } else {
        dst_p
    };
    if let Some(p) = final_dst.parent() {
        std::fs::create_dir_all(p).map_err(|e| ReachError::Io { path: p.display().to_string(), source: e })?;
    }
    std::fs::rename(&src_p, &final_dst).map_err(|e| ReachError::Io { path: src_p.display().to_string(), source: e })?;
    Ok(serde_json::json!({ "ok": true, "source": src_p.display().to_string(), "destination": final_dst.display().to_string() }))
}

fn write_delete(args: &Value, sec: &Security) -> ReachResult<Value> {
    let entries = args["entries"].as_array().ok_or_else(|| ReachError::InvalidArgument("delete requires entries array".into()))?;
    let mut results = Vec::new();
    for entry in entries {
        let r = delete_one(entry, sec);
        results.push(match r { Ok(v) => v, Err(e) => e.to_json() });
    }
    Ok(serde_json::json!({ "ok": true, "results": results }))
}

fn delete_one(entry: &Value, sec: &Security) -> ReachResult<Value> {
    let path = entry["path"].as_str().ok_or_else(|| ReachError::InvalidArgument("delete requires path".into()))?;
    let recursive = entry["recursive"].as_bool().unwrap_or(false);
    let resolved = sec.check_exists(path)?;
    if resolved.is_dir() {
        if recursive {
            std::fs::remove_dir_all(&resolved).map_err(|e| ReachError::Io { path: resolved.display().to_string(), source: e })?;
        } else {
            std::fs::remove_dir(&resolved).map_err(|e| ReachError::Io { path: resolved.display().to_string(), source: e })?;
        }
    } else {
        std::fs::remove_file(&resolved).map_err(|e| ReachError::Io { path: resolved.display().to_string(), source: e })?;
    }
    Ok(serde_json::json!({ "ok": true, "path": resolved.display().to_string() }))
}

fn write_touch(args: &Value, sec: &Security) -> ReachResult<Value> {
    let entries = args["entries"].as_array().ok_or_else(|| ReachError::InvalidArgument("touch requires entries array".into()))?;
    let mut results = Vec::new();
    for entry in entries {
        let path = entry["path"].as_str().unwrap_or("");
        let r = (|| -> ReachResult<Value> {
            let resolved = sec.check(path)?;
            if !resolved.exists() {
                if let Some(p) = resolved.parent() {
                    std::fs::create_dir_all(p).map_err(|e| ReachError::Io { path: p.display().to_string(), source: e })?;
                }
                std::fs::File::create(&resolved).map_err(|e| ReachError::Io { path: resolved.display().to_string(), source: e })?;
            }
            // Update mtime by writing 0 bytes
            let file = std::fs::OpenOptions::new().write(true).open(&resolved)
                .map_err(|e| ReachError::Io { path: resolved.display().to_string(), source: e })?;
            drop(file);
            Ok(serde_json::json!({ "ok": true, "path": resolved.display().to_string() }))
        })();
        results.push(match r { Ok(v) => v, Err(e) => e.to_json() });
    }
    Ok(serde_json::json!({ "ok": true, "results": results }))
}

fn write_archive(args: &Value, sec: &Security) -> ReachResult<Value> {
    let format = args["format"].as_str().unwrap_or("zip");
    let archive_path = args["archive_path"].as_str().ok_or_else(|| ReachError::InvalidArgument("archive requires archive_path".into()))?;
    let source_paths: Vec<&str> = args["source_paths"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let archive_resolved = sec.check(archive_path)?;
    if let Some(p) = archive_resolved.parent() {
        std::fs::create_dir_all(p).map_err(|e| ReachError::Io { path: p.display().to_string(), source: e })?;
    }

    let sources: Vec<std::path::PathBuf> = source_paths.iter()
        .map(|p| sec.check_exists(p))
        .collect::<Result<Vec<_>, _>>()?;

    match format {
        "zip" => {
            let file = std::fs::File::create(&archive_resolved).map_err(|e| ReachError::Io { path: archive_resolved.display().to_string(), source: e })?;
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::FileOptions::<()>::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for src in &sources {
                add_to_zip(&mut zip, src, src.file_name().unwrap_or_default().to_str().unwrap_or("file"), opts)?;
            }
            zip.finish().map_err(|e| ReachError::Archive(e.to_string()))?;
        }
        "tar.gz" | "tgz" => {
            let file = std::fs::File::create(&archive_resolved).map_err(|e| ReachError::Io { path: archive_resolved.display().to_string(), source: e })?;
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut tar = tar::Builder::new(enc);
            for src in &sources {
                let name = src.file_name().unwrap_or_default().to_str().unwrap_or("file");
                if src.is_dir() {
                    tar.append_dir_all(name, src).map_err(|e| ReachError::Archive(e.to_string()))?;
                } else {
                    tar.append_path_with_name(src, name).map_err(|e| ReachError::Archive(e.to_string()))?;
                }
            }
            tar.into_inner().map_err(|e| ReachError::Archive(e.to_string()))?
                .finish().map_err(|e| ReachError::Archive(e.to_string()))?;
        }
        _ => return Err(ReachError::InvalidArgument(format!("unsupported archive format '{format}' — use zip or tar.gz"))),
    }

    Ok(serde_json::json!({
        "ok": true,
        "archivePath": archive_resolved.display().to_string(),
        "format": format,
        "sources": sources.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
    }))
}

fn add_to_zip<W: std::io::Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    path: &Path,
    name: &str,
    opts: zip::write::FileOptions<()>,
) -> ReachResult<()> {
    if path.is_dir() {
        zip.add_directory(format!("{}/", name), opts).map_err(|e| ReachError::Archive(e.to_string()))?;
        for entry in std::fs::read_dir(path).map_err(|e| ReachError::Io { path: path.display().to_string(), source: e })? {
            let entry = entry.map_err(|e| ReachError::Io { path: path.display().to_string(), source: e })?;
            let child_name = format!("{}/{}", name, entry.file_name().to_str().unwrap_or("file"));
            add_to_zip(zip, &entry.path(), &child_name, opts)?;
        }
    } else {
        zip.start_file(name, opts).map_err(|e| ReachError::Archive(e.to_string()))?;
        let bytes = std::fs::read(path).map_err(|e| ReachError::Io { path: path.display().to_string(), source: e })?;
        use std::io::Write;
        zip.write_all(&bytes).map_err(|e| ReachError::Archive(e.to_string()))?;
    }
    Ok(())
}

fn write_unarchive(args: &Value, sec: &Security) -> ReachResult<Value> {
    let archive_path = args["archive_path"].as_str().ok_or_else(|| ReachError::InvalidArgument("unarchive requires archive_path".into()))?;
    let destination = args["destination_path"].as_str().ok_or_else(|| ReachError::InvalidArgument("unarchive requires destination_path".into()))?;
    let format = args["format"].as_str().unwrap_or("zip");

    let archive_resolved = sec.check_exists(archive_path)?;
    let dest_resolved = sec.check(destination)?;
    std::fs::create_dir_all(&dest_resolved).map_err(|e| ReachError::Io { path: dest_resolved.display().to_string(), source: e })?;

    match format {
        "zip" => {
            let file = std::fs::File::open(&archive_resolved).map_err(|e| ReachError::Io { path: archive_resolved.display().to_string(), source: e })?;
            let mut archive = zip::ZipArchive::new(file).map_err(|e| ReachError::Archive(e.to_string()))?;
            archive.extract(&dest_resolved).map_err(|e| ReachError::Archive(e.to_string()))?;
        }
        "tar.gz" | "tgz" => {
            let file = std::fs::File::open(&archive_resolved).map_err(|e| ReachError::Io { path: archive_resolved.display().to_string(), source: e })?;
            let dec = flate2::read::GzDecoder::new(file);
            let mut archive = tar::Archive::new(dec);
            archive.unpack(&dest_resolved).map_err(|e| ReachError::Archive(e.to_string()))?;
        }
        _ => return Err(ReachError::InvalidArgument(format!("unsupported format '{format}'"))),
    }

    Ok(serde_json::json!({
        "ok": true,
        "archivePath": archive_resolved.display().to_string(),
        "destination": dest_resolved.display().to_string(),
    }))
}

fn base64_decode(s: &str) -> ReachResult<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let decode_char = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' | b'\n' | b'\r' => Some(0),
            _ => None,
        }
    };
    let chunks: Vec<&[u8]> = bytes.chunks(4).collect();
    for chunk in chunks {
        if chunk.len() < 2 { break; }
        let b0 = decode_char(chunk[0]).ok_or_else(|| ReachError::InvalidArgument("invalid base64".into()))? as usize;
        let b1 = decode_char(chunk[1]).ok_or_else(|| ReachError::InvalidArgument("invalid base64".into()))? as usize;
        out.push(((b0 << 2) | (b1 >> 4)) as u8);
        if chunk.len() > 2 && chunk[2] != b'=' {
            let b2 = decode_char(chunk[2]).ok_or_else(|| ReachError::InvalidArgument("invalid base64".into()))? as usize;
            out.push((((b1 & 0xf) << 4) | (b2 >> 2)) as u8);
            if chunk.len() > 3 && chunk[3] != b'=' {
                let b3 = decode_char(chunk[3]).ok_or_else(|| ReachError::InvalidArgument("invalid base64".into()))? as usize;
                out.push((((b2 & 0x3) << 6) | b3) as u8);
            }
        }
    }
    Ok(out)
}
