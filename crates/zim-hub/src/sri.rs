//! Build-time SRI hashes for vendored client assets. See `build.rs`.

include!(concat!(env!("OUT_DIR"), "/sri.rs"));
