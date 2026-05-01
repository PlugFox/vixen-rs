// Captures git SHA + build timestamp + rust version + profile + target as
// `cargo:rustc-env=VIXEN_*` vars. Read back via `env!` from src/build_info.rs.
//
// Re-runs only on git state changes (HEAD, index, packed-refs) so unrelated edits
// don't bust the build cache.

use std::process::Command;
use std::str;

fn main() {
    let git_dir = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .ok()
        .and_then(|o| {
            str::from_utf8(&o.stdout)
                .ok()
                .map(str::trim)
                .map(str::to_owned)
        });

    let git_hash = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            str::from_utf8(&o.stdout)
                .ok()
                .map(str::trim)
                .map(str::to_owned)
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=VIXEN_GIT_HASH={git_hash}");

    let build_date = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    println!("cargo:rustc-env=VIXEN_BUILD_DATE={build_date}");

    let rust_version = rustc_version::version()
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=VIXEN_RUST_VERSION={rust_version}");

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=VIXEN_BUILD_PROFILE={profile}");

    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=VIXEN_BUILD_TARGET={target}");

    if let Some(d) = git_dir {
        println!("cargo:rerun-if-changed={d}/HEAD");
        println!("cargo:rerun-if-changed={d}/index");
        println!("cargo:rerun-if-changed={d}/packed-refs");
    }
}
