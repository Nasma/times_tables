use serde::{Deserialize, Serialize};

pub const MAX_RESPONSES: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseRecord {
    pub answered_at_secs: i64,
    pub elapsed_secs: f64,
    pub correct: bool,
}

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
    pub times_correct: u32,
    pub times_wrong: u32,
    pub consecutive_correct: u32,
    /// Achievement tier: 0=not started, 1=learning, 2=solid, 3=fast, 4=mastered. Never reverts.
    #[serde(default)]
    pub best_tier: u8,
    /// Consecutive fast (< 3s) correct answers for the current streak.
    #[serde(default)]
    pub consecutive_fast_correct: u32,
    /// Full response history. Not persisted in JSON — loaded from CSV files.
    #[serde(skip)]
    pub responses: Vec<ResponseRecord>,
}

impl ProblemStats {
    pub fn new(problem: Problem) -> Self {
        Self {
            problem,
            times_correct: 0,
            times_wrong: 0,
            consecutive_correct: 0,
            best_tier: 0,
            consecutive_fast_correct: 0,
            responses: Vec::new(),
        }
    }

    pub fn is_mastered(&self) -> bool {
        self.consecutive_correct >= 3
    }

    pub fn record_answer(&mut self, correct: bool, response_secs: f64) {
        let is_fast = response_secs < 3.0;

        if correct {
            let is_fast = response_secs < 2.0;
            self.times_correct += 1;
            self.consecutive_correct += 1;

            if is_fast {
                self.consecutive_fast_correct += 1;
            } else {
                self.consecutive_fast_correct = 0;
            }
        } else {
            self.times_wrong += 1;
            self.consecutive_correct = 0;
            self.consecutive_fast_correct = 0;
        }

        // Advance achievement tier — never reverts.
        // 1=learning, 2=solid, 3=fast, 4=mastered
        if self.times_correct > 0 {
            self.best_tier = self.best_tier.max(1);
        }
        if self.is_mastered() {
            self.best_tier = self.best_tier.max(2);
        }
        if self.best_tier >= 2 && correct && is_fast {
            self.best_tier = self.best_tier.max(3);
        }
        if self.consecutive_fast_correct >= 3 {
            self.best_tier = self.best_tier.max(4);
        }
    }

    pub fn add_response(&mut self, record: ResponseRecord) {
        self.responses.push(record);
        let len = self.responses.len();
        if len > MAX_RESPONSES {
            self.responses.drain(0..len - MAX_RESPONSES);
        }
    }

    pub fn errors_in_last_5(&self) -> u32 {
        self.responses.iter().rev().take(5).filter(|r| !r.correct).count() as u32
    }

    pub fn recent_correct_times(&self) -> Vec<f64> {
        self.responses.iter().rev().filter(|r| r.correct).map(|r| r.elapsed_secs).collect()
    }

    pub fn last_asked_at_secs(&self) -> Option<i64> {
        self.responses.last().map(|r| r.answered_at_secs)
    }

}

/// Estimate response time for a problem from a list of correct elapsed times,
/// most recent first. Returns None if fewer than 2 correct answers.
///
/// Algorithm: take up to 5 most recent, discard the slowest, then weighted
/// average the rest with weights [k, k-1, ..., 1] from most recent.
pub fn estimate_response_time(recent_correct: &[f64]) -> Option<f64> {
    if recent_correct.len() < 2 {
        return None;
    }
    let candidates: Vec<f64> = recent_correct.iter().copied().take(5).collect();

    let worst_idx = candidates
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap();

    let remaining: Vec<f64> = candidates
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != worst_idx)
        .map(|(_, &v)| v)
        .collect();

    let k = remaining.len();
    let (weighted_sum, total_weight) = remaining.iter().enumerate().fold(
        (0.0f64, 0.0f64),
        |(sum, total), (i, &t)| {
            let w = (k - i) as f64;
            (sum + w * t, total + w)
        },
    );

    Some(weighted_sum / total_weight)
}

/// Estimate standard deviation of response time for a problem from a list of correct
/// elapsed times, most recent first. Always returns a value — uses a conservative
/// prior of 5.0 seconds when data is sparse:
///   - 0–1 correct answers: returns 5.0
///   - 2–4 correct answers: average of 5.0 and measured sample standard deviation
///   - 5+ correct answers: measured sample standard deviation
///
/// Like `estimate_response_time`, takes up to 5 most recent and discards the slowest
/// before computing.
pub fn estimate_response_time_sd(recent_correct: &[f64]) -> f64 {
    const PRIOR: f64 = 5.0;

    if recent_correct.len() < 2 {
        return PRIOR;
    }

    let candidates: Vec<f64> = recent_correct.iter().copied().take(5).collect();

    let worst_idx = candidates
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap();

    let remaining: Vec<f64> = candidates
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != worst_idx)
        .map(|(_, &v)| v)
        .collect();

    let measured = if remaining.len() < 2 {
        0.0
    } else {
        let mean = remaining.iter().sum::<f64>() / remaining.len() as f64;
        let variance = remaining.iter().map(|&x| (x - mean).powi(2)).sum::<f64>()
            / (remaining.len() - 1) as f64;
        variance.sqrt()
    };

    if candidates.len() < 5 {
        (PRIOR + measured) / 2.0
    } else {
        measured
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

#[cfg(test)]
mod tests {
    use super::{estimate_response_time, estimate_response_time_sd};

    #[test]
    fn test_estimate_fewer_than_2_returns_none() {
        assert_eq!(estimate_response_time(&[]), None);
        assert_eq!(estimate_response_time(&[1.0]), None);
    }

    #[test]
    fn test_estimate_2_correct_ditches_worst() {
        // 2 answers: ditch worst (5.0), left with 1.0 → estimate = 1.0
        assert_eq!(estimate_response_time(&[1.0, 5.0]), Some(1.0));
    }

    #[test]
    fn test_estimate_3_correct_weights_2_1() {
        // most recent first: [1.0, 2.0, 9.0], ditch worst (9.0) → [1.0, 2.0]
        // weighted: (2*1.0 + 1*2.0) / 3 = 4/3
        let result = estimate_response_time(&[1.0, 2.0, 9.0]).unwrap();
        assert!((result - 4.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_estimate_5_correct_uses_only_5_most_recent() {
        // 6 values but only first 5 taken (most recent first)
        // [1.0, 2.0, 3.0, 4.0, 5.0] — ignores 99.0
        // ditch worst (5.0) → [1.0, 2.0, 3.0, 4.0]
        // weights 4,3,2,1: (4*1 + 3*2 + 2*3 + 1*4) / 10 = 20/10 = 2.0
        let result = estimate_response_time(&[1.0, 2.0, 3.0, 4.0, 5.0, 99.0]).unwrap();
        assert!((result - 2.0).abs() < 1e-9);
    }

    // ── variance tests ────────────────────────────────────────────────────────

    #[test]
    fn test_variance_sparse_returns_prior() {
        assert!((estimate_response_time_sd(&[]) - 5.0).abs() < 1e-9);
        assert!((estimate_response_time_sd(&[2.0]) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_variance_2_correct_blends_with_prior() {
        // 2 correct: ditch worst (5.0) → 1 remaining → measured = 0.0
        // blend: (5.0 + 0.0) / 2 = 2.5
        let result = estimate_response_time_sd(&[1.0, 5.0]);
        assert!((result - 2.5).abs() < 1e-9);
    }

    #[test]
    fn test_variance_3_correct_blends_measured() {
        // [1.0, 3.0, 9.0]: ditch worst (9.0) → [1.0, 3.0]
        // sample variance: mean=2.0, sum_sq=2, var=2.0, sd=√2
        // blend: (5.0 + √2) / 2
        let result = estimate_response_time_sd(&[1.0, 3.0, 9.0]);
        assert!((result - (5.0 + 2.0f64.sqrt()) / 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_variance_5_correct_uses_only_measured() {
        // [2.0, 2.0, 2.0, 2.0, 2.0]: ditch one (all equal) → [2.0, 2.0, 2.0, 2.0]
        // variance = 0.0, no blending (have 5)
        let result = estimate_response_time_sd(&[2.0, 2.0, 2.0, 2.0, 2.0]);
        assert!((result - 0.0).abs() < 1e-9);
    }
}
