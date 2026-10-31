use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn permitted_host(host: &str, port: u16) -> bool {
    [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
    ]
    .iter()
    .any(|allowed| host.eq_ignore_ascii_case(allowed))
}

pub fn permitted_origin(origin: Option<&str>, port: u16) -> bool {
    origin.is_none_or(|value| {
        value
            .strip_prefix("http://")
            .is_some_and(|host| permitted_host(host, port))
    })
}

pub fn component(value: &str) -> Result<&str, String> {
    let uppercase = value.to_ascii_uppercase();
    let reserved = matches!(uppercase.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            uppercase.strip_prefix(prefix).is_some_and(|suffix| {
                suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
            })
        });
    if value.is_empty()
        || value.len() > 96
        || reserved
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        Err("invalid opaque identifier".into())
    } else {
        Ok(value)
    }
}

pub fn upload_name(value: &str) -> Result<&str, String> {
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
            })
        });
    if value.is_empty()
        || value.len() > 240
        || reserved
        || value.ends_with(['.', ' '])
        || value.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
        })
    {
        Err("upload filename must be one portable filename".into())
    } else {
        Ok(value)
    }
}

pub fn project_path(data: &Path, root: &str) -> Result<PathBuf, String> {
    let id = component(
        root.strip_prefix("/projects/")
            .ok_or("invalid project handle")?,
    )?;
    let projects = data.join("projects");
    if fs::symlink_metadata(&projects)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("project storage must not be a symlink".into());
    }
    let target = projects.join(id);
    if let Ok(metadata) = fs::symlink_metadata(&target) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("project handle does not identify an owned directory".into());
        }
        let actual = fs::canonicalize(&target).map_err(|e| e.to_string())?;
        let expected = fs::canonicalize(&projects)
            .map_err(|e| e.to_string())?
            .join(id);
        if actual != expected {
            return Err("project directory escaped its storage root".into());
        }
    }
    Ok(target)
}
