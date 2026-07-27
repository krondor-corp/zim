//! Build-time env-var population for `crate::version::BuildInfo`.
//!
//! Mirrors `_zim-peer/build.rs`. Each `cargo:rustc-env=KEY=VALUE`
//! line gets surfaced via `env!("KEY")` (or `option_env!`) in the
//! generated binary.

use std::env;
use std::process::Command;

fn report_build_profile() {
    println!(
        "cargo:rustc-env=BUILD_PROFILE={}",
        env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string())
    );
}

fn report_repository_version() {
    let version = match env::var("CI_BUILD_REF") {
        Ok(val) if !val.is_empty() => val,
        // `--exclude '*'` keeps describe off the tag names so a build at
        // a `zim-v*` tag still stamps the commit sha (short_hash() takes
        // the first 7 chars — a tag name there renders as "zim-v0.").
        _ => match Command::new("git")
            .args(["describe", "--always", "--dirty", "--exclude", "*"])
            .output()
        {
            Ok(output) if output.status.success() => String::from_utf8(output.stdout)
                .unwrap_or_else(|_| "unknown".to_string())
                .trim()
                .to_string(),
            _ => match Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
            {
                Ok(output) if output.status.success() => String::from_utf8(output.stdout)
                    .unwrap_or_else(|_| "unknown".to_string())
                    .trim()
                    .to_string(),
                _ => env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "unknown".to_string()),
            },
        },
    };
    println!("cargo:rustc-env=REPO_VERSION={version}");
}

fn report_build_timestamp() {
    let timestamp = chrono::Utc::now().to_rfc3339();
    println!("cargo:rustc-env=BUILD_TIMESTAMP={timestamp}");
}

fn report_rust_version() {
    let rust_version = match Command::new("rustc").args(["--version"]).output() {
        Ok(output) if output.status.success() => String::from_utf8(output.stdout)
            .unwrap_or_else(|_| "unknown".to_string())
            .trim()
            .to_string(),
        _ => "unknown".to_string(),
    };
    println!("cargo:rustc-env=RUST_VERSION={rust_version}");
}

fn report_target_info() {
    if let Ok(target) = env::var("TARGET") {
        println!("cargo:rustc-env=BUILD_TARGET={target}");
    }
    if let Ok(host) = env::var("HOST") {
        println!("cargo:rustc-env=BUILD_HOST={host}");
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
    report_build_profile();
    report_repository_version();
    report_build_timestamp();
    report_rust_version();
    report_target_info();
}
