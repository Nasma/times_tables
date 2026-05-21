# Efficient Times Tables Practice

A web app for learning multiplication tables (and addition/subtraction) using spaced repetition. It tracks which facts you find hardest and focuses practice on those, so you get better faster.

Live at [times-tables.fly.dev](https://times-tables.fly.dev).

## Features

- **Spaced repetition**: Problems you struggle with come back sooner; ones you know well are spaced further apart
- **Response-time scoring**: Slow or incorrect answers increase a problem's priority; fast answers reduce it
- **Error correction**: On a wrong answer, the correct answer is shown and you must type it before moving on
- **Achievement tiers**: Problems progress through learning → solid → fast → mastered, shown on a 12×12 progress grid
- **Race mode**: Timed challenge through all unlocked problems; tracks last and best race times
- **Multiple modes**: Times tables, addition, and subtraction
- **Teacher view**: Invite students and track their progress
- **Persistent progress**: Progress is saved server-side with user accounts (Google OAuth or username/password)

## How it works

Problems are selected based on your recent error rate and estimated response time — problems you get wrong or answer slowly come up more often. A small amount of randomness prevents the session from becoming too predictable.

A problem is considered *mastered* once you've answered it correctly three times in a row.

## Running locally

Requires [Rust](https://rustup.rs/).

```bash
cargo build --bin server
./target/debug/server
```

Then open [http://localhost:3000](http://localhost:3000).

**Optional environment variables:**

| Variable | Description |
|----------|-------------|
| `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` | Enable Google OAuth login |
| `BASE_URL` | OAuth redirect URI base (default: `http://localhost:3000`) |
| `DATA_DIR` | SQLite DB directory (default: `~/.local/share/times_tables_server/`) |

## Data storage

All data lives under the directory controlled by `DATA_DIR` (default: `~/.local/share/times_tables_server/` on Linux).

| Path | Contents |
|------|----------|
| `db.sqlite` | Users, sessions, and per-user spaced-repetition state (JSON blob per mode) |
| `responses/<user_id>/times_tables/<a>x<b>.csv` | Per-problem answer log for times tables |
| `responses/<user_id>/addition/<a>+<b>.csv` | Per-problem answer log for addition |
| `responses/<user_id>/subtraction/<a>+<b>.csv` | Per-problem answer log for subtraction |

Each CSV row is `timestamp,elapsed_secs,answer,correct`.

## Deployment

Deployed on [Fly.io](https://fly.io). Push to `master` triggers an automatic deploy via GitHub Actions.
