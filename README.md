# Times Tables Practice

A desktop app for learning multiplication tables using spaced repetition. It tracks which facts you find hardest and focuses practice on those, so you get better faster.

## Features

- **Spaced repetition**: Problems you struggle with come back sooner; ones you know well are spaced further apart
- **Response-time scoring**: Answering quickly earns a higher ease factor boost than a slow correct answer
- **Progressive table unlock**: Start with the 1× table. New tables unlock as you master 75% of the current set, introduced in a pedagogically friendly order (1, 10, 5, 11, 2, 3, 9, 4, 6, 7, 8, 12)
- **Error correction**: On a wrong answer, the correct answer is shown and you must type it before moving on
- **Persistent progress**: Your progress is saved automatically between sessions
- **Session and all-time stats**: Streak, mastered count, due count, correct/wrong tallies

## How it works

Each problem has an *ease factor* (starting at 2.5) and a *review interval*. When you answer:

- **Correct**: The interval multiplies by the ease factor, scheduling the next review further in the future. The ease factor increases by 0.05–0.15 depending on how quickly you answered.
- **Wrong**: The interval resets to zero and the ease factor drops by 0.2, so the problem comes back immediately and more frequently.

A problem is considered *mastered* once you've answered it correctly three times in a row with an ease factor of 2.0 or above.

## Building

Requires [Rust](https://rustup.rs/).

```bash
cargo build --release
./target/release/times_tables
```

## Data storage

Progress is saved as JSON in the platform's standard data directory:

| Platform | Location |
|----------|----------|
| Linux    | `~/.local/share/practice/times_tables/progress.json` |
| macOS    | `~/Library/Application Support/com.practice.times_tables/progress.json` |
| Windows  | `%APPDATA%\practice\times_tables\data\progress.json` |

To reset progress, use the **Reset progress** button in the app (a confirmation step prevents accidental resets).
