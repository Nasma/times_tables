'use strict';

// ── State ─────────────────────────────────────────────────────────────────────

const state = {
  problem: null,           // { a, b }
  awaitingCorrection: false,
  correctAnswer: null,
  pendingNextProblem: null, // next problem to show after correction
  problemStartMs: 0,
  streak: 0,
  sessionCorrect: 0,
  sessionWrong: 0,
  mastered: 0,
  total: 0,
  due: 0,
};

// ── DOM refs ──────────────────────────────────────────────────────────────────

const $ = id => document.getElementById(id);

const authView        = $('auth-view');
const practiceView    = $('practice-view');
const usernameInput   = $('username');
const passwordInput   = $('password');
const loginBtn        = $('login-btn');
const registerBtn     = $('register-btn');
const authError       = $('auth-error');
const problemText     = $('problem-text');
const normalMode      = $('normal-mode');
const answerInput     = $('answer-input');
const submitBtn       = $('submit-btn');
const correctionMode  = $('correction-mode');
const incorrectMsg    = $('incorrect-msg');
const correctionInput = $('correction-input');
const streakEl        = $('streak');
const masteredEl      = $('mastered');
const totalEl         = $('total');
const dueEl           = $('due');
const sessionCorrectEl = $('session-correct');
const sessionWrongEl  = $('session-wrong');
const resetBtn        = $('reset-btn');
const resetConfirm    = $('reset-confirm');
const resetYes        = $('reset-yes');
const resetCancel     = $('reset-cancel');
const logoutBtn       = $('logout-btn');
const googleAuth      = $('google-auth');
const progressGrid    = $('progress-grid');

// ── API helpers ───────────────────────────────────────────────────────────────

function getToken() {
  return localStorage.getItem('token');
}

function authHeaders() {
  const t = getToken();
  return t ? { 'Authorization': `Bearer ${t}` } : {};
}

async function apiPost(path, body) {
  return fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify(body),
  });
}

async function apiGet(path) {
  return fetch(path, { headers: authHeaders() });
}

// ── View helpers ──────────────────────────────────────────────────────────────

function showAuth() {
  authView.classList.remove('hidden');
  practiceView.classList.add('hidden');
  usernameInput.focus();
}

function showPractice() {
  authView.classList.add('hidden');
  practiceView.classList.remove('hidden');
}

function setAuthError(msg) {
  authError.textContent = msg;
  authError.classList.toggle('hidden', !msg);
}

function showNormalMode() {
  state.awaitingCorrection = false;
  normalMode.classList.remove('hidden');
  correctionMode.classList.add('hidden');
  answerInput.value = '';
  answerInput.focus();
}

function showCorrectionMode(userAnswer, correctAnswer, nextProblem) {
  state.awaitingCorrection = true;
  state.correctAnswer = correctAnswer;
  state.pendingNextProblem = nextProblem;
  incorrectMsg.textContent = `${userAnswer} is wrong. Type the answer: ${correctAnswer}`;
  normalMode.classList.add('hidden');
  correctionMode.classList.remove('hidden');
  correctionInput.value = '';
  correctionInput.focus();
}

function displayProblem(problem) {
  state.problem = problem;
  state.problemStartMs = Date.now();
  problemText.textContent = `${problem.a} × ${problem.b} = ?`;
  showNormalMode();
}

function updateStats() {
  streakEl.textContent = state.streak;
  masteredEl.textContent = state.mastered;
  totalEl.textContent = state.total;
  dueEl.textContent = state.due;
  sessionCorrectEl.textContent = state.sessionCorrect;
  sessionWrongEl.textContent = state.sessionWrong;
}

function renderGrid(grid) {
  progressGrid.innerHTML = '';
  grid.forEach((status, i) => {
    const a = Math.floor(i / 12) + 1;
    const b = (i % 12) + 1;
    const cell = document.createElement('div');
    cell.className = `grid-cell ${status}`;
    cell.title = `${a} × ${b} = ${a * b}`;
    progressGrid.appendChild(cell);
  });
}

// ── Auth ──────────────────────────────────────────────────────────────────────

async function loadState() {
  const res = await apiGet('/api/state');
  if (res.status === 401) {
    localStorage.removeItem('token');
    showAuth();
    return;
  }
  if (!res.ok) {
    showAuth();
    return;
  }
  const data = await res.json();
  state.mastered = data.mastered;
  state.total = data.total;
  state.due = data.due;
  updateStats();
  renderGrid(data.grid);
  displayProblem(data.problem);
  showPractice();
}

async function doAuth(endpoint) {
  const username = usernameInput.value.trim();
  const password = passwordInput.value;
  if (!username || !password) {
    setAuthError('Please enter username and password.');
    return;
  }
  setAuthError('');

  const res = await apiPost(endpoint, { username, password });
  if (res.ok) {
    const data = await res.json();
    localStorage.setItem('token', data.token);
    passwordInput.value = '';
    // Reset session stats on new login
    state.streak = 0;
    state.sessionCorrect = 0;
    state.sessionWrong = 0;
    await loadState();
  } else {
    const msg = await res.text();
    setAuthError(msg || 'Something went wrong.');
  }
}

loginBtn.addEventListener('click', () => doAuth('/api/login'));
registerBtn.addEventListener('click', () => doAuth('/api/register'));

[usernameInput, passwordInput].forEach(el => {
  el.addEventListener('keydown', e => {
    if (e.key === 'Enter') doAuth('/api/login');
  });
});

// ── Answer submission ─────────────────────────────────────────────────────────

async function submitAnswer() {
  if (!state.problem) return;
  const raw = answerInput.value.trim();
  if (raw === '') return;
  const answer = parseInt(raw, 10);
  if (isNaN(answer)) {
    answerInput.value = '';
    return;
  }

  const elapsedSecs = (Date.now() - state.problemStartMs) / 1000;

  const res = await apiPost('/api/answer', {
    a: state.problem.a,
    b: state.problem.b,
    answer,
    elapsed_secs: elapsedSecs,
  });

  if (res.status === 401) {
    localStorage.removeItem('token');
    showAuth();
    return;
  }

  if (!res.ok) return;

  const data = await res.json();
  state.mastered = data.mastered;
  state.total = data.total;
  state.due = data.due;
  renderGrid(data.grid);

  if (data.correct) {
    state.streak += 1;
    state.sessionCorrect += 1;
    updateStats();
    displayProblem(data.next_problem);
  } else {
    state.streak = 0;
    state.sessionWrong += 1;
    updateStats();
    showCorrectionMode(answer, data.correct_answer, data.next_problem);
  }
}

function checkCorrection() {
  const raw = correctionInput.value.trim();
  if (raw === '') return;
  const typed = parseInt(raw, 10);
  if (typed === state.correctAnswer) {
    displayProblem(state.pendingNextProblem);
  }
}

submitBtn.addEventListener('click', submitAnswer);

answerInput.addEventListener('keydown', e => {
  if (e.key === 'Enter') submitAnswer();
});

correctionInput.addEventListener('keydown', e => {
  if (e.key === 'Enter') checkCorrection();
});

// ── Reset ─────────────────────────────────────────────────────────────────────

resetBtn.addEventListener('click', () => {
  resetConfirm.classList.remove('hidden');
  resetBtn.classList.add('hidden');
});

resetCancel.addEventListener('click', () => {
  resetConfirm.classList.add('hidden');
  resetBtn.classList.remove('hidden');
});

resetYes.addEventListener('click', async () => {
  resetConfirm.classList.add('hidden');
  resetBtn.classList.remove('hidden');

  const res = await apiPost('/api/reset', {});
  if (res.status === 401) {
    localStorage.removeItem('token');
    showAuth();
    return;
  }
  if (!res.ok) return;

  state.streak = 0;
  state.sessionCorrect = 0;
  state.sessionWrong = 0;
  await loadState();
});

// ── Logout ────────────────────────────────────────────────────────────────────

logoutBtn.addEventListener('click', async () => {
  await apiPost('/api/logout', {});
  localStorage.removeItem('token');
  showAuth();
});

// ── Boot ──────────────────────────────────────────────────────────────────────

// Handle token/error passed back from OAuth redirect via URL hash
let oauthError = null;
const hash = window.location.hash;
if (hash.startsWith('#token=')) {
  localStorage.setItem('token', hash.slice('#token='.length));
  history.replaceState(null, '', window.location.pathname);
} else if (hash.startsWith('#auth_error=')) {
  oauthError = decodeURIComponent(hash.slice('#auth_error='.length));
  history.replaceState(null, '', window.location.pathname);
}

// Show Google button if server has OAuth credentials configured
fetch('/api/config')
  .then(r => r.json())
  .then(cfg => { if (cfg.google_oauth) googleAuth.classList.remove('hidden'); })
  .catch(() => {});

if (getToken()) {
  loadState();
} else {
  showAuth();
  if (oauthError) setAuthError(oauthError);
}
