/*
 * test.c — smoke test for the mce C ABI.
 *
 * Demonstrates the ownership and two-call buffer contracts: builds the standard
 * starting position, prints the FEN and the space-separated legal moves, checks
 * the legal-move count (20) and a couple of perft values, then plays the Fool's
 * mate line and verifies the checkmate outcome.
 *
 * Build + run (from this `bindings/c/` directory):
 *     ./build_test.sh
 *
 * or by hand against the static lib:
 *     cargo build --release
 *     cc -I include test.c -o test_runner \
 *        target/release/libmce.a -lpthread -ldl -lm
 *     ./test_runner
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "mce.h"

static int failures = 0;

#define CHECK(cond, msg)                                                    \
    do {                                                                    \
        if (cond) {                                                         \
            printf("  ok   - %s\n", (msg));                                 \
        } else {                                                            \
            printf("  FAIL - %s\n", (msg));                                 \
            failures++;                                                     \
        }                                                                   \
    } while (0)

/* Read a string out of an mce two-call API into a heap buffer (caller frees). */
static char *read_string(size_t (*fn)(const McePosition *, char *, size_t),
                         const McePosition *pos) {
    size_t need = fn(pos, NULL, 0);
    if (need == 0) {
        return NULL;
    }
    char *buf = (char *)malloc(need);
    if (buf == NULL) {
        return NULL;
    }
    size_t got = fn(pos, buf, need);
    if (got != need) {
        free(buf);
        return NULL;
    }
    return buf;
}

static size_t count_words(const char *s) {
    size_t n = 0;
    int in_word = 0;
    for (; *s; s++) {
        if (*s == ' ') {
            in_word = 0;
        } else if (!in_word) {
            in_word = 1;
            n++;
        }
    }
    return n;
}

int main(void) {
    printf("mce C ABI smoke test\n");

    /* --- Standard starting position --------------------------------------- */
    McePosition *pos = mce_position_startpos("chess");
    CHECK(pos != NULL, "startpos(\"chess\") returns a handle");
    if (pos == NULL) {
        return 1;
    }

    char *fen = read_string(mce_position_to_fen, pos);
    CHECK(fen != NULL, "to_fen succeeds");
    if (fen != NULL) {
        printf("  startpos FEN: %s\n", fen);
        CHECK(strcmp(fen,
                     "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
                  == 0,
              "to_fen matches the standard startpos FEN");
        free(fen);
    }

    char *moves = read_string(mce_position_legal_moves, pos);
    CHECK(moves != NULL, "legal_moves succeeds");
    if (moves != NULL) {
        size_t count = count_words(moves);
        printf("  legal moves (%zu): %s\n", count, moves);
        CHECK(count == 20, "startpos has 20 legal moves");
        free(moves);
    }

    /* --- Perft ------------------------------------------------------------ */
    unsigned long long p1 = (unsigned long long)mce_perft(pos, 1);
    unsigned long long p2 = (unsigned long long)mce_perft(pos, 2);
    unsigned long long p3 = (unsigned long long)mce_perft(pos, 3);
    printf("  perft(1)=%llu perft(2)=%llu perft(3)=%llu\n", p1, p2, p3);
    CHECK(p1 == 20ULL, "perft(1) == 20");
    CHECK(p2 == 400ULL, "perft(2) == 400");
    CHECK(p3 == 8902ULL, "perft(3) == 8902");

    CHECK(mce_position_is_check(pos) == 0, "startpos is not check");
    CHECK(mce_position_outcome(pos) == MCE_OUTCOME_ONGOING,
          "startpos outcome is ongoing");

    mce_position_free(pos);

    /* --- Fool's mate: 1. f3 e5 2. g4 Qh4# --------------------------------- */
    McePosition *game = mce_position_startpos("chess");
    const char *line[] = {"f2f3", "e7e5", "g2g4", "d8h4"};
    int all_ok = 1;
    for (size_t i = 0; i < sizeof(line) / sizeof(line[0]); i++) {
        if (mce_position_play_uci(game, line[i]) != 0) {
            all_ok = 0;
        }
    }
    CHECK(all_ok, "play_uci accepts the Fool's-mate line");
    CHECK(mce_position_is_check(game) == 1, "position is check after Qh4#");
    CHECK(mce_position_outcome(game) == MCE_OUTCOME_BLACK_WINS,
          "outcome is BLACK_WINS after Fool's mate");

    /* Illegal move is rejected and leaves the position unchanged. */
    CHECK(mce_position_play_uci(game, "e2e4") == 2,
          "illegal move after mate returns error code 2");

    mce_position_free(game);

    /* --- Error handling --------------------------------------------------- */
    CHECK(mce_position_startpos("notavariant") == NULL,
          "unknown variant returns NULL");
    CHECK(mce_position_new_from_fen("garbage", "chess") == NULL,
          "bad FEN returns NULL");
    mce_position_free(NULL); /* documented no-op */

    /* --- A variant -------------------------------------------------------- */
    McePosition *atomic = mce_position_startpos("atomic");
    CHECK(atomic != NULL && mce_perft(atomic, 1) == 20ULL,
          "atomic startpos perft(1) == 20");
    mce_position_free(atomic);

    if (failures == 0) {
        printf("\nAll checks passed.\n");
        return 0;
    }
    printf("\n%d check(s) FAILED.\n", failures);
    return 1;
}
