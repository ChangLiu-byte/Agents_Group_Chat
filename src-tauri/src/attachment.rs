use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Manager};

/// One row of `attachments`: an image the user attached to a message. Also
/// sent to the frontend as part of [`crate::session::Message`].
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Attachment {
    pub id: String,
    pub message_id: String,
    /// Relative to the attachments folder (see [`attachments_dir`]), e.g.
    /// `"<uuid>.png"`. Never an absolute path, so the app data folder can
    /// move without breaking old rows.
    pub file_path: String,
    /// "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    pub mime_type: String,
    /// Unix timestamp (seconds since epoch).
    pub created_at: i64,
}

/// One image as the frontend sends it along with a user message.
#[derive(Debug, Clone, Deserialize)]
pub struct ImageUpload {
    /// Original file name - only its extension is used.
    pub file_name: String,
    pub mime_type: String,
    pub data_base64: String,
}

/// An [`ImageUpload`] that passed validation, ready to be written to disk.
#[derive(Debug)]
pub struct ValidatedImage {
    pub extension: &'static str,
    pub mime_type: &'static str,
    pub bytes: Vec<u8>,
}

/// Every format all four providers accept, as (extensions, mime type).
const ALLOWED_FORMATS: [(&[&str], &str); 4] = [
    (&["png"], "image/png"),
    (&["jpg", "jpeg"], "image/jpeg"),
    (&["gif"], "image/gif"),
    (&["webp"], "image/webp"),
];

const UNSUPPORTED_HINT: &str = "仅支持 PNG / JPEG / GIF / WebP";

/// The image format the file header says this is, going by magic bytes
/// alone - so a renamed non-image file gets rejected here with a clear
/// message instead of as an opaque HTTP 400 from the provider later.
fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Check that extension, declared mime type and actual file header all
/// agree on one supported format, and decode the base64 payload.
pub fn validate_upload(upload: &ImageUpload) -> Result<ValidatedImage, String> {
    let name = &upload.file_name;
    let extension = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| format!("图片 \"{name}\" 没有扩展名，{UNSUPPORTED_HINT}"))?;

    let (extensions, mime_type) = ALLOWED_FORMATS
        .iter()
        .find(|(exts, _)| exts.contains(&extension.as_str()))
        .ok_or_else(|| format!("不支持的图片格式 \"{name}\"，{UNSUPPORTED_HINT}"))?;
    let extension = extensions
        .iter()
        .find(|e| **e == extension)
        .expect("extension was just found in this list");

    if !upload.mime_type.eq_ignore_ascii_case(mime_type) {
        return Err(format!(
            "图片 \"{name}\" 的类型 \"{}\" 与扩展名不符，{UNSUPPORTED_HINT}",
            upload.mime_type
        ));
    }

    let bytes = BASE64
        .decode(upload.data_base64.as_bytes())
        .map_err(|e| format!("图片 \"{name}\" 数据解码失败：{e}"))?;

    if sniff_mime(&bytes) != Some(*mime_type) {
        return Err(format!(
            "图片 \"{name}\" 的文件内容不是有效的 {mime_type}，{UNSUPPORTED_HINT}"
        ));
    }

    Ok(ValidatedImage {
        extension,
        mime_type,
        bytes,
    })
}

/// `<app data dir>/attachments`, created if missing.
pub fn attachments_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("attachments");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Write the image under `dir` as `<uuid>.<ext>` and return that relative
/// path (what goes into `attachments.file_path`).
///
/// TODO: the original file is stored as-is, with no size limit or
/// compression. If attachments get large (providers cap single images at a
/// few MB - Anthropic ~5MB, OpenAI 20MB), a limit or downscaling step could
/// be added here.
pub fn save_image(dir: &Path, image: &ValidatedImage) -> Result<String, String> {
    let relative = format!("{}.{}", uuid::Uuid::new_v4(), image.extension);
    std::fs::write(dir.join(&relative), &image.bytes).map_err(|e| e.to_string())?;
    Ok(relative)
}

/// Best-effort removal of files written by [`save_image`], for when the DB
/// insert that should reference them fails.
pub fn remove_images(dir: &Path, relative_paths: &[String]) {
    for relative in relative_paths {
        if let Err(e) = std::fs::remove_file(dir.join(relative)) {
            eprintln!("failed to clean up attachment \"{relative}\": {e}");
        }
    }
}

/// Read a stored attachment and return it base64-encoded, for embedding in
/// a provider request.
pub fn load_image_base64(dir: &Path, relative: &str) -> Result<String, String> {
    // Paths are always ones we generated, but never let a row point outside
    // the attachments folder.
    let is_plain_relative = Path::new(relative)
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    if !is_plain_relative {
        return Err(format!("invalid attachment path \"{relative}\""));
    }
    let bytes = std::fs::read(dir.join(relative)).map_err(|e| e.to_string())?;
    Ok(BASE64.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG_HEADER: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];

    fn upload(file_name: &str, mime_type: &str, bytes: &[u8]) -> ImageUpload {
        ImageUpload {
            file_name: file_name.into(),
            mime_type: mime_type.into(),
            data_base64: BASE64.encode(bytes),
        }
    }

    #[test]
    fn accepts_matching_png() {
        let image = validate_upload(&upload("Shot.PNG", "image/png", PNG_HEADER)).unwrap();
        assert_eq!(image.extension, "png");
        assert_eq!(image.mime_type, "image/png");
        assert_eq!(image.bytes, PNG_HEADER);
    }

    #[test]
    fn keeps_jpeg_extension_spelling() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(validate_upload(&upload("a.jpeg", "image/jpeg", &jpeg)).unwrap().extension, "jpeg");
        assert_eq!(validate_upload(&upload("a.jpg", "image/jpeg", &jpeg)).unwrap().extension, "jpg");
    }

    #[test]
    fn accepts_gif_and_webp() {
        assert!(validate_upload(&upload("a.gif", "image/gif", b"GIF89a....")).is_ok());
        assert!(validate_upload(&upload("a.webp", "image/webp", b"RIFF\0\0\0\0WEBPVP8 ")).is_ok());
    }

    #[test]
    fn rejects_unsupported_extension() {
        let err = validate_upload(&upload("a.bmp", "image/bmp", b"BM")).unwrap_err();
        assert!(err.contains("不支持"));
        assert!(validate_upload(&upload("noext", "image/png", PNG_HEADER)).is_err());
    }

    #[test]
    fn rejects_mime_extension_mismatch() {
        assert!(validate_upload(&upload("a.png", "image/jpeg", PNG_HEADER)).is_err());
    }

    #[test]
    fn rejects_renamed_non_image() {
        assert!(validate_upload(&upload("a.png", "image/png", b"not an image")).is_err());
        // Real JPEG bytes behind a .png name.
        assert!(validate_upload(&upload("a.png", "image/png", &[0xFF, 0xD8, 0xFF, 0xE0])).is_err());
    }

    #[test]
    fn rejects_bad_base64() {
        let bad = ImageUpload {
            file_name: "a.png".into(),
            mime_type: "image/png".into(),
            data_base64: "!!!".into(),
        };
        assert!(validate_upload(&bad).is_err());
    }

    #[test]
    fn load_refuses_paths_outside_the_folder() {
        let dir = Path::new(".");
        assert!(load_image_base64(dir, "../secret.png").is_err());
        assert!(load_image_base64(dir, "/etc/passwd").is_err());
    }
}
