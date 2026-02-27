use tt_core::problem::Problem;
use tt_core::spaced_rep::SpacedRepetition;
use crate::storage;
use eframe::egui;
use std::time::Instant;

#[derive(PartialEq)]
enum FeedbackState {
    None,
    Incorrect { correct_answer: u32, user_answer: u32 },
}

pub struct TimesTablesApp {
    spaced_rep: SpacedRepetition,
    current_problem: Option<Problem>,
    last_problem: Option<Problem>,
    problem_start: Instant,
    answer_input: String,
    feedback: FeedbackState,
    streak: u32,
    session_correct: u32,
    session_wrong: u32,
    confirm_reset: bool,
}

impl Default for TimesTablesApp {
    fn default() -> Self {
        let spaced_rep = storage::load_or_new();
        let mut current_problem = spaced_rep.get_next_problem(None);
        if current_problem.is_none() {
            current_problem = spaced_rep.get_extra_practice_problem(None);
        }

        Self {
            spaced_rep,
            current_problem,
            last_problem: None,
            problem_start: Instant::now(),
            answer_input: String::new(),
            feedback: FeedbackState::None,
            streak: 0,
            session_correct: 0,
            session_wrong: 0,
            confirm_reset: false,
        }
    }
}

impl TimesTablesApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    fn submit_answer(&mut self) {
        let Some(problem) = self.current_problem else {
            return;
        };

        let user_answer: u32 = match self.answer_input.trim().parse() {
            Ok(n) => n,
            Err(_) => {
                self.answer_input.clear();
                return;
            }
        };

        let response_secs = self.problem_start.elapsed().as_secs_f64();
        let correct_answer = problem.answer();
        let is_correct = user_answer == correct_answer;

        self.spaced_rep.record_answer(&problem, is_correct, response_secs);

        if is_correct {
            self.streak += 1;
            self.session_correct += 1;
            let _ = storage::save(&self.spaced_rep);
            self.next_problem();
        } else {
            self.feedback = FeedbackState::Incorrect { correct_answer, user_answer };
            self.streak = 0;
            self.session_wrong += 1;
            self.answer_input.clear();
            let _ = storage::save(&self.spaced_rep);
        }
    }

    fn check_correction(&mut self) {
        if let FeedbackState::Incorrect { correct_answer, .. } = self.feedback {
            if let Ok(typed) = self.answer_input.trim().parse::<u32>() {
                if typed == correct_answer {
                    self.next_problem();
                }
            }
        }
    }

    fn next_problem(&mut self) {
        self.last_problem = self.current_problem;
        self.current_problem = self.spaced_rep.get_next_problem(self.last_problem.as_ref());
        if self.current_problem.is_none() {
            self.current_problem =
                self.spaced_rep.get_extra_practice_problem(self.last_problem.as_ref());
        }
        self.problem_start = Instant::now();
        self.answer_input.clear();
        self.feedback = FeedbackState::None;
    }

    fn reset_progress(&mut self) {
        self.spaced_rep = SpacedRepetition::new();
        self.current_problem = self.spaced_rep.get_next_problem(None);
        self.last_problem = None;
        self.problem_start = Instant::now();
        self.answer_input.clear();
        self.feedback = FeedbackState::None;
        self.streak = 0;
        self.session_correct = 0;
        self.session_wrong = 0;
        self.confirm_reset = false;
        let _ = storage::save(&self.spaced_rep);
    }
}

impl eframe::App for TimesTablesApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.heading("Times Tables Practice");
                ui.add_space(30.0);

                match &self.current_problem {
                    Some(problem) => {
                        ui.label(
                            egui::RichText::new(problem.display())
                                .size(48.0)
                                .strong(),
                        );
                        ui.add_space(20.0);

                        match &self.feedback {
                            FeedbackState::None => {
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut self.answer_input)
                                        .hint_text("Enter answer")
                                        .font(egui::TextStyle::Heading)
                                        .desired_width(150.0)
                                        .horizontal_align(egui::Align::Center),
                                );

                                if response.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    self.submit_answer();
                                }

                                response.request_focus();

                                ui.add_space(15.0);

                                if ui
                                    .add_sized([120.0, 40.0], egui::Button::new("Submit"))
                                    .clicked()
                                {
                                    self.submit_answer();
                                }
                            }
                            FeedbackState::Incorrect { correct_answer, user_answer } => {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} is wrong. Type the answer: {}",
                                        user_answer, correct_answer
                                    ))
                                    .size(24.0)
                                    .color(egui::Color32::from_rgb(220, 20, 60)),
                                );
                                ui.add_space(15.0);

                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut self.answer_input)
                                        .hint_text(correct_answer.to_string())
                                        .font(egui::TextStyle::Heading)
                                        .desired_width(150.0)
                                        .horizontal_align(egui::Align::Center),
                                );

                                if response.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    self.check_correction();
                                }

                                response.request_focus();
                            }
                        }
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("All mastered!")
                                .size(32.0)
                                .color(egui::Color32::from_rgb(50, 205, 50)),
                        );
                        ui.add_space(10.0);
                        ui.label("Congratulations! You've mastered all times tables!");
                    }
                }
            });

            ui.add_space(40.0);
            ui.separator();
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label(format!("Streak: {}", self.streak));
                ui.separator();
                ui.label(format!(
                    "Mastered: {}/{}",
                    self.spaced_rep.mastered_count(),
                    self.spaced_rep.unlocked_problems()
                ));
                ui.separator();
                ui.label(format!("Due: {}", self.spaced_rep.due_count()));
            });

            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label(format!(
                    "Tables: {}",
                    self.spaced_rep.unlocked_tables_display()
                ));
                if let Some(next) = self.spaced_rep.next_table_to_unlock() {
                    ui.separator();
                    ui.label(format!("Next: {}×", next));
                }
            });

            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label(format!(
                    "Session: {} correct, {} wrong",
                    self.session_correct, self.session_wrong
                ));
                ui.separator();
                ui.label(format!(
                    "All-time: {} correct, {} wrong",
                    self.spaced_rep.total_correct(),
                    self.spaced_rep.total_wrong()
                ));
            });

            ui.add_space(15.0);

            if self.confirm_reset {
                ui.horizontal(|ui| {
                    ui.label("Reset all progress?");
                    if ui.button("Yes, reset").clicked() {
                        self.reset_progress();
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_reset = false;
                    }
                });
            } else if ui.small_button("Reset progress").clicked() {
                self.confirm_reset = true;
            }
        });
    }
}
