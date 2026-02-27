# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build all crates
cargo build

# Build specific binary
cargo build --bin server
cargo build --bin desktop

# Run server (from workspace root)
./target/debug/server

# Run desktop app
./target/debug/desktop

# Run tests
cargo test

# Run tests for a specific crate
cargo test -p tt_core
```

## Architecture

Cargo workspace with three crates:

- **`core/` (`tt_core`)** — shared library: `Problem`/`ProblemStats` structs and `SpacedRepetition` engine. Both binaries depend on this.
- **`server/`** — Axum web server with SQLite persistence (via `sqlx`), user accounts, and a vanilla JS frontend.
- **`desktop/`** — egui desktop app with JSON file persistence (via `directories` crate).

### Spaced repetition logic (`core/`)

`ProblemStats` tracks per-problem state: `ease_factor` (starts 2.5), `interval_days`, and `consecutive_correct`. On a correct answer the interval multiplies by the ease factor and the ease factor increases by 0.05–0.15 based on response time (<3s / 3–8s / >8s). On a wrong answer the interval resets to 0 and ease factor drops by 0.2 (floor 1.3). A problem is **mastered** when `consecutive_correct >= 3` and `ease_factor >= 2.0`.

`SpacedRepetition` manages the full set of 1×1 through 12×12 problems and progressive table unlocking. Tables unlock in a fixed pedagogical order (`[1, 10, 5, 11, 2, 3, 9, 4, 6, 7, 8, 12]`); the next table unlocks when 75% of currently unlocked problems are mastered. `get_next_problem()` returns the due problem with the lowest ease factor; `get_extra_practice_problem()` is used when nothing is due (sorts all unlocked by ease factor).

### Server (`server/src/main.rs`)

Single-file Axum server. Static files (`index.html`, `style.css`, `app.js`) are embedded at compile time with `include_str!` — no build step required, but the server binary must be rebuilt to pick up frontend changes.

User state is serialized as JSON (`serde_json`) and stored in the `progress` table as a single blob per user. Sessions are Bearer tokens, 30-day expiry.

**Environment variables:**
- `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` — enables Google OAuth (optional; username/password auth always available)
- `BASE_URL` — used for OAuth redirect URI (default: `http://localhost:3000`)

DB is at `~/.local/share/times_tables_server/db.sqlite` (Linux). Schema migrations run at startup in `init_db` + `migrate_db`.

### Desktop app (`desktop/src/`)

`app.rs` contains the egui UI (`TimesTablesApp`). `storage.rs` handles JSON persistence. Progress is saved to the platform data dir after every answer.
