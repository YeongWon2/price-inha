// 지나가며 잡담(server 22차 4번, 원본 DeskRPG `npc-smalltalk` 동작 — docs/research/deskrpg.md 10절). 화면만:
// 서버 · 기록 · AI에 보내지 않는다. 대사는 우리 것(주식 사무실 잡담 16쌍)이다. 시각은 부르는 쪽이 넘기는 밀리초.

/** 잡담 수치(원본과 같다). 시간은 밀리초, 거리는 세계 단위. */
export const SMALLTALK_RULES = {
  /** 두 담당 사이 거리가 이 값 이하일 때만. */
  range: 1.8,
  /** 먼저 말한 쪽 말풍선: 시작부터 이 시간까지. */
  firstUntilMs: 4_500,
  /** 답하는 쪽 말풍선: 시작 뒤 이 시간부터. */
  replyFromMs: 2_000,
  /** 답하는 쪽 말풍선: 시작 뒤 이 시간까지. */
  replyUntilMs: 6_500,
  /** 둘 다 시작부터 이 시간 동안 멈춘다. */
  pauseMs: 7_500,
  /** 담당마다 쉬는 시간. */
  personalMs: 45_000,
  /** 같은 쌍이 쉬는 시간. */
  pairMs: 90_000,
  /** 전체가 쉬는 시간. */
  globalMs: 12_000,
} as const;

/** 질문 · 답 한 쌍. */
export interface SmalltalkLine {
  ask: string;
  answer: string;
}

/** 주식 사무실 잡담 16쌍(한국어, 우리 대사 — 원본 대사를 옮기지 않는다). */
export const SMALLTALK_LINES: readonly SmalltalkLine[] = [];

/** 잡담을 볼 담당 하나의 지금 모습. */
export interface Walker {
  id: string;
  x: number;
  z: number;
  /** 일이 없다(논의 · 답 · 일 중이 아니다). */
  idle: boolean;
  /** 걷는 중이다. */
  walking: boolean;
  /** 대화 · 상태 말풍선이 있다(있으면 잡담하지 않는다). */
  busy: boolean;
}

/** 시작한 잡담 하나. */
export interface Exchange {
  /** 먼저 말한 담당. */
  first: string;
  /** 답하는 담당. */
  second: string;
  /** [`SMALLTALK_LINES`]의 번호. */
  line: number;
  /** 시작 시각(밀리초). */
  start: number;
}

/** 지금 보일 잡담 말 하나. */
export interface SmalltalkSay {
  agent: string;
  text: string;
}

/** 지나가며 잡담 판. */
export class Smalltalk {
  /** `random`은 [0, 1)을 주는 함수(시험은 고정 값을 넘긴다). */
  constructor(random: () => number = Math.random) {
    void random;
  }

  /**
   * 조건(둘 다 한가 · 한쪽 이상 걷는 중 · 거리 [`SMALLTALK_RULES.range`] 이하 · `canSee`로 서로 보임 · 대화 · 상태 말풍선
   * 없음 · 쉬는 시간 지남)이 맞는 쌍이 있으면 잡담 하나를 `now`에 시작해 돌려준다(한 번에 하나). 바로 앞 잡담과 같은
   * 대사는 고르지 않는다. 없으면 `null`.
   */
  tick(walkers: readonly Walker[], canSee: (a: string, b: string) => boolean, now: number): Exchange | null {
    throw new Error("todo");
  }

  /** `now`에 보일 잡담 말들: 먼저 말한 쪽은 질문(시작 ~ 4.5초), 답하는 쪽은 답(2초 ~ 6.5초). */
  speaking(now: number): SmalltalkSay[] {
    throw new Error("todo");
  }

  /** 그 담당이 `now`에 잡담 때문에 멈춰 있는가(시작부터 [`SMALLTALK_RULES.pauseMs`]). */
  paused(agent: string, now: number): boolean {
    throw new Error("todo");
  }
}
