'use strict';

// ── State ─────────────────────────────────────────────────────────────────────

const state = {
  problem: null,           // { a, b }
  awaitingCorrection: false,
  correctAnswer: null,
  pendingNextProblem: null, // next problem to show after correction
  problemStartMs: 0,
  enabledTables: [1,2,3,4,5,6,7,8,9,10,11,12],
  role: 'student',
  mode: localStorage.getItem('mode') || 'times_tables',
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
const progressGrid    = $('progress-grid');
const tableRowsEl     = $('table-rows');
const selectAllTablesBtn = $('select-all-tables');
const clearAllTablesBtn  = $('clear-all-tables');
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
  teacherView.classList.add('hidden');
  usernameInput.focus();
}

function updateModeUI() {
  modeToggle.querySelectorAll('input[name="mode"]').forEach(r => {
    r.checked = r.value === state.mode;
  });
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
  if (state.mode === 'subtraction') {
    problemText.textContent = `${problem.a + problem.b} − ${problem.a} = ?`;
  } else {
    const op = state.mode === 'addition' ? '+' : '×';
    problemText.textContent = `${problem.a} ${op} ${problem.b} = ?`;
  }
  showNormalMode();
}


const TIER_VAL = { not_started: 0, learning: 1, solid: 2, fast: 3, mastered: 4 };

function renderTableRows(grid) {
  const dim = state.mode === 'times_tables' ? 12 : 10;
  const opSymbol = state.mode === 'times_tables' ? '×' : (state.mode === 'subtraction' ? '−' : '+');

  tableRowsEl.innerHTML = '';
  for (let n = 1; n <= dim; n++) {
    const tiers = grid.slice((n - 1) * dim, n * dim).map(s => TIER_VAL[s]);
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
    lbl.textContent = `${n}${opSymbol}`;
    row.appendChild(lbl);

    const milestones = [];
    let counterText = '';
    if (min >= 2) milestones.push('solid');
    if (min >= 3) milestones.push('fast');
    if (min >= 4) milestones.push('mastered');

    if (min < 2)      counterText = `${countSolid}/${dim} solid`;
    else if (min < 3) counterText = `${countFast}/${dim} fast`;
    else if (min < 4) counterText = `${countMastered}/${dim} mastered`;

    for (const m of milestones) {
      const badge = document.createElement('span');
      badge.className = `table-milestone ${m}`;
      badge.textContent = m.charAt(0).toUpperCase() + m.slice(1);
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

  const res = await apiPost('/api/tables', { enabled: state.enabledTables, mode: state.mode });
  if (res.status === 401) {
    localStorage.removeItem('token');
    showAuth();
    return;
  }
  if (!res.ok) return;

  const data = await res.json();
  state.enabledTables = data.enabled_tables;
  renderGrid(data.grid);
  renderTableRows(data.grid);
  displayProblem(data.problem);
}

function renderGrid(grid) {
  const dim = state.mode === 'times_tables' ? 12 : 10;
  progressGrid.innerHTML = '';
  progressGrid.style.setProperty('--grid-cols', dim);
  grid.forEach((status, i) => {
    const a = Math.floor(i / dim) + 1;
    const b = (i % dim) + 1;
    const cell = document.createElement('div');
    cell.className = `grid-cell ${status}`;
    if (state.mode === 'subtraction') {
      cell.title = `${a + b} − ${a} = ${b}`;
    } else if (state.mode === 'addition') {
      cell.title = `${a} + ${b} = ${a + b}`;
    } else {
      cell.title = `${a} × ${b} = ${a * b}`;
    }
    progressGrid.appendChild(cell);
  });
}

// ── Auth ──────────────────────────────────────────────────────────────────────

async function loadState() {
  const res = await apiGet(`/api/state?mode=${state.mode}`);
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
  state.role = data.role || 'student';

  if (state.role === 'teacher') {
    showTeacher();
    await loadTeacherData();
  } else {
    state.enabledTables = data.enabled_tables;
    renderGrid(data.grid);
    renderTableRows(data.grid);
    displayProblem(data.problem);
    showPractice();
  }
}

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
  if (!username || !password) {
    setAuthError('Please enter username and password.');
    return;
  }
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
    const msg = await res.text();
    setAuthError(msg || 'Something went wrong.');
  }
}

loginBtn.addEventListener('click', () => doAuth('/api/login'));
registerBtn.addEventListener('click', () => doAuth('/api/register'));

// Role tabs
roleTabs.addEventListener('click', e => {
  const tab = e.target.closest('.role-tab');
  if (!tab) return;
  selectedRole = tab.dataset.role;
  roleTabs.querySelectorAll('.role-tab').forEach(t => t.classList.toggle('active', t === tab));
});

googleBtn.addEventListener('click', () => {
  window.location.href = `/api/auth/google?role=${selectedRole}`;
});

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
    mode: state.mode,
  });

  if (res.status === 401) {
    localStorage.removeItem('token');
    showAuth();
    return;
  }

  if (!res.ok) return;

  const data = await res.json();
  state.enabledTables = data.enabled_tables;
  renderGrid(data.grid);
  renderTableRows(data.grid);

  if (data.correct) {
    displayProblem(data.next_problem);
  } else {
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

function enforceNumeric(e) {
  if (e.data !== null && !/^\d+$/.test(e.data)) e.preventDefault();
}

answerInput.addEventListener('beforeinput', enforceNumeric);
correctionInput.addEventListener('beforeinput', enforceNumeric);

answerInput.addEventListener('keydown', e => {
  if (e.key === 'Enter') submitAnswer();
});

correctionInput.addEventListener('keydown', e => {
  if (e.key === 'Enter') checkCorrection();
});

// ── Table bulk actions ────────────────────────────────────────────────────────

selectAllTablesBtn.addEventListener('click', async () => {
  const all = state.mode === 'times_tables'
    ? [1,2,3,4,5,6,7,8,9,10,11,12]
    : [1,2,3,4,5,6,7,8,9,10];
  const res = await apiPost('/api/tables', { enabled: all, mode: state.mode });
  if (!res.ok) return;
  const data = await res.json();
  state.enabledTables = data.enabled_tables;
  renderGrid(data.grid);
  renderTableRows(data.grid);
});

clearAllTablesBtn.addEventListener('click', async () => {
  const res = await apiPost('/api/tables', { enabled: [], mode: state.mode });
  if (!res.ok) return;
  const data = await res.json();
  state.enabledTables = data.enabled_tables;
  renderGrid(data.grid);
  renderTableRows(data.grid);
});

// ── Mode toggle ───────────────────────────────────────────────────────────────

modeToggle.addEventListener('change', async e => {
  const radio = e.target.closest('input[name="mode"]');
  if (!radio) return;
  state.mode = radio.value;
  localStorage.setItem('mode', state.mode);
  state.awaitingCorrection = false;
  updateModeUI();
  await loadState();
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

// Handle token/error/choice passed back from OAuth redirect via URL hash
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
  $('oauth-create-new').textContent  = `Create a new ${requestedRole} account`;
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
