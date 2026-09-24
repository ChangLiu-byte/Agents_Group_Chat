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

/** Keep in sync with the `Message` struct in `src-tauri/src/session.rs`.
 * Also the payload of the `message-added` event. */
export interface Message {
  id: string;
  session_id: string;
  /** `null` means the user sent this message. */
  agent_id: string | null;
  round_number: number;
  content: string;
  refers_to: string | null;
  /** Unix timestamp (seconds since epoch). */
  created_at: number;
}

/** Payload of the `agent-error` event (see `commands/chat.rs`). */
export interface AgentErrorEvent {
  session_id: string;
  round_number: number;
  agent_id: string;
  agent_name: string;
  error: string;
}

/** Payload of the `round-complete` event. */
export interface RoundCompleteEvent {
  session_id: string;
}
