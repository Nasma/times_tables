use crate::problem::estimate_response_time_sd;
use crate::problem::{estimate_response_time, generate_addition_problems, generate_all_problems, Problem, ProblemStats};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

fn default_enabled_tables() -> HashSet<u8> {
    (1u8..=12).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacedRepetition {
    stats: HashMap<String, ProblemStats>,
    #[serde(default = "default_enabled_tables")]
    enabled_tables: HashSet<u8>,
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
            enabled_tables: default_enabled_tables(),
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
            enabled_tables: (1u8..=10).collect(),
            total_answers: 0,
            cached_total_time: 0.0,
        }
    }

    pub fn new_subtraction() -> Self {
        Self::new_addition()
    }

    pub fn is_problem_enabled(&self, problem: &Problem) -> bool {
        let (a, b) = problem.tables_required();
        self.enabled_tables.contains(&a) || self.enabled_tables.contains(&b)
    }

    pub fn is_table_enabled(&self, table: u8) -> bool {
        self.enabled_tables.contains(&table)
    }

    pub fn set_table_enabled(&mut self, table: u8, enabled: bool) {
        if enabled {
            self.enabled_tables.insert(table);
        } else {
            self.enabled_tables.remove(&table);
        }
    }

    pub fn set_enabled_tables(&mut self, tables: impl IntoIterator<Item = u8>) {
        self.enabled_tables = tables.into_iter().filter(|&t| t >= 1 && t <= 12).collect();
    }

    pub fn get_enabled_tables(&self) -> Vec<u8> {
        let mut v: Vec<u8> = self.enabled_tables.iter().copied().collect();
        v.sort();
        v
    }

    pub fn get_next_problem(&self, last: Option<&Problem>) -> Option<Problem> {
        let mut problems: Vec<_> = self
            .stats
            .values()
            .filter(|s| {
                self.is_problem_enabled(&s.problem)
                    && last.map_or(true, |l| s.problem != *l)
            })
            .collect();

        if problems.is_empty() {
            return None;
        }

        let estimated_time_with_sd = |s: &ProblemStats| {
            estimate_response_time(&s.recent_correct_times()) +
            estimate_response_time_sd(&s.recent_correct_times())
        };
        problems.sort_by(|a, b| {
            b.errors_in_last_5().cmp(&a.errors_in_last_5()).then(
                estimated_time_with_sd(b)
                    .partial_cmp(&estimated_time_with_sd(a))
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });

        use rand::seq::SliceRandom;
        let mut pool: Vec<&ProblemStats> = problems.iter().copied().take(8).collect();

        // Also include the 2 least-recently-asked problems (or never asked).
        let mut by_age = problems.clone();
        by_age.sort_by_key(|s| s.last_asked_at_secs());
        for oldest in by_age.iter().take(2) {
            if !pool.iter().any(|p| p.problem == oldest.problem) {
                pool.push(oldest);
            }
        }

        pool.choose(&mut rand::thread_rng()).map(|s| s.problem)
    }

    pub fn get_extra_practice_problem(&self, last: Option<&Problem>) -> Option<Problem> {
        self.get_next_problem(last)
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

    pub fn enabled_problem_list(&self) -> Vec<Problem> {
        let mut problems: Vec<Problem> = self
            .stats
            .values()
            .filter(|s| self.is_problem_enabled(&s.problem))
            .map(|s| s.problem)
            .collect();
        problems.sort_by_key(|p| (p.a, p.b));
        problems
    }

    pub fn enabled_problems(&self) -> usize {
        self.stats
            .values()
            .filter(|s| self.is_problem_enabled(&s.problem))
            .count()
    }

    pub fn mastered_count(&self) -> usize {
        self.stats
            .values()
            .filter(|s| self.is_problem_enabled(&s.problem) && s.is_mastered())
            .count()
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
