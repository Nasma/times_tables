use crate::problem::{generate_addition_problems, generate_all_problems, Problem, ProblemStats};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacedRepetition {
    stats: HashMap<String, ProblemStats>,
    #[serde(default)]
    total_answers: u32,
    #[serde(default)]
    cached_total_time: f64,
}

impl Default for SpacedRepetition {
    fn default() -> Self {
        Self::new()
    }
}

impl SpacedRepetition {
    pub fn new() -> Self {
        let mut stats = HashMap::new();
        for problem in generate_all_problems() {
            stats.insert(problem.key(), ProblemStats::new(problem));
        }
        Self {
            stats,
            total_answers: 0,
            cached_total_time: 0.0,
        }
    }

    pub fn new_addition() -> Self {
        let mut stats = HashMap::new();
        for problem in generate_addition_problems() {
            stats.insert(problem.key(), ProblemStats::new(problem));
        }
        Self {
            stats,
            total_answers: 0,
            cached_total_time: 0.0,
        }
    }

    pub fn new_subtraction() -> Self {
        Self::new_addition()
    }

    pub fn get_stats(&self, problem: &Problem) -> Option<&ProblemStats> {
        self.stats.get(&problem.key())
    }

    pub fn get_stats_mut(&mut self, problem: &Problem) -> Option<&mut ProblemStats> {
        self.stats.get_mut(&problem.key())
    }

    pub fn record_answer(&mut self, problem: &Problem, correct: bool, response_secs: f64) {
        if let Some(stats) = self.stats.get_mut(&problem.key()) {
            stats.record_answer(correct, response_secs);
        }
        self.total_answers += 1;
        if self.total_answers % 50 == 0 {
            self.recompute_total_time();
        }
    }

    fn recompute_total_time(&mut self) {
        self.cached_total_time = self.stats.values().map(|s| s.correct_time()).sum();
    }

    pub fn cached_total_time(&self) -> f64 {
        self.cached_total_time
    }

    pub fn all_stats(&self) -> impl Iterator<Item = &ProblemStats> {
        self.stats.values()
    }

    pub fn compute_total_time(&self) -> f64 {
        self.stats.values().map(|s| s.correct_time()).sum()
    }

    pub fn total_answers(&self) -> u32 {
        self.total_answers
    }

    pub fn total_problems(&self) -> usize {
        self.stats.len()
    }

    pub fn mastered_count(&self) -> usize {
        self.stats.values().filter(|s| s.is_mastered()).count()
    }

    pub fn total_correct(&self) -> u32 {
        self.stats.values().map(|s| s.times_correct).sum()
    }

    pub fn total_wrong(&self) -> u32 {
        self.stats
            .values()
            .map(|s| s.responses.iter().filter(|r| !r.correct).count() as u32)
            .sum()
    }

    /// Returns a size×size vec with the achievement tier of each cell.
    pub fn grid_status_sized(&self, size: u8) -> Vec<&'static str> {
        (1u8..=size)
            .flat_map(|a| {
                (1u8..=size).map(move |b| {
                    let key = Problem::new(a, b).key();
                    match self.stats.get(&key).map(|s| s.best_tier) {
                        Some(4) => "mastered",
                        Some(3) => "fast",
                        Some(2) => "solid",
                        Some(1) => "learning",
                        _ => "not_started",
                    }
                })
            })
            .collect()
    }

    /// Returns a 144-element vec (a=1..12, b=1..12) with the achievement tier of each cell.
    pub fn grid_status(&self) -> Vec<&'static str> {
        self.grid_status_sized(12)
    }
}
