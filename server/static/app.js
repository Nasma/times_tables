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
  enabledTables: [1,2,3,4,5,6,7,8,9,10,11,12],
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
const tableRowsEl     = $('table-rows');

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

const TIER_VAL = { not_started: 0, learning: 1, solid: 2, fast: 3, mastered: 4 };

function renderTableRows(grid) {
  tableRowsEl.innerHTML = '';
  for (let n = 1; n <= 12; n++) {
    const tiers = grid.slice((n - 1) * 12, n * 12).map(s => TIER_VAL[s]);
    const min = Math.min(...tiers);
    const countSolid    = tiers.filter(t => t >= 2).length;
    const countFast     = tiers.filter(t => t >= 3).length;
    const countMastered = tiers.filter(t => t >= 4).length;

    const row = document.createElement('div');
    row.className = 'table-row';

    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = state.enabledTables.includes(n);
    cb.addEventListener('change', () => onTableToggle(n, cb.checked));
    row.appendChild(cb);

    const lbl = document.createElement('span');
    lbl.className = 'table-row-label';
    lbl.textContent = `${n}×`;
    row.appendChild(lbl);

    let milestoneClass = '', milestoneText = '', counterText = '';
    if (min >= 4) {
      milestoneClass = 'mastered'; milestoneText = 'Mastered';
    } else if (min >= 3) {
      milestoneClass = 'fast'; milestoneText = 'Fast';
      counterText = `${countMastered}/12 mastered`;
    } else if (min >= 2) {
      milestoneClass = 'solid'; milestoneText = 'Solid';
      counterText = `${countFast}/12 fast`;
    } else {
      counterText = `${countSolid}/12 solid`;
    }

    if (milestoneText) {
      const badge = document.createElement('span');
      badge.className = `table-milestone ${milestoneClass}`;
      badge.textContent = milestoneText;
      row.appendChild(badge);
    }

    if (counterText) {
      const counter = document.createElement('span');
      counter.className = 'table-counter';
      counter.textContent = counterText;
      row.appendChild(counter);
    }

    tableRowsEl.appendChild(row);
  }
}

async function onTableToggle(table, enabled) {
  if (enabled) {
    state.enabledTables = [...state.enabledTables, table].sort((a, b) => a - b);
  } else {
    state.enabledTables = state.enabledTables.filter(t => t !== table);
  }

  const res = await apiPost('/api/tables', { enabled: state.enabledTables });
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
  state.enabledTables = data.enabled_tables;
  updateStats();
  renderGrid(data.grid);
  renderTableRows(data.grid);
  displayProblem(data.problem);
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
  state.enabledTables = data.enabled_tables;
  updateStats();
  renderGrid(data.grid);
  renderTableRows(data.grid);
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
  state.enabledTables = data.enabled_tables;
  renderGrid(data.grid);
  renderTableRows(data.grid);

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
