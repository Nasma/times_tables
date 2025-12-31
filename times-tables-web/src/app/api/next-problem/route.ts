import { auth } from "@/lib/auth";
import { prisma } from "@/lib/db";
import {
  generateAllProblems,
  selectNextProblem,
  TABLE_ORDER,
  isProblemUnlocked,
  isMastered,
} from "@/lib/spaced-rep";
import { NextResponse } from "next/server";

export async function GET() {
  const session = await auth();

  if (!session?.user?.id) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const userId = session.user.id;

  // Get user's unlocked tables
  const user = await prisma.user.findUnique({
    where: { id: userId },
    select: { unlockedTables: true },
  });

  const unlockedTables = user?.unlockedTables ?? 1;

  // Get or initialize progress for all problems
  let progress = await prisma.problemProgress.findMany({
    where: { userId },
  });

  // Initialize missing problems
  const allProblems = generateAllProblems();
  const existingKeys = new Set(progress.map((p) => `${p.problemA}x${p.problemB}`));

  const missingProblems = allProblems.filter(
    (p) => !existingKeys.has(`${p.a}x${p.b}`)
  );

  if (missingProblems.length > 0) {
    await prisma.problemProgress.createMany({
      data: missingProblems.map((p) => ({
        userId,
        problemA: p.a,
        problemB: p.b,
      })),
    });

    progress = await prisma.problemProgress.findMany({
      where: { userId },
    });
  }

  // Select next problem
  const nextProblem = selectNextProblem(progress, unlockedTables);

  // Calculate stats
  const unlockedProblems = progress.filter((p) =>
    isProblemUnlocked(p.problemA, p.problemB, unlockedTables)
  );
  const masteredCount = unlockedProblems.filter((p) => isMastered(p)).length;

  const stats = {
    mastered: masteredCount,
    unlocked: unlockedProblems.length,
    total: progress.reduce((sum, p) => sum + p.timesCorrect + p.timesWrong, 0),
    streak: 0, // Streak is tracked client-side per session
    unlockedTables: TABLE_ORDER.slice(0, unlockedTables).join(", "),
    nextTable: unlockedTables < TABLE_ORDER.length ? TABLE_ORDER[unlockedTables] : null,
  };

  return NextResponse.json({ problem: nextProblem, stats });
}
