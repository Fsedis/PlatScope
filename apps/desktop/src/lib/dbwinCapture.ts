export interface DbwinEntry {
  sequence: number;
  receivedAt: string;
  elapsedMs: number;
  kind: "session_start" | "session_stop" | "message" | "marker";
  processId: number | null;
  message: string;
}

export interface DbwinCaptureStatus {
  connected: boolean;
  supported: boolean;
  sharedListener: boolean;
  active: boolean;
  path: string | null;
  startedAt: string | null;
  stoppedAt: string | null;
  messages: number;
  markers: number;
  bytes: number;
  error: string | null;
  stopReason: string | null;
  entries: DbwinEntry[];
}
