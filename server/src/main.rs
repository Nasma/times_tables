use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Json, Router,
};
use qrcode::{render::svg, QrCode};
use chrono::Utc;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, Row, SqlitePool};
use std::path::PathBuf;
use std::sync::Arc;
use tt_core::{problem::{estimate_response_time, estimate_response_time_sd, generate_addition_problems, generate_all_problems, Problem}, spaced_rep::SpacedRepetition};
use chrono::DateTime;

// ── Mode ──────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
enum Mode {
    #[default]
    TimesTables,
    Addition,
    Subtraction,
}

// ── App state ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    http: reqwest::Client,
    google_client_id: Option<String>,
    google_client_secret: Option<String>,
    base_url: String,
    responses_dir: PathBuf,
}

// ── Request / Response types ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct AuthRequest {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
    #[serde(default = "default_role")]
    role: String,
}

fn default_role() -> String { "student".to_string() }

#[derive(Serialize)]
struct TokenResponse {
    token: String,
}

#[derive(Serialize)]
struct ProblemDto {
    a: u8,
    b: u8,
}

#[derive(Serialize)]
struct StateResponse {
    problem: ProblemDto,
    mastered: usize,
    total: usize,
    grid: Vec<&'static str>,
    enabled_tables: Vec<u8>,
    role: String,
    mode: String,
    total_time: f64,
    total_answers: u32,
    last_race_ms: Option<i64>,
    best_race_ms: Option<i64>,
    correct_10m: u32,
}

#[derive(Serialize)]
struct JoinInfoResponse {
    teacher_username: String,
}

#[derive(Serialize)]
struct TeacherInviteResponse {
    token: String,
    join_url: String,
    qr_svg: String,
}

#[derive(Serialize)]
struct StudentInfo {
    username: String,
    mastered: usize,
    total: usize,
    last_answered_secs: Option<i64>,
    correct_5m: u32,
    total_5m: u32,
    correct_10m: u32,
    total_10m: u32,
    correct_30m: u32,
    total_30m: u32,
    correct_day: u32,
    total_day: u32,
}

#[derive(Serialize)]
struct TeacherStudentsResponse {
    students: Vec<StudentInfo>,
}

#[derive(Deserialize)]
struct StateQuery {
    #[serde(default)]
    mode: Mode,
}

#[derive(Deserialize)]
struct SetTablesRequest {
    enabled: Vec<u8>,
    #[serde(default)]
    mode: Mode,
}

#[derive(Deserialize)]
struct AnswerRequest {
    a: u8,
    b: u8,
    answer: u32,
    #[serde(default = "default_elapsed")]
    elapsed_secs: f64,
    #[serde(default)]
    mode: Mode,
}

fn default_elapsed() -> f64 {
    5.0
}

#[derive(Serialize)]
struct AnswerResponse {
    correct: bool,
    correct_answer: u32,
    next_problem: ProblemDto,
    mastered: usize,
    total: usize,
    grid: Vec<&'static str>,
    enabled_tables: Vec<u8>,
    mode: String,
    total_time: f64,
    total_answers: u32,
    correct_10m: u32,
}

#[derive(Deserialize)]
struct OAuthCompleteRequest {
    token: String,
    action: String, // "create_new" or "use_existing"
}

#[derive(Deserialize)]
struct OAuthStartParams {
    #[serde(default = "default_role")]
    role: String,
}

#[derive(Deserialize)]
struct OAuthCallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct GoogleUserInfo {
    id: String,
    email: String,
}

#[derive(Serialize)]
struct ConfigResponse {
    google_oauth: bool,
}

// ── Error helpers ─────────────────────────────────────────────────────────────

type AppResult<T> = Result<Json<T>, (StatusCode, String)>;

fn app_err(status: StatusCode, msg: impl ToString) -> (StatusCode, String) {
    (status, msg.to_string())
}

fn internal(msg: impl ToString) -> (StatusCode, String) {
    app_err(StatusCode::INTERNAL_SERVER_ERROR, msg)
}

// ── Auth helpers ──────────────────────────────────────────────────────────────

async fn authenticate(db: &SqlitePool, headers: &HeaderMap) -> Option<i64> {
    let auth = headers.get("Authorization")?.to_str().ok()?;
    let token = auth.strip_prefix("Bearer ")?;
    let now = Utc::now().to_rfc3339();

    let row =
        sqlx::query("SELECT user_id FROM sessions WHERE token = ? AND expires_at > ?")
            .bind(token)
            .bind(&now)
            .fetch_optional(db)
            .await
            .ok()??;

    row.try_get("user_id").ok()
}

fn generate_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

async fn create_session(db: &SqlitePool, user_id: i64) -> Result<String, (StatusCode, String)> {
    let token = generate_token();
    let expires_at = (Utc::now() + chrono::Duration::days(30)).to_rfc3339();
    sqlx::query("INSERT INTO sessions (token, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(&token)
        .bind(user_id)
        .bind(&expires_at)
        .execute(db)
        .await
        .map_err(internal)?;
    Ok(token)
}

// ── DB helpers ────────────────────────────────────────────────────────────────

async fn load_user_state(
    db: &SqlitePool,
    user_id: i64,
    mode: Mode,
) -> Result<SpacedRepetition, (StatusCode, String)> {
    let (table, default_sr): (&str, fn() -> SpacedRepetition) = match mode {
        Mode::Addition => ("progress_addition", SpacedRepetition::new_addition),
        Mode::Subtraction => ("progress_subtraction", SpacedRepetition::new_subtraction),
        Mode::TimesTables => ("progress", SpacedRepetition::new),
    };
    let query = format!("SELECT data FROM {} WHERE user_id = ?", table);
    let row = sqlx::query(&query)
        .bind(user_id)
        .fetch_optional(db)
        .await
        .map_err(internal)?;

    match row {
        Some(r) => {
            let data: String = r.try_get("data").map_err(internal)?;
            serde_json::from_str(&data).map_err(internal)
        }
        None => Ok(default_sr()),
    }
}

async fn save_user_state(
    db: &SqlitePool,
    user_id: i64,
    mode: Mode,
    sr: &SpacedRepetition,
) -> Result<(), (StatusCode, String)> {
    let data = serde_json::to_string(sr).map_err(internal)?;
    let table = match mode {
        Mode::Addition => "progress_addition",
        Mode::Subtraction => "progress_subtraction",
        Mode::TimesTables => "progress",
    };
    let query = format!(
        "INSERT INTO {} (user_id, data) VALUES (?, ?) ON CONFLICT(user_id) DO UPDATE SET data = excluded.data",
        table
    );
    sqlx::query(&query)
        .bind(user_id)
        .bind(&data)
        .execute(db)
        .await
        .map_err(internal)?;
    Ok(())
}

// ── Race helpers ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RecordRaceRequest {
    mode: Mode,
    elapsed_ms: i64,
}

#[derive(Serialize)]
struct RaceStatsResponse {
    last_race_ms: Option<i64>,
    best_race_ms: Option<i64>,
}

async fn get_race_stats(
    db: &SqlitePool,
    user_id: i64,
    mode: &str,
) -> Result<(Option<i64>, Option<i64>), (StatusCode, String)> {
    let last = sqlx::query(
        "SELECT elapsed_ms FROM races WHERE user_id = ? AND mode = ?
         ORDER BY completed_at DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(mode)
    .fetch_optional(db)
    .await
    .map_err(internal)?
    .and_then(|r| r.try_get::<i64, _>("elapsed_ms").ok());

    let best = sqlx::query(
        "SELECT MIN(elapsed_ms) as best FROM races WHERE user_id = ? AND mode = ?",
    )
    .bind(user_id)
    .bind(mode)
    .fetch_optional(db)
    .await
    .map_err(internal)?
    .and_then(|r| r.try_get::<i64, _>("best").ok());

    Ok((last, best))
}

async fn record_race(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<RecordRaceRequest>,
) -> AppResult<RaceStatsResponse> {
    let user_id = authenticate(&state.db, &headers)
        .await
        .ok_or_else(|| app_err(StatusCode::UNAUTHORIZED, "Unauthorized"))?;

    let mode_str = match req.mode {
        Mode::Addition => "addition",
        Mode::Subtraction => "subtraction",
        Mode::TimesTables => "times_tables",
    };
    let completed_at = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO races (user_id, mode, elapsed_ms, completed_at) VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(mode_str)
    .bind(req.elapsed_ms)
    .bind(&completed_at)
    .execute(&state.db)
    .await
    .map_err(internal)?;

    // Trim to 100 most recent per (user, mode).
    sqlx::query(
        "DELETE FROM races WHERE id IN (
            SELECT id FROM races WHERE user_id = ? AND mode = ?
            ORDER BY completed_at DESC LIMIT -1 OFFSET 100
         )",
    )
    .bind(user_id)
    .bind(mode_str)
    .execute(&state.db)
    .await
    .map_err(internal)?;

    let (last_race_ms, best_race_ms) = get_race_stats(&state.db, user_id, &mode_str).await?;
    Ok(Json(RaceStatsResponse { last_race_ms, best_race_ms }))
}

// ── Teacher helpers ───────────────────────────────────────────────────────────

async fn get_user_role(db: &SqlitePool, user_id: i64) -> Result<String, (StatusCode, String)> {
    let row = sqlx::query("SELECT role FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(db)
        .await
        .map_err(internal)?;
    row.try_get("role").map_err(internal)
}

async fn get_or_create_invite_token(
    db: &SqlitePool,
    teacher_id: i64,
) -> Result<String, (StatusCode, String)> {
    if let Some(row) = sqlx::query("SELECT token FROM teacher_invites WHERE teacher_id = ?")
        .bind(teacher_id)
        .fetch_optional(db)
        .await
        .map_err(internal)?
    {
        return row.try_get("token").map_err(internal);
    }
    let token = generate_token();
    let created_at = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO teacher_invites (teacher_id, token, created_at) VALUES (?, ?, ?)",
    )
    .bind(teacher_id)
    .bind(&token)
    .bind(&created_at)
    .execute(db)
    .await
    .map_err(internal)?;
    Ok(token)
}

fn generate_qr_svg(data: &str) -> String {
    let code = QrCode::new(data.as_bytes())
        .unwrap_or_else(|_| QrCode::new(b"error").unwrap());
    code.render::<svg::Color>().min_dimensions(200, 200).build()
}

// ── Response history ──────────────────────────────────────────────────────────

fn parse_csv_line(line: &str) -> Option<tt_core::problem::ResponseRecord> {
    let mut parts = line.splitn(4, ',');
    let answered_at_secs = parts.next()?.parse::<DateTime<Utc>>().ok()?.timestamp();
    let elapsed_secs: f64 = parts.next()?.parse().ok()?;
    let _answer = parts.next()?;
    let correct = parts.next()? == "1";
    Some(tt_core::problem::ResponseRecord { answered_at_secs, elapsed_secs, correct })
}

async fn load_responses_into_sr(
    responses_dir: &std::path::Path,
    user_id: i64,
    mode: Mode,
    sr: &mut SpacedRepetition,
) {
    let user_dir = responses_dir.join(user_id.to_string());
    let problems: Vec<Problem> = match mode {
        Mode::Addition | Mode::Subtraction => generate_addition_problems(),
        Mode::TimesTables => generate_all_problems(),
    };
    for problem in problems {
        let content = match mode {
            Mode::Addition | Mode::Subtraction => {
                let subdir = if mode == Mode::Addition { "addition" } else { "subtraction" };
                let path = user_dir.join(subdir).join(format!("{}+{}.csv", problem.a, problem.b));
                tokio::fs::read_to_string(&path).await.unwrap_or_default()
            }
            Mode::TimesTables => {
                // Try new subfolder first, fall back to legacy flat path.
                let new_path = user_dir.join("times_tables").join(format!("{}x{}.csv", problem.a, problem.b));
                let legacy_path = user_dir.join(format!("{}x{}.csv", problem.a, problem.b));
                tokio::fs::read_to_string(&new_path).await
                    .or(tokio::fs::read_to_string(&legacy_path).await)
                    .unwrap_or_default()
            }
        };
        let records: Vec<_> = content.lines().filter_map(parse_csv_line).collect();
        if let Some(stats) = sr.get_stats_mut(&problem) {
            stats.responses = records;
        }
    }
}

async fn load_user_data(
    db: &SqlitePool,
    responses_dir: &std::path::Path,
    user_id: i64,
    mode: Mode,
) -> Result<SpacedRepetition, (StatusCode, String)> {
    let mut sr = load_user_state(db, user_id, mode).await?;
    load_responses_into_sr(responses_dir, user_id, mode, &mut sr).await;
    Ok(sr)
}

async fn load_student_activity(
    responses_dir: &std::path::Path,
    student_id: i64,
) -> (Option<i64>, [u32; 4], [u32; 4]) {
    let now = Utc::now().timestamp();
    let windows: [i64; 4] = [5 * 60, 10 * 60, 30 * 60, 24 * 3600];
    let mut correct = [0u32; 4];
    let mut total = [0u32; 4];
    let mut last_answered: Option<i64> = None;

    for a in 1u8..=12 {
        for b in 1u8..=12 {
            let path = responses_dir
                .join(student_id.to_string())
                .join(format!("{}x{}.csv", a, b));
            let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
            for line in content.lines() {
                if let Some(record) = parse_csv_line(line) {
                    if last_answered.map_or(true, |t| record.answered_at_secs > t) {
                        last_answered = Some(record.answered_at_secs);
                    }
                    let age = now - record.answered_at_secs;
                    for (i, &window) in windows.iter().enumerate() {
                        if age <= window {
                            total[i] += 1;
                            if record.correct { correct[i] += 1; }
                        }
                    }
                }
            }
        }
    }

    (last_answered, correct, total)
}

const MAX_RESPONSE_LINES: usize = tt_core::problem::MAX_RESPONSES;

async fn append_response(
    responses_dir: &std::path::Path,
    user_id: i64,
    mode: Mode,
    a: u8,
    b: u8,
    new_line: &str,
) -> Result<(), (StatusCode, String)> {
    let user_dir = responses_dir.join(user_id.to_string());
    let (subdir, filename) = match mode {
        Mode::Addition => ("addition", format!("{}+{}.csv", a, b)),
        Mode::Subtraction => ("subtraction", format!("{}+{}.csv", a, b)),
        Mode::TimesTables => ("times_tables", format!("{}x{}.csv", a, b)),
    };
    let subdir_path = user_dir.join(subdir);
    tokio::fs::create_dir_all(&subdir_path).await.map_err(internal)?;
    let file_path = subdir_path.join(filename);

    let existing = tokio::fs::read_to_string(&file_path).await.unwrap_or_default();
    let mut lines: Vec<&str> = existing.lines().collect();
    lines.push(new_line);

    let start = lines.len().saturating_sub(MAX_RESPONSE_LINES);
    let content = lines[start..].join("\n") + "\n";

    tokio::fs::write(&file_path, content).await.map_err(internal)?;
    Ok(())
}

fn correct_in_last_10m(sr: &SpacedRepetition) -> u32 {
    let cutoff = Utc::now().timestamp() - 600;
    sr.all_stats()
        .flat_map(|s| s.responses.iter())
        .filter(|r| r.correct && r.answered_at_secs >= cutoff)
        .count() as u32
}

// ── Problem selection ─────────────────────────────────────────────────────────

fn pick_problem(sr: &SpacedRepetition, last: Option<&Problem>) -> ProblemDto {
    use rand::seq::SliceRandom;

    let enabled = sr.enabled_problem_list();
    if enabled.is_empty() {
        return ProblemDto { a: 1, b: 1 };
    }

    struct Entry {
        problem: Problem,
        errors_in_last_5: u32,
        estimated_time: f64,
        last_asked_at_secs: Option<i64>,
    }

    let mut all: Vec<Entry> = enabled
        .iter()
        .filter_map(|p| {
            let stats = sr.get_stats(p)?;
            let correct_times = stats.recent_correct_times();
            Some(Entry {
                problem: *p,
                errors_in_last_5: stats.errors_in_last_5(),
                estimated_time: estimate_response_time(&correct_times),
                last_asked_at_secs: stats.last_asked_at_secs(),
            })
        })
        .collect();

    // Sort: most errors first, then slowest estimated time first.
    all.sort_by(|a, b| {
        b.errors_in_last_5.cmp(&a.errors_in_last_5).then(
            b.estimated_time
                .partial_cmp(&a.estimated_time)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    // Candidate pool: top 9 by priority + the least-recently-asked problem.
    let mut candidates: Vec<Problem> = all.iter().take(9).map(|e| e.problem).collect();

    if let Some(oldest) = all.iter().min_by_key(|e| e.last_asked_at_secs) {
        if !candidates.contains(&oldest.problem) {
            candidates.push(oldest.problem);
        }
    }

    // Prefer not to repeat the last problem shown.
    let pool: Vec<Problem> = if candidates.len() > 1 {
        candidates
            .iter()
            .copied()
            .filter(|p| last.map_or(true, |l| p != l))
            .collect()
    } else {
        candidates.clone()
    };
    let pool = if pool.is_empty() { candidates } else { pool };

    let chosen = pool
        .choose(&mut rand::thread_rng())
        .copied()
        .unwrap_or(Problem::new(1, 1));

    ProblemDto { a: chosen.a, b: chosen.b }
}

// ── History ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct HistoryEntry {
    answered_at_secs: i64,
    a: u8,
    b: u8,
    elapsed_secs: f64,
    correct: bool,
}

#[derive(Serialize)]
struct HistoryResponse {
    mode: String,
    entries: Vec<HistoryEntry>,
}

async fn serve_history() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../static/history.html"),
    )
}

async fn get_history(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<DebugQuery>,
) -> AppResult<HistoryResponse> {
    let user_id = authenticate(&state.db, &headers)
        .await
        .ok_or_else(|| app_err(StatusCode::UNAUTHORIZED, "Unauthorized"))?;

    let mode_str = match q.mode {
        Mode::Addition => "addition",
        Mode::Subtraction => "subtraction",
        Mode::TimesTables => "times_tables",
    };

    let all_problems = match q.mode {
        Mode::Addition | Mode::Subtraction => generate_addition_problems(),
        Mode::TimesTables => generate_all_problems(),
    };

    let mut entries: Vec<HistoryEntry> = Vec::new();
    for problem in all_problems {
        let (subdir, filename) = match q.mode {
            Mode::Addition | Mode::Subtraction => {
                let sub = if q.mode == Mode::Addition { "addition" } else { "subtraction" };
                (sub, format!("{}+{}.csv", problem.a, problem.b))
            }
            Mode::TimesTables => ("times_tables", format!("{}x{}.csv", problem.a, problem.b)),
        };
        let path = state.responses_dir
            .join(user_id.to_string())
            .join(subdir)
            .join(&filename);
        let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        for line in content.lines() {
            if let Some(record) = parse_csv_line(line) {
                entries.push(HistoryEntry {
                    answered_at_secs: record.answered_at_secs,
                    a: problem.a,
                    b: problem.b,
                    elapsed_secs: record.elapsed_secs,
                    correct: record.correct,
                });
            }
        }
    }

    entries.sort_by(|a, b| b.answered_at_secs.cmp(&a.answered_at_secs));
    entries.truncate(500);

    Ok(Json(HistoryResponse { mode: mode_str.to_string(), entries }))
}

// ── Debug ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct DebugProblemEntry {
    a: u8,
    b: u8,
    errors_in_last_5: u32,
    estimated_time: f64,
    estimated_time_sd: f64,
    no_data: bool,
    last_5_attempts: String,
}

#[derive(Deserialize)]
struct DebugQuery {
    #[serde(default)]
    mode: Mode,
}

#[derive(Serialize)]
struct DebugResponse {
    mode: String,
    problems: Vec<DebugProblemEntry>,
}

async fn serve_debug() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../static/debug.html"),
    )
}

async fn get_debug(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<DebugQuery>,
) -> AppResult<DebugResponse> {
    let user_id = authenticate(&state.db, &headers)
        .await
        .ok_or_else(|| app_err(StatusCode::UNAUTHORIZED, "Unauthorized"))?;

    let sr = load_user_data(&state.db, &state.responses_dir, user_id, q.mode).await?;

    let mode_str = match q.mode {
        Mode::Addition => "addition",
        Mode::Subtraction => "subtraction",
        Mode::TimesTables => "times_tables",
    };

    let all_problems = match q.mode {
        Mode::Addition | Mode::Subtraction => generate_addition_problems(),
        Mode::TimesTables => generate_all_problems(),
    };

    struct Entry {
        problem: Problem,
        errors_in_last_5: u32,
        estimated_time: f64,
        estimated_time_sd: f64,
        no_data: bool,
        last_5_attempts: String,
    }

    let mut entries: Vec<Entry> = all_problems
        .into_iter()
        .map(|p| {
            let (errors_in_last_5, estimated_time, estimated_time_sd, no_data, last_5_attempts) = sr
                .get_stats(&p)
                .map(|s| {
                    let correct_times = s.recent_correct_times();
                    let estimated = estimate_response_time(&correct_times);
                    let sd = estimate_response_time_sd(&correct_times);
                    let attempts = s.responses.iter().rev().take(5)
                        .map(|r| if r.correct {
                            format!("{}s", r.elapsed_secs.round() as i64)
                        } else {
                            "X".to_string()
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    (s.errors_in_last_5(), estimated, sd, correct_times.is_empty(), attempts)
                })
                .unwrap_or((0, 10.0, 5.0, true, String::new()));
            Entry { problem: p, errors_in_last_5, estimated_time, estimated_time_sd, no_data, last_5_attempts }
        })
        .collect();

    entries.sort_by(|a, b| {
        b.errors_in_last_5.cmp(&a.errors_in_last_5).then(
            b.estimated_time
                .partial_cmp(&a.estimated_time)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    let problems = entries
        .into_iter()
        .map(|e| DebugProblemEntry {
            a: e.problem.a,
            b: e.problem.b,
            errors_in_last_5: e.errors_in_last_5,
            estimated_time: e.estimated_time,
            estimated_time_sd: e.estimated_time_sd,
            no_data: e.no_data,
            last_5_attempts: e.last_5_attempts,
        })
        .collect();

    Ok(Json(DebugResponse { mode: mode_str.to_string(), problems }))
}

// ── Static file handlers ──────────────────────────────────────────────────────

async fn serve_index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn serve_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../static/style.css"),
    )
}

async fn serve_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        include_str!("../static/app.js"),
    )
}

// ── API handlers ──────────────────────────────────────────────────────────────

async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> AppResult<TokenResponse> {
    if req.username.trim().is_empty() || req.password.is_empty() {
        return Err(app_err(StatusCode::BAD_REQUEST, "Username and password required"));
    }
    let role = if req.role == "teacher" { "teacher" } else { "student" };

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(req.password.as_bytes(), &salt)
        .map_err(|e| internal(e))?
        .to_string();

    let result = sqlx::query(
        "INSERT INTO users (username, password_hash, role) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(req.username.trim())
    .bind(&password_hash)
    .bind(role)
    .fetch_one(&state.db)
    .await;

    let user_id: i64 = match result {
        Ok(row) => row.try_get("id").map_err(internal)?,
        Err(e) if e.to_string().contains("UNIQUE") => {
            return Err(app_err(StatusCode::CONFLICT, "Username already taken"));
        }
        Err(e) => return Err(internal(e)),
    };

    let token = create_session(&state.db, user_id).await?;
    Ok(Json(TokenResponse { token }))
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuthRequest>,
) -> AppResult<TokenResponse> {
    let row = sqlx::query("SELECT id, password_hash FROM users WHERE username = ?")
        .bind(req.username.trim())
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?
        .ok_or_else(|| app_err(StatusCode::UNAUTHORIZED, "Invalid username or password"))?;

    let user_id: i64 = row.try_get("id").map_err(internal)?;
    let stored_hash: String = row.try_get("password_hash").map_err(internal)?;

    if stored_hash.is_empty() {
        return Err(app_err(StatusCode::UNAUTHORIZED, "This account uses Google sign-in"));
    }

    let parsed =
        PasswordHash::new(&stored_hash).map_err(|e| internal(e))?;
    Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed)
        .map_err(|_| app_err(StatusCode::UNAUTHORIZED, "Invalid username or password"))?;

    let token = create_session(&state.db, user_id).await?;
    Ok(Json(TokenResponse { token }))
}

async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, String)> {
    if let Some(auth) = headers.get("Authorization").and_then(|v| v.to_str().ok()) {
        if let Some(token) = auth.strip_prefix("Bearer ") {
            sqlx::query("DELETE FROM sessions WHERE token = ?")
                .bind(token)
                .execute(&state.db)
                .await
                .map_err(internal)?;
        }
    }
    Ok(StatusCode::OK)
}

async fn get_state(
    State(state): State<Arc<AppState>>,
    Query(params): Query<StateQuery>,
    headers: HeaderMap,
) -> AppResult<StateResponse> {
    let user_id = authenticate(&state.db, &headers)
        .await
        .ok_or_else(|| app_err(StatusCode::UNAUTHORIZED, "Unauthorized"))?;

    let mode = params.mode;
    let sr = load_user_data(&state.db, &state.responses_dir, user_id, mode).await?;
    let role = get_user_role(&state.db, user_id).await?;
    let problem = pick_problem(&sr, None);
    let (grid, mode_str) = match mode {
        Mode::Addition => (sr.grid_status_sized(10), "addition".to_string()),
        Mode::Subtraction => (sr.grid_status_sized(10), "subtraction".to_string()),
        Mode::TimesTables => (sr.grid_status(), "times_tables".to_string()),
    };

    let (last_race_ms, best_race_ms) = get_race_stats(&state.db, user_id, &mode_str).await?;

    Ok(Json(StateResponse {
        problem,
        mastered: sr.mastered_count(),
        total: sr.enabled_problems(),
        grid,
        enabled_tables: sr.get_enabled_tables(),
        role,
        mode: mode_str,
        total_time: sr.compute_total_time(),
        total_answers: sr.total_answers(),
        last_race_ms,
        best_race_ms,
        correct_10m: correct_in_last_10m(&sr),
    }))
}

async fn submit_answer(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<AnswerRequest>,
) -> AppResult<AnswerResponse> {
    let user_id = authenticate(&state.db, &headers)
        .await
        .ok_or_else(|| app_err(StatusCode::UNAUTHORIZED, "Unauthorized"))?;

    let mode = req.mode;
    let mut sr = load_user_data(&state.db, &state.responses_dir, user_id, mode).await?;
    let problem = Problem::new(req.a, req.b);
    let correct_answer = match mode {
        Mode::Addition => req.a as u32 + req.b as u32,
        // Subtraction: problem (a,b) displays as (a+b)-a=?, answer is b.
        Mode::Subtraction => req.b as u32,
        Mode::TimesTables => problem.answer(),
    };
    let correct = req.answer == correct_answer;

    sr.record_answer(&problem, correct, req.elapsed_secs);
    save_user_state(&state.db, user_id, mode, &sr).await?;

    let now = Utc::now();
    let line = format!("{},{},{},{}", now.to_rfc3339(), req.elapsed_secs, req.answer, correct as u8);
    append_response(&state.responses_dir, user_id, mode, req.a, req.b, &line).await?;

    if let Some(stats) = sr.get_stats_mut(&problem) {
        stats.add_response(tt_core::problem::ResponseRecord {
            answered_at_secs: now.timestamp(),
            elapsed_secs: req.elapsed_secs,
            correct,
        });
    }

    let next = pick_problem(&sr, Some(&problem));
    let (grid, mode_str) = match mode {
        Mode::Addition => (sr.grid_status_sized(10), "addition".to_string()),
        Mode::Subtraction => (sr.grid_status_sized(10), "subtraction".to_string()),
        Mode::TimesTables => (sr.grid_status(), "times_tables".to_string()),
    };

    Ok(Json(AnswerResponse {
        correct,
        correct_answer,
        next_problem: next,
        mastered: sr.mastered_count(),
        total: sr.enabled_problems(),
        grid,
        enabled_tables: sr.get_enabled_tables(),
        mode: mode_str,
        total_time: sr.cached_total_time(),
        total_answers: sr.total_answers(),
        correct_10m: correct_in_last_10m(&sr),
    }))
}

async fn set_enabled_tables(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SetTablesRequest>,
) -> AppResult<StateResponse> {
    let user_id = authenticate(&state.db, &headers)
        .await
        .ok_or_else(|| app_err(StatusCode::UNAUTHORIZED, "Unauthorized"))?;

    let mode = req.mode;
    let mut sr = load_user_data(&state.db, &state.responses_dir, user_id, mode).await?;
    sr.set_enabled_tables(req.enabled);
    save_user_state(&state.db, user_id, mode, &sr).await?;

    let role = get_user_role(&state.db, user_id).await?;
    let problem = pick_problem(&sr, None);
    let (grid, mode_str) = match mode {
        Mode::Addition => (sr.grid_status_sized(10), "addition".to_string()),
        Mode::Subtraction => (sr.grid_status_sized(10), "subtraction".to_string()),
        Mode::TimesTables => (sr.grid_status(), "times_tables".to_string()),
    };
    let (last_race_ms, best_race_ms) = get_race_stats(&state.db, user_id, &mode_str).await?;

    Ok(Json(StateResponse {
        problem,
        mastered: sr.mastered_count(),
        total: sr.enabled_problems(),
        grid,
        enabled_tables: sr.get_enabled_tables(),
        role,
        mode: mode_str,
        total_time: sr.compute_total_time(),
        total_answers: sr.total_answers(),
        last_race_ms,
        best_race_ms,
        correct_10m: correct_in_last_10m(&sr),
    }))
}

#[derive(Deserialize, Default)]
struct ResetRequest {
    #[serde(default)]
    mode: Mode,
}

async fn reset_progress(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ResetRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user_id = authenticate(&state.db, &headers)
        .await
        .ok_or_else(|| app_err(StatusCode::UNAUTHORIZED, "Unauthorized"))?;

    let fresh_sr = match req.mode {
        Mode::Addition => SpacedRepetition::new_addition(),
        Mode::Subtraction => SpacedRepetition::new_subtraction(),
        Mode::TimesTables => SpacedRepetition::new(),
    };
    save_user_state(&state.db, user_id, req.mode, &fresh_sr).await?;

    // Delete response files for this mode only.
    let user_dir = state.responses_dir.join(user_id.to_string());
    let subdir = match req.mode {
        Mode::Addition => "addition",
        Mode::Subtraction => "subtraction",
        Mode::TimesTables => "times_tables",
    };
    let _ = tokio::fs::remove_dir_all(user_dir.join(subdir)).await;
    // Also remove legacy flat CSVs for times_tables resets.
    if req.mode == Mode::TimesTables {
        if let Ok(mut entries) = tokio::fs::read_dir(&user_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let p = entry.path();
                if p.extension().map_or(false, |e| e == "csv") {
                    let _ = tokio::fs::remove_file(p).await;
                }
            }
        }
    }

    Ok(StatusCode::OK)
}

// ── Teacher / join handlers ───────────────────────────────────────────────────

async fn get_teacher_invite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> AppResult<TeacherInviteResponse> {
    let user_id = authenticate(&state.db, &headers)
        .await
        .ok_or_else(|| app_err(StatusCode::UNAUTHORIZED, "Unauthorized"))?;

    if get_user_role(&state.db, user_id).await? != "teacher" {
        return Err(app_err(StatusCode::FORBIDDEN, "Not a teacher"));
    }

    let token = get_or_create_invite_token(&state.db, user_id).await?;
    let join_url = format!("{}/join/{}", state.base_url, token);
    let qr_svg = generate_qr_svg(&join_url);

    Ok(Json(TeacherInviteResponse { token, join_url, qr_svg }))
}

async fn get_teacher_students(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> AppResult<TeacherStudentsResponse> {
    let user_id = authenticate(&state.db, &headers)
        .await
        .ok_or_else(|| app_err(StatusCode::UNAUTHORIZED, "Unauthorized"))?;

    if get_user_role(&state.db, user_id).await? != "teacher" {
        return Err(app_err(StatusCode::FORBIDDEN, "Not a teacher"));
    }

    let rows = sqlx::query(
        "SELECT u.id, u.username, p.data
         FROM teacher_students ts
         JOIN users u ON u.id = ts.student_id
         LEFT JOIN progress p ON p.user_id = u.id
         WHERE ts.teacher_id = ?
         ORDER BY u.username",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(internal)?;

    let mut students = Vec::new();
    for row in rows {
        let student_id: i64 = row.try_get("id").map_err(internal)?;
        let username: String = row.try_get("username").map_err(internal)?;
        let data: Option<String> = row.try_get("data").ok().flatten();
        let (mastered, total) = data
            .and_then(|d| serde_json::from_str::<SpacedRepetition>(&d).ok())
            .map(|sr| (sr.mastered_count(), sr.enabled_problems()))
            .unwrap_or((0, 0));
        let (last_answered_secs, correct, tot) =
            load_student_activity(&state.responses_dir, student_id).await;
        students.push(StudentInfo {
            username,
            mastered,
            total,
            last_answered_secs,
            correct_5m: correct[0],
            total_5m: tot[0],
            correct_10m: correct[1],
            total_10m: tot[1],
            correct_30m: correct[2],
            total_30m: tot[2],
            correct_day: correct[3],
            total_day: tot[3],
        });
    }

    Ok(Json(TeacherStudentsResponse { students }))
}

async fn get_join_info(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> AppResult<JoinInfoResponse> {
    let row = sqlx::query(
        "SELECT u.username FROM teacher_invites ti
         JOIN users u ON u.id = ti.teacher_id
         WHERE ti.token = ?",
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await
    .map_err(internal)?
    .ok_or_else(|| app_err(StatusCode::NOT_FOUND, "Invalid invite link"))?;

    let teacher_username: String = row.try_get("username").map_err(internal)?;
    Ok(Json(JoinInfoResponse { teacher_username }))
}

async fn accept_join(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, String)> {
    let user_id = authenticate(&state.db, &headers)
        .await
        .ok_or_else(|| app_err(StatusCode::UNAUTHORIZED, "Unauthorized"))?;

    let row = sqlx::query("SELECT teacher_id FROM teacher_invites WHERE token = ?")
        .bind(&token)
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?
        .ok_or_else(|| app_err(StatusCode::NOT_FOUND, "Invalid invite link"))?;

    let teacher_id: i64 = row.try_get("teacher_id").map_err(internal)?;

    if teacher_id == user_id {
        return Err(app_err(StatusCode::BAD_REQUEST, "Cannot join your own class"));
    }

    sqlx::query(
        "INSERT OR IGNORE INTO teacher_students (teacher_id, student_id) VALUES (?, ?)",
    )
    .bind(teacher_id)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(internal)?;

    Ok(StatusCode::OK)
}

async fn serve_join() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../static/join.html"),
    )
}

async fn get_config(State(state): State<Arc<AppState>>) -> Json<ConfigResponse> {
    Json(ConfigResponse {
        google_oauth: state.google_client_id.is_some(),
    })
}

async fn google_auth_start(
    State(state): State<Arc<AppState>>,
    Query(params): Query<OAuthStartParams>,
) -> Result<Redirect, (StatusCode, String)> {
    let client_id = state.google_client_id.as_ref().unwrap();
    let redirect_uri = format!("{}/api/auth/google/callback", state.base_url);
    let oauth_state = generate_token();
    let created_at = Utc::now().to_rfc3339();
    let role = if params.role == "teacher" { "teacher" } else { "student" };

    sqlx::query("INSERT INTO oauth_states (state, role, created_at) VALUES (?, ?, ?)")
        .bind(&oauth_state)
        .bind(role)
        .bind(&created_at)
        .execute(&state.db)
        .await
        .map_err(internal)?;

    let url = reqwest::Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        &[
            ("client_id", client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", "openid email"),
            ("state", oauth_state.as_str()),
        ],
    )
    .map_err(internal)?;

    Ok(Redirect::to(url.as_str()))
}

enum OAuthRedirect {
    Token(String),
    Choice { pending_token: String, existing_role: String, requested_role: String },
}

async fn google_auth_inner(
    state: Arc<AppState>,
    params: OAuthCallbackParams,
) -> Result<OAuthRedirect, String> {
    if let Some(err) = params.error {
        return Err(err);
    }

    let code = params.code.ok_or_else(|| "missing_code".to_string())?;
    let oauth_state = params.state.ok_or_else(|| "missing_state".to_string())?;

    let row = sqlx::query("DELETE FROM oauth_states WHERE state = ? RETURNING state, role")
        .bind(&oauth_state)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| "db_error".to_string())?;

    let state_row = row.ok_or_else(|| "invalid_state".to_string())?;
    let role: String = state_row.try_get("role").unwrap_or_else(|_| "student".to_string());

    let redirect_uri = format!("{}/api/auth/google/callback", state.base_url);
    let client_id = state.google_client_id.as_ref().unwrap();
    let client_secret = state.google_client_secret.as_ref().unwrap();

    let token_res = state
        .http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|_| "token_exchange_failed".to_string())?;

    if !token_res.status().is_success() {
        return Err("token_exchange_failed".to_string());
    }

    let token_data: GoogleTokenResponse = token_res
        .json()
        .await
        .map_err(|_| "token_parse_failed".to_string())?;

    let user_res = state
        .http
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(&token_data.access_token)
        .send()
        .await
        .map_err(|_| "userinfo_failed".to_string())?;

    if !user_res.status().is_success() {
        return Err("userinfo_failed".to_string());
    }

    let user_info: GoogleUserInfo = user_res
        .json()
        .await
        .map_err(|_| "userinfo_parse_failed".to_string())?;

    let oauth_result = find_or_create_google_user(&state.db, &user_info.id, &user_info.email, &role)
        .await
        .map_err(|_| "db_error".to_string())?;

    match oauth_result {
        OAuthUserResult::UserId(user_id) => {
            let token = create_session(&state.db, user_id)
                .await
                .map_err(|_| "session_error".to_string())?;
            Ok(OAuthRedirect::Token(token))
        }
        OAuthUserResult::NeedsChoice { existing_user_id, existing_role } => {
            let pending_token = generate_token();
            let created_at = Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO pending_oauth (token, google_id, email, requested_role, existing_user_id, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&pending_token)
            .bind(&user_info.id)
            .bind(&user_info.email)
            .bind(&role)
            .bind(existing_user_id)
            .bind(&created_at)
            .execute(&state.db)
            .await
            .map_err(|_| "db_error".to_string())?;

            Ok(OAuthRedirect::Choice { pending_token, existing_role, requested_role: role })
        }
    }
}

async fn google_auth_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<OAuthCallbackParams>,
) -> Redirect {
    match google_auth_inner(state, params).await {
        Ok(OAuthRedirect::Token(token)) =>
            Redirect::to(&format!("/#token={}", token)),
        Ok(OAuthRedirect::Choice { pending_token, existing_role, requested_role }) =>
            Redirect::to(&format!(
                "/#oauth_choice={}&existing_role={}&requested_role={}",
                pending_token, existing_role, requested_role
            )),
        Err(msg) => Redirect::to(&format!("/#auth_error={}", msg)),
    }
}

async fn google_auth_complete(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OAuthCompleteRequest>,
) -> AppResult<TokenResponse> {
    let row = sqlx::query(
        "DELETE FROM pending_oauth WHERE token = ? \
         RETURNING google_id, email, requested_role, existing_user_id, created_at",
    )
    .bind(&req.token)
    .fetch_optional(&state.db)
    .await
    .map_err(internal)?
    .ok_or_else(|| app_err(StatusCode::BAD_REQUEST, "Invalid or expired token"))?;

    let created_at: String = row.try_get("created_at").map_err(internal)?;
    let created = created_at.parse::<DateTime<Utc>>().map_err(internal)?;
    if (Utc::now() - created).num_seconds() > 600 {
        return Err(app_err(StatusCode::BAD_REQUEST, "Token expired"));
    }

    let existing_user_id: i64 = row.try_get("existing_user_id").map_err(internal)?;

    let user_id = if req.action == "use_existing" {
        existing_user_id
    } else {
        let google_id: String = row.try_get("google_id").map_err(internal)?;
        let email: String = row.try_get("email").map_err(internal)?;
        let requested_role: String = row.try_get("requested_role").map_err(internal)?;
        create_google_user_with_role(&state.db, &google_id, &email, &requested_role).await?
    };

    let token = create_session(&state.db, user_id).await?;
    Ok(Json(TokenResponse { token }))
}

enum OAuthUserResult {
    UserId(i64),
    NeedsChoice { existing_user_id: i64, existing_role: String },
}

async fn find_or_create_google_user(
    db: &SqlitePool,
    google_id: &str,
    email: &str,
    role: &str,
) -> Result<OAuthUserResult, (StatusCode, String)> {
    // Account with same google_id and same role — just log in.
    if let Some(row) = sqlx::query("SELECT id FROM users WHERE google_id = ? AND role = ?")
        .bind(google_id)
        .bind(role)
        .fetch_optional(db)
        .await
        .map_err(internal)?
    {
        return Ok(OAuthUserResult::UserId(row.try_get("id").map_err(internal)?));
    }

    // Account with same google_id but different role — offer a choice.
    if let Some(row) = sqlx::query("SELECT id, role FROM users WHERE google_id = ?")
        .bind(google_id)
        .fetch_optional(db)
        .await
        .map_err(internal)?
    {
        let existing_user_id: i64 = row.try_get("id").map_err(internal)?;
        let existing_role: String = row.try_get("role").map_err(internal)?;
        return Ok(OAuthUserResult::NeedsChoice { existing_user_id, existing_role });
    }

    // Email matches a password account — link Google ID, keep existing role.
    if let Some(row) = sqlx::query("SELECT id, role FROM users WHERE username = ?")
        .bind(email)
        .fetch_optional(db)
        .await
        .map_err(internal)?
    {
        let user_id: i64 = row.try_get("id").map_err(internal)?;
        let existing_role: String = row.try_get("role").map_err(internal)?;
        sqlx::query("UPDATE users SET google_id = ? WHERE id = ?")
            .bind(google_id)
            .bind(user_id)
            .execute(db)
            .await
            .map_err(internal)?;
        if existing_role == role {
            return Ok(OAuthUserResult::UserId(user_id));
        } else {
            return Ok(OAuthUserResult::NeedsChoice { existing_user_id: user_id, existing_role });
        }
    }

    // Brand new user — create with the requested role.
    let user_id = create_google_user_with_role(db, google_id, email, role).await?;
    Ok(OAuthUserResult::UserId(user_id))
}

/// Insert a new Google-linked user, falling back to "email (role)" if the
/// plain email username is already taken by the user's other-role account.
async fn create_google_user_with_role(
    db: &SqlitePool,
    google_id: &str,
    email: &str,
    role: &str,
) -> Result<i64, (StatusCode, String)> {
    let result = sqlx::query(
        "INSERT INTO users (username, password_hash, google_id, role) VALUES (?, '', ?, ?) RETURNING id",
    )
    .bind(email)
    .bind(google_id)
    .bind(role)
    .fetch_one(db)
    .await;

    match result {
        Ok(row) => Ok(row.try_get("id").map_err(internal)?),
        Err(e) if e.to_string().contains("UNIQUE") => {
            let alt = format!("{} ({})", email, role);
            let row = sqlx::query(
                "INSERT INTO users (username, password_hash, google_id, role) VALUES (?, '', ?, ?) RETURNING id",
            )
            .bind(&alt)
            .bind(google_id)
            .bind(role)
            .fetch_one(db)
            .await
            .map_err(internal)?;
            Ok(row.try_get("id").map_err(internal)?)
        }
        Err(e) => Err(internal(e)),
    }
}

// ── DB setup ──────────────────────────────────────────────────────────────────

fn get_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("DATA_DIR") {
        PathBuf::from(dir)
    } else {
        let dirs = ProjectDirs::from("com", "practice", "times_tables_server")
            .expect("Could not determine data directory");
        dirs.data_dir().to_path_buf()
    }
}

async fn get_db_pool() -> SqlitePool {
    let data_dir = get_data_dir();
    std::fs::create_dir_all(&data_dir).expect("Could not create data directory");
    let db_path = data_dir.join("db.sqlite");

    let opts = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);

    SqlitePool::connect_with(opts)
        .await
        .expect("Could not connect to database")
}

async fn init_db(pool: &SqlitePool) {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .expect("Could not create users table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS progress (
            user_id INTEGER PRIMARY KEY REFERENCES users(id),
            data TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .expect("Could not create progress table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS sessions (
            token TEXT PRIMARY KEY,
            user_id INTEGER NOT NULL REFERENCES users(id),
            expires_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .expect("Could not create sessions table");
}

async fn migrate_db(pool: &SqlitePool) {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS oauth_states (
            state TEXT PRIMARY KEY,
            role TEXT NOT NULL DEFAULT 'student',
            created_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .expect("Could not create oauth_states table");

    // Add role column to existing oauth_states tables that predate this migration.
    let _ = sqlx::query("ALTER TABLE oauth_states ADD COLUMN role TEXT NOT NULL DEFAULT 'student'")
        .execute(pool)
        .await;

    // Ignore error if column already exists
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN google_id TEXT")
        .execute(pool)
        .await;

    // Old single-account-per-google-id index; drop so we can allow one account per role.
    let _ = sqlx::query("DROP INDEX IF EXISTS idx_users_google_id")
        .execute(pool)
        .await;

    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_users_google_id_role \
         ON users(google_id, role) WHERE google_id IS NOT NULL",
    )
    .execute(pool)
    .await
    .expect("Could not create google_id_role index");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS pending_oauth (
            token TEXT PRIMARY KEY,
            google_id TEXT NOT NULL,
            email TEXT NOT NULL,
            requested_role TEXT NOT NULL,
            existing_user_id INTEGER NOT NULL,
            created_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .expect("Could not create pending_oauth table");

    let _ = sqlx::query("ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'student'")
        .execute(pool)
        .await;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS teacher_invites (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            teacher_id INTEGER NOT NULL REFERENCES users(id),
            token TEXT UNIQUE NOT NULL,
            created_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .expect("Could not create teacher_invites table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS teacher_students (
            teacher_id INTEGER NOT NULL REFERENCES users(id),
            student_id INTEGER NOT NULL REFERENCES users(id),
            PRIMARY KEY (teacher_id, student_id)
        )",
    )
    .execute(pool)
    .await
    .expect("Could not create teacher_students table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS progress_addition (
            user_id INTEGER PRIMARY KEY REFERENCES users(id),
            data TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .expect("Could not create progress_addition table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS progress_subtraction (
            user_id INTEGER PRIMARY KEY REFERENCES users(id),
            data TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .expect("Could not create progress_subtraction table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS races (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL REFERENCES users(id),
            mode TEXT NOT NULL,
            elapsed_ms INTEGER NOT NULL,
            completed_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .expect("Could not create races table");

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_races_user_mode \
         ON races(user_id, mode, completed_at DESC)",
    )
    .execute(pool)
    .await
    .expect("Could not create races index");
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let db = get_db_pool().await;
    init_db(&db).await;
    migrate_db(&db).await;

    let responses_dir = get_data_dir().join("responses");
    std::fs::create_dir_all(&responses_dir).expect("Could not create responses directory");

    let google_client_id = std::env::var("GOOGLE_CLIENT_ID").ok();
    let google_client_secret = std::env::var("GOOGLE_CLIENT_SECRET").ok();
    let base_url = std::env::var("BASE_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());
    let has_google = google_client_id.is_some();

    let state = Arc::new(AppState {
        db,
        http: reqwest::Client::new(),
        google_client_id,
        google_client_secret,
        base_url,
        responses_dir,
    });

    let mut app = Router::new()
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/state", get(get_state))
        .route("/api/answer", post(submit_answer))
        .route("/api/reset", post(reset_progress))
        .route("/api/tables", post(set_enabled_tables))
        .route("/api/config", get(get_config))
        .route("/api/race", post(record_race))
        .route("/api/teacher/invite", get(get_teacher_invite))
        .route("/api/teacher/students", get(get_teacher_students))
        .route("/api/join_info/:token", get(get_join_info))
        .route("/api/join/:token", post(accept_join))
        .route("/join/:token", get(serve_join))
        .route("/", get(serve_index))
        .route("/style.css", get(serve_css))
        .route("/app.js", get(serve_js))
        .route("/debug", get(serve_debug))
        .route("/api/debug", get(get_debug))
        .route("/history", get(serve_history))
        .route("/api/history", get(get_history));

    if has_google {
        app = app
            .route("/api/auth/google", get(google_auth_start))
            .route("/api/auth/google/callback", get(google_auth_callback))
            .route("/api/auth/google/complete", post(google_auth_complete));
    }

    let app = app.with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Could not bind to port 3000");

    println!("Server running at http://localhost:3000");
    axum::serve(listener, app).await.expect("Server error");
}
