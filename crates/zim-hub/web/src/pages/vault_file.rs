//! Turning a vault file's decrypted bytes into something the editor can
//! show (MIME guess, object URL, fingerprint, decoded text), and the
//! open-file state the vault page keeps it in.

use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use yew::Reducible;

use crate::backend::{ReadError, VaultBackend};
use crate::pages::vault_editor::OpenFile;
use crate::util::jserr;

/// How long a file whose blob is still syncing to the hub is retried
/// before giving up: `SYNC_RETRIES` attempts, `SYNC_RETRY_MS` apart.
pub(crate) const SYNC_RETRIES: u32 = 30;
pub(crate) const SYNC_RETRY_MS: i32 = 2_000;

pub(crate) fn guess_mime(name: &str) -> String {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "flac" => "audio/flac",
        "json" => "application/json",
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "html" | "htm" => "text/html",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "txt" | "md" | "markdown" | "log" | "toml" | "yaml" | "yml" | "ini" | "conf" | "rs"
        | "py" | "go" | "c" | "h" | "cpp" | "sh" | "sql" => "text/plain",
        _ => "application/octet-stream",
    }
    .to_string()
}

pub(crate) fn is_textual(mime: &str) -> bool {
    mime.starts_with("text/")
        || matches!(
            mime,
            "application/json" | "application/xml" | "application/javascript"
        )
}

/// Build an object URL over `bytes` tagged with `mime`, for blob-backed
/// rendering (`<img src>`, `<iframe src>`, download links).
pub(crate) fn object_url(bytes: &[u8], mime: &str) -> Result<String, String> {
    let arr = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&arr);
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    let blob =
        web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts).map_err(jserr)?;
    web_sys::Url::create_object_url_with_blob(&blob).map_err(jserr)
}

/// Fingerprint decrypted bytes for [`OpenFile::digest`].
pub(crate) fn digest(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Load `path` into an [`OpenFile`] (decrypt, object-URL, decode text).
pub(crate) async fn load_file(backend: &VaultBackend, path: String) -> Result<OpenFile, ReadError> {
    static LOADS: AtomicU32 = AtomicU32::new(0);
    let bytes = backend.cat(path.clone()).await?;
    let name = path.rsplit('/').next().unwrap_or_default();
    let mime = guess_mime(name);
    let url = object_url(&bytes, &mime).map_err(ReadError::Failed)?;
    let digest = digest(&bytes);
    let text = if is_textual(&mime) && bytes.len() < 2_000_000 {
        String::from_utf8(bytes).ok()
    } else {
        None
    };
    Ok(OpenFile {
        path,
        mime,
        url,
        text,
        digest,
        rev: LOADS.fetch_add(1, Ordering::Relaxed),
    })
}

/// The file in the editor pane, behind a reducer so async tasks act on
/// the *current* open file rather than the one their render saw.
#[derive(Default, PartialEq)]
pub(crate) struct OpenState(pub Option<OpenFile>);

pub(crate) enum OpenAction {
    /// Show this file (a fresh load: the editor remounts) or none.
    Show(Option<OpenFile>),
    /// Autosave wrote `text` to `path`. If it's open, that is its new
    /// baseline — updated in place, same `rev`, so the editor keeps its
    /// cursor and scroll.
    Saved { path: String, text: String },
    /// The follow loop re-read the open file after a head change. Shown
    /// only if that file is still open and its bytes really changed —
    /// never for our own just-saved bytes.
    Remote(OpenFile),
    /// `path` (or the folder it names) was deleted.
    Removed(String),
    /// The file at `from` moved; this is it at its new path.
    Moved { from: String, file: OpenFile },
}

impl Reducible for OpenState {
    type Action = OpenAction;

    fn reduce(self: Rc<Self>, action: OpenAction) -> Rc<Self> {
        let open = self.0.as_ref();
        match action {
            OpenAction::Show(file) => Rc::new(Self(file)),
            OpenAction::Saved { path, text } => match open {
                Some(f) if f.path == path => {
                    let bytes = text.as_bytes();
                    let mut next = f.clone();
                    next.digest = digest(bytes);
                    if let Ok(url) = object_url(bytes, &f.mime) {
                        next.url = url;
                    }
                    next.text = Some(text);
                    Rc::new(Self(Some(next)))
                }
                _ => self,
            },
            OpenAction::Remote(file) => match open {
                Some(f) if f.path == file.path && f.digest != file.digest => {
                    Rc::new(Self(Some(file)))
                }
                _ => {
                    let _ = web_sys::Url::revoke_object_url(&file.url);
                    self
                }
            },
            OpenAction::Removed(path) => match open {
                Some(f) if f.path == path || f.path.starts_with(&format!("{path}/")) => {
                    Rc::new(Self(None))
                }
                _ => self,
            },
            OpenAction::Moved { from, file } => match open {
                Some(f) if f.path == from => Rc::new(Self(Some(file))),
                _ => {
                    let _ = web_sys::Url::revoke_object_url(&file.url);
                    self
                }
            },
        }
    }
}
