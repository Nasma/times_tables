use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const TABLE_ORDER: [u8; 12] = [1, 10, 5, 11, 2, 3, 9, 4, 6, 7, 8, 12];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Problem {
    pub a: u8,
    pub b: u8,
}

impl Problem {
    pub fn new(a: u8, b: u8) -> Self {
        Self { a, b }
    }

    pub fn answer(&self) -> u32 {
        self.a as u32 * self.b as u32
    }

    pub fn display(&self) -> String {
        format!("{} × {} = ?", self.a, self.b)
    }

    pub fn key(&self) -> String {
        format!("{}x{}", self.a, self.b)
    }

    pub fn tables_required(&self) -> (u8, u8) {
        (self.a, self.b)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemStats {
    pub problem: Problem,
    pub ease_factor: f64,
    pub interval_days: f64,
    pub next_review: DateTime<Utc>,
    pub times_correct: u32,
    pub times_wrong: u32,
    pub consecutive_correct: u32,
}

impl ProblemStats {
    pub fn new(problem: Problem) -> Self {
        Self {
            problem,
            ease_factor: 2.5,
            interval_days: 0.0,
            next_review: Utc::now(),
            times_correct: 0,
            times_wrong: 0,
            consecutive_correct: 0,
        }
    }

    pub fn is_due(&self) -> bool {
        Utc::now() >= self.next_review
    }

    pub fn is_mastered(&self) -> bool {
        self.consecutive_correct >= 3 && self.ease_factor >= 2.0
    }

    pub fn record_answer(&mut self, correct: bool, response_secs: f64) {
        if correct {
            self.times_correct += 1;
            self.consecutive_correct += 1;

            if self.interval_days == 0.0 {
                self.interval_days = 1.0;
            } else if self.interval_days < 1.0 {
                self.interval_days = 1.0;
            } else {
                self.interval_days *= self.ease_factor;
            }

            // Adjust ease factor based on response time
            // Fast (< 3s): +0.15, Normal (3-8s): +0.1, Slow (> 8s): +0.05
            let ease_bonus = if response_secs < 3.0 {
                0.15
            } else if response_secs <= 8.0 {
                0.1
            } else {
                0.05
            };
            self.ease_factor += ease_bonus;
            if self.ease_factor > 3.0 {
                self.ease_factor = 3.0;
            }
        } else {
            self.times_wrong += 1;
            self.consecutive_correct = 0;
            self.interval_days = 0.0;
            self.ease_factor -= 0.2;
            if self.ease_factor < 1.3 {
                self.ease_factor = 1.3;
            }
        }

        self.next_review = Utc::now() + chrono::Duration::seconds((self.interval_days * 86400.0) as i64);
    }
}

pub fn generate_all_problems() -> Vec<Problem> {
    let mut problems = Vec::new();
    for a in 1..=12 {
        for b in 1..=12 {
            problems.push(Problem::new(a, b));
        }
    }
    problems
}
