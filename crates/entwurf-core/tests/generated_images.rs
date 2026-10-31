use entwurf_core::{
    files,
    ingest::{hash_bytes, IngestOptions},
    storage::{self, ProjectPaths},
    workspace::Workspace,
};
use std::io::Cursor;
fn png() -> Vec<u8> {
    let image =
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(3, 2, image::Rgb([20, 40, 60])));
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}
fn workspace(dir: &std::path::Path) -> (Workspace, String, String) {
    let mut ws = Workspace::create(dir.join("project"), "Generated images").unwrap();
    let s = ws.create_task("Image task", None).unwrap();
    let task = s.selected_task_id.unwrap();
    let source = dir.join("source.md");
    std::fs::write(&source, "# Evidence\n\nA source statement.").unwrap();
    let s = ws
        .import_sources(&task, &[source], &IngestOptions::standard())
        .unwrap();
    (ws, task, s.current_version.unwrap().id)
}
#[test]
fn image_import_is_durable_idempotent_and_does_not_rewrite_knowledge() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ws, task, base) = workspace(dir.path());
    let bytes = png();
    let hash = hash_bytes(&bytes);
    let prompt = "a".repeat(64);
    let before = serde_json::to_value(ws.snapshot(Some(&task)).unwrap().current_version).unwrap();
    let s = ws
        .import_generated_image(&task, &base, "run-one", &prompt, &bytes, &hash, "image/png")
        .unwrap();
    assert_eq!(serde_json::to_value(&s.current_version).unwrap(), before);
    let record = s.project.generated_images.as_ref().unwrap()[0].clone();
    assert_eq!((record.width, record.height), (3, 2));
    assert_eq!(record.byte_len, bytes.len() as u64);
    assert_eq!(
        files::read(ws.generated_image_path(&task, &record.id).unwrap()).unwrap(),
        bytes
    );
    let audit_len = s.project.audit.len();
    assert_eq!(
        ws.import_generated_image(&task, &base, "run-one", &prompt, &bytes, &hash, "image/png")
            .unwrap()
            .project
            .audit
            .len(),
        audit_len
    );
    let mut ws = Workspace::open(dir.path().join("project")).unwrap();
    assert_eq!(
        ws.snapshot(None)
            .unwrap()
            .project
            .generated_images
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    assert!(ws
        .import_generated_image(&task, &base, "run-two", &prompt, &bytes, &hash, "image/png")
        .is_err());
    assert!(ws
        .import_generated_image(
            &task,
            &base,
            "run-one",
            &"b".repeat(64),
            &bytes,
            &hash,
            "image/png"
        )
        .is_err());
    std::fs::write(&record.stored_path, b"tampered").unwrap();
    assert!(ws.generated_image_path(&task, &record.id).is_err());
    assert!(ws
        .import_generated_image(&task, &base, "run-one", &prompt, &bytes, &hash, "image/png")
        .is_err());
}
#[test]
fn rejected_images_never_publish_records_or_files() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ws, task, base) = workspace(dir.path());
    let bytes = png();
    let hash = hash_bytes(&bytes);
    let prompt = "a".repeat(64);
    for (version, data, expected, mime) in [
        ("stale", bytes.as_slice(), hash.as_str(), "image/png"),
        (&base, bytes.as_slice(), hash.as_str(), "image/jpeg"),
        (&base, b"not an image".as_slice(), "bad", "image/png"),
        (&base, bytes.as_slice(), hash.as_str(), "image/svg+xml"),
    ] {
        assert!(ws
            .import_generated_image(&task, version, "run-one", &prompt, data, expected, mime)
            .is_err());
    }
    assert!(ws
        .snapshot(None)
        .unwrap()
        .project
        .generated_images
        .is_none());
    assert!(!dir.path().join("project/generated").exists());
}
#[test]
fn image_records_and_run_identity_are_immutable() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ws, task, base) = workspace(dir.path());
    let bytes = png();
    let hash = hash_bytes(&bytes);
    let s = ws
        .import_generated_image(
            &task,
            &base,
            "run-one",
            &"a".repeat(64),
            &bytes,
            &hash,
            "image/png",
        )
        .unwrap();
    let paths = ProjectPaths::new(ws.root());
    for change in ["remove", "rewrite", "duplicate", "wrong-version"] {
        let mut p = s.project.clone();
        let images = p.generated_images.as_mut().unwrap();
        match change {
            "remove" => images.clear(),
            "rewrite" => images[0].mime_type = "image/jpeg".into(),
            "duplicate" => images.push(images[0].clone()),
            _ => images[0].base_version_id = "unknown".into(),
        }
        assert!(
            storage::save_project(&paths, &p).is_err(),
            "accepted {change}"
        );
    }
    let p = storage::load_project(&paths).unwrap();
    assert_eq!(p.generated_images.unwrap().len(), 1);
}
#[test]
fn legacy_project_omits_images_and_stale_new_delivery_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ws, task, base) = workspace(dir.path());
    assert!(serde_json::to_value(ws.snapshot(None).unwrap().project)
        .unwrap()
        .get("generated_images")
        .is_none());
    ws.save_settings("eng").unwrap(); // Settings alone do not change knowledge version.
    let s = ws.snapshot(None).unwrap();
    ws.rollback_version(&task, &base).unwrap();
    assert_eq!(s.current_version.unwrap().id, base);
    let bytes = png();
    assert!(ws
        .import_generated_image(
            &task,
            &base,
            "run",
            &"a".repeat(64),
            &bytes,
            &hash_bytes(&bytes),
            "image/png"
        )
        .is_err());
}

#[test]
fn image_size_and_pixel_bounds_are_checked_before_decoding() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ws, task, base) = workspace(dir.path());
    let oversized = vec![0; 32 * 1024 * 1024 + 1];
    assert!(ws
        .import_generated_image(
            &task,
            &base,
            "large",
            &"a".repeat(64),
            &oversized,
            &"0".repeat(64),
            "image/png"
        )
        .is_err());
    let mut header = Vec::new();
    {
        let mut writer = png::Encoder::new(&mut header, 8001, 8000)
            .write_header()
            .unwrap();
        writer.write_chunk(png::chunk::IDAT, &[0x78, 0x9c]).unwrap();
    }
    let err = ws
        .import_generated_image(
            &task,
            &base,
            "pixels",
            &"a".repeat(64),
            &header,
            &hash_bytes(&header),
            "image/png",
        )
        .unwrap_err();
    assert!(err.to_string().contains("64 megapixels"), "{err}");
    assert!(ws
        .snapshot(None)
        .unwrap()
        .project
        .generated_images
        .is_none());
}

#[cfg(unix)]
#[test]
fn generated_image_symlinks_cannot_escape_owned_storage() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let (mut ws, task, base) = workspace(dir.path());
    let bytes = png();
    let hash = hash_bytes(&bytes);
    let external = dir.path().join("external");
    std::fs::create_dir(&external).unwrap();
    symlink(&external, dir.path().join("project/generated")).unwrap();
    assert!(ws
        .import_generated_image(
            &task,
            &base,
            "outside",
            &"a".repeat(64),
            &bytes,
            &hash,
            "image/png"
        )
        .is_err());
    assert!(!external.join(hash).exists());
}

#[test]
fn supported_image_formats_decode_and_same_run_cannot_change_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ws, task, base) = workspace(dir.path());
    let image =
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(3, 2, image::Rgb([20, 40, 60])));
    for (i, format, mime) in [
        (0, image::ImageFormat::Png, "image/png"),
        (1, image::ImageFormat::Jpeg, "image/jpeg"),
        (2, image::ImageFormat::WebP, "image/webp"),
    ] {
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, format).unwrap();
        let bytes = out.into_inner();
        let hash = hash_bytes(&bytes);
        if i > 0 {
            assert!(ws
                .import_generated_image(&task, &base, "run-0", &"a".repeat(64), &bytes, &hash, mime)
                .is_err());
        }
        let s = ws
            .import_generated_image(
                &task,
                &base,
                &format!("run-{i}"),
                &"a".repeat(64),
                &bytes,
                &hash,
                mime,
            )
            .unwrap();
        assert_eq!(s.project.generated_images.unwrap().len(), i + 1);
    }
}
