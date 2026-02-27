use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, Row, SqlitePool};
use std::sync::Arc;
use tt_core::{problem::Problem, spaced_rep::SpacedRepetition};

// ── App state ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    http: reqwest::Client,
    google_client_id: Option<String>,
    google_client_secret: Option<String>,
    base_url: String,
}

// ── Request / Response types ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct AuthRequest {
    username: String,
    password: String,
}

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
    due: usize,
}

#[derive(Deserialize)]
struct AnswerRequest {
    a: u8,
    b: u8,
    answer: u32,
    #[serde(default = "default_elapsed")]
    elapsed_secs: f64,
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
    due: usize,
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
) -> Result<SpacedRepetition, (StatusCode, String)> {
    let row = sqlx::query("SELECT data FROM progress WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(db)
        .await
        .map_err(internal)?;

    match row {
        Some(r) => {
            let data: String = r.try_get("data").map_err(internal)?;
            serde_json::from_str(&data).map_err(internal)
        }
        None => Ok(SpacedRepetition::new()),
    }
}

async fn save_user_state(
    db: &SqlitePool,
    user_id: i64,
    sr: &SpacedRepetition,
) -> Result<(), (StatusCode, String)> {
    let data = serde_json::to_string(sr).map_err(internal)?;
    sqlx::query(
        "INSERT INTO progress (user_id, data) VALUES (?, ?)
         ON CONFLICT(user_id) DO UPDATE SET data = excluded.data",
    )
    .bind(user_id)
    .bind(&data)
    .execute(db)
    .await
    .map_err(internal)?;
    Ok(())
}

// ── Problem selection ─────────────────────────────────────────────────────────

fn pick_problem(sr: &SpacedRepetition, last: Option<&Problem>) -> ProblemDto {
    let p = sr
        .get_next_problem(last)
        .or_else(|| sr.get_extra_practice_problem(last))
        // If last was the only problem, ignore it and repeat
        .or_else(|| sr.get_next_problem(None))
        .or_else(|| sr.get_extra_practice_problem(None))
        .unwrap_or_else(|| Problem::new(1, 1));
    ProblemDto { a: p.a, b: p.b }
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
    Json(req): Json<AuthRequest>,
) -> AppResult<TokenResponse> {
    if req.username.trim().is_empty() || req.password.is_empty() {
        return Err(app_err(StatusCode::BAD_REQUEST, "Username and password required"));
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(req.password.as_bytes(), &salt)
        .map_err(|e| internal(e))?
        .to_string();

    let result = sqlx::query(
        "INSERT INTO users (username, password_hash) VALUES (?, ?) RETURNING id",
    )
    .bind(req.username.trim())
    .bind(&password_hash)
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
    headers: HeaderMap,
) -> AppResult<StateResponse> {
    let user_id = authenticate(&state.db, &headers)
        .await
        .ok_or_else(|| app_err(StatusCode::UNAUTHORIZED, "Unauthorized"))?;

    let sr = load_user_state(&state.db, user_id).await?;
    let problem = pick_problem(&sr, None);

    Ok(Json(StateResponse {
        problem,
        mastered: sr.mastered_count(),
        total: sr.unlocked_problems(),
        due: sr.due_count(),
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

    let mut sr = load_user_state(&state.db, user_id).await?;
    let problem = Problem::new(req.a, req.b);
    let correct_answer = problem.answer();
    let correct = req.answer == correct_answer;

    sr.record_answer(&problem, correct, req.elapsed_secs);
    save_user_state(&state.db, user_id, &sr).await?;

    let next = pick_problem(&sr, Some(&problem));

    Ok(Json(AnswerResponse {
        correct,
        correct_answer,
        next_problem: next,
        mastered: sr.mastered_count(),
        total: sr.unlocked_problems(),
        due: sr.due_count(),
    }))
}

async fn reset_progress(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, String)> {
    let user_id = authenticate(&state.db, &headers)
        .await
        .ok_or_else(|| app_err(StatusCode::UNAUTHORIZED, "Unauthorized"))?;

    let sr = SpacedRepetition::new();
    save_user_state(&state.db, user_id, &sr).await?;
    Ok(StatusCode::OK)
}

async fn get_config(State(state): State<Arc<AppState>>) -> Json<ConfigResponse> {
    Json(ConfigResponse {
        google_oauth: state.google_client_id.is_some(),
    })
}

async fn google_auth_start(
    State(state): State<Arc<AppState>>,
) -> Result<Redirect, (StatusCode, String)> {
    let client_id = state.google_client_id.as_ref().unwrap();
    let redirect_uri = format!("{}/api/auth/google/callback", state.base_url);
    let oauth_state = generate_token();
    let created_at = Utc::now().to_rfc3339();

    sqlx::query("INSERT INTO oauth_states (state, created_at) VALUES (?, ?)")
        .bind(&oauth_state)
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

async fn google_auth_inner(
    state: Arc<AppState>,
    params: OAuthCallbackParams,
) -> Result<String, String> {
    if let Some(err) = params.error {
        return Err(err);
    }

    let code = params.code.ok_or_else(|| "missing_code".to_string())?;
    let oauth_state = params.state.ok_or_else(|| "missing_state".to_string())?;

    let row = sqlx::query("DELETE FROM oauth_states WHERE state = ? RETURNING state")
        .bind(&oauth_state)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| "db_error".to_string())?;

    if row.is_none() {
        return Err("invalid_state".to_string());
    }

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

    let user_id = find_or_create_google_user(&state.db, &user_info.id, &user_info.email)
        .await
        .map_err(|_| "db_error".to_string())?;

    let token = create_session(&state.db, user_id)
        .await
        .map_err(|_| "session_error".to_string())?;

    Ok(token)
}

async fn google_auth_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<OAuthCallbackParams>,
) -> Redirect {
    match google_auth_inner(state, params).await {
        Ok(token) => Redirect::to(&format!("/#token={}", token)),
        Err(msg) => Redirect::to(&format!("/#auth_error={}", msg)),
    }
}

async fn find_or_create_google_user(
    db: &SqlitePool,
    google_id: &str,
    email: &str,
) -> Result<i64, (StatusCode, String)> {
    if let Some(row) = sqlx::query("SELECT id FROM users WHERE google_id = ?")
        .bind(google_id)
        .fetch_optional(db)
        .await
        .map_err(internal)?
    {
        return Ok(row.try_get("id").map_err(internal)?);
    }

    if let Some(row) = sqlx::query("SELECT id FROM users WHERE username = ?")
        .bind(email)
        .fetch_optional(db)
        .await
        .map_err(internal)?
    {
        let user_id: i64 = row.try_get("id").map_err(internal)?;
        sqlx::query("UPDATE users SET google_id = ? WHERE id = ?")
            .bind(google_id)
            .bind(user_id)
            .execute(db)
            .await
            .map_err(internal)?;
        return Ok(user_id);
    }

    let row = sqlx::query(
        "INSERT INTO users (username, password_hash, google_id) VALUES (?, '', ?) RETURNING id",
    )
    .bind(email)
    .bind(google_id)
    .fetch_one(db)
    .await
    .map_err(internal)?;

    Ok(row.try_get("id").map_err(internal)?)
}

// ── DB setup ──────────────────────────────────────────────────────────────────

async fn get_db_pool() -> SqlitePool {
    let data_dir = if let Ok(dir) = std::env::var("DATA_DIR") {
        std::path::PathBuf::from(dir)
    } else {
        let dirs = ProjectDirs::from("com", "practice", "times_tables_server")
            .expect("Could not determine data directory");
        dirs.data_dir().to_path_buf()
    };
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
            created_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .expect("Could not create oauth_states table");

    // Ignore error if column already exists
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN google_id TEXT")
        .execute(pool)
        .await;

    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_users_google_id \
         ON users(google_id) WHERE google_id IS NOT NULL",
    )
    .execute(pool)
    .await
    .expect("Could not create google_id index");
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let db = get_db_pool().await;
    init_db(&db).await;
    migrate_db(&db).await;

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
    });

    let mut app = Router::new()
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/state", get(get_state))
        .route("/api/answer", post(submit_answer))
        .route("/api/reset", post(reset_progress))
        .route("/api/config", get(get_config))
        .route("/", get(serve_index))
        .route("/style.css", get(serve_css))
        .route("/app.js", get(serve_js));

    if has_google {
        app = app
            .route("/api/auth/google", get(google_auth_start))
            .route("/api/auth/google/callback", get(google_auth_callback));
    }

    let app = app.with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Could not bind to port 3000");

    println!("Server running at http://localhost:3000");
    axum::serve(listener, app).await.expect("Server error");
}
