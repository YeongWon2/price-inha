// 논의 진행 단계 모델(server 51차, C21 · E51 · V63). 뼈대.

/** 논의 진행 단계 이벤트. */
export const KIND_OFFICE_PROGRESS = "office.progress";
/** 다시 시도 상한. */
export const RETRY_MAX = 0;

/** 담당 하나의 단계. */
export type Stage = "waiting" | "running" | "done" | "error";

/** 단계 표시. */
export const STAGE_MARK: Readonly<Record<Stage, string>> = { waiting: "", running: "", done: "", error: "" };

/** 담당 하나의 진행. */
export interface ProgressMember {
  agent: string;
  state: Stage;
  retry: number | null;
  retryMax: number;
}

/** `office.progress`의 내용. */
export interface Progress {
  room: string;
  topic: string;
  members: ProgressMember[];
  calls: number;
  done: number;
  startedAt: number;
  at: number;
  closed: boolean;
}

/** 진행 표의 줄 하나. */
export interface ProgressRow {
  agent: string;
  name: string;
  state: Stage;
  mark: string;
  label: string;
}

export function readProgress(payload: unknown): Progress | null {
  void payload;
  return null;
}

export function progressRows(progress: Progress, nameOf: (id: string) => string): ProgressRow[] {
  void progress;
  void nameOf;
  return [];
}

export function elapsedText(ms: number): string {
  void ms;
  return "";
}

export function progressSummary(progress: Progress, sinceReceivedMs: number): string {
  void progress;
  void sinceReceivedMs;
  return "";
}
