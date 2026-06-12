//! Publication map: paths marked as publicly served by the gateway.
//!
//! A published path is just a `Entry` in the manifest, keyed by its
//! `AbsPath`. On each save the Fs refreshes these against the current
//! tree — paths that no longer resolve are pruned.

use std::collections::BTreeMap;

use super::abs_path::AbsPath;
use super::entry::Entry;

/// The set of paths the gateway may serve without vault membership.
/// Each entry is the current `Entry` at that path — the hub uses the
/// Entry's link + secret to fetch and decrypt the content.
pub type Published = BTreeMap<AbsPath, Entry>;
