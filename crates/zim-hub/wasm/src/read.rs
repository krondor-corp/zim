//! Read-side helpers shared by the live [`WasmFs`](crate::WasmFs) and the
//! read-only [`WasmVersion`](crate::WasmVersion): both list and decrypt
//! from a materialised [`Fs`] tree, one at the head and one at a
//! historical height.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use zim_core::fs::{AbsPath, Entry, Fs};

use crate::fs::HubBlobStore;

/// `Error.name` of a `cat` failure whose blob the hub doesn't have yet.
pub const BLOB_PENDING: &str = "BlobPending";

fn abs(path: &str) -> Result<AbsPath, JsError> {
    AbsPath::new(path).ok_or_else(|| JsError::new(&format!("invalid path: {path}")))
}

/// List a directory as JSON: `[{name, kind, hash, mime}]`.
pub(crate) async fn ls(fs: &Fs<HubBlobStore>, path: &str) -> Result<String, JsError> {
    let entries = fs
        .ls(&abs(path)?)
        .await
        .map_err(|e| JsError::new(&format!("ls: {e}")))?;

    #[derive(Serialize)]
    struct View {
        name: String,
        kind: &'static str,
        hash: String,
        mime: Option<String>,
    }

    let views: Vec<View> = entries
        .into_iter()
        .map(|(p, e)| {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            match e {
                Entry::File { link, mime, .. } => View {
                    name,
                    kind: "file",
                    hash: link.hash().to_hex(),
                    mime: mime.0.as_ref().map(|m| m.to_string()),
                },
                Entry::Dir { link, .. } => View {
                    name,
                    kind: "dir",
                    hash: link.hash().to_hex(),
                    mime: None,
                },
            }
        })
        .collect();

    serde_json::to_string(&views).map_err(|e| JsError::new(&format!("json: {e}")))
}

/// Decrypt a file's plaintext. A blob the hub hasn't mirrored yet fails
/// with `Error.name` [`BLOB_PENDING`].
pub(crate) async fn cat(fs: &Fs<HubBlobStore>, path: &str) -> Result<Vec<u8>, JsValue> {
    fs.cat(&abs(path)?).await.map_err(|e| {
        let err = js_sys::Error::new(&format!("cat: {e}"));
        if e.missing_blob().is_some() {
            err.set_name(BLOB_PENDING);
        }
        err.into()
    })
}
