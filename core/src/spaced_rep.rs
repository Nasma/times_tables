use crate::problem::{generate_all_problems, Problem, ProblemStats};
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
        }
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
        let mut due_problems: Vec<_> = self
            .stats
            .values()
            .filter(|s| {
                s.is_due()
                    && self.is_problem_enabled(&s.problem)
                    && last.map_or(true, |l| s.problem != *l)
            })
            .collect();

        if due_problems.is_empty() {
            return None;
        }

        due_problems.sort_by(|a, b| {
            a.ease_factor
                .partial_cmp(&b.ease_factor)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        due_problems.first().map(|s| s.problem)
    }

    pub fn get_extra_practice_problem(&self, last: Option<&Problem>) -> Option<Problem> {
        let mut enabled: Vec<_> = self
            .stats
            .values()
            .filter(|s| {
                self.is_problem_enabled(&s.problem)
                    && last.map_or(true, |l| s.problem != *l)
            })
            .collect();

        if enabled.is_empty() {
            return None;
        }

        enabled.sort_by(|a, b| {
            a.ease_factor
                .partial_cmp(&b.ease_factor)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        enabled.first().map(|s| s.problem)
    }

    pub fn record_answer(&mut self, problem: &Problem, correct: bool, response_secs: f64) {
        if let Some(stats) = self.stats.get_mut(&problem.key()) {
            stats.record_answer(correct, response_secs);
        }
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

    pub fn due_count(&self) -> usize {
        self.stats
            .values()
            .filter(|s| self.is_problem_enabled(&s.problem) && s.is_due())
            .count()
    }

    pub fn total_correct(&self) -> u32 {
        self.stats.values().map(|s| s.times_correct).sum()
    }

    pub fn total_wrong(&self) -> u32 {
        self.stats.values().map(|s| s.times_wrong).sum()
    }

    /// Returns a 144-element vec (a=1..12, b=1..12) with the achievement tier of each cell.
    pub fn grid_status(&self) -> Vec<&'static str> {
        (1u8..=12)
            .flat_map(|a| {
                (1u8..=12).map(move |b| {
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
}
