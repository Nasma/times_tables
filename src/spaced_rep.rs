use crate::problem::{generate_all_problems, Problem, ProblemStats, TABLE_ORDER};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacedRepetition {
    stats: HashMap<String, ProblemStats>,
    #[serde(default = "default_unlocked")]
    unlocked_tables: usize,
}

fn default_unlocked() -> usize {
    1
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
            unlocked_tables: 1,
        }
    }

    fn unlocked_table_set(&self) -> HashSet<u8> {
        TABLE_ORDER.iter().take(self.unlocked_tables).copied().collect()
    }

    fn is_problem_unlocked(&self, problem: &Problem) -> bool {
        let unlocked = self.unlocked_table_set();
        let (a, b) = problem.tables_required();
        unlocked.contains(&a) && unlocked.contains(&b)
    }

    fn check_unlock_next_table(&mut self) {
        if self.unlocked_tables >= TABLE_ORDER.len() {
            return;
        }

        let unlocked_problems: Vec<_> = self
            .stats
            .values()
            .filter(|s| self.is_problem_unlocked(&s.problem))
            .collect();

        if unlocked_problems.is_empty() {
            return;
        }

        let mastered = unlocked_problems.iter().filter(|s| s.is_mastered()).count();
        let total = unlocked_problems.len();

        if mastered >= total * 3 / 4 {
            self.unlocked_tables += 1;
        }
    }

    pub fn get_next_problem(&self, last: Option<&Problem>) -> Option<Problem> {
        let mut due_problems: Vec<_> = self
            .stats
            .values()
            .filter(|s| {
                s.is_due()
                    && self.is_problem_unlocked(&s.problem)
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
        let mut unlocked: Vec<_> = self
            .stats
            .values()
            .filter(|s| {
                self.is_problem_unlocked(&s.problem)
                    && last.map_or(true, |l| s.problem != *l)
            })
            .collect();

        if unlocked.is_empty() {
            return None;
        }

        unlocked.sort_by(|a, b| {
            a.ease_factor
                .partial_cmp(&b.ease_factor)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        unlocked.first().map(|s| s.problem)
    }

    pub fn record_answer(&mut self, problem: &Problem, correct: bool) {
        if let Some(stats) = self.stats.get_mut(&problem.key()) {
            stats.record_answer(correct);
        }
        self.check_unlock_next_table();
    }

    pub fn unlocked_problems(&self) -> usize {
        self.stats
            .values()
            .filter(|s| self.is_problem_unlocked(&s.problem))
            .count()
    }

    pub fn mastered_count(&self) -> usize {
        self.stats
            .values()
            .filter(|s| self.is_problem_unlocked(&s.problem) && s.is_mastered())
            .count()
    }

    pub fn due_count(&self) -> usize {
        self.stats
            .values()
            .filter(|s| self.is_problem_unlocked(&s.problem) && s.is_due())
            .count()
    }

    pub fn total_correct(&self) -> u32 {
        self.stats.values().map(|s| s.times_correct).sum()
    }

    pub fn total_wrong(&self) -> u32 {
        self.stats.values().map(|s| s.times_wrong).sum()
    }

    pub fn unlocked_tables_display(&self) -> String {
        TABLE_ORDER
            .iter()
            .take(self.unlocked_tables)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn next_table_to_unlock(&self) -> Option<u8> {
        TABLE_ORDER.get(self.unlocked_tables).copied()
    }
}
