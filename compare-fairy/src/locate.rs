//! Locating (or building) the Fairy-Stockfish (FSF) binary.
//!
//! GPL FENCE: this only *finds or compiles* an FSF binary on the host; it never
//! commits, vendors, or links it. The compiled binary lives under a build dir
//! that is git-ignored. If FSF cannot be obtained, [`locate`] returns the reason
//! so the harness can skip gracefully and print install instructions.
//!
//! Resolution order:
//! 1. `$MCR_FSF_BIN` (or `$FAIRY_STOCKFISH`) pointing at an executable;
//! 2. a `fairy-stockfish` / `fairystockfish` on `PATH`;
//! 3. a previously built binary under the crate's `build/` dir;
//! 4. (only with `--build`) shallow-fetch of a **pinned** FSF commit (see
//!    [`PINNED_FSF_COMMIT`]) + `make` into `build/`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Where a usable FSF binary came from (for the report header).
#[derive(Debug, Clone)]
pub enum Source {
    /// Found via an environment variable.
    Env(String),
    /// Found on `PATH`.
    Path(String),
    /// A previously built binary under the crate `build/` dir.
    Prebuilt(PathBuf),
    /// Freshly cloned + built from upstream.
    Built(PathBuf),
}

/// A located, runnable FSF binary.
#[derive(Debug, Clone)]
pub struct Located {
    /// Absolute (or PATH-resolvable) command to invoke.
    pub bin: String,
    /// How it was obtained.
    pub source: Source,
}

/// The upstream Fairy-Stockfish repository.
const FSF_REPO: &str = "https://github.com/fairy-stockfish/Fairy-Stockfish";

/// The **pinned** Fairy-Stockfish commit the `--build` path checks out, rather
/// than bleeding-edge `master` (issue #394).
///
/// # Why pin
///
/// FSF is this crate's live differential oracle. Cloning `master --depth 1`
/// tracks a moving target: the same unmodified mcr main can pass one day and, on
/// the next rebuild, disagree — not because mcr changed, but because upstream FSF
/// did. Pinning makes the oracle reproducible: the same commit gives the same
/// verdict for everyone, forever, and any real change of verdict is then a change
/// in *mcr*, which is what we want to catch.
///
/// # Why this commit
///
/// `1b5bdd40499bd5c7417bdc532d52fef8847bdf3f` — "Add Georgian chess" (#1004),
/// 2026-05-23. It is the newest upstream commit that actually touches the engine
/// or `variants.ini`; the only later `master` commit at the time of pinning
/// (`fb78cb5`, the `020726 LB` build) is a pure GitHub-Actions dependabot bump
/// (`.github/workflows/*.yml` only — verified `git diff --name-only 1b5bdd4
/// fb78cb5` lists no `src/` or `variants.ini` file), so it is byte-for-byte
/// identical in chess behaviour. This commit's variant rules match mcr's
/// validated perft pins, and the full differential-fuzz sweep (seeds 1–3,
/// 8 games × 60 plies, every variant) reports **0 divergences** against it once
/// the two documented non-mcr/known cases are accounted for: the Empire FSF
/// castle artifact (skipped node — see `difffuzz::is_empire_no_queenside_castle_artifact`,
/// mcr is correct) and the Tori pheasant pin bug held back for follow-up (see
/// `difffuzz::HELD_BACK`). Health check for a fresh build: xiangqi `perft 3`
/// = 79666.
///
/// To advance the pin, pick a newer commit, rebuild, and re-run the full sweep
/// (`--difffuzz --seed {1,2,3} --games 8 --plies 60`) to confirm it stays clean.
const PINNED_FSF_COMMIT: &str = "1b5bdd40499bd5c7417bdc532d52fef8847bdf3f";

/// The build directory under the crate where a cloned/compiled FSF lives. It is
/// git-ignored (see the repo `.gitignore`).
fn build_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("build")
}

/// Candidate paths for an already-built binary inside the build dir.
fn prebuilt_candidates() -> Vec<PathBuf> {
    let src = build_dir().join("Fairy-Stockfish").join("src");
    vec![
        src.join("stockfish"),
        src.join("fairy-stockfish"),
        build_dir().join("stockfish"),
        build_dir().join("fairy-stockfish"),
    ]
}

/// Try to find a runnable FSF binary without building. Returns `Ok` with the
/// binary, or `Err` with a human-readable reason it could not be found.
pub fn locate(allow_build: bool) -> Result<Located, String> {
    // 1. Environment override.
    for var in ["MCR_FSF_BIN", "FAIRY_STOCKFISH"] {
        if let Ok(p) = std::env::var(var) {
            if !p.is_empty() && is_executable(Path::new(&p)) {
                return Ok(Located {
                    bin: p.clone(),
                    source: Source::Env(var.to_string()),
                });
            }
        }
    }

    // 2. On PATH.
    for name in ["fairy-stockfish", "fairystockfish"] {
        if which(name).is_some() {
            return Ok(Located {
                bin: name.to_string(),
                source: Source::Path(name.to_string()),
            });
        }
    }

    // 3. Previously built under build/.
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
        "no fairy-stockfish binary found (set $MCR_FSF_BIN, put it on PATH, or pass --build)"
            .to_string(),
    )
}

/// `git clone` + `make` upstream FSF into the git-ignored build dir.
fn build_from_source() -> Result<Located, String> {
    let dir = build_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let repo = dir.join("Fairy-Stockfish");

    if !repo.join("src").join("Makefile").exists() {
        eprintln!(
            "compare-fairy: fetching pinned Fairy-Stockfish {} into {} ...",
            PINNED_FSF_COMMIT,
            repo.display()
        );
        fetch_pinned(&repo)?;
    }

    let src = repo.join("src");
    eprintln!("compare-fairy: building Fairy-Stockfish (make -j) ...");
    let jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .to_string();
    // Use the portable default arch target; `build` infers a sensible ARCH.
    let status = Command::new("make")
        .current_dir(&src)
        .args(["-j", &jobs, "build", "ARCH=x86-64"])
        .status()
        .map_err(|e| format!("make failed to start: {e}"))?;
    if !status.success() {
        return Err("make build of Fairy-Stockfish failed".to_string());
    }

    let bin = src.join("stockfish");
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

/// Shallow-clone exactly the pinned FSF commit into `repo` (issue #394).
///
/// A plain `git clone --depth 1` only fetches the branch tip, so it cannot land
/// on an arbitrary historical commit. Instead we `init` an empty repo, add the
/// remote, `fetch --depth 1` the pinned SHA directly (GitHub serves fetch-by-full-
/// SHA), and detach onto it — a single-commit checkout with no history, as cheap
/// as the old shallow clone but reproducible.
fn fetch_pinned(repo: &Path) -> Result<(), String> {
    std::fs::create_dir_all(repo).map_err(|e| format!("mkdir {}: {e}", repo.display()))?;
    let git = |args: &[&str]| -> Result<(), String> {
        let status = Command::new("git")
            .current_dir(repo)
            .args(args)
            .status()
            .map_err(|e| format!("git {args:?} failed to start: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("git {args:?} failed"))
        }
    };
    if !repo.join(".git").exists() {
        git(&["init", "-q"])?;
        git(&["remote", "add", "origin", FSF_REPO])?;
    }
    git(&["fetch", "--depth", "1", "origin", PINNED_FSF_COMMIT])?;
    git(&["checkout", "-q", "--detach", PINNED_FSF_COMMIT])
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

/// Install / build instructions printed when FSF cannot be located, so a skipped
/// run is actionable.
pub const INSTALL_HELP: &str = "\
Fairy-Stockfish was not found. To run the comparison, provide its UCI binary by
ONE of:

  * Set MCR_FSF_BIN to an existing fairy-stockfish executable:
        MCR_FSF_BIN=/path/to/fairy-stockfish cargo run --release

  * Put `fairy-stockfish` (or `fairystockfish`) on your PATH.

  * Let this harness build it (clones + compiles upstream into a git-ignored
    build/ dir; needs git + make + a C++ compiler):
        cargo run --release -- --build

  * Build it manually (GPL-3.0+ — driven as a SUBPROCESS only, never linked).
    Check out the pinned commit so the oracle matches this harness (issue #394):
        git clone https://github.com/fairy-stockfish/Fairy-Stockfish
        cd Fairy-Stockfish
        git checkout 1b5bdd40499bd5c7417bdc532d52fef8847bdf3f
        cd src && make -j build ARCH=x86-64 largeboards=yes
        MCR_FSF_BIN=$PWD/stockfish cargo run --release
";
