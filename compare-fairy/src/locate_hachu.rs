//! Locating (or building) the HaChu binary — the large-shogi differential oracle
//! (issue #379).
//!
//! GPL FENCE: HaChu (H.G. Muller's reference engine for Chu / Dai / Tenjiku and
//! other large shogi) is driven purely as a SUBPROCESS oracle, exactly like
//! Fairy-Stockfish (see `locate.rs` / `uci.rs`). This module only *finds or
//! compiles* a `hachu` binary on the host; it never commits, vendors, or links
//! it, and it never copies any HaChu source into mce or this crate. The compiled
//! binary lives under a build dir that is git-ignored (`/compare-fairy/build`).
//! If HaChu cannot be obtained, [`locate`] returns the reason so the harness can
//! skip gracefully and print build instructions.
//!
//! Resolution order:
//! 1. `$MCE_HACHU_BIN` pointing at an executable;
//! 2. a `hachu` on `PATH`;
//! 3. a previously built binary under the crate's `build/hachu/` dir;
//! 4. (only with `--build-hachu`) `git clone` + `make` of upstream HaChu into
//!    `build/hachu`.
//!
//! HaChu speaks the XBoard/WinBoard (CECP) protocol, NOT UCI, so it is driven by
//! `xboard.rs` rather than `uci.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Where a usable HaChu binary came from (for the report header).
#[derive(Debug, Clone)]
pub enum Source {
    /// Found via the `$MCE_HACHU_BIN` environment variable.
    Env,
    /// Found on `PATH`.
    Path(String),
    /// A previously built binary under the crate `build/hachu/` dir.
    Prebuilt(PathBuf),
    /// Freshly cloned + built from upstream.
    Built(PathBuf),
}

/// A located, runnable HaChu binary.
#[derive(Debug, Clone)]
pub struct Located {
    /// Absolute (or PATH-resolvable) command to invoke.
    pub bin: String,
    /// How it was obtained.
    pub source: Source,
}

/// Upstream HaChu source. This is a public-domain (CC0) mirror of H.G. Muller's
/// HaChu; it is only ever *cloned + built* into the git-ignored build dir and
/// driven as a subprocess — never vendored, never linked.
const HACHU_REPO: &str = "https://github.com/ddugovic/hachu";

/// The build directory under the crate. Shares the git-ignored `build/` root with
/// the FSF checkout (see `locate.rs`); HaChu lives in its own `hachu/` subdir.
fn build_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("build")
        .join("hachu")
}

/// Candidate paths for an already-built binary inside the build dir.
fn prebuilt_candidates() -> Vec<PathBuf> {
    vec![build_dir().join("hachu")]
}

/// Try to find a runnable HaChu binary without building. Returns `Ok` with the
/// binary, or `Err` with a human-readable reason it could not be found.
pub fn locate(allow_build: bool) -> Result<Located, String> {
    // 1. Environment override.
    if let Ok(p) = std::env::var("MCE_HACHU_BIN") {
        if !p.is_empty() && is_executable(Path::new(&p)) {
            return Ok(Located {
                bin: p,
                source: Source::Env,
            });
        }
    }

    // 2. On PATH.
    if which("hachu").is_some() {
        return Ok(Located {
            bin: "hachu".to_string(),
            source: Source::Path("hachu".to_string()),
        });
    }

    // 3. Previously built under build/hachu/.
    for cand in prebuilt_candidates() {
        if is_executable(&cand) {
            return Ok(Located {
                bin: cand.to_string_lossy().into_owned(),
                source: Source::Prebuilt(cand),
            });
        }
    }

    // 4. Build from source, only when explicitly allowed.
    if allow_build {
        return build_from_source();
    }

    Err(
        "no hachu binary found (set $MCE_HACHU_BIN, put `hachu` on PATH, or pass --build-hachu)"
            .to_string(),
    )
}

/// `git clone` + `make` upstream HaChu into the git-ignored build dir. HaChu
/// builds from a handful of plain C files with a single `Makefile` (needs only
/// `git`, `make`, and a C compiler — no XBoard/GUI dependency for the engine).
fn build_from_source() -> Result<Located, String> {
    let repo = build_dir();
    std::fs::create_dir_all(repo.parent().unwrap_or(&repo))
        .map_err(|e| format!("mkdir {}: {e}", repo.display()))?;

    if !repo.join("Makefile").exists() {
        eprintln!("compare-fairy: cloning HaChu into {} ...", repo.display());
        let status = Command::new("git")
            .args(["clone", "--depth", "1", HACHU_REPO])
            .arg(&repo)
            .status()
            .map_err(|e| format!("git clone failed to start: {e}"))?;
        if !status.success() {
            return Err("git clone of HaChu failed".to_string());
        }
    }

    eprintln!("compare-fairy: building HaChu (make hachu) ...");
    // Build only the `hachu` engine target (not the man page / install targets,
    // which need `pod2man` / `xboard`).
    let status = Command::new("make")
        .current_dir(&repo)
        .arg("hachu")
        .status()
        .map_err(|e| format!("make failed to start: {e}"))?;
    if !status.success() {
        return Err("make build of HaChu failed".to_string());
    }

    let bin = repo.join("hachu");
    if is_executable(&bin) {
        Ok(Located {
            bin: bin.to_string_lossy().into_owned(),
            source: Source::Built(bin),
        })
    } else {
        Err(format!(
            "build completed but no executable at {}",
            bin.display()
        ))
    }
}

/// Whether `p` exists and is an executable file.
fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// Resolve `name` on `PATH` (a tiny `which`).
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if is_executable(&cand) {
            return Some(cand);
        }
    }
    None
}

/// Install / build instructions printed when HaChu cannot be located, so a
/// skipped run is actionable.
pub const INSTALL_HELP: &str = "\
HaChu (the large-shogi differential oracle) was not found. To run the HaChu
comparison, provide its binary by ONE of:

  * Set MCE_HACHU_BIN to an existing hachu executable:
        MCE_HACHU_BIN=/path/to/hachu cargo run --release -- --hachu

  * Put `hachu` on your PATH.

  * Let this harness build it (clones + compiles upstream into a git-ignored
    build/hachu/ dir; needs git + make + a C compiler):
        cargo run --release -- --hachu --build-hachu

  * Build it manually (HaChu is driven as a SUBPROCESS oracle only, never
    linked; H.G. Muller's reference engine for Chu / Dai / Tenjiku shogi):
        git clone https://github.com/ddugovic/hachu
        cd hachu && make hachu
        MCE_HACHU_BIN=$PWD/hachu cargo run --release -- --hachu
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prebuilt_candidate_lives_under_gitignored_build_dir() {
        // The prebuilt path must sit under `build/` (which the repo `.gitignore`
        // excludes), so a compiled HaChu binary is never accidentally committed.
        let cand = &prebuilt_candidates()[0];
        assert!(cand.ends_with("hachu"));
        assert!(cand.to_string_lossy().contains("/build/hachu/"));
    }

    #[test]
    fn locate_without_build_is_clean_error_when_absent() {
        // With no env var / PATH / prebuilt binary, locate must return a helpful
        // Err rather than panicking (mirrors FSF's graceful skip). We cannot
        // assume the host has hachu, so only assert the shape when it is absent.
        if std::env::var_os("MCE_HACHU_BIN").is_none() && which("hachu").is_none() {
            if let Err(reason) = locate(false) {
                assert!(reason.contains("hachu"));
            }
        }
    }
}
