#[path = "../src/access.rs"]
mod access;

use std::{fs, path::PathBuf};

fn temporary() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "entwurf-web-access-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(path.join("projects")).unwrap();
    path
}

#[test]
fn host_requires_exact_bound_loopback_authority() {
    for host in ["127.0.0.1:48123", "localhost:48123", "[::1]:48123"] {
        assert!(access::permitted_host(host, 48123));
    }
    for host in [
        "evil.test:48123",
        "localhost.evil.test:48123",
        "127.0.0.1:80",
        "127.0.0.1:48123@evil.test",
        "localhost:48123/anything",
    ] {
        assert!(!access::permitted_host(host, 48123), "{host}");
    }
}

#[test]
fn foreign_or_null_origin_cannot_bootstrap_a_local_session() {
    assert!(access::permitted_origin(None, 48123));
    assert!(access::permitted_origin(
        Some("http://127.0.0.1:48123"),
        48123
    ));
    assert!(!access::permitted_origin(Some("https://evil.test"), 48123));
    assert!(!access::permitted_origin(Some("null"), 48123));
    assert!(!access::permitted_origin(
        Some("http://localhost:48123/path"),
        48123
    ));
}

#[test]
fn opaque_project_handles_cannot_be_filesystem_paths() {
    let data = temporary();
    assert_eq!(
        access::project_path(&data, "/projects/proj-123").unwrap(),
        data.join("projects/proj-123")
    );
    for root in [
        "C:\\Users\\other",
        "/etc/passwd",
        "/projects/../outside",
        "/projects/a/b",
        "/projects/a\\b",
        "/projects/",
        "/projects/%2e%2e",
        "/projects/CON",
        "/projects/LPT1",
    ] {
        assert!(access::project_path(&data, root).is_err(), "{root}");
    }
    fs::remove_dir_all(data).unwrap();
}

#[test]
fn client_and_upload_identifiers_reject_reserved_or_ambiguous_components() {
    assert!(access::component("client-123").is_ok());
    for value in [
        "",
        "..",
        "NUL",
        "AUX",
        "COM9",
        "a/b",
        "a\\b",
        "abc ",
        "user:name",
    ] {
        assert!(access::component(value).is_err(), "{value}");
    }
}

#[test]
fn upload_names_preserve_unicode_but_reject_windows_devices_and_aliases() {
    assert!(access::upload_name("概率论 01.md").is_ok());
    for name in [
        "CON.txt", "nul.png", "COM1.md", "a.", "a ", "../a", "a/b", "a\\b", "a:b", "a?b", "a\n.md",
        "", ".",
    ] {
        assert!(access::upload_name(name).is_err(), "{name:?}");
    }
}

#[cfg(unix)]
#[test]
fn project_symlink_cannot_escape_owned_data_directory() {
    let data = temporary();
    let outside = temporary();
    std::os::unix::fs::symlink(&outside, data.join("projects/escape")).unwrap();
    assert!(access::project_path(&data, "/projects/escape").is_err());
    fs::remove_dir_all(data).unwrap();
    fs::remove_dir_all(outside).unwrap();
}
