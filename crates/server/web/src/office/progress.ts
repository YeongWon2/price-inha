// 논의 진행 단계 모델(server 51차, C21 · E51 · V63 · R5-11). DOM 없이 받은 내용을 그리기 값으로 바꾼다.
// 원본(docs/research/invest-agent-teams.md F 5): 담당별 상태(대기 · 진행 · 완료 · 오류) + 아래 한 줄 요약
// ("담당 n/6 · 호출 n · 경과 m:ss"), 다시 시도 중이면 "다시 시도 n/3"을 보여 멈춘 게 아님을 알린다.
// - 내용은 이벤트 `office.progress`와 `GET /api/office`의 `progress` 칸(같은 모양). 끝난 담당 수(`done`)는 서버가 센다.
// - 경과는 서버가 준 `at - started_at`에 받은 뒤로 흐른 시간을 더한다(서버 · 화면 시계가 달라도 맞다).

/** 논의 진행 단계 이벤트. */
export const KIND_OFFICE_PROGRESS = "office.progress";
/** 다시 시도 상한(E51). */
export const RETRY_MAX = 3;

/** 담당 하나의 단계. */
export type Stage = "waiting" | "running" | "done" | "error";

/** 단계 표시: 대기 ○ · 진행 ● · 완료 ✓ · 오류 !. */
export const STAGE_MARK: Readonly<Record<Stage, string>> = { waiting: "○", running: "●", done: "✓", error: "!" };
const STAGE_TEXT: Readonly<Record<Stage, string>> = { waiting: "대기", running: "진행", done: "완료", error: "오류" };
const STAGES = Object.keys(STAGE_MARK) as Stage[];

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
  /** 끝난 논의의 마지막 상태인가. */
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

type Obj = Record<string, unknown>;

function isObj(value: unknown): value is Obj {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function whole(value: unknown): number | null {
  return typeof value === "number" && Number.isInteger(value) ? value : null;
}

function readMember(value: unknown): ProgressMember | null {
  if (!isObj(value) || typeof value.agent !== "string") { return null; }
  const state = STAGES.find((one) => one === value.state);
  const retryMax = whole(value.retry_max) ?? RETRY_MAX;
  const retry = value.retry === undefined || value.retry === null ? null : whole(value.retry);
  if (!state || (value.retry !== undefined && value.retry !== null && retry === null)) { return null; }
  return { agent: value.agent, state, retry, retryMax };
}

/** 받은 내용을 읽는다. 모양이 틀리면 null. */
export function readProgress(payload: unknown): Progress | null {
  if (!isObj(payload) || !Array.isArray(payload.members)) { return null; }
  const calls = whole(payload.calls);
  const startedAt = whole(payload.started_at);
  const at = whole(payload.at);
  if (calls === null || startedAt === null || at === null) { return null; }
  const members = payload.members.map(readMember);
  if (members.some((one) => one === null)) { return null; }
  const list = members as ProgressMember[];
  return {
    room: typeof payload.room === "string" ? payload.room : "",
    topic: typeof payload.topic === "string" ? payload.topic : "",
    members: list,
    calls,
    done: whole(payload.done) ?? list.filter((one) => one.state === "done").length,
    startedAt,
    at,
    closed: payload.closed === true,
  };
}

/** 담당마다 표시 · 글. 다시 시도 중(진행 · 오류에 retry가 있으면)이면 "다시 시도 n/3". */
export function progressRows(progress: Progress, nameOf: (id: string) => string): ProgressRow[] {
  return progress.members.map((member) => {
    const retrying = member.retry !== null && member.state === "running";
    return {
      agent: member.agent,
      name: nameOf(member.agent),
      state: member.state,
      mark: STAGE_MARK[member.state],
      label: retrying ? `다시 시도 ${member.retry}/${member.retryMax}` : STAGE_TEXT[member.state],
    };
  });
}

/** 경과 글 m:ss(음수는 0:00). */
export function elapsedText(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

/** 아래 한 줄: "담당 n/6 · 호출 n · 경과 m:ss". `sinceReceivedMs`는 이 내용을 받은 뒤 흐른 시간. */
export function progressSummary(progress: Progress, sinceReceivedMs: number): string {
  const elapsed = progress.at - progress.startedAt + (progress.closed ? 0 : Math.max(0, sinceReceivedMs));
  return `담당 ${progress.done}/${progress.members.length} · 호출 ${progress.calls} · 경과 ${elapsedText(elapsed)}`;
}
