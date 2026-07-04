"""Smoke tests for the mcr Python bindings.

Build the extension first (`maturin develop` in bindings/python), then run
`pytest tests/`.
"""

import pytest

import mcr


def test_startpos_legal_move_count():
    pos = mcr.Position()
    moves = pos.legal_moves()
    assert len(moves) == 20
    # UCI strings, sorted-stable membership check on a couple of known moves.
    assert "e2e4" in moves
    assert "g1f3" in moves


def test_legal_moves_san():
    pos = mcr.Position()
    san = pos.legal_moves_san()
    assert len(san) == 20
    assert "Nf3" in san
    assert "e4" in san


def test_perft_startpos():
    pos = mcr.Position()
    assert mcr.perft(pos, 0) == 1
    assert mcr.perft(pos, 1) == 20
    assert mcr.perft(pos, 2) == 400
    assert mcr.perft(pos, 3) == 8902


def test_fen_round_trip():
    fen = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2"
    pos = mcr.Position(fen)
    assert pos.fen == fen
    assert pos.turn == "white"


def test_startpos_fen():
    pos = mcr.Position()
    assert pos.fen == "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"


def test_push_mutates_play_does_not():
    pos = mcr.Position()
    start_fen = pos.fen
    nxt = pos.play("e2e4")
    # play() leaves the original untouched.
    assert pos.fen == start_fen
    assert nxt.turn == "black"
    # push() mutates in place.
    pos.push("e2e4")
    assert pos.fen == nxt.fen


def test_san_and_parse_san_round_trip():
    pos = mcr.Position()
    assert pos.san("g1f3") == "Nf3"
    assert pos.parse_san("Nf3") == "g1f3"
    assert pos.parse_san("e4") == "e2e4"


def test_checkmate_detection():
    # Fool's mate: 1. f3 e5 2. g4 Qh4#
    pos = mcr.Position()
    for uci in ("f2f3", "e7e5", "g2g4", "d8h4"):
        pos.push(uci)
    assert pos.is_check()
    assert pos.is_checkmate()
    assert not pos.is_stalemate()
    assert pos.outcome() == "0-1"
    assert pos.end_reason() == "checkmate"


def test_stalemate_detection():
    # Classic stalemate: black king a8, white king c7, white queen b6, black to move.
    pos = mcr.Position("k7/2K5/1Q6/8/8/8/8/8 b - - 0 1")
    assert not pos.legal_moves()
    assert not pos.is_check()
    assert pos.is_stalemate()
    assert not pos.is_checkmate()
    assert pos.outcome() == "1/2-1/2"
    assert pos.end_reason() == "stalemate"


def test_ongoing_game_has_no_outcome():
    pos = mcr.Position()
    assert pos.outcome() is None
    assert pos.end_reason() is None
    assert not pos.is_checkmate()
    assert not pos.is_stalemate()


def test_status_labels():
    assert mcr.Position().status() == "ongoing"

    mate = mcr.Position()
    for uci in ("f2f3", "e7e5", "g2g4", "d8h4"):
        mate.push(uci)
    assert mate.status() == "checkmate"

    stale = mcr.Position("k7/2K5/1Q6/8/8/8/8/8 b - - 0 1")
    assert stale.status() == "stalemate"


def test_is_attacked_and_attackers():
    pos = mcr.Position()
    # White attacks f3 in the start position, but not e4.
    assert pos.is_attacked("f3", "white") is True
    assert pos.is_attacked("e4", "white") is False
    # Attackers of f3 by White: g1 knight + e2/g2 pawns.
    assert set(pos.attackers("f3", "white")) == {"g1", "e2", "g2"}
    # Black does not attack f3.
    assert pos.attackers("f3", "black") == []


def test_attacks_from_and_mobility():
    pos = mcr.Position()
    # The g1 knight attacks e2 (own pawn, defended), f3 and h3.
    assert set(pos.attacks_from("g1")) == {"e2", "f3", "h3"}
    assert pos.mobility("g1") == 3
    # An empty square attacks nothing.
    assert pos.attacks_from("e4") == []
    assert pos.mobility("e4") == 0


def test_analysis_bad_inputs_raise():
    pos = mcr.Position()
    with pytest.raises(ValueError):
        pos.is_attacked("z9", "white")
    with pytest.raises(ValueError):
        pos.is_attacked("e4", "purple")
    with pytest.raises(ValueError):
        pos.mobility("nope")


def test_zobrist_is_stable_and_distinct():
    a = mcr.Position()
    b = mcr.Position()
    assert a.zobrist() == b.zobrist()
    a.push("e2e4")
    assert a.zobrist() != b.zobrist()


def test_str_renders_board():
    pos = mcr.Position()
    board = str(pos)
    lines = board.splitlines()
    assert len(lines) == 8
    assert lines[0] == "r n b q k b n r"
    assert lines[7] == "R N B Q K B N R"


def test_repr_round_trips_via_eval_friendly_fields():
    pos = mcr.Position()
    r = repr(pos)
    assert "Position(" in r
    assert "variant=" in r


def test_variant_startpos_and_perft():
    atomic = mcr.Position(variant="atomic")
    assert atomic.variant == "atomic"
    assert len(atomic.legal_moves()) == 20
    # Atomic perft(2) from the start position.
    assert mcr.perft(atomic, 1) == 20

    zh = mcr.Position.startpos("crazyhouse")
    assert zh.variant == "crazyhouse"
    assert len(zh.legal_moves()) == 20

    # Alias resolution: "koth" -> king of the hill.
    koth = mcr.Position(variant="koth")
    assert koth.variant == "kingofthehill"


def test_invalid_fen_raises_value_error():
    with pytest.raises(ValueError):
        mcr.Position("not a fen")


def test_unknown_variant_raises_value_error():
    with pytest.raises(ValueError):
        mcr.Position(variant="definitely-not-a-variant")


def test_illegal_uci_push_raises_value_error():
    pos = mcr.Position()
    with pytest.raises(ValueError):
        pos.push("e2e5")  # not a legal move
    with pytest.raises(ValueError):
        pos.push("garbage")


def test_invalid_san_raises_value_error():
    pos = mcr.Position()
    with pytest.raises(ValueError):
        pos.parse_san("Qxz9")


# -- Fairy (geometry-layer) variants ----------------------------------------


def test_fairy_xiangqi_startpos_and_perft():
    pos = mcr.FairyPosition("xiangqi")
    assert pos.variant == "xiangqi"
    assert pos.turn == "white"
    # FSF-confirmed Xiangqi startpos perft sequence (tests/perft_xiangqi.rs).
    assert len(pos.legal_moves()) == 44
    assert pos.perft(0) == 1
    assert pos.perft(1) == 44
    assert pos.perft(2) == 1920
    assert pos.perft(3) == 79666


def test_fairy_shogi_startpos_and_perft():
    pos = mcr.FairyPosition.startpos("shogi")
    assert pos.variant == "shogi"
    # FSF-confirmed Shogi startpos perft sequence (tests/perft_shogi.rs).
    assert pos.perft(1) == 30
    assert pos.perft(2) == 900


def test_fairy_alias_resolution():
    # "cchess" -> xiangqi, like the Rust FromStr alias set.
    pos = mcr.FairyPosition("cchess")
    assert pos.variant == "xiangqi"


def test_fairy_push_mutates_play_does_not():
    pos = mcr.FairyPosition("xiangqi")
    start_fen = pos.fen
    first = pos.legal_moves()[0]
    nxt = pos.play(first)
    # play() leaves the original untouched.
    assert pos.fen == start_fen
    assert nxt.turn == "black"
    # push() mutates in place.
    pos.push(first)
    assert pos.fen == nxt.fen


def test_fairy_ongoing_game_has_no_outcome():
    pos = mcr.FairyPosition("xiangqi")
    assert pos.outcome() is None
    assert pos.end_reason() is None
    assert not pos.is_checkmate()
    assert not pos.is_stalemate()
    assert pos.status() == "ongoing"


def test_fairy_variants_catalogue():
    names = mcr.variants()
    assert "xiangqi" in names
    assert "shogi" in names
    assert "janggi" in names


def test_fairy_unknown_variant_raises_value_error():
    with pytest.raises(ValueError):
        mcr.FairyPosition("definitely-not-a-variant")


def test_fairy_illegal_uci_push_raises_value_error():
    pos = mcr.FairyPosition("xiangqi")
    with pytest.raises(ValueError):
        pos.push("a0a9")  # not a legal move
    with pytest.raises(ValueError):
        pos.push("garbage")
