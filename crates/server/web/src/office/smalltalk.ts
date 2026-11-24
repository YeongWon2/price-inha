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
export const SMALLTALK_LINES: readonly SmalltalkLine[] = [
  { ask: "오늘 장 분위기 어때요?", answer: "아직은 눈치 보는 중이에요." },
  { ask: "커피 한 잔 하셨어요?", answer: "벌써 두 잔째예요." },
  { ask: "어제 공시 많았죠?", answer: "정리하느라 혼났어요." },
  { ask: "점심 뭐 드실래요?", answer: "장 끝나고 생각해 볼게요." },
  { ask: "환율 좀 보셨어요?", answer: "조금 올랐더라고요." },
  { ask: "오늘 거래량 괜찮나요?", answer: "평소보다 조금 많아요." },
  { ask: "손절선은 다 점검하셨죠?", answer: "아침에 한 번 더 봤어요." },
  { ask: "요즘 바쁘시죠?", answer: "실적 시즌이라 그렇네요." },
  { ask: "모니터 하나 더 놓을까요?", answer: "자리가 있으면 좋겠네요." },
  { ask: "어제 복기는 끝내셨어요?", answer: "거의 다 썼어요." },
  { ask: "오후에 회의 있나요?", answer: "논의 생기면 바로 모여요." },
  { ask: "장 시작 전에 뭐 보세요?", answer: "밤사이 해외 지수요." },
  { ask: "의자 바꾸셨어요?", answer: "허리가 편해졌어요." },
  { ask: "오늘 날씨 좋죠?", answer: "창밖 볼 틈은 없네요." },
  { ask: "주말엔 뭐 하셨어요?", answer: "책 좀 읽었어요." },
  { ask: "기다리는 일정 있어요?", answer: "다음 주 실적 발표요." },
];

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
  private readonly random: () => number;
  /** 지금(또는 마지막) 잡담. */
  private current: Exchange | null = null;
  /** 바로 앞 잡담의 대사 번호. */
  private lastLine = -1;
  /** 전체가 마지막으로 시작한 시각. */
  private lastStart = Number.NEGATIVE_INFINITY;
  /** 담당마다 마지막으로 시작한 시각. */
  private readonly personal = new Map<string, number>();
  /** 쌍마다 마지막으로 시작한 시각. */
  private readonly pairs = new Map<string, number>();

  /** `random`은 [0, 1)을 주는 함수(시험은 고정 값을 넘긴다). */
  constructor(random: () => number = Math.random) {
    this.random = random;
  }

  private rested(id: string, now: number): boolean {
    return now - (this.personal.get(id) ?? Number.NEGATIVE_INFINITY) >= SMALLTALK_RULES.personalMs;
  }

  private pairRested(a: string, b: string, now: number): boolean {
    return now - (this.pairs.get(pairKey(a, b)) ?? Number.NEGATIVE_INFINITY) >= SMALLTALK_RULES.pairMs;
  }

  private ready(a: Walker, b: Walker, canSee: (a: string, b: string) => boolean, now: number): boolean {
    if (!a.idle || !b.idle || a.busy || b.busy) return false;
    if (!a.walking && !b.walking) return false;
    if (Math.hypot(a.x - b.x, a.z - b.z) > SMALLTALK_RULES.range) return false;
    if (!this.rested(a.id, now) || !this.rested(b.id, now) || !this.pairRested(a.id, b.id, now)) return false;
    return canSee(a.id, b.id) && canSee(b.id, a.id);
  }

  private pickLine(): number {
    const count = SMALLTALK_LINES.length;
    let line = Math.min(count - 1, Math.floor(this.random() * count));
    if (line === this.lastLine && count > 1) line = (line + 1) % count;
    return line;
  }

  /**
   * 조건(둘 다 한가 · 한쪽 이상 걷는 중 · 거리 [`SMALLTALK_RULES.range`] 이하 · `canSee`로 서로 보임 · 대화 · 상태 말풍선
   * 없음 · 쉬는 시간 지남)이 맞는 쌍이 있으면 잡담 하나를 `now`에 시작해 돌려준다(한 번에 하나). 바로 앞 잡담과 같은
   * 대사는 고르지 않는다. 없으면 `null`.
   */
  tick(walkers: readonly Walker[], canSee: (a: string, b: string) => boolean, now: number): Exchange | null {
    if (now - this.lastStart < SMALLTALK_RULES.globalMs) return null;
    for (let i = 0; i < walkers.length; i += 1) {
      for (let j = i + 1; j < walkers.length; j += 1) {
        const [a, b] = [walkers[i], walkers[j]];
        if (!this.ready(a, b, canSee, now)) continue;
        const swap = this.random() >= 0.5;
        const exchange: Exchange = {
          first: swap ? b.id : a.id,
          second: swap ? a.id : b.id,
          line: this.pickLine(),
          start: now,
        };
        this.current = exchange;
        this.lastLine = exchange.line;
        this.lastStart = now;
        this.personal.set(a.id, now);
        this.personal.set(b.id, now);
        this.pairs.set(pairKey(a.id, b.id), now);
        return exchange;
      }
    }
    return null;
  }

  /** `now`에 보일 잡담 말들: 먼저 말한 쪽은 질문(시작 ~ 4.5초), 답하는 쪽은 답(2초 ~ 6.5초). */
  speaking(now: number): SmalltalkSay[] {
    const ex = this.current;
    if (!ex) return [];
    const line = SMALLTALK_LINES[ex.line];
    const since = now - ex.start;
    const out: SmalltalkSay[] = [];
    if (since >= 0 && since < SMALLTALK_RULES.firstUntilMs) out.push({ agent: ex.first, text: line.ask });
    if (since >= SMALLTALK_RULES.replyFromMs && since < SMALLTALK_RULES.replyUntilMs) {
      out.push({ agent: ex.second, text: line.answer });
    }
    return out;
  }

  /** 그 담당이 `now`에 잡담 때문에 멈춰 있는가(시작부터 [`SMALLTALK_RULES.pauseMs`]). */
  paused(agent: string, now: number): boolean {
    const ex = this.current;
    if (!ex || (ex.first !== agent && ex.second !== agent)) return false;
    const since = now - ex.start;
    return since >= 0 && since < SMALLTALK_RULES.pauseMs;
  }
}

/** 순서와 상관없는 쌍 열쇠. */
function pairKey(a: string, b: string): string {
  return a < b ? `${a}|${b}` : `${b}|${a}`;
}
