export const TABLE_ORDER = [1, 10, 5, 11, 2, 3, 9, 4, 6, 7, 8, 12] as const;

export interface Problem {
  a: number;
  b: number;
}

export interface ProblemStats {
  visitorId: string;
  visitorType: "user" | "anonymous";
  problemA: number;
  problemB: number;
  easeFactor: number;
  intervalDays: number;
  nextReview: Date;
  timesCorrect: number;
  timesWrong: number;
  consecutiveCorrect: number;
}

export function generateAllProblems(): Problem[] {
  const problems: Problem[] = [];
  for (let a = 1; a <= 12; a++) {
    for (let b = 1; b <= 12; b++) {
      problems.push({ a, b });
    }
  }
  return problems;
}

export function problemKey(a: number, b: number): string {
  return `${a}x${b}`;
}

export function getUnlockedTableSet(unlockedTables: number): Set<number> {
  return new Set(TABLE_ORDER.slice(0, unlockedTables));
}

export function isProblemUnlocked(
  a: number,
  b: number,
  unlockedTables: number
): boolean {
  const unlocked = getUnlockedTableSet(unlockedTables);
  return unlocked.has(a) && unlocked.has(b);
}

export function isMastered(stats: {
  consecutiveCorrect: number;
  easeFactor: number;
}): boolean {
  return stats.consecutiveCorrect >= 3 && stats.easeFactor >= 2.0;
}

export function calculateNewStats(
  current: {
    easeFactor: number;
    intervalDays: number;
    timesCorrect: number;
    timesWrong: number;
    consecutiveCorrect: number;
  },
  correct: boolean,
  responseSecs: number
): {
  easeFactor: number;
  intervalDays: number;
  nextReview: Date;
  timesCorrect: number;
  timesWrong: number;
  consecutiveCorrect: number;
} {
  let { easeFactor, intervalDays, timesCorrect, timesWrong, consecutiveCorrect } =
    current;

  if (correct) {
    timesCorrect += 1;
    consecutiveCorrect += 1;

    if (intervalDays === 0) {
      intervalDays = 1;
    } else if (intervalDays < 1) {
      intervalDays = 1;
    } else {
      intervalDays *= easeFactor;
    }

    // Adjust ease factor based on response time
    const easeBonus =
      responseSecs < 3 ? 0.15 : responseSecs <= 8 ? 0.1 : 0.05;
    easeFactor = Math.min(3.0, easeFactor + easeBonus);
  } else {
    timesWrong += 1;
    consecutiveCorrect = 0;
    intervalDays = 0;
    easeFactor = Math.max(1.3, easeFactor - 0.2);
  }

  const nextReview = new Date(Date.now() + intervalDays * 86400 * 1000);

  return {
    easeFactor,
    intervalDays,
    nextReview,
    timesCorrect,
    timesWrong,
    consecutiveCorrect,
  };
}

export function shouldUnlockNextTable(
  progressList: Array<{
    problemA: number;
    problemB: number;
    consecutiveCorrect: number;
    easeFactor: number;
  }>,
  unlockedTables: number
): boolean {
  if (unlockedTables >= TABLE_ORDER.length) return false;

  const unlockedProblems = progressList.filter((p) =>
    isProblemUnlocked(p.problemA, p.problemB, unlockedTables)
  );

  if (unlockedProblems.length === 0) return false;

  const masteredCount = unlockedProblems.filter((p) => isMastered(p)).length;
  return masteredCount >= (unlockedProblems.length * 3) / 4;
}

export function selectNextProblem(
  progressList: Array<{
    problemA: number;
    problemB: number;
    easeFactor: number;
    nextReview: Date;
  }>,
  unlockedTables: number,
  lastProblem?: Problem
): Problem | null {
  const now = new Date();

  // Filter to unlocked, due problems (excluding last problem)
  let candidates = progressList.filter(
    (p) =>
      isProblemUnlocked(p.problemA, p.problemB, unlockedTables) &&
      new Date(p.nextReview) <= now &&
      !(lastProblem && p.problemA === lastProblem.a && p.problemB === lastProblem.b)
  );

  // If no due problems, get any unlocked problem for extra practice
  if (candidates.length === 0) {
    candidates = progressList.filter(
      (p) =>
        isProblemUnlocked(p.problemA, p.problemB, unlockedTables) &&
        !(lastProblem && p.problemA === lastProblem.a && p.problemB === lastProblem.b)
    );
  }

  if (candidates.length === 0) return null;

  // Sort by ease factor (lowest first = hardest)
  candidates.sort((a, b) => a.easeFactor - b.easeFactor);

  const selected = candidates[0];
  return { a: selected.problemA, b: selected.problemB };
}
