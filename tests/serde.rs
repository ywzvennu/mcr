//! Round-trip tests for the optional `serde` support (issue #112).
//!
//! The whole file is gated behind the `serde` feature, so a default
//! `cargo test` ignores it and a `cargo test --features serde` exercises it.
//! Each case serializes a value to JSON and asserts the deserialized value is
//! equal — the representations chosen for the engine's public types must all be
//! lossless for legal inputs.

#![cfg(feature = "serde")]

use mce::geometry::{
    Board as WideBoard, CannonShogi, Chess8x8, GateRole, GateSquare, GenericPlacement,
    GenericPosition, Geometry, Seirawan, Shogi, Shogi9x9, Sittuyin, Square as WideSquare, WideMove,
    WideMoveKind, WideRole, WideVariant, Xiangqi, Xiangqi9x10,
};
use mce::{
    AnyVariant, Bitboard, Board, CheckCounters, Color, CrazyhouseState, EndReason, File, Move,
    MoveKind, Outcome, Piece, Position, Rank, Role, Square, VariantId,
};

/// Serializes `value` to JSON and back, asserting the result equals the input.
#[track_caller]
fn round_trip<T>(value: T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(&value).expect("serialize");
    let back: T = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        value, back,
        "round trip changed the value (json was {json})"
    );
}

#[test]
fn scalars_and_enums_round_trip() {
    for color in Color::ALL {
        round_trip(color);
    }
    for role in Role::ALL {
        round_trip(role);
    }
    for file in File::ALL {
        round_trip(file);
    }
    for rank in Rank::ALL {
        round_trip(rank);
    }
    for color in Color::ALL {
        for role in Role::ALL {
            round_trip(Piece::new(color, role));
        }
    }
}

#[test]
fn square_round_trips_every_index() {
    for index in 0..64u8 {
        round_trip(Square::new(index));
    }
}

#[test]
fn square_rejects_out_of_range_index() {
    // A square is wire-encoded as its 0..64 index; 64 and up must be refused.
    assert!(serde_json::from_str::<Square>("64").is_err());
    assert!(serde_json::from_str::<Square>("255").is_err());
}

#[test]
fn bitboard_round_trips() {
    for bb in [
        Bitboard::EMPTY,
        Bitboard::FULL,
        Bitboard::FILE_A,
        Bitboard::RANK_8,
        Bitboard(0x00FF_00FF_00FF_00FF),
    ] {
        round_trip(bb);
    }
}

#[test]
fn moves_round_trip_every_kind() {
    let cases = [
        Move::new(Square::E2, Square::E4, MoveKind::Quiet),
        Move::new(Square::D4, Square::E5, MoveKind::Capture),
        Move::new(Square::E2, Square::E4, MoveKind::DoublePawnPush),
        Move::new(Square::D5, Square::E6, MoveKind::EnPassant),
        Move::new(Square::E1, Square::G1, MoveKind::CastleKingside),
        Move::new(Square::E1, Square::C1, MoveKind::CastleQueenside),
        Move::new(
            Square::E7,
            Square::E8,
            MoveKind::Promotion {
                role: Role::Queen,
                capture: false,
            },
        ),
        // Capturing promotion: the capture flag is geometric (file changes).
        Move::new(
            Square::D7,
            Square::E8,
            MoveKind::Promotion {
                role: Role::Knight,
                capture: true,
            },
        ),
        Move::drop(Role::Knight, Square::F3),
        Move::drop(Role::Pawn, Square::E4),
    ];
    for mv in cases {
        round_trip(mv);
    }
}

#[test]
fn board_round_trips_via_fen_placement() {
    for board in [Board::empty(), Board::standard()] {
        round_trip(board);
    }
    // The serialized form is exactly the FEN placement string.
    let json = serde_json::to_string(&Board::standard()).unwrap();
    assert_eq!(json, "\"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR\"");
}

#[test]
fn position_round_trips_via_fen() {
    let fens = [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        // Kiwipete: castling rights, en passant possibilities, many pieces.
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        // En-passant target set, a non-trivial clock and fullmove number.
        "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq c6 0 2",
    ];
    for fen in fens {
        let pos = Position::from_fen(fen).expect("valid fen");
        round_trip(pos.clone());
        // The serialized form is exactly the FEN string, re-parsed identically.
        let json = serde_json::to_string(&pos).unwrap();
        assert_eq!(json, format!("{fen:?}"));
    }
}

#[test]
fn position_rejects_malformed_fen() {
    assert!(serde_json::from_str::<Position>("\"not a fen\"").is_err());
}

#[test]
fn outcome_and_end_reason_round_trip() {
    round_trip(Outcome::Draw);
    round_trip(Outcome::Decisive {
        winner: Color::White,
    });
    round_trip(Outcome::Decisive {
        winner: Color::Black,
    });
    for reason in [
        EndReason::Checkmate,
        EndReason::VariantWin,
        EndReason::KingInTheHill,
        EndReason::ThreeChecks,
        EndReason::RaceFinished,
        EndReason::RaceDraw,
        EndReason::KingExploded,
        EndReason::HordeDefeated,
        EndReason::Stalemate,
        EndReason::InsufficientMaterial,
        EndReason::SeventyFiveMoveRule,
        EndReason::FivefoldRepetition,
        EndReason::FiftyMoveRule,
        EndReason::ThreefoldRepetition,
    ] {
        round_trip(reason);
    }
}

#[test]
fn variant_id_round_trips_every_arm() {
    for id in [
        VariantId::Standard,
        VariantId::Chess960,
        VariantId::Atomic,
        VariantId::Antichess,
        VariantId::Crazyhouse,
        VariantId::KingOfTheHill,
        VariantId::ThreeCheck,
        VariantId::RacingKings,
        VariantId::Horde,
    ] {
        round_trip(id);
    }
}

#[test]
fn any_variant_round_trips_via_variant_and_fen() {
    // A standard start, plus a crazyhouse position whose FEN carries pockets,
    // so the round trip exercises variant-specific state, not just the board.
    let standard = AnyVariant::startpos(VariantId::Standard);
    round_trip(standard);

    let crazy = AnyVariant::from_fen(
        VariantId::Crazyhouse,
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Pn] w KQkq - 0 1",
    )
    .expect("valid crazyhouse fen");
    let json = serde_json::to_string(&crazy).expect("serialize");
    let back: AnyVariant = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(crazy, back);
    assert_eq!(back.variant_id(), VariantId::Crazyhouse);
}

#[test]
fn variant_state_types_round_trip_structurally() {
    // The variant `State` types derive serde directly and round-trip on their
    // own, independent of any position.
    round_trip(CheckCounters::default());
    round_trip(CheckCounters { white: 2, black: 1 });

    round_trip(CrazyhouseState::default());
    let mut state = CrazyhouseState::default();
    // A couple of pocketed roles for White and a promoted square marked.
    state.pockets[0][0] = 3; // White holds three pawns.
    state.pockets[1][4] = 1; // Black holds one queen.
    state.promoted = Bitboard::FILE_A;
    round_trip(state);
}

#[test]
fn crazyhouse_state_round_trips() {
    // Reach a position with pieces in pocket by playing a capture in
    // crazyhouse, then round-trip the variant via its FEN (which encodes the
    // pocket). The exact pocket contents must survive.
    let after_capture = AnyVariant::from_fen(
        VariantId::Crazyhouse,
        "rnbqkbnr/pppp1ppp/8/4p3/3P4/8/PPP1PPPP/RNBQKBNR[] b KQkq - 0 2",
    )
    .expect("valid crazyhouse fen");
    // exd4 is a capture: Black pockets a white pawn.
    let mv = after_capture
        .legal_moves()
        .into_iter()
        .find(|m| m.from() == Square::E5 && m.to() == Square::D4)
        .expect("exd4 is legal");
    let after = after_capture.play(&mv);
    let json = serde_json::to_string(&after).expect("serialize");
    let back: AnyVariant = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        after, back,
        "crazyhouse pocket state must survive (json {json})"
    );
}

// === geometry (wide / fairy-variant) layer =================================

/// Serializes a generic position, asserts the JSON is exactly its quoted FEN, and
/// asserts the deserialized position re-serializes to the same FEN. (A
/// `GenericPosition` has no `PartialEq`; its FEN is its canonical lossless form.)
#[track_caller]
fn position_round_trips<G, V>(pos: GenericPosition<G, V>)
where
    G: Geometry,
    V: WideVariant<G>,
{
    let fen = pos.to_fen();
    let json = serde_json::to_string(&pos).expect("serialize");
    assert_eq!(json, format!("{fen:?}"), "serialized form must be the FEN");
    let back: GenericPosition<G, V> = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.to_fen(), fen, "round trip changed the position");
}

#[test]
fn generic_positions_round_trip_via_fen() {
    // A representative position per family: an 8x8 gating variant with reserves in
    // hand (Seirawan), a hand/drop variant (Shogi), a non-power-of-two board
    // (Xiangqi, 9x10), a setup-phase placement-pocket variant (Sittuyin), and an
    // overflow-role variant whose `WideRole` index runs past 64 (Cannon Shogi).
    position_round_trips(Seirawan::startpos());
    position_round_trips(Shogi::startpos());
    position_round_trips(Xiangqi::startpos());
    position_round_trips(Sittuyin::startpos());
    position_round_trips(CannonShogi::startpos());

    // A non-startpos FEN with a mid-game clock (the documented mce Xiangqi
    // dialect, side to move flipped).
    position_round_trips(
        Xiangqi::from_fen("rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR b - - 3 7")
            .expect("valid xiangqi fen"),
    );
}

#[test]
fn generic_position_rejects_malformed_fen() {
    assert!(serde_json::from_str::<Seirawan>("\"not a fen\"").is_err());
    assert!(serde_json::from_str::<Shogi>("\"garbage\"").is_err());
}

#[test]
fn wide_board_round_trips_via_fen_placement() {
    let cases = [
        WideBoard::<Chess8x8>::from_fen_placement("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR")
            .expect("valid 8x8 placement"),
        WideBoard::<Chess8x8>::empty(),
    ];
    for board in cases {
        let json = serde_json::to_string(&board).expect("serialize");
        let back: WideBoard<Chess8x8> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(board, back, "board round trip changed the value");
    }
    // The serialized form is exactly the FEN placement string.
    let board =
        WideBoard::<Chess8x8>::from_fen_placement("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR")
            .unwrap();
    let json = serde_json::to_string(&board).unwrap();
    assert_eq!(json, "\"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR\"");

    // A 9x10 Xiangqi board, exercising a non-8x8 geometry.
    let xq = WideBoard::<Xiangqi9x10>::from_fen_placement(
        "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR",
    )
    .expect("valid xiangqi placement");
    round_trip(xq);
}

#[test]
fn wide_moves_round_trip_every_kind() {
    let from = WideSquare::<Chess8x8>::new(12);
    let to = WideSquare::<Chess8x8>::new(28);
    let plain = [
        WideMove::new(from, to, WideMoveKind::Quiet),
        WideMove::new(from, to, WideMoveKind::Capture),
        WideMove::new(from, to, WideMoveKind::DoublePawnPush),
        WideMove::new(from, to, WideMoveKind::EnPassant),
        WideMove::new(from, to, WideMoveKind::CastleKingside),
        WideMove::new(from, to, WideMoveKind::CastleQueenside),
    ];
    for mv in plain {
        round_trip(mv);
    }

    // Promotions and drops over every role index, including the overflow roles
    // (index >= 64) that pushed the packed role field to seven bits.
    for role in (0..WideRole::COUNT).filter_map(WideRole::from_index) {
        round_trip(WideMove::new(
            from,
            to,
            WideMoveKind::Promotion {
                role,
                capture: false,
            },
        ));
        round_trip(WideMove::new(
            from,
            to,
            WideMoveKind::Promotion {
                role,
                capture: true,
            },
        ));
        round_trip(WideMove::drop(role, to));
    }

    // The fairy addenda: a Seirawan gate, an S-House hand-gate, and a Duck
    // placement, each on top of a base move.
    let base = WideMove::new(from, to, WideMoveKind::Quiet);
    round_trip(base.with_gate(GateRole::Hawk, GateSquare::Origin));
    round_trip(base.with_gate(GateRole::Elephant, GateSquare::RookOrigin));
    round_trip(base.with_hand_gate::<Chess8x8>(WideRole::Knight, GateSquare::Origin));
    round_trip(base.with_hand_gate::<Chess8x8>(WideRole::Rook, GateSquare::RookOrigin));
    round_trip(base.with_duck::<Chess8x8>(WideSquare::<Chess8x8>::new(40)));

    // A drop on a wide board with a high square index.
    let s = WideSquare::<Shogi9x9>::new(80);
    round_trip(WideMove::drop(WideRole::Pawn, s));
}

#[test]
fn generic_placement_round_trips() {
    round_trip(GenericPlacement::NONE);

    let mut white = [0u8; WideRole::COUNT];
    let mut black = [0u8; WideRole::COUNT];
    white[WideRole::Pawn.index()] = 3;
    white[WideRole::Knight.index()] = 1;
    black[WideRole::Queen.index()] = 2;
    // An overflow-index role, to cover the full count array.
    if let Some(role) = WideRole::from_index(WideRole::COUNT - 1) {
        black[role.index()] = 1;
    }
    round_trip(GenericPlacement::new(white, black));
}

#[test]
fn generic_placement_rejects_wrong_length() {
    // The pocket wire shape must carry exactly one count per role per side.
    assert!(serde_json::from_str::<GenericPlacement>(r#"{"white":[1,2,3],"black":[]}"#).is_err());
}
