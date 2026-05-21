'use strict';

// ── State ─────────────────────────────────────────────────────────────────────

const state = {
  problem: null,
  awaitingCorrection: false,
  correctAnswer: null,
  pendingNextProblem: null,
  problemStartMs: 0,
  role: 'student',
  mode: localStorage.getItem('mode') || 'times_tables',
  problems: {},   // "AxB" → { consecutiveCorrect, bestTier, consecutiveFastCorrect, timesCorrect, recentResponses[] }
  lastRaceMs: null,
  bestRaceMs: null,
  race: {
    active: false,
    queue: [],
    currentProblem: null,
    totalQuestions: 0,
    startMs: 0,
    timerInterval: null,
  },
};

let selectedRole = 'student';

// ── DOM refs ──────────────────────────────────────────────────────────────────

const $ = id => document.getElementById(id);

const authView        = $('auth-view');
const practiceView    = $('practice-view');
const teacherView     = $('teacher-view');
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
const resetBtn        = $('reset-btn');
const resetConfirm    = $('reset-confirm');
const resetYes        = $('reset-yes');
const resetCancel     = $('reset-cancel');
const logoutBtn       = $('logout-btn');
const googleAuth      = $('google-auth');
const googleBtn       = $('google-btn');
const totalTimeRow    = $('total-time-row');
const totalTimeValue  = $('total-time-value');
const raceInfoEl         = $('race-info');
const raceTimerEl        = $('race-timer');
const raceProgressEl     = $('race-progress-text');
const raceCancelBtn      = $('race-cancel-btn');
const startRaceBtn       = $('start-race-btn');
const raceLastTimeEl     = $('race-last-time');
const raceBestTimeEl     = $('race-best-time');
const modeToggle         = $('mode-toggle');
const practiceHeading    = $('practice-heading');
const roleTabs        = $('role-tabs');
const teacherLogoutBtn = $('teacher-logout-btn');
const copyInviteBtn   = $('copy-invite-btn');
const inviteUrlEl     = $('invite-url');
const qrContainer     = $('qr-container');
const studentListEl   = $('student-list');
const noStudentsMsg   = $('no-students-msg');

// ── API helpers ───────────────────────────────────────────────────────────────

function getToken() { return localStorage.getItem('token'); }
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

// ── Local storage helpers ─────────────────────────────────────────────────────

function cacheKey() { return `tt_problems_${state.mode}`; }
const QUEUE_KEY = 'tt_answer_queue';

function saveProblems() {
  try { localStorage.setItem(cacheKey(), JSON.stringify(state.problems)); } catch (_) {}
}
function loadCachedProblems() {
  try { return JSON.parse(localStorage.getItem(cacheKey())); } catch (_) { return null; }
}
function loadQueue() {
  try { return JSON.parse(localStorage.getItem(QUEUE_KEY) || '[]'); } catch (_) { return []; }
}
function saveQueue(q) {
  try { localStorage.setItem(QUEUE_KEY, JSON.stringify(q)); } catch (_) {}
}

// ── Problem key ───────────────────────────────────────────────────────────────

function pkey(a, b) { return `${a}x${b}`; }

// ── Algorithms (ported from Rust core) ───────────────────────────────────────

// Weighted average of recent correct times, discarding the worst outlier.
// most-recent-first input; returns estimated seconds.
function estimateTime(recentCorrect) {
  const MAX = 10;
  if (!recentCorrect.length) return MAX;
  if (recentCorrect.length === 1) return Math.min(MAX, recentCorrect[0]);
  const c = recentCorrect.slice(0, 5);
  const wi = c.reduce((mi, v, i, a) => v > a[mi] ? i : mi, 0);
  const r = c.filter((_, i) => i !== wi);
  const k = r.length;
  let ws = 0, tw = 0;
  r.forEach((t, i) => { const w = k - i; ws += w * t; tw += w; });
  return Math.min(MAX, ws / tw);
}

// Penalty-adjusted time score (avg correct / fraction correct) over last 5.
function correctTime(ps) {
  const MAX = 10;
  const last5 = ps.recentResponses.slice(-5);
  if (!last5.length) return MAX;
  const correct = last5.filter(r => r.correct);
  if (!correct.length) return MAX;
  const avg = correct.reduce((s, r) => s + r.elapsedSecs, 0) / correct.length;
  return Math.min(MAX, avg / (correct.length / last5.length));
}

function pickProblem(last) {
  const dim = state.mode === 'times_tables' ? 12 : 10;
  const entries = [];
  for (let a = 1; a <= dim; a++) {
    for (let b = 1; b <= dim; b++) {
      const ps = state.problems[pkey(a, b)];
      if (!ps) continue;
      const last5 = ps.recentResponses.slice(-5);
      const errorsInLast5 = last5.filter(r => !r.correct).length;
      const recentCorrect = ps.recentResponses.slice().reverse().filter(r => r.correct).map(r => r.elapsedSecs);
      const lastAskedAt = ps.recentResponses.length ? ps.recentResponses[ps.recentResponses.length - 1].answeredAtSecs : null;
      entries.push({ a, b, errorsInLast5, estimatedTime: estimateTime(recentCorrect), lastAskedAt });
    }
  }
  if (!entries.length) return { a: 1, b: 1 };

  entries.sort((x, y) => (y.errorsInLast5 - x.errorsInLast5) || (y.estimatedTime - x.estimatedTime));

  let candidates = entries.slice(0, 9).map(e => ({ a: e.a, b: e.b }));
  const oldest = entries.slice().sort((x, y) => {
    if (x.lastAskedAt === null) return -1;
    if (y.lastAskedAt === null) return 1;
    return x.lastAskedAt - y.lastAskedAt;
  })[0];
  if (!candidates.some(c => c.a === oldest.a && c.b === oldest.b)) candidates.push({ a: oldest.a, b: oldest.b });

  let pool = candidates.length > 1
    ? candidates.filter(c => !last || c.a !== last.a || c.b !== last.b)
    : candidates;
  if (!pool.length) pool = candidates;

  return pool[Math.floor(Math.random() * pool.length)];
}

function recordAnswer(a, b, correct, elapsedSecs) {
  const ps = state.problems[pkey(a, b)];
  if (!ps) return;
  const isFastStreak = elapsedSecs < 2.0;
  const isFastTier   = elapsedSecs < 3.0;
  if (correct) {
    ps.timesCorrect++;
    ps.consecutiveCorrect++;
    ps.consecutiveFastCorrect = isFastStreak ? ps.consecutiveFastCorrect + 1 : 0;
  } else {
    ps.consecutiveCorrect = 0;
    ps.consecutiveFastCorrect = 0;
  }
  if (ps.timesCorrect > 0)            ps.bestTier = Math.max(ps.bestTier, 1);
  if (ps.consecutiveCorrect >= 3)     ps.bestTier = Math.max(ps.bestTier, 2);
  if (ps.bestTier >= 2 && correct && isFastTier) ps.bestTier = Math.max(ps.bestTier, 3);
  if (ps.consecutiveFastCorrect >= 3) ps.bestTier = Math.max(ps.bestTier, 4);
  ps.recentResponses.push({ correct, elapsedSecs, answeredAtSecs: Math.floor(Date.now() / 1000) });
  if (ps.recentResponses.length > 100) ps.recentResponses.shift();
}

function correctAnswer(a, b) {
  if (state.mode === 'addition')    return a + b;
  if (state.mode === 'subtraction') return b;
  return a * b;
}

// ── Derived state ─────────────────────────────────────────────────────────────


function computeMastered() {
  return Object.values(state.problems).filter(ps => ps.consecutiveCorrect >= 3).length;
}

function computeTotalTime() {
  return Object.values(state.problems).reduce((s, ps) => s + correctTime(ps), 0);
}

function computeCorrectInWindow(seconds) {
  const cutoff = Math.floor(Date.now() / 1000) - seconds;
  return Object.values(state.problems).reduce((s, ps) =>
    s + ps.recentResponses.filter(r => r.correct && r.answeredAtSecs >= cutoff).length, 0);
}

function computeTotalCorrect() {
  return Object.values(state.problems).reduce((s, ps) => s + ps.timesCorrect, 0);
}

// ── Render helpers ────────────────────────────────────────────────────────────

function renderStats() {
  $('stat-10m').textContent  = computeCorrectInWindow(600);
  $('stat-day').textContent  = computeCorrectInWindow(86400);
  $('stat-week').textContent = computeCorrectInWindow(604800);
  $('stat-total').textContent = computeTotalCorrect();
}



function renderTotalTime(totalTime) {
  const secs = Math.round(totalTime);
  const m = Math.floor(secs / 60), s = secs % 60;
  totalTimeValue.textContent = m > 0 ? `${m}m ${s}s` : `${s}s`;
}

function renderLastRaceTime(lastMs, bestMs) {
  raceLastTimeEl.textContent = lastMs != null ? formatRaceTime(lastMs) : '—';
  raceBestTimeEl.textContent = bestMs != null ? formatRaceTime(bestMs) : '—';
}

function refreshUI() {
  renderTotalTime(computeTotalTime());
  renderStats();
}

// ── View helpers ──────────────────────────────────────────────────────────────

function showAuth() {
  authView.classList.remove('hidden');
  practiceView.classList.add('hidden');
  teacherView.classList.add('hidden');
  usernameInput.focus();
}

function updateModeUI() {
  modeToggle.querySelectorAll('input[name="mode"]').forEach(r => { r.checked = r.value === state.mode; });
  const headings = {
    addition:     'Efficient Addition Practice',
    subtraction:  'Efficient Subtraction Practice',
    times_tables: 'Efficient Times Tables Practice',
  };
  practiceHeading.textContent = headings[state.mode] || headings.times_tables;
}

function showPractice() {
  authView.classList.add('hidden');
  practiceView.classList.remove('hidden');
  teacherView.classList.add('hidden');
  updateModeUI();
  answerInput.focus();
}

function showTeacher() {
  authView.classList.add('hidden');
  practiceView.classList.add('hidden');
  teacherView.classList.remove('hidden');
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

function showCorrectionMode(userAnswer, correct, nextProblem) {
  state.awaitingCorrection = true;
  state.correctAnswer = correct;
  state.pendingNextProblem = nextProblem;
  incorrectMsg.textContent = `${userAnswer} is wrong. Type the answer: ${correct}`;
  normalMode.classList.add('hidden');
  correctionMode.classList.remove('hidden');
  correctionInput.value = '';
  correctionInput.focus();
}

function displayProblem(problem) {
  state.problem = problem;
  state.problemStartMs = Date.now();
  if (state.mode === 'subtraction') {
    problemText.textContent = `${problem.a + problem.b} − ${problem.a} = ?`;
  } else {
    const op = state.mode === 'addition' ? '+' : '×';
    problemText.textContent = `${problem.a} ${op} ${problem.b} = ?`;
  }
  showNormalMode();
}

// ── Race mode ─────────────────────────────────────────────────────────────────

function formatRaceTime(ms) {
  const tenths = Math.floor(ms / 100) % 10;
  const secs   = Math.floor(ms / 1000) % 60;
  const mins   = Math.floor(ms / 60000);
  return mins > 0
    ? `${mins}:${String(secs).padStart(2, '0')}.${tenths}`
    : `${secs}.${tenths}`;
}

function buildRaceQueue() {
  const dim = state.mode === 'times_tables' ? 12 : 10;
  const problems = [];
  for (let a = 1; a <= dim; a++)
    for (let b = 1; b <= dim; b++)
      problems.push({ a, b });
  for (let i = problems.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [problems[i], problems[j]] = [problems[j], problems[i]];
  }
  return problems;
}

function startRace() {
  state.race.queue          = buildRaceQueue();
  state.race.totalQuestions = state.race.queue.length;
  state.race.active         = true;
  state.race.startMs        = Date.now();
  state.awaitingCorrection  = false;
  document.body.classList.add('race-active');
  practiceHeading.textContent = 'Race Mode';
  raceInfoEl.classList.remove('hidden');
  startRaceBtn.classList.add('hidden');
  raceCancelBtn.classList.remove('hidden');
  state.race.timerInterval = setInterval(() => {
    const elapsed = Date.now() - state.race.startMs;
    raceTimerEl.textContent = formatRaceTime(elapsed);
    const done = state.race.totalQuestions - state.race.queue.length;
    raceProgressEl.textContent = `${done} / ${state.race.totalQuestions}`;
  }, 100);
  nextRaceProblem();
}

function cancelRace() {
  clearInterval(state.race.timerInterval);
  state.race.active = false;
  state.race.queue  = [];
  document.body.classList.remove('race-active');
  raceInfoEl.classList.add('hidden');
  startRaceBtn.classList.remove('hidden');
  raceCancelBtn.classList.add('hidden');
  updateModeUI();
  loadState();
}

const pbModal = $('pb-modal');
const pbTimeEl = $('pb-time');
pbModal.addEventListener('click', () => pbModal.classList.add('hidden'));

async function finishRace() {
  const elapsed = Date.now() - state.race.startMs;
  clearInterval(state.race.timerInterval);
  state.race.active = false;
  state.race.queue  = [];
  document.body.classList.remove('race-active');
  raceInfoEl.classList.add('hidden');
  startRaceBtn.classList.remove('hidden');
  raceCancelBtn.classList.add('hidden');
  updateModeUI();
  try {
    const res = await apiPost('/api/race', { mode: state.mode, elapsed_ms: elapsed });
    if (res.ok) {
      const data = await res.json();
      const isPB = data.last_race_ms != null && data.last_race_ms === data.best_race_ms;
      state.lastRaceMs = data.last_race_ms;
      state.bestRaceMs = data.best_race_ms;
      renderLastRaceTime(data.last_race_ms, data.best_race_ms);
      if (isPB) {
        pbTimeEl.textContent = formatRaceTime(data.best_race_ms);
        pbModal.classList.remove('hidden');
      }
    }
  } catch (_) {}
  refreshUI();
  state.problem = pickProblem(null);
  displayProblem(state.problem);
}

function nextRaceProblem() {
  if (state.race.queue.length === 0) { finishRace(); return; }
  state.race.currentProblem = state.race.queue.shift();
  displayProblem(state.race.currentProblem);
}

// ── Sync status ───────────────────────────────────────────────────────────────

const syncEl = $('sync-status');
let syncTimer = null;

function setSynced() {
  clearTimeout(syncTimer);
  syncTimer = null;
  syncEl.textContent = 'synced';
  syncEl.className = 'sync-status synced';
}

function setUnsynced() {
  syncEl.textContent = 'not synced';
  syncEl.className = 'sync-status unsynced';
}

function markPending() {
  clearTimeout(syncTimer);
  syncTimer = setTimeout(setUnsynced, 2000);
}

// ── Offline queue ─────────────────────────────────────────────────────────────

async function flushQueue() {
  let q = loadQueue();
  while (q.length) {
    try {
      const res = await apiPost('/api/answer', q[0]);
      if (!res.ok) { setUnsynced(); break; }
      q.shift();
      saveQueue(q);
    } catch (_) { setUnsynced(); break; }
  }
  if (!loadQueue().length) setSynced();
}

window.addEventListener('online', flushQueue);

// Post answer to server in background; queue on failure.
function postAnswer(a, b, answer, elapsedSecs) {
  markPending();
  const payload = { a, b, answer, elapsed_secs: elapsedSecs, mode: state.mode, answered_at_secs: Math.floor(Date.now() / 1000) };
  apiPost('/api/answer', payload).then(res => {
    if (res.ok) { setSynced(); flushQueue(); }
    else        { setUnsynced(); const q = loadQueue(); q.push(payload); saveQueue(q); }
  }).catch(() => { setUnsynced(); const q = loadQueue(); q.push(payload); saveQueue(q); });
}

// ── Load state ────────────────────────────────────────────────────────────────

function initProblems(serverProblems) {
  state.problems = {};
  for (const [key, sp] of Object.entries(serverProblems)) {
    state.problems[key] = {
      consecutiveCorrect:      sp.consecutive_correct,
      bestTier:                sp.best_tier,
      consecutiveFastCorrect:  sp.consecutive_fast_correct,
      timesCorrect:            sp.times_correct,
      recentResponses: sp.recent_responses.map(r => ({
        correct: r.correct, elapsedSecs: r.elapsed_secs, answeredAtSecs: r.answered_at_secs,
      })),
    };
  }
}

async function loadState() {
  let serverData = null;
  try {
    const res = await apiGet(`/api/state?mode=${state.mode}`);
    if (res.status === 401) { localStorage.removeItem('token'); showAuth(); return; }
    if (res.ok) serverData = await res.json();
  } catch (_) {}

  if (serverData) {
    state.role = serverData.role || 'student';
    initProblems(serverData.problems);
    saveProblems();
    state.lastRaceMs = serverData.last_race_ms ?? null;
    state.bestRaceMs = serverData.best_race_ms ?? null;
    flushQueue();
  } else {
    setUnsynced();
    const cached = loadCachedProblems();
    if (!cached) { showAuth(); return; }
    state.problems = cached;
  }

  if (state.role === 'teacher') {
    showTeacher();
    await loadTeacherData();
  } else {
    refreshUI();
    renderLastRaceTime(state.lastRaceMs, state.bestRaceMs);
    state.problem = pickProblem(null);
    displayProblem(state.problem);
    showPractice();
  }
}

// ── Auth ──────────────────────────────────────────────────────────────────────

function timeAgo(secs) {
  if (secs == null) return 'never';
  const diff = Math.floor(Date.now() / 1000) - secs;
  if (diff < 60)    return `${diff}s ago`;
  if (diff < 3600)  return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

function windowStat(correct, total) {
  if (total === 0) return '<span class="w-none">—</span>';
  const cls = correct === total ? 'w-perfect' : correct > 0 ? 'w-partial' : 'w-zero';
  return `<span class="${cls}">${correct}/${total}</span>`;
}

async function loadTeacherData() {
  const [inviteRes, studentsRes] = await Promise.all([
    apiGet('/api/teacher/invite'),
    apiGet('/api/teacher/students'),
  ]);
  if (inviteRes.ok) {
    const inv = await inviteRes.json();
    inviteUrlEl.textContent = inv.join_url;
    qrContainer.innerHTML = inv.qr_svg;
  }
  if (studentsRes.ok) {
    const data = await studentsRes.json();
    studentListEl.innerHTML = '';
    if (data.students.length === 0) {
      noStudentsMsg.classList.remove('hidden');
    } else {
      noStudentsMsg.classList.add('hidden');
      for (const s of data.students) {
        const row = document.createElement('div');
        row.className = 'student-row';
        row.innerHTML = `
          <div class="student-row-main">
            <span class="student-name">${s.username}</span>
            <span class="student-last">${timeAgo(s.last_answered_secs)}</span>
            <span class="student-progress">${s.mastered}/${s.total} mastered</span>
          </div>
          <div class="student-row-windows">
            <span class="window-label">5m</span>${windowStat(s.correct_5m, s.total_5m)}
            <span class="window-label">10m</span>${windowStat(s.correct_10m, s.total_10m)}
            <span class="window-label">30m</span>${windowStat(s.correct_30m, s.total_30m)}
            <span class="window-label">day</span>${windowStat(s.correct_day, s.total_day)}
          </div>`;
        studentListEl.appendChild(row);
      }
    }
  }
}

async function doAuth(endpoint) {
  const username = usernameInput.value.trim();
  const password = passwordInput.value;
  if (!username || !password) { setAuthError('Please enter username and password.'); return; }
  setAuthError('');
  const body = endpoint.includes('register')
    ? { username, password, role: selectedRole }
    : { username, password };
  const res = await apiPost(endpoint, body);
  if (res.ok) {
    const data = await res.json();
    localStorage.setItem('token', data.token);
    passwordInput.value = '';
    await loadState();
  } else {
    setAuthError(await res.text() || 'Something went wrong.');
  }
}

loginBtn.addEventListener('click', () => doAuth('/api/login'));
registerBtn.addEventListener('click', () => doAuth('/api/register'));

roleTabs.addEventListener('click', e => {
  const tab = e.target.closest('.role-tab');
  if (!tab) return;
  selectedRole = tab.dataset.role;
  roleTabs.querySelectorAll('.role-tab').forEach(t => t.classList.toggle('active', t === tab));
});

googleBtn.addEventListener('click', () => { window.location.href = `/api/auth/google?role=${selectedRole}`; });

[usernameInput, passwordInput].forEach(el => {
  el.addEventListener('keydown', e => { if (e.key === 'Enter') doAuth('/api/login'); });
});

// ── Answer submission ─────────────────────────────────────────────────────────

async function submitAnswer() {
  if (!state.problem) return;
  const raw = answerInput.value.trim();
  if (raw === '') return;
  const answer = parseInt(raw, 10);
  if (isNaN(answer)) { answerInput.value = ''; return; }

  const { a, b } = state.problem;
  const elapsedSecs = (Date.now() - state.problemStartMs) / 1000;
  const correct = answer === correctAnswer(a, b);

  recordAnswer(a, b, correct, elapsedSecs);
  refreshUI();
  saveProblems();
  postAnswer(a, b, answer, elapsedSecs);

  if (state.race.active) {
    if (correct) {
      nextRaceProblem();
    } else {
      showCorrectionMode(answer, correctAnswer(a, b), null);
    }
  } else {
    const next = pickProblem({ a, b });
    if (correct) {
      displayProblem(next);
    } else {
      showCorrectionMode(answer, correctAnswer(a, b), next);
    }
  }
}

async function checkCorrection() {
  const raw = correctionInput.value.trim();
  if (raw === '') return;
  const typed = parseInt(raw, 10);
  if (typed !== state.correctAnswer) return;

  if (state.race.active) {
    const pos = Math.floor(Math.random() * (state.race.queue.length + 1));
    state.race.queue.splice(pos, 0, state.race.currentProblem);
    const { a, b } = state.race.currentProblem;
    const elapsedSecs = (Date.now() - state.problemStartMs) / 1000;
    recordAnswer(a, b, true, elapsedSecs);
    refreshUI();
    saveProblems();
    postAnswer(a, b, typed, elapsedSecs);
    nextRaceProblem();
  } else {
    displayProblem(state.pendingNextProblem);
  }
}

submitBtn.addEventListener('click', submitAnswer);

function enforceNumeric(e) {
  if (e.data !== null && !/^\d+$/.test(e.data)) e.preventDefault();
}
answerInput.addEventListener('beforeinput', enforceNumeric);
correctionInput.addEventListener('beforeinput', enforceNumeric);
answerInput.addEventListener('keydown', e => { if (e.key === 'Enter') submitAnswer(); });
correctionInput.addEventListener('keydown', e => { if (e.key === 'Enter') checkCorrection(); });

startRaceBtn.addEventListener('click', startRace);
raceCancelBtn.addEventListener('click', cancelRace);

// ── Mode toggle ───────────────────────────────────────────────────────────────

modeToggle.addEventListener('change', async e => {
  const radio = e.target.closest('input[name="mode"]');
  if (!radio) return;
  if (state.race.active) {
    clearInterval(state.race.timerInterval);
    state.race.active = false;
    state.race.queue  = [];
    document.body.classList.remove('race-active');
    raceInfoEl.classList.add('hidden');
    startRaceBtn.classList.remove('hidden');
    raceCancelBtn.classList.add('hidden');
  }
  state.mode = radio.value;
  localStorage.setItem('mode', state.mode);
  state.awaitingCorrection = false;
  updateModeUI();
  await loadState();
});

// ── Reset ─────────────────────────────────────────────────────────────────────

resetBtn.addEventListener('click', () => { resetConfirm.classList.remove('hidden'); resetBtn.classList.add('hidden'); });
resetCancel.addEventListener('click', () => { resetConfirm.classList.add('hidden'); resetBtn.classList.remove('hidden'); });

resetYes.addEventListener('click', async () => {
  resetConfirm.classList.add('hidden');
  resetBtn.classList.remove('hidden');
  const res = await apiPost('/api/reset', { mode: state.mode });
  if (res.status === 401) { localStorage.removeItem('token'); showAuth(); return; }
  if (!res.ok) return;
  localStorage.removeItem(cacheKey());
  await loadState();
});

// ── Logout ────────────────────────────────────────────────────────────────────

logoutBtn.addEventListener('click', async () => {
  await apiPost('/api/logout', {});
  localStorage.removeItem('token');
  showAuth();
});

teacherLogoutBtn.addEventListener('click', async () => {
  await apiPost('/api/logout', {});
  localStorage.removeItem('token');
  showAuth();
});

copyInviteBtn.addEventListener('click', () => {
  const url = inviteUrlEl.textContent;
  if (!url) return;
  navigator.clipboard.writeText(url).then(() => {
    copyInviteBtn.textContent = 'Copied!';
    setTimeout(() => { copyInviteBtn.textContent = 'Copy'; }, 2000);
  });
});

// ── Boot ──────────────────────────────────────────────────────────────────────

let oauthError = null;
const hash = window.location.hash;
if (hash.startsWith('#token=')) {
  localStorage.setItem('token', hash.slice('#token='.length));
  history.replaceState(null, '', window.location.pathname);
} else if (hash.startsWith('#auth_error=')) {
  oauthError = decodeURIComponent(hash.slice('#auth_error='.length));
  history.replaceState(null, '', window.location.pathname);
} else if (hash.startsWith('#oauth_choice=')) {
  const p = new URLSearchParams(hash.slice(1));
  history.replaceState(null, '', window.location.pathname);
  const pendingToken   = p.get('oauth_choice');
  const existingRole   = p.get('existing_role');
  const requestedRole  = p.get('requested_role');
  $('oauth-choice-msg').textContent =
    `You already have a ${existingRole} account linked to this Google account.`;
  $('oauth-create-new').textContent   = `Create a new ${requestedRole} account`;
  $('oauth-use-existing').textContent = `Sign in with my ${existingRole} account`;
  $('oauth-choice-modal').classList.remove('hidden');

  async function completeOAuth(action) {
    $('oauth-choice-modal').classList.add('hidden');
    const res = await fetch('/api/auth/google/complete', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ token: pendingToken, action }),
    });
    if (res.ok) {
      const data = await res.json();
      localStorage.setItem('token', data.token);
      window.location.reload();
    } else {
      showAuth();
      setAuthError('Sign-in failed. Please try again.');
    }
  }

  $('oauth-create-new').addEventListener('click',  () => completeOAuth('create_new'));
  $('oauth-use-existing').addEventListener('click', () => completeOAuth('use_existing'));
}

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
