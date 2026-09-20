/** Keep in sync with the `Session` struct in `src-tauri/src/session.rs`. */
export interface Session {
  id: string;
  topic: string;
  mode: SessionMode;
  status: SessionStatus;
  /** Unix timestamp (seconds since epoch). */
  created_at: number;
}

export type SessionMode = "sequential" | "mention";
export type SessionStatus = "idle" | "running" | "awaiting_user";
