// 근거 카드의 순수 모델(server 50차 · additions.md C23 · E53 · R6-4 · R6-5 · V68 · V69, 모양 근거
// docs/research/invest-agent-teams.md F 1 · 2): 서버가 내려 준 담당별 의견 · 팀장 요약 · 찬반 수 · 자료 이름을
// 화면에 보일 글로 바꾼다. 세기 · 정렬은 서버가 하고, 여기서는 받은 차례 그대로 글만 만든다.
// 히스토리 화면의 매매 하나(`GET /api/trades/{id}`의 `basis`)와 사무실(`GET /api/trades/{id}/basis`)이 같은 것을 쓴다.

/** 담당 의견의 방향(서버 글자). */
export type Direction = "bull" | "bear" | "neutral";
/** 담당 의견의 확신도(서버 글자, 3등급). */
export type Confidence = "low" | "mid" | "high";
/** 팀장 결정(서버 글자). */
export type Decision = "buy" | "sell" | "watch";

/** 찬반 수(서버가 센 것). */
export interface VotesWire {
  bull: number;
  bear: number;
  neutral: number;
  abstain: number;
}

/** 근거 카드 줄의 색 갈래: 방향 셋과 기권. */
export type Tone = Direction | "abstain";

/** 담당 한 줄(화면 글). */
export interface OpinionRow {
  agent: string;
  /** 담당 이름(서버가 준 것, 없으면 id). */
  name: string;
  abstained: boolean;
  tone: Tone;
  /** "강세" · "약세" · "중립" · "기권". */
  direction: string;
  /** "높음" · "보통" · "낮음", 기권이거나 없으면 "없음". */
  confidence: string;
  /** 확신도 막대 칸 수(0~3). */
  level: number;
  /** 한 줄 이유, 비었으면 "없음". */
  reason: string;
  /** "보는 기간 20거래일", 없으면 "보는 기간 없음". */
  horizon: string;
}

/** 팀장 칸(화면 글). */
export interface LeaderView {
  /** "사기" · "팔기" · "지켜보기". */
  decision: string;
  summary: string;
  flip: string;
}

/** 근거 카드 한 장(화면 글). */
export interface BasisCard {
  /** 의견 · 팀장 요약 기록이 있는가. 없으면 "기록 없음"(칸이 생기기 전 매매). */
  recorded: boolean;
  rows: OpinionRow[];
  /** "강세 n · 약세 n · 중립 n · 기권 n". */
  votes: string;
  /** 팀장 요약 기록이 없으면 `null`. */
  leader: LeaderView | null;
  /** 자료 이름을 " · "로 이은 것, 없으면 "없음". */
  sources: string;
}

/** 옛 매매(칸이 생기기 전)의 글. */
export const NO_RECORD = "기록 없음";
/** 빈 칸의 글. */
export const NONE = "없음";
/** 기권의 글. */
export const ABSTAIN = "기권";

const DIRECTION_TEXT: Readonly<Record<Direction, string>> = { bull: "강세", bear: "약세", neutral: "중립" };
const CONFIDENCE_TEXT: Readonly<Record<Confidence, string>> = { low: "낮음", mid: "보통", high: "높음" };
const CONFIDENCE_LEVEL: Readonly<Record<Confidence, number>> = { low: 1, mid: 2, high: 3 };
const DECISION_TEXT: Readonly<Record<Decision, string>> = { buy: "사기", sell: "팔기", watch: "지켜보기" };

type Wire = Record<string, unknown>;

function isObject(value: unknown): value is Wire {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function oneOf<K extends string>(table: Readonly<Record<K, unknown>>, value: unknown): K | null {
  return typeof value === "string" && Object.hasOwn(table, value) ? (value as K) : null;
}

/** 글이면 앞뒤 공백을 뗀 것, 비었거나 글이 아니면 "없음". */
function filled(value: unknown): string {
  return typeof value === "string" && value.trim() !== "" ? value.trim() : NONE;
}

function count(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? Math.floor(value) : 0;
}

/** 찬반 수 글: "강세 n · 약세 n · 중립 n · 기권 n"(R4-4 · V55와 같은 말). */
export function voteText(votes: VotesWire): string {
  return `강세 ${count(votes.bull)} · 약세 ${count(votes.bear)} · 중립 ${count(votes.neutral)} · 기권 ${count(votes.abstain)}`;
}

/** 서버 찬반 수(모양이 틀리면 0). */
function readVotes(value: unknown): VotesWire {
  const v = isObject(value) ? value : {};
  return { bull: count(v.bull), bear: count(v.bear), neutral: count(v.neutral), abstain: count(v.abstain) };
}

/** 담당 한 줄. `agent`가 글이 아니면 `null`(그 줄만 뺀다). */
function readRow(value: unknown): OpinionRow | null {
  if (!isObject(value) || typeof value.agent !== "string" || value.agent === "") return null;
  const direction = oneOf(DIRECTION_TEXT, value.direction);
  const abstained = value.abstained === true || direction === null;
  const confidence = abstained ? null : oneOf(CONFIDENCE_TEXT, value.confidence);
  const days = value.horizon_days;
  const horizon = typeof days === "number" && Number.isInteger(days) && days > 0 ? `${days}거래일` : NONE;
  return {
    agent: value.agent,
    name: typeof value.name === "string" && value.name !== "" ? value.name : value.agent,
    abstained,
    tone: abstained || direction === null ? "abstain" : direction,
    direction: abstained || direction === null ? ABSTAIN : DIRECTION_TEXT[direction],
    confidence: confidence === null ? NONE : CONFIDENCE_TEXT[confidence],
    level: confidence === null ? 0 : CONFIDENCE_LEVEL[confidence],
    reason: filled(value.reason),
    horizon: `보는 기간 ${horizon}`,
  };
}

function readLeader(value: unknown): LeaderView | null {
  if (!isObject(value)) return null;
  const decision = oneOf(DECISION_TEXT, value.decision);
  if (decision === null) return null;
  return { decision: DECISION_TEXT[decision], summary: filled(value.summary), flip: filled(value.flip_condition) };
}

function readSources(value: unknown): string {
  if (!Array.isArray(value)) return NONE;
  const names = value
    .map((source: unknown) => (isObject(source) && typeof source.name === "string" ? source.name.trim() : ""))
    .filter((name) => name !== "");
  return names.length > 0 ? [...new Set(names)].join(" · ") : NONE;
}

/** 서버 몸통(모양이 틀린 칸은 없는 것으로 본다)을 근거 카드 글로 바꾼다. */
export function readBasisCard(wire: unknown): BasisCard {
  const body = isObject(wire) ? wire : {};
  const list: unknown[] = Array.isArray(body.opinions) ? body.opinions : [];
  const rows = list.map(readRow).filter((row): row is OpinionRow => row !== null);
  const leader = readLeader(body.leader);
  return {
    recorded: rows.length > 0 || leader !== null,
    rows,
    votes: voteText(readVotes(body.votes)),
    leader,
    sources: readSources(body.sources),
  };
}
