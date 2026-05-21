# Efficient Times Tables Practice

A web app for learning multiplication tables (and addition/subtraction) using spaced repetition. It tracks which facts you find hardest and focuses practice on those, so you get better faster.

Live at [times-tables.fly.dev](https://times-tables.fly.dev).

## Features

- **Spaced repetition**: Problems you struggle with come back sooner; ones you know well are spaced further apart
- **Response-time scoring**: Answering quickly earns a higher ease factor boost than a slow correct answer
- **Error correction**: On a wrong answer, the correct answer is shown and you must type it before moving on
- **Achievement tiers**: Problems progress through learning → solid → fast → mastered, shown on a 12×12 progress grid
- **Race mode**: Timed challenge through all unlocked problems; tracks last and best race times
- **Multiple modes**: Times tables, addition, and subtraction
- **Teacher view**: Invite students and track their progress
- **Persistent progress**: Progress is saved server-side with user accounts (Google OAuth or username/password)

## How it works

Each problem has an *ease factor* (starting at 2.5) and a *review interval*. When you answer:

- **Correct**: The interval multiplies by the ease factor, scheduling the next review further in the future. The ease factor increases by 0.05–0.15 depending on how quickly you answered.
- **Wrong**: The interval resets to zero and the ease factor drops by 0.2, so the problem comes back immediately and more frequently.

A problem is considered *mastered* once you've answered it correctly three times in a row with an ease factor of 2.0 or above.

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

## Deployment

Deployed on [Fly.io](https://fly.io). Push to `master` triggers an automatic deploy via GitHub Actions.
