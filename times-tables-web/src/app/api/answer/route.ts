import { auth } from "@/lib/auth";
import { prisma } from "@/lib/db";
import {
  calculateNewStats,
  selectNextProblem,
  shouldUnlockNextTable,
  TABLE_ORDER,
  isProblemUnlocked,
  isMastered,
} from "@/lib/spaced-rep";
import { NextResponse } from "next/server";

export async function POST(request: Request) {
  const session = await auth();

  if (!session?.user?.id) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const userId = session.user.id;
  const { problemA, problemB, userAnswer, responseSecs } = await request.json();

  const correctAnswer = problemA * problemB;
  const correct = userAnswer === correctAnswer;

  // Get current progress
  const current = await prisma.problemProgress.findUnique({
    where: {
      userId_problemA_problemB: { userId, problemA, problemB },
    },
  });

  if (!current) {
    return NextResponse.json({ error: "Problem not found" }, { status: 404 });
  }

  // Calculate new stats
  const newStats = calculateNewStats(current, correct, responseSecs);

  // Update progress
  await prisma.problemProgress.update({
    where: { id: current.id },
    data: newStats,
  });

  // Check if should unlock next table
  const user = await prisma.user.findUnique({
    where: { id: userId },
    select: { unlockedTables: true },
  });

  let unlockedTables = user?.unlockedTables ?? 1;

  const allProgress = await prisma.problemProgress.findMany({
    where: { userId },
  });

  if (shouldUnlockNextTable(allProgress, unlockedTables)) {
    unlockedTables += 1;
    await prisma.user.update({
      where: { id: userId },
      data: { unlockedTables },
    });
  }

  // Get next problem
  const lastProblem = { a: problemA, b: problemB };
  const nextProblem = selectNextProblem(allProgress, unlockedTables, lastProblem);

  // Calculate stats
  const unlockedProblems = allProgress.filter((p) =>
    isProblemUnlocked(p.problemA, p.problemB, unlockedTables)
  );
  const masteredCount = unlockedProblems.filter((p) => isMastered(p)).length;

  const stats = {
    mastered: masteredCount,
    unlocked: unlockedProblems.length,
    total: allProgress.reduce((sum, p) => sum + p.timesCorrect + p.timesWrong, 0),
    streak: 0,
    unlockedTables: TABLE_ORDER.slice(0, unlockedTables).join(", "),
    nextTable: unlockedTables < TABLE_ORDER.length ? TABLE_ORDER[unlockedTables] : null,
  };

  return NextResponse.json({ correct, nextProblem, stats });
}
