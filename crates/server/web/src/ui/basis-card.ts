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

/** 찬반 수 글: "강세 n · 약세 n · 중립 n · 기권 n"(R4-4 · V55와 같은 말). */
export function voteText(votes: VotesWire): string {
  void votes;
  return "";
}

/** 서버 몸통(모양이 틀린 칸은 없는 것으로 본다)을 근거 카드 글로 바꾼다. */
export function readBasisCard(wire: unknown): BasisCard {
  void wire;
  return { recorded: false, rows: [], votes: "", leader: null, sources: NONE };
}
