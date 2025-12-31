import { auth } from "@/lib/auth";
import { prisma } from "@/lib/db";
import { NextResponse } from "next/server";

export async function POST() {
  const session = await auth();

  if (!session?.user?.id) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const userId = session.user.id;

  // Delete all progress
  await prisma.problemProgress.deleteMany({
    where: { userId },
  });

  // Reset unlocked tables
  await prisma.user.update({
    where: { id: userId },
    data: { unlockedTables: 1 },
  });

  return NextResponse.json({ success: true });
}
