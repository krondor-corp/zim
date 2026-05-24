pub mod blob;
pub mod history;
pub mod raw;
pub mod tree;

/// Build a breadcrumb trail (`[(label, href_or_None_for_current), ...]`) from
/// a bucket id and a path. Used by every bucket page.
pub(crate) fn breadcrumb(bucket_id: uuid::Uuid, path: &str) -> Vec<(String, Option<String>)> {
    let mut out = vec![("buckets".to_string(), Some("/".to_string()))];
    out.push((bucket_id.to_string(), Some(format!("/b/{bucket_id}/tree"))));
    let mut acc = String::new();
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    for (idx, seg) in segments.iter().enumerate() {
        acc.push('/');
        acc.push_str(seg);
        let is_last = idx + 1 == segments.len();
        let href = if is_last {
            None
        } else {
            Some(format!("/b/{bucket_id}/tree{acc}"))
        };
        out.push(((*seg).to_string(), href));
    }
    out
}
