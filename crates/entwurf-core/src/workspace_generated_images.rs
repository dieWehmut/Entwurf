//! Generated illustrations are retained assets; importing one never alters knowledge versions.
use super::*;
use crate::project::GeneratedImage;
use std::io::{Cursor, Read};
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_PIXELS: u64 = 64_000_000;
fn hash_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn dimensions(bytes: &[u8], mime: &str) -> Result<(u32, u32)> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err(invalid("generated image must be between 1 byte and 32 MiB"));
    }
    let format = image::guess_format(bytes)
        .map_err(|_| invalid("generated image format is not recognized"))?;
    let expected = match mime {
        "image/png" => image::ImageFormat::Png,
        "image/jpeg" => image::ImageFormat::Jpeg,
        "image/webp" => image::ImageFormat::WebP,
        _ => return Err(invalid("generated image MIME must be PNG, JPEG, or WebP")),
    };
    if format != expected {
        return Err(invalid("generated image MIME does not match its bytes"));
    }
    let (width, height) = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|e| invalid(format!("invalid generated image dimensions: {e}")))?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(invalid("generated image exceeds 64 megapixels"));
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|e| invalid(format!("generated image cannot be decoded: {e}")))?;
    if decoded.width() != width || decoded.height() != height {
        return Err(invalid("generated image dimensions changed during decode"));
    }
    Ok((width, height))
}
impl Workspace {
    fn generated_directory(&self, create: bool) -> Result<PathBuf> {
        let root = fs::canonicalize(self.root()).map_err(|e| EntwurfError::read(self.root(), e))?;
        let directory = root.join("generated");
        if create {
            fs::create_dir_all(&directory).map_err(|e| EntwurfError::write(&directory, e))?;
        }
        if fs::canonicalize(&directory).map_err(|e| EntwurfError::read(&directory, e))? != directory
        {
            return Err(invalid(
                "generated image directory is outside project-owned storage",
            ));
        }
        Ok(directory)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn import_generated_image(
        &mut self,
        task_id: &str,
        base_version_id: &str,
        run_id: &str,
        prompt_sha256: &str,
        bytes: &[u8],
        expected_sha256: &str,
        mime_type: &str,
    ) -> Result<WorkspaceSnapshot> {
        if run_id.trim().is_empty()
            || run_id.len() > 128
            || run_id.chars().any(char::is_control)
            || !hash_valid(prompt_sha256)
            || !hash_valid(expected_sha256)
        {
            return Err(invalid(
                "invalid generated image run identity or SHA-256 metadata",
            ));
        }
        if bytes.len() > MAX_BYTES || crate::ingest::hash_bytes(bytes) != expected_sha256 {
            return Err(invalid("generated image size or SHA-256 mismatch"));
        }
        let (width, height) = dimensions(bytes, mime_type)?;
        let task = self
            .project
            .task(task_id)
            .ok_or_else(|| invalid("generated image task is unknown"))?;
        if !task.versions.iter().any(|v| v.id == base_version_id) {
            return Err(invalid("generated image base version is unknown"));
        }
        if let Some(existing) = self
            .project
            .generated_images
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|i| i.run_id == run_id || i.sha256 == expected_sha256)
        {
            if existing.run_id != run_id
                || existing.sha256 != expected_sha256
                || existing.task_id != task_id
                || existing.base_version_id != base_version_id
                || existing.prompt_sha256 != prompt_sha256
                || existing.mime_type != mime_type
                || existing.byte_len != bytes.len() as u64
                || (existing.width, existing.height) != (width, height)
            {
                return Err(invalid(
                    "generated image run or content identity already belongs to another delivery",
                ));
            }
            self.generated_image_path(task_id, &existing.id)?;
            return self.snapshot(Some(task_id));
        }
        if task.current_version.as_deref() != Some(base_version_id) {
            return Err(invalid("generated image base version changed; start a new generation for the current version"));
        }
        let path = self.generated_directory(true)?.join(expected_sha256);
        if fs::exists(&path) {
            if fs::canonicalize(&path).map_err(|e| EntwurfError::read(&path, e))? != path {
                return Err(invalid(
                    "generated image path is outside project-owned storage",
                ));
            }
            let file = fs::File::open(&path).map_err(|e| EntwurfError::read(&path, e))?;
            let metadata = file.metadata().map_err(|e| EntwurfError::read(&path, e))?;
            if !metadata.is_file() || metadata.len() != bytes.len() as u64 {
                return Err(invalid(
                    "retained generated image size differs from its content identity",
                ));
            }
            let mut retained = Vec::new();
            file.take(MAX_BYTES as u64 + 1)
                .read_to_end(&mut retained)
                .map_err(|e| EntwurfError::read(&path, e))?;
            if retained != bytes {
                return Err(invalid(
                    "retained generated image differs from its content identity",
                ));
            }
        } else {
            storage::write_atomic(&path, bytes)?;
        }
        let mut project = self.project.clone();
        project
            .generated_images
            .get_or_insert_with(Vec::new)
            .push(GeneratedImage {
                id: new_id(),
                run_id: run_id.into(),
                task_id: task_id.into(),
                base_version_id: base_version_id.into(),
                prompt_sha256: prompt_sha256.into(),
                sha256: expected_sha256.into(),
                mime_type: mime_type.into(),
                byte_len: bytes.len() as u64,
                width,
                height,
                stored_path: path.to_string_lossy().into_owned(),
                created_at: chrono::Utc::now(),
                origin: "chatgpt_ui".into(),
            });
        project.audit(
            AuditKind::ImageImported,
            Some(task_id.into()),
            format!("generated image retained from run {run_id}; sha256 {expected_sha256}"),
        );
        self.commit(project, Some(task_id))
    }
    pub fn generated_image_path(&self, task_id: &str, image_id: &str) -> Result<PathBuf> {
        let image = self
            .project
            .generated_images
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|i| i.id == image_id && i.task_id == task_id)
            .ok_or_else(|| invalid("generated image does not belong to this task"))?;
        if !hash_valid(&image.sha256) || image.byte_len > MAX_BYTES as u64 {
            return Err(invalid("invalid stored generated image identity"));
        }
        let path = self.generated_directory(false)?.join(&image.sha256);
        if fs::canonicalize(&image.stored_path)
            .map_err(|e| EntwurfError::read(&image.stored_path, e))?
            != path
        {
            return Err(invalid("generated image is outside project-owned storage"));
        }
        let file = fs::File::open(&path).map_err(|e| EntwurfError::read(&path, e))?;
        let metadata = file.metadata().map_err(|e| EntwurfError::read(&path, e))?;
        if !metadata.is_file() || metadata.len() != image.byte_len {
            return Err(invalid("retained generated image size changed"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| EntwurfError::read(&path, e))?;
        if bytes.len() as u64 != image.byte_len || crate::ingest::hash_bytes(&bytes) != image.sha256
        {
            return Err(invalid("retained generated image SHA-256 changed"));
        }
        if dimensions(&bytes, &image.mime_type)? != (image.width, image.height) {
            return Err(invalid("retained generated image dimensions changed"));
        }
        Ok(path)
    }
}
