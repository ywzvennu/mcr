# Contributing to mce

Thank you for your interest in contributing!

## Clean-Room Rule

> **IMPORTANT — read this before writing a single line.**

`mce` is a clean-room implementation.
Every contributor must write original code.

**You must not copy, translate, or closely paraphrase source code from any copyleft chess engine**, including but not limited to:

- Stockfish (GPL-3.0)
- Leela Chess Zero (GPL-3.0)
- shakmaty (GPL-3.0)
- pleco (GPL-2.0)
- any other GPL- or LGPL-licensed chess library

Understanding chess rules and algorithms from their documentation, papers, or specifications is fine.
Copying or closely porting their *code* is not.

If you are unsure whether a source is permissible, open an issue and ask before contributing.

## Branch Naming

| Type | Pattern | Example |
|------|---------|---------|
| Feature | `feat/<issue>-<slug>` | `feat/7-bitboard-movegen` |
| Chore / infra | `chore/<issue>-<slug>` | `chore/2-docs` |
| Bug fix | `fix/<issue>-<slug>` | `fix/12-en-passant-pin` |

## One Concern per PR

Each pull request must address a single, self-contained concern.
Do not bundle unrelated fixes, formatting cleanups, or dependency bumps with feature work.

## Commit Style

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <short summary>

[optional body]

[optional footer(s)]
```

Common types: `feat`, `fix`, `chore`, `docs`, `test`, `refactor`, `perf`, `ci`.

Example:

```
feat(movegen): add sliding piece attack tables
```

## Required Local Checks

All of the following must pass before opening a PR:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Fix any warnings or test failures before pushing.

## Opening a PR

- Fill in the pull request template completely.
- Reference the issue your PR addresses (`Closes #N`).
- Confirm the clean-room declaration in the checklist.
