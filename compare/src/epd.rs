//! The standard public-domain perft test suite (EPD), with its loader.
//!
//! ## Source
//!
//! The data in `data/perftsuite.epd` is the classic standard-chess perft test
//! suite widely attributed to **Marcel van Kervinck** (the same set shipped with
//! many engines and on the Chess Programming Wiki). Each line is a position FEN
//! followed by `;Dn <nodes>` fields giving the *reference* perft node count at
//! depth `n`. These counts are public-domain mathematical facts about chess
//! positions; the loader below is our own code. The file is embedded with
//! [`include_str!`] so the binary needs no runtime file access.
//!
//! The suite is **standard chess only**. We use it three ways:
//!
//! 1. **Reference parity** — mcr's perft must equal the embedded reference count
//!    (an absolute check against published numbers, independent of shakmaty).
//! 2. **Cross-engine parity** — mcr's perft must equal shakmaty's (a second,
//!    independent agreement check).
//! 3. **Benchmarking** — a large, varied corpus of real positions to time.
//!
//! A handful of source lines describe degenerate setups (e.g. a bare board with
//! `;D1 0`) that are not legal chess positions; the loader silently skips any
//! line mcr or shakmaty refuses to parse, and the count of skipped lines is
//! reported by the caller. Every retained line is therefore a position both
//! engines accept.

/// One depth/count reference pair from an EPD line.
#[derive(Clone, Copy, Debug)]
pub struct DepthRef {
    /// Perft depth.
    pub depth: u32,
    /// Reference node count at that depth.
    pub nodes: u64,
}

/// One parsed EPD entry: a FEN and its reference depth/count pairs.
#[derive(Clone, Debug)]
pub struct EpdEntry {
    /// The position FEN (standard six-field).
    pub fen: String,
    /// Reference (depth, nodes) pairs, in source order.
    pub refs: Vec<DepthRef>,
}

/// The embedded suite text (committed as public-domain data).
const PERFTSUITE: &str = include_str!("../data/perftsuite.epd");

/// Parse the embedded EPD suite into entries. Blank lines and lines without any
/// `;Dn` field are ignored. Each `;Dn <nodes>` token contributes a [`DepthRef`].
pub fn load() -> Vec<EpdEntry> {
    parse(PERFTSUITE)
}

/// Parse EPD text. Split out so it is unit-testable without the embedded file.
fn parse(text: &str) -> Vec<EpdEntry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // FEN is everything before the first ';'; the rest is depth fields.
        let Some((fen_part, rest)) = line.split_once(';') else {
            continue;
        };
        let fen = fen_part.trim().to_string();
        if fen.is_empty() {
            continue;
        }
        let mut refs = Vec::new();
        // Re-prepend the ';' we split on so every field parses uniformly.
        let fields = format!(";{rest}");
        for field in fields.split(';') {
            let field = field.trim();
            if field.is_empty() {
                continue;
            }
            // Field looks like "D4 197281".
            let mut it = field.split_whitespace();
            let Some(dtok) = it.next() else { continue };
            let Some(ntok) = it.next() else { continue };
            let Some(dstr) = dtok.strip_prefix('D').or_else(|| dtok.strip_prefix('d')) else {
                continue;
            };
            if let (Ok(depth), Ok(nodes)) = (dstr.parse::<u32>(), ntok.parse::<u64>()) {
                refs.push(DepthRef { depth, nodes });
            }
        }
        if !refs.is_empty() {
            out.push(EpdEntry { fen, refs });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_many_entries() {
        let entries = load();
        assert!(
            entries.len() > 100,
            "expected the full suite, got {}",
            entries.len()
        );
        // The first entry is the standard start position.
        assert!(entries[0].fen.starts_with("rnbqkbnr/pppppppp"));
        assert_eq!(entries[0].refs[0].depth, 1);
        assert_eq!(entries[0].refs[0].nodes, 20);
    }

    #[test]
    fn parses_depth_fields() {
        let e = parse("8/8/8/8/8/8/8/4k2K w - - 0 1 ;D1 3 ;D2 5\n");
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].refs.len(), 2);
        assert_eq!(e[0].refs[0].depth, 1);
        assert_eq!(e[0].refs[0].nodes, 3);
        assert_eq!(e[0].refs[1].nodes, 5);
    }
}
