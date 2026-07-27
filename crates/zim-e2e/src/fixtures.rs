//! The declarative fixture model — `bin/dev_/fixtures.toml`, parsed
//! properly (serde) instead of by the dev harness's line-oriented bash
//! parser. Same file, one source of truth: the bash side applies it to
//! the interactive dev environment; this harness applies it to a
//! hermetic throwaway one and returns a verdict.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct FixtureFile {
    #[serde(rename = "fixture", default)]
    pub fixtures: Vec<Fixture>,
}

/// One `[[fixture]]` entry. The `type` field picks the variant; fields
/// are per-variant, so unknown/missing keys fail loudly at parse time —
/// typos in fixtures.toml surface before anything runs.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Fixture {
    Vault {
        name: String,
        node: Option<String>,
    },
    File {
        vault: String,
        path: String,
        #[serde(default)]
        content: Option<String>,
        #[serde(default)]
        source: Option<String>,
        node: Option<String>,
    },
    Dir {
        vault: String,
        path: String,
        node: Option<String>,
    },
    Share {
        vault: String,
        peer: String,
        node: Option<String>,
    },
    Mv {
        vault: String,
        from: String,
        to: String,
        node: Option<String>,
    },
    VaultRead {
        vault: String,
        path: String,
        #[serde(default)]
        content: Option<String>,
        node: Option<String>,
    },
    Mount {
        vault: String,
        mount_point: String,
        node: Option<String>,
    },
    MountVerify {
        mount_point: String,
    },
    Unmount {
        vault: String,
        node: Option<String>,
    },
    FuseLs {
        mount_point: String,
        path: String,
    },
    FuseRead {
        mount_point: String,
        path: String,
        #[serde(default)]
        content: Option<String>,
    },
    FuseWrite {
        mount_point: String,
        path: String,
        content: String,
    },
    FuseMv {
        mount_point: String,
        from: String,
        to: String,
    },
    FuseMvIn {
        mount_point: String,
        path: String,
        content: String,
    },
    FuseMvOut {
        mount_point: String,
        path: String,
    },
    FuseRm {
        mount_point: String,
        path: String,
    },
}

impl Fixture {
    /// Is this fixture part of the FUSE block (skipped when FUSE is
    /// unavailable — a skip, never a failure)?
    pub fn is_fuse(&self) -> bool {
        matches!(
            self,
            Fixture::Mount { .. }
                | Fixture::MountVerify { .. }
                | Fixture::Unmount { .. }
                | Fixture::FuseLs { .. }
                | Fixture::FuseRead { .. }
                | Fixture::FuseWrite { .. }
                | Fixture::FuseMv { .. }
                | Fixture::FuseMvIn { .. }
                | Fixture::FuseMvOut { .. }
                | Fixture::FuseRm { .. }
        )
    }
}

pub fn load(path: &std::path::Path) -> anyhow::Result<Vec<Fixture>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))?;
    let file: FixtureFile = toml::from_str(&text)
        .map_err(|e| anyhow::anyhow!("parse {}: {e}", path.display()))?;
    Ok(file.fixtures)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Alice's fixtures file parses, and the story it tells is intact:
    // vaults are created before files land in them, the share precedes
    // the mv that bob's first sync will replay, and the FUSE block is
    // recognized as skippable.
    #[test]
    fn the_shipped_fixtures_file_parses_and_reads_in_order() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../bin/dev_/fixtures.toml");
        let fixtures = load(&root).expect("shipped fixtures.toml must parse");
        assert!(fixtures.len() >= 10, "expected a real fixture set");

        // The first fixture creates a vault — nothing can precede storage.
        assert!(matches!(fixtures[0], Fixture::Vault { .. }));

        // Every FUSE fixture is classified as such (skip-gating relies on it).
        let fuse_count = fixtures.iter().filter(|f| f.is_fuse()).count();
        assert!(fuse_count >= 5, "the FUSE block should be recognized");
    }

    #[test]
    fn unknown_fixture_fields_fail_loudly() {
        let bad = r#"
            [[fixture]]
            type = "vault"
            name = "demo"
            nodee = "alice"
        "#;
        let parsed: Result<FixtureFile, _> = toml::from_str(bad);
        assert!(parsed.is_err(), "typo'd field must be a parse error");
    }
}
