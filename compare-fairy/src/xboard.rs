//! A minimal XBoard/WinBoard (CECP) driver for a HaChu subprocess.
//!
//! GPL FENCE: this module never links HaChu. It spawns the externally provided
//! `hachu` binary as a child process and talks to it over stdin/stdout using the
//! CECP text protocol. Everything here is original mcr-side code; the HaChu
//! binary's licensing does not cross the process boundary.
//!
//! Why CECP and not UCI: unlike Fairy-Stockfish, HaChu speaks the XBoard/WinBoard
//! Chess-Engine Communication Protocol, so the handshake and commands differ from
//! `uci.rs`:
//!
//! * `xboard` + `protover 2` handshake, reading the `feature ...` lines up to
//!   `feature done=1`, and capturing the advertised `variants="..."` list (this
//!   is how we confirm the large-shogi variants — `chu`, `dai`, `tenjiku` — are
//!   available, the readiness signal for issue #380);
//! * `variant <name>` to select the variant;
//! * `force` to stop the engine auto-replying, so positions can be driven;
//! * `setboard <fen>` to set a position (HaChu advertises `setboard=1`);
//! * `usermove <move>` to play a move (HaChu advertises `usermove=1`);
//! * `ping <n>` / `pong <n>` for synchronisation.
//!
//! HaChu has no native `perft` command (its only non-standard debug commands are
//! `p`/`f`/`w`/`b`/`l`, which print board/attack diagnostics). A node-by-node
//! perft is therefore driven *externally* — the harness walks the tree and uses
//! HaChu as a move oracle. That external walk needs mcr's own large-shogi move
//! generator, which is issue #380; see `hachu.rs`.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

/// A live HaChu subprocess with buffered stdio.
#[derive(Debug)]
pub struct Engine {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// The variant names advertised in the `feature variants="..."` line during
    /// the `protover 2` handshake. Used by [`has_variant`](Engine::has_variant)
    /// so the harness can skip cleanly when a needed large-shogi variant is
    /// absent instead of reporting a spurious mismatch.
    variants: Vec<String>,
    /// Monotonic counter for `ping`/`pong` synchronisation.
    ping_seq: u32,
}

impl Engine {
    /// Spawn `path` and complete the `xboard` / `protover 2` handshake, capturing
    /// the advertised variant list.
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
            variants: Vec::new(),
            ping_seq: 0,
        };

        eng.send("xboard")?;
        eng.send("protover 2")?;
        eng.variants = eng.read_features_capturing_variants()?;
        Ok(eng)
    }

    /// Read the `protover 2` reply up to `feature done=1`, returning the variant
    /// names listed in the `feature variants="a,b,c"` line. Bounded by a timeout
    /// so a misbehaving binary cannot hang the harness.
    fn read_features_capturing_variants(&mut self) -> Result<Vec<String>, String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut variants = Vec::new();
        loop {
            if Instant::now() > deadline {
                return Err("timed out waiting for \"feature done=1\"".to_string());
            }
            let mut line = String::new();
            let n = self
                .stdout
                .read_line(&mut line)
                .map_err(|e| format!("read failed: {e}"))?;
            if n == 0 {
                return Err("engine closed stdout before \"feature done=1\"".to_string());
            }
            let trimmed = line.trim();
            if let Some(list) = parse_feature_variants(trimmed) {
                variants = list;
            }
            if is_feature_done(trimmed) {
                return Ok(variants);
            }
        }
    }

    /// The variant names this HaChu binary advertises.
    #[must_use]
    pub fn variants(&self) -> &[String] {
        &self.variants
    }

    /// Returns `true` if this binary advertises the named `variant`.
    #[must_use]
    pub fn has_variant(&self, name: &str) -> bool {
        self.variants.iter().any(|v| v == name)
    }

    /// Write one command followed by a newline.
    fn send(&mut self, cmd: &str) -> Result<(), String> {
        writeln!(self.stdin, "{cmd}").map_err(|e| format!("write {cmd:?} failed: {e}"))?;
        self.stdin.flush().map_err(|e| format!("flush failed: {e}"))
    }

    /// Select the variant (e.g. `chu`, `dai`, `tenjiku`) and drop the engine into
    /// `force` mode so it does not start replying with its own moves.
    pub fn select_variant(&mut self, name: &str) -> Result<(), String> {
        self.send(&format!("variant {name}"))?;
        self.send("force")?;
        self.sync()
    }

    /// Select `name`, allocate `memory_mb` MB of hash and drop into `force` mode,
    /// in the exact order HaChu 0.23 needs for a subsequent `usermove` (the hash
    /// allocation must precede any move, or it segfaults). This is the entry point
    /// for the external move-list perft walk on the large-shogi variants.
    pub fn start_variant(&mut self, name: &str, memory_mb: u32) -> Result<(), String> {
        self.send(&format!("variant {name}"))?;
        self.send(&format!("memory {memory_mb}"))?;
        self.send("level 0 0 10")?;
        self.send("force")?;
        self.sync()
    }

    /// Read HaChu's generated legal-move list for the current position by handing
    /// it a deliberately illegal move (`usermove z9z9`), which under HaChu's
    /// always-on debug output makes it print its full generated move list, then a
    /// `ping`/`pong` sentinel to bound the read. Returns the coordinate move
    /// strings, sorted and deduped, with the `p32p32` null placeholder and any
    /// non-coordinate entries dropped.
    ///
    /// GPL fence unchanged: this only reads the subprocess's text output.
    pub fn dump_legal_moves(&mut self) -> Result<Vec<String>, String> {
        self.send("usermove z9z9")?;
        self.ping_seq += 1;
        let seq = self.ping_seq;
        self.send(&format!("ping {seq}"))?;
        let want = format!("pong {seq}");
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut moves = Vec::new();
        loop {
            if Instant::now() > deadline {
                return Err("timed out reading HaChu move dump".to_string());
            }
            let mut line = String::new();
            let n = self
                .stdout
                .read_line(&mut line)
                .map_err(|e| format!("read failed: {e}"))?;
            if n == 0 {
                return Err("engine closed stdout during move dump".to_string());
            }
            let t = line.trim();
            if t == want {
                break;
            }
            if let Some(mv) = parse_dump_move(t) {
                moves.push(mv);
            }
        }
        moves.sort();
        moves.dedup();
        Ok(moves)
    }

    /// Set the position from a HaChu-dialect FEN (`setboard`).
    ///
    /// Reserved for the node-by-node perft walk in issue #380 (setting arbitrary
    /// large-shogi positions for the oracle); not exercised by the current
    /// readiness mode, hence `allow(dead_code)`.
    #[allow(dead_code)]
    pub fn setboard(&mut self, fen: &str) -> Result<(), String> {
        self.send(&format!("setboard {fen}"))?;
        self.sync()
    }

    /// Play a move in coordinate notation (`usermove`).
    ///
    /// HaChu applies a legal move silently and prints `Illegal move` for an
    /// illegal one; callers that need the legality verdict should read via
    /// [`is_illegal_move_line`]. Used by the Dai external perft walk to replay a
    /// move sequence before dumping the resulting move list.
    pub fn usermove(&mut self, mv: &str) -> Result<(), String> {
        self.send(&format!("usermove {mv}"))
    }

    /// Round-trip a `ping`/`pong` to flush any pending engine output and confirm
    /// the child is still responsive. Bounded by a timeout.
    pub fn sync(&mut self) -> Result<(), String> {
        self.ping_seq += 1;
        let seq = self.ping_seq;
        self.send(&format!("ping {seq}"))?;
        let want = format!("pong {seq}");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if Instant::now() > deadline {
                return Err(format!("timed out waiting for {want:?}"));
            }
            let mut line = String::new();
            let n = self
                .stdout
                .read_line(&mut line)
                .map_err(|e| format!("read failed: {e}"))?;
            if n == 0 {
                return Err(format!("engine closed stdout before {want:?}"));
            }
            if line.trim() == want {
                return Ok(());
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

/// Parse a CECP `feature variants="a,b,c"` line into its variant names.
///
/// Returns `None` for any line that does not carry a `variants="..."` token.
pub fn parse_feature_variants(line: &str) -> Option<Vec<String>> {
    let start = line.find("variants=\"")? + "variants=\"".len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    let list = &rest[..end];
    Some(
        list.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// Parse one line of HaChu's illegal-move move-list dump into its coordinate move.
///
/// A dump line looks like `# -58. 00009259 00013299 j5j6`: a `#` marker, an index
/// ending in `.`, two hex words, then the move text. Returns the move text when it
/// is a real coordinate move on the 15-file (`a..o`) board — a `<file><rank>`
/// origin and destination, with an optional trailing `+` promotion marker — and
/// `None` for the `p32p32` null placeholder, headers, or any other line.
pub fn parse_dump_move(line: &str) -> Option<String> {
    let mut it = line.split_whitespace();
    if it.next()? != "#" {
        return None;
    }
    let idx = it.next()?;
    if !idx.ends_with('.') {
        return None;
    }
    let _h1 = it.next()?;
    let _h2 = it.next()?;
    let mv = it.next()?;
    if is_coordinate_move(mv) {
        Some(mv.trim_end_matches('+').to_string())
    } else {
        None
    }
}

/// Whether `s` is a `<file><rank><file><rank>` coordinate move on a large-shogi
/// board up to 16x16 (files `a..p`, ranks `1..16`), with an optional trailing `+`
/// promotion marker. The upper bound covers Tenjiku (16 files / ranks); the smaller
/// Dai board (a..o, 1..15) never produces the extra file/rank, so widening the
/// bound leaves the Dai walk unchanged.
fn is_coordinate_move(s: &str) -> bool {
    let s = s.trim_end_matches('+');
    let b = s.as_bytes();
    let mut i = 0;
    let mut squares = 0;
    while i < b.len() {
        // File letter a..p.
        if !(b'a'..=b'p').contains(&b[i]) {
            return false;
        }
        i += 1;
        // One- or two-digit rank 1..16.
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        match s[start..i].parse::<u32>() {
            Ok(r) if (1..=16).contains(&r) => {}
            _ => return false,
        }
        squares += 1;
    }
    squares == 2
}

/// Whether `line` is the `feature done=1` handshake terminator.
pub fn is_feature_done(line: &str) -> bool {
    line.split_whitespace().any(|tok| tok == "done=1")
}

/// Whether `line` is HaChu's rejection of an illegal move.
///
/// The legality verdict is consumed by the issue #380 tree walk (`usermove` +
/// this check as the per-move oracle); the current readiness mode does not drive
/// moves, hence `allow(dead_code)`. Covered by a unit test below.
#[allow(dead_code)]
pub fn is_illegal_move_line(line: &str) -> bool {
    line.trim_start().starts_with("Illegal move")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_variants_parses_hachu_line() {
        let line = "feature variants=\"chu,nocastle,shogi,dai,tenjiku,shatranj,makruk,lion\"";
        let v = parse_feature_variants(line).expect("variants parse");
        assert!(v.contains(&"chu".to_string()));
        assert!(v.contains(&"dai".to_string()));
        assert!(v.contains(&"tenjiku".to_string()));
        assert_eq!(v.len(), 8);
    }

    #[test]
    fn non_variant_feature_lines_yield_none() {
        assert_eq!(parse_feature_variants("feature ping=1 setboard=1"), None);
        assert_eq!(parse_feature_variants("feature done=1"), None);
        assert_eq!(parse_feature_variants(""), None);
    }

    #[test]
    fn feature_done_detected() {
        assert!(is_feature_done("feature done=1"));
        assert!(is_feature_done("feature myname=\"HaChu 0.23\" done=1"));
        assert!(!is_feature_done("feature done=0"));
        assert!(!is_feature_done("feature ping=1"));
    }

    #[test]
    fn dump_move_parses_coordinate_moves() {
        assert_eq!(
            parse_dump_move("# -58. 00009259 00013299 j5j6"),
            Some("j5j6".to_string())
        );
        // Two-digit rank and a promotion marker.
        assert_eq!(
            parse_dump_move("# 7. 0000abcd 00001234 m4m11+"),
            Some("m4m11".to_string())
        );
        // The null placeholder and header/other lines are not moves.
        assert_eq!(parse_dump_move("# -41. fffffff8 00013299 p32p32+"), None);
        assert_eq!(parse_dump_move("# suppress = a17"), None);
        assert_eq!(parse_dump_move("# moveNr = 77 in {-8,77}"), None);
        assert_eq!(parse_dump_move("pong 3"), None);
        // Off-board file (past 'o') is rejected.
        assert_eq!(parse_dump_move("# 1. 0 0 z9z9"), None);
    }

    #[test]
    fn illegal_move_line_detected() {
        assert!(is_illegal_move_line("Illegal move"));
        assert!(is_illegal_move_line("Illegal move {no such piece}"));
        assert!(!is_illegal_move_line("pong 3"));
        assert!(!is_illegal_move_line("move a4a5"));
    }
}
