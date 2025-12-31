"use client";

import { useEffect, useState, useCallback } from "react";

interface Problem {
  a: number;
  b: number;
}

interface Stats {
  mastered: number;
  unlocked: number;
  total: number;
  streak: number;
  unlockedTables: string;
  nextTable: number | null;
}

type FeedbackState =
  | { type: "none" }
  | { type: "incorrect"; correctAnswer: number; userAnswer: number };

export default function PracticePage() {
  const [problem, setProblem] = useState<Problem | null>(null);
  const [answer, setAnswer] = useState("");
  const [feedback, setFeedback] = useState<FeedbackState>({ type: "none" });
  const [stats, setStats] = useState<Stats | null>(null);
  const [startTime, setStartTime] = useState<number>(Date.now());
  const [loading, setLoading] = useState(true);
  const [confirmReset, setConfirmReset] = useState(false);

  const fetchNextProblem = useCallback(async () => {
    const res = await fetch("/api/next-problem");
    const data = await res.json();
    setProblem(data.problem);
    setStats(data.stats);
    setStartTime(Date.now());
    setAnswer("");
    setFeedback({ type: "none" });
    setLoading(false);
  }, []);

  useEffect(() => {
    fetchNextProblem();
  }, [fetchNextProblem]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!problem || feedback.type !== "none") return;

    const userAnswer = parseInt(answer.trim());
    if (isNaN(userAnswer)) {
      setAnswer("");
      return;
    }

    const correctAnswer = problem.a * problem.b;
    const responseSecs = (Date.now() - startTime) / 1000;

    const res = await fetch("/api/answer", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        problemA: problem.a,
        problemB: problem.b,
        userAnswer,
        responseSecs,
      }),
    });

    const data = await res.json();
    setStats(data.stats);

    if (userAnswer === correctAnswer) {
      // Correct - immediately go to next problem
      setProblem(data.nextProblem);
      setStartTime(Date.now());
      setAnswer("");
    } else {
      // Incorrect - show feedback and require typing correct answer
      setFeedback({ type: "incorrect", correctAnswer, userAnswer });
      setAnswer("");
    }
  };

  const handleCorrection = (e: React.FormEvent) => {
    e.preventDefault();

    if (feedback.type !== "incorrect") return;

    const typed = parseInt(answer.trim());
    if (typed === feedback.correctAnswer) {
      fetchNextProblem();
    }
  };

  const handleReset = async () => {
    await fetch("/api/reset", { method: "POST" });
    setConfirmReset(false);
    fetchNextProblem();
  };

  if (loading) {
    return (
      <main className="min-h-screen flex items-center justify-center bg-gradient-to-b from-blue-50 to-blue-100">
        <div className="text-gray-600">Loading...</div>
      </main>
    );
  }

  return (
    <main className="min-h-screen flex flex-col items-center justify-center bg-gradient-to-b from-blue-50 to-blue-100 p-4">
      <div className="bg-white rounded-2xl shadow-xl p-8 max-w-md w-full">
        <h1 className="text-2xl font-bold text-center text-gray-800 mb-6">
          Times Tables Practice
        </h1>

        {problem ? (
          <div className="text-center">
            <div className="text-5xl font-bold text-gray-800 mb-6">
              {problem.a} × {problem.b} = ?
            </div>

            {feedback.type === "none" ? (
              <form onSubmit={handleSubmit} className="space-y-4">
                <input
                  type="number"
                  value={answer}
                  onChange={(e) => setAnswer(e.target.value)}
                  placeholder="Enter answer"
                  className="w-32 text-center text-2xl p-3 border-2 border-gray-300 rounded-lg focus:border-blue-500 focus:outline-none"
                  autoFocus
                />
                <div>
                  <button
                    type="submit"
                    className="px-8 py-3 bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors font-medium"
                  >
                    Submit
                  </button>
                </div>
              </form>
            ) : (
              <div className="space-y-4">
                <p className="text-xl text-red-500 font-medium">
                  {feedback.userAnswer} is wrong. Type the answer:{" "}
                  {feedback.correctAnswer}
                </p>
                <form onSubmit={handleCorrection}>
                  <input
                    type="number"
                    value={answer}
                    onChange={(e) => setAnswer(e.target.value)}
                    placeholder={String(feedback.correctAnswer)}
                    className="w-32 text-center text-2xl p-3 border-2 border-red-300 rounded-lg focus:border-red-500 focus:outline-none"
                    autoFocus
                  />
                </form>
              </div>
            )}
          </div>
        ) : (
          <div className="text-center">
            <p className="text-2xl text-green-500 font-bold mb-2">
              All mastered!
            </p>
            <p className="text-gray-600">
              Congratulations! You&apos;ve mastered all times tables!
            </p>
          </div>
        )}

        {stats && (
          <>
            <div className="mt-8 pt-6 border-t border-gray-200">
              <div className="grid grid-cols-3 gap-4 text-center text-sm">
                <div>
                  <div className="text-2xl font-bold text-gray-800">
                    {stats.streak}
                  </div>
                  <div className="text-gray-500">Streak</div>
                </div>
                <div>
                  <div className="text-2xl font-bold text-gray-800">
                    {stats.mastered}/{stats.unlocked}
                  </div>
                  <div className="text-gray-500">Mastered</div>
                </div>
                <div>
                  <div className="text-2xl font-bold text-gray-800">
                    {stats.total}
                  </div>
                  <div className="text-gray-500">Total</div>
                </div>
              </div>

              <div className="mt-4 text-center text-sm text-gray-600">
                <p>Tables: {stats.unlockedTables}</p>
                {stats.nextTable && <p>Next: {stats.nextTable}×</p>}
              </div>
            </div>

            <div className="mt-6 text-center">
              {confirmReset ? (
                <div className="space-x-2">
                  <span className="text-sm text-gray-600">Reset progress?</span>
                  <button
                    onClick={handleReset}
                    className="text-sm text-red-500 hover:underline"
                  >
                    Yes, reset
                  </button>
                  <button
                    onClick={() => setConfirmReset(false)}
                    className="text-sm text-gray-500 hover:underline"
                  >
                    Cancel
                  </button>
                </div>
              ) : (
                <button
                  onClick={() => setConfirmReset(true)}
                  className="text-sm text-gray-400 hover:text-gray-600"
                >
                  Reset progress
                </button>
              )}
            </div>
          </>
        )}
      </div>
    </main>
  );
}
