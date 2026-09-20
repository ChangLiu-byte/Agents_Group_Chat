import { useState } from "react";
import { AgentsPage } from "./components/AgentsPage";
import { SessionsPage } from "./components/SessionsPage";

type Tab = "agents" | "sessions";

const tabClass = (active: boolean) =>
  "rounded-md px-3 py-1.5 text-sm font-medium " +
  (active ? "bg-indigo-600 text-white" : "text-slate-600 hover:bg-slate-100");

function App() {
  const [tab, setTab] = useState<Tab>("sessions");

  return (
    <main className="mx-auto max-w-3xl px-4 py-8 space-y-6">
      <nav className="flex items-center gap-2 border-b border-slate-200 pb-4">
        <button onClick={() => setTab("sessions")} className={tabClass(tab === "sessions")}>
          会话
        </button>
        <button onClick={() => setTab("agents")} className={tabClass(tab === "agents")}>
          Agent 配置
        </button>
      </nav>

      {tab === "agents" ? <AgentsPage /> : <SessionsPage />}
    </main>
  );
}

export default App;
