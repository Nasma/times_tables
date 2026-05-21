# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build all crates
cargo build

# Build server
cargo build --bin server

# Run server (from workspace root)
./target/debug/server

# Run tests
cargo test

# Run tests for a specific crate
cargo test -p tt_core
```

## Architecture

Cargo workspace with two crates:

- **`core/` (`tt_core`)** — shared library: `Problem`/`ProblemStats` structs and `SpacedRepetition` engine.
- **`server/`** — Axum web server with SQLite persistence (via `sqlx`), user accounts, and a vanilla JS frontend.

### Spaced repetition logic (`core/`)

`ProblemStats` tracks per-problem state: `times_correct`, `consecutive_correct`, `best_tier`, `consecutive_fast_correct`, and a `responses` history (last 100, loaded from CSV). A problem is **mastered** when `consecutive_correct >= 3`.

`SpacedRepetition` manages the full set of problems. Problem selection (in the server's `pick_problem`) prioritises by errors in the last 5 answers, then by estimated response time (weighted recent average, worst outlier discarded). A small random pool adds variety.

### Server (`server/src/main.rs`)

Single-file Axum server. Static files (`index.html`, `style.css`, `app.js`) are embedded at compile time with `include_str!` — no build step required, but the server binary must be rebuilt to pick up frontend changes.

User state is serialized as JSON (`serde_json`) and stored in the `progress` table as a single blob per user. Sessions are Bearer tokens, 30-day expiry.

**Environment variables:**
- `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` — enables Google OAuth (optional; username/password auth always available)
- `BASE_URL` — used for OAuth redirect URI (default: `http://localhost:3000`)

DB is at `~/.local/share/times_tables_server/db.sqlite` (Linux). Schema migrations run at startup in `init_db` + `migrate_db`.

