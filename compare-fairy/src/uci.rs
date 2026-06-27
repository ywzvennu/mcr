//! A minimal UCI driver for a Fairy-Stockfish (FSF) subprocess.
//!
//! GPL FENCE: this module never links FSF. It spawns the externally provided
//! `fairy-stockfish` binary as a child process and talks to it over stdin/stdout
//! using the UCI text protocol. Everything here is original mce-side code; the
//! FSF binary's GPL licensing does not cross the process boundary.
//!
//! The driver speaks only the subset of UCI the perft comparison needs:
//!
//! * `uci` / `isready` handshake;
//! * `setoption name UCI_Variant value <name>` and
//!   `setoption name UCI_Chess960 value <bool>`;
//! * `position fen <fen>`;
//! * `go perft <depth>`, parsing the `Nodes searched: <n>` total (and, on
//!   request, the per-move `<uci>: <n>` divide lines used to localise a
//!   mismatch).

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

/// A live FSF subprocess with buffered stdio, ready to run perft.
#[derive(Debug)]
pub struct Engine {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// The variant currently selected via `UCI_Variant`, to avoid redundant
    /// `setoption` round-trips between positions of the same variant.
    current_variant: Option<String>,
    /// The `UCI_Chess960` flag currently set.
    current_chess960: bool,
    /// The variant names this binary advertises in its `UCI_Variant` combo option
    /// (captured during the `uci` handshake). Used to detect when a large-board
    /// variant (e.g. `grand`) is absent because FSF was built without
    /// `largeboards=yes`, so the harness can skip rather than report a spurious
    /// mismatch on a silently-truncated 8x8 position.
    variants: Vec<String>,
}

/// The result of a single `go perft <depth>` call.
#[derive(Debug, Clone)]
pub struct PerftResult {
    /// The total node count FSF reported on the `Nodes searched:` line.
    pub nodes: u64,
    /// Wall-clock time spent inside the `go perft` round-trip (spawn-to-result
    /// for this one call), used for the head-to-head throughput numbers.
    pub elapsed: Duration,
    /// Per-move divide lines (`uci -> nodes`), captured only when requested.
    pub divide: Vec<(String, u64)>,
}

impl Engine {
    /// Spawn `path` and complete the `uci` / `isready` handshake.
    ///
    /// Returns an error string (never panics) if the binary fails to start or
    /// does not answer the handshake, so the caller can skip gracefully.
    pub fn spawn(path: &str) -> Result<Self, String> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to spawn {path:?}: {e}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "child has no stdin".to_string())?;
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| "child has no stdout".to_string())?,
        );

        let mut eng = Engine {
            child,
            stdin,
            stdout,
            current_variant: None,
            current_chess960: false,
            variants: Vec::new(),
        };

        eng.send("uci")?;
        eng.variants = eng.read_uciok_capturing_variants()?;
        eng.send("isready")?;
        eng.wait_for("readyok")?;
        Ok(eng)
    }

    /// Read the `uci` handshake up to `uciok`, returning the variant names listed
    /// in the `option name UCI_Variant type combo ...` line (the tokens following
    /// each `var`). Bounded by the same timeout as [`Engine::wait_for`].
    fn read_uciok_capturing_variants(&mut self) -> Result<Vec<String>, String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut variants = Vec::new();
        loop {
            if Instant::now() > deadline {
                return Err("timed out waiting for \"uciok\"".to_string());
            }
            let mut line = String::new();
            let n = self
                .stdout
                .read_line(&mut line)
                .map_err(|e| format!("read failed: {e}"))?;
            if n == 0 {
                return Err("engine closed stdout before \"uciok\"".to_string());
            }
            let trimmed = line.trim();
            if trimmed.contains("UCI_Variant") && trimmed.contains("combo") {
                // `... var chess var 3check var atomic ...`: every token right
                // after a `var` token is a variant name.
                let toks: Vec<&str> = trimmed.split_whitespace().collect();
                for w in toks.windows(2) {
                    if w[0] == "var" {
                        variants.push(w[1].to_string());
                    }
                }
            }
            if trimmed == "uciok" {
                return Ok(variants);
            }
        }
    }

    /// Returns `true` if this binary advertises the named `UCI_Variant`.
    #[must_use]
    pub fn has_variant(&self, name: &str) -> bool {
        self.variants.iter().any(|v| v == name)
    }

    /// Write one command followed by a newline.
    fn send(&mut self, cmd: &str) -> Result<(), String> {
        writeln!(self.stdin, "{cmd}").map_err(|e| format!("write {cmd:?} failed: {e}"))?;
        self.stdin.flush().map_err(|e| format!("flush failed: {e}"))
    }

    /// Read lines until one equals `token` (trimmed). Bounded by a timeout so a
    /// misbehaving binary cannot hang the harness.
    fn wait_for(&mut self, token: &str) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if Instant::now() > deadline {
                return Err(format!("timed out waiting for {token:?}"));
            }
            let mut line = String::new();
            let n = self
                .stdout
                .read_line(&mut line)
                .map_err(|e| format!("read failed: {e}"))?;
            if n == 0 {
                return Err(format!("engine closed stdout before {token:?}"));
            }
            if line.trim() == token {
                return Ok(());
            }
        }
    }

    /// Select the variant and Chess960 flag for subsequent positions, skipping
    /// the round-trip when the engine is already in that state.
    pub fn set_variant(&mut self, name: &str, chess960: bool) -> Result<(), String> {
        if self.current_variant.as_deref() != Some(name) {
            self.send(&format!("setoption name UCI_Variant value {name}"))?;
            self.current_variant = Some(name.to_string());
        }
        if self.current_chess960 != chess960 {
            self.send(&format!(
                "setoption name UCI_Chess960 value {}",
                if chess960 { "true" } else { "false" }
            ))?;
            self.current_chess960 = chess960;
        }
        // Sync after the option changes so the next `position` lands cleanly.
        self.send("isready")?;
        self.wait_for("readyok")
    }

    /// Set the position from a (FSF-dialect) FEN.
    pub fn set_position(&mut self, fen: &str) -> Result<(), String> {
        self.send(&format!("position fen {fen}"))
    }

    /// Run `go perft <depth>` and parse the result.
    ///
    /// `want_divide` keeps the per-move breakdown (only worth the allocation
    /// when reproducing a mismatch). The `Nodes searched:` line is required; its
    /// absence is an error.
    pub fn go_perft(&mut self, depth: u32, want_divide: bool) -> Result<PerftResult, String> {
        let start = Instant::now();
        self.send(&format!("go perft {depth}"))?;

        let deadline = start + Duration::from_secs(600);
        let mut divide = Vec::new();
        loop {
            if Instant::now() > deadline {
                return Err(format!("timed out during go perft {depth}"));
            }
            let mut line = String::new();
            let n = self
                .stdout
                .read_line(&mut line)
                .map_err(|e| format!("read failed: {e}"))?;
            if n == 0 {
                return Err("engine closed stdout during perft".to_string());
            }
            let line = line.trim();
            if let Some(rest) = line
                .strip_prefix("Nodes searched:")
                .or_else(|| line.strip_prefix("Nodes searched :"))
            {
                let nodes = rest
                    .trim()
                    .parse::<u64>()
                    .map_err(|e| format!("bad node count in {line:?}: {e}"))?;
                return Ok(PerftResult {
                    nodes,
                    elapsed: start.elapsed(),
                    divide,
                });
            }
            // Divide lines look like `e2e4: 20`. Skip `info`/empty lines.
            if want_divide {
                if let Some((mv, count)) = parse_divide_line(line) {
                    divide.push((mv, count));
                }
            }
        }
    }

    /// Best-effort clean shutdown: send `quit` and reap the child.
    pub fn quit(mut self) {
        let _ = self.send("quit");
        let _ = self.child.wait();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // If the engine was not explicitly `quit`, make sure we do not leak the
        // child process. `kill` is harmless if it has already exited.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Parse a `go perft` divide line of the form `<uci>: <nodes>`.
///
/// Returns `None` for `info`, blank, or otherwise non-divide lines.
fn parse_divide_line(line: &str) -> Option<(String, u64)> {
    let (mv, count) = line.split_once(':')?;
    let mv = mv.trim();
    let count = count.trim();
    if mv.is_empty() || mv.starts_with("info") || mv.contains(' ') {
        return None;
    }
    let count: u64 = count.parse().ok()?;
    Some((mv.to_string(), count))
}

#[cfg(test)]
mod tests {
    use super::parse_divide_line;

    #[test]
    fn divide_line_parses() {
        assert_eq!(parse_divide_line("e2e4: 20"), Some(("e2e4".into(), 20)));
        assert_eq!(parse_divide_line("a7a8q: 7"), Some(("a7a8q".into(), 7)));
    }

    #[test]
    fn non_divide_lines_rejected() {
        assert_eq!(parse_divide_line("Nodes searched: 400"), None);
        assert_eq!(parse_divide_line("info depth 1"), None);
        assert_eq!(parse_divide_line(""), None);
        assert_eq!(parse_divide_line("readyok"), None);
    }
}
