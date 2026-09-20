import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Agent } from "../types/agent";
import type { Session } from "../types/session";
import { NewSessionForm } from "./NewSessionForm";
import { SessionDetail } from "./SessionDetail";

type View = { kind: "list" } | { kind: "new" } | { kind: "detail"; sessionId: string };

function formatTimestamp(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString();
}

const modeLabel: Record<Session["mode"], string> = {
  sequential: "小组发言",
  mention: "点名发言",
};

const statusLabel: Record<Session["status"], string> = {
  idle: "待开始",
  running: "进行中",
  awaiting_user: "等待用户",
};

export function SessionsPage() {
  const [view, setView] = useState<View>({ kind: "list" });
  const [sessions, setSessions] = useState<Session[]>([]);
  const [allAgents, setAllAgents] = useState<Agent[]>([]);
  const [error, setError] = useState<string | null>(null);

  async function refreshSessions() {
    try {
      const result = await invoke<Session[]>("list_sessions");
      setSessions(result);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    refreshSessions();
  }, []);

  useEffect(() => {
    if (view.kind !== "new") return;
    invoke<Agent[]>("list_agents")
      .then(setAllAgents)
      .catch((e) => setError(String(e)));
  }, [view.kind]);

  if (view.kind === "detail") {
    return (
      <SessionDetail
        sessionId={view.sessionId}
        onBack={() => {
          setView({ kind: "list" });
          refreshSessions();
        }}
      />
    );
  }

  if (view.kind === "new") {
    return (
      <NewSessionForm
        allAgents={allAgents}
        onCreated={(session) => {
          setView({ kind: "detail", sessionId: session.id });
          refreshSessions();
        }}
        onCancel={() => setView({ kind: "list" })}
      />
    );
  }

  return (
    <div className="space-y-6">
      <header className="flex items-center justify-between">
        <h1 className="text-xl font-bold text-slate-900">会话</h1>
        <button
          onClick={() => setView({ kind: "new" })}
          className="rounded-md bg-indigo-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-indigo-500"
        >
          + 新建会话
        </button>
      </header>

      {error && (
        <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
          {error}
        </div>
      )}

      <section className="space-y-2">
        {sessions.length === 0 && (
          <p className="text-sm text-slate-500">还没有创建任何会话。</p>
        )}
        {sessions.map((session) => (
          <button
            key={session.id}
            onClick={() => setView({ kind: "detail", sessionId: session.id })}
            className="flex w-full items-center justify-between rounded-lg border border-slate-200 bg-white p-3 text-left shadow-sm hover:border-indigo-300"
          >
            <div>
              <p className="text-sm font-semibold text-slate-900">{session.topic}</p>
              <p className="text-xs text-slate-500">{formatTimestamp(session.created_at)}</p>
            </div>
            <div className="flex items-center gap-2 text-xs">
              <span className="rounded-full bg-slate-100 px-2 py-0.5 text-slate-600">
                {modeLabel[session.mode]}
              </span>
              <span className="rounded-full bg-slate-100 px-2 py-0.5 text-slate-600">
                {statusLabel[session.status]}
              </span>
            </div>
          </button>
        ))}
      </section>
    </div>
  );
}
