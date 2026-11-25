// 머리 위 말풍선(server 22차 3번 · 5번, 원본 DeskRPG 동작 — docs/research/deskrpg.md 10절).
// 장면 없이 시험할 수 있게 글 다듬기 · 폭 · 말풍선 판(수명 · 우선순위) · 자리 고르기 · 가벼운 화면의 최근 말을
// 순수 함수와 작은 클래스로 둔다. 시각은 부르는 쪽이 넘기는 밀리초(`now`)다.
// (34차 4번) 상태 표시 · 말하는 중 글자는 legend.ts의 머리 위 표시 정의를 쓴다(범례와 한곳에서 정의).

import { markOf } from "./legend";

/** 말풍선 글의 최대 글자 수. 넘으면 [`SPEECH_CUT`]자 + "..."(원본 `speech-previews`). */
export const SPEECH_MAX = 240;
/** 넘친 글을 자르는 글자 수. */
export const SPEECH_CUT = 237;
/** 글 말풍선이 마지막으로 바뀐 뒤 보이는 시간(밀리초). */
export const SPEECH_LIFE_MS = 12_000;
/** 글 없이 "말하는 중"만 알 때의 표시 글. */
export const TYPING_TEXT: string = markOf("typing").glyph;
/** [`TYPING_TEXT`] 말풍선이 보이는 시간(밀리초). */
export const TYPING_LIFE_MS = 4_000;
/** 가벼운 화면에서 담당 줄 아래 최근 말이 보이는 시간(밀리초). */
export const LATEST_LIFE_MS = 12_000;
/** 말풍선 폭의 하한 · 상한 · 화면 여백(px). 폭은 `max(80, min(190, 화면폭 - 16))`. */
export const BUBBLE_MIN_WIDTH = 80;
export const BUBBLE_MAX_WIDTH = 190;
export const BUBBLE_SCREEN_MARGIN = 16;
/** 머리와 말풍선 사이의 틈(px): 첫 자리의 위쪽 끝은 `머리 y - 높이 - 8`. */
export const BUBBLE_HEAD_GAP = 8;
/** 좌우 후보가 비켜나는 거리(말풍선 폭의 배수). */
export const BUBBLE_SIDE_SHIFT = 0.55;
/**
 * (23차 2번) 위아래 후보가 비켜나는 간격(px): `높이 + 6`(원본 `label-layout`, 22차의 `높이 + 8`을 고친다).
 * 첫 자리의 위쪽 끝은 `max(8, 머리 y - 높이 - 8)`이다(화면 위 여백 [`BUBBLE_TOP_MARGIN`]).
 */
export const BUBBLE_STACK_GAP = 6;
/** (23차 2번) 첫 자리 위쪽 끝의 하한(px). */
export const BUBBLE_TOP_MARGIN = 8;
/** 말풍선 넓이의 합이 화면 넓이의 이 비율을 넘으면 모두 말풍선 줄로 보낸다. */
export const LANE_AREA_RATIO = 0.24;

/** 말풍선 종류. 우선순위: 대화(논의 · 답) > 상태 > 잡담. */
export type SpeechKind = "talk" | "status" | "smalltalk";

/** 담당의 상태(원본 표의 뜻만 같게). */
export type SpeechState = "thinking" | "writing" | "waiting" | "failed";

/** 상태 표시: 생각 중 💭 · 쓰는 중 💬 · 기다림 ⏳ · 실패 ❗. 글 앞에 붙인다. */
export const STATE_ICONS: Readonly<Record<SpeechState, string>> = {
  thinking: markOf("thinking").glyph,
  writing: markOf("writing").glyph,
  waiting: markOf("waiting").glyph,
  failed: markOf("failed").glyph,
};

/** 빈칸(줄바꿈 포함)을 하나로 접고 앞뒤를 깎은 뒤, [`SPEECH_MAX`]자를 넘으면 [`SPEECH_CUT`]자 + "..."로 자른다. */
export function trimSpeech(text: string): string {
  const folded = text.replace(/\s+/g, " ").trim();
  const chars = Array.from(folded);
  if (chars.length <= SPEECH_MAX) return folded;
  return `${chars.slice(0, SPEECH_CUT).join("")}...`;
}

/** 말풍선 폭(px): `max(80, min(190, 화면폭 - 16))`. */
export function bubbleWidth(screenWidth: number): number {
  return Math.max(BUBBLE_MIN_WIDTH, Math.min(BUBBLE_MAX_WIDTH, screenWidth - BUBBLE_SCREEN_MARGIN));
}

/** 보이는 말풍선 하나. `until`이 `Infinity`면 지울 때까지 보인다. */
export interface Bubble {
  agent: string;
  kind: SpeechKind;
  /** 보일 글(상태 표시가 있으면 앞에 붙은 것, [`trimSpeech`]를 거친 것). */
  text: string;
  /** 이 시각(밀리초)부터 보이지 않는다. */
  until: number;
}

/**
 * 담당마다의 말풍선 판. 한 담당에 종류마다 하나씩 두고, 보일 때는 우선순위가 가장 높은 하나만 보인다.
 */
export class SpeechBoard {
  /** 담당 id → 종류 → 말풍선. */
  private readonly bubbles = new Map<string, Map<SpeechKind, Bubble>>();

  private put(bubble: Bubble): void {
    let mine = this.bubbles.get(bubble.agent);
    if (!mine) {
      mine = new Map();
      this.bubbles.set(bubble.agent, mine);
    }
    mine.set(bubble.kind, bubble);
  }

  private live(agent: string, kind: SpeechKind, now: number): Bubble | undefined {
    const bubble = this.bubbles.get(agent)?.get(kind);
    return bubble && now < bubble.until ? bubble : undefined;
  }

  /**
   * 대화 말풍선(논의 글 · 담당 답)을 둔다. 같은 담당의 대화 말풍선은 통째로 바꾼다(스트리밍 중에는 지금까지의 글
   * 전체를 넘긴다 — 덧붙이지 않는다). 수명은 `now`부터 [`SPEECH_LIFE_MS`]. `state`가 있으면 그 표시를 글 앞에 붙인다.
   */
  say(agent: string, text: string, now: number, state?: SpeechState): void {
    const shown = state ? trimSpeech(`${STATE_ICONS[state]} ${text}`) : trimSpeech(text);
    this.put({ agent, kind: "talk", text: shown, until: now + SPEECH_LIFE_MS });
  }

  /** 글 없이 "말하는 중"만 알 때: 대화 말풍선을 [`TYPING_TEXT`]로 [`TYPING_LIFE_MS`] 동안 둔다. */
  typing(agent: string, now: number): void {
    this.put({ agent, kind: "talk", text: TYPING_TEXT, until: now + TYPING_LIFE_MS });
  }

  /** 상태 말풍선(표시만)을 둔다. 지울 때까지 보인다. */
  status(agent: string, state: SpeechState, now: number): void {
    void now;
    this.put({ agent, kind: "status", text: STATE_ICONS[state], until: Number.POSITIVE_INFINITY });
  }

  /** 잡담 말풍선을 `until`까지 둔다. */
  smalltalk(agent: string, text: string, now: number, until: number): void {
    void now;
    this.put({ agent, kind: "smalltalk", text: trimSpeech(text), until });
  }

  /** 그 담당의 말풍선을 지운다(`kind`가 없으면 모두). */
  clear(agent: string, kind?: SpeechKind): void {
    if (kind === undefined) this.bubbles.delete(agent);
    else this.bubbles.get(agent)?.delete(kind);
  }

  /** `now`에 그 담당에게 보이는 말풍선(우선순위가 가장 높은 하나). 없으면 `undefined`. */
  shown(agent: string, now: number): Bubble | undefined {
    return (
      this.live(agent, "talk", now) ?? this.live(agent, "status", now) ?? this.live(agent, "smalltalk", now)
    );
  }

  /** `now`에 대화나 상태 말풍선이 있는가(있으면 잡담하지 않는다). */
  busy(agent: string, now: number): boolean {
    return this.live(agent, "talk", now) !== undefined || this.live(agent, "status", now) !== undefined;
  }
}

/** 화면 위 사각형(px, 왼쪽 위 기준). */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** 크기(px). */
export interface Size {
  width: number;
  height: number;
}

/** 말풍선을 띄울 담당 하나: 머리 위치를 화면에 투영한 점 · 말풍선 크기 · 이름표 자리. */
export interface HeadSpot {
  agent: string;
  /** 투영한 머리 점(px). */
  x: number;
  y: number;
  bubble: Size;
  /** 이름표 자리(지킨다 — 말풍선이 덮지 않는다). */
  label: Rect;
}

/** 자리 고르기의 결과. */
export interface Placement {
  /** 머리 위에 띄운 것(넘긴 순서). */
  placed: { agent: string; rect: Rect }[];
  /** 화면 모서리의 세로 말풍선 줄로 보낸 것(넘긴 순서). */
  lane: string[];
  /** 머리가 화면 밖이라 숨긴 것. */
  hidden: string[];
}

/**
 * 말풍선 자리 고르기(넘긴 순서대로). 머리가 화면 밖이면 숨긴다. 첫 자리는 머리 바로 위(가운데 맞춤, 위쪽 끝
 * `max(8, 머리 y - 높이 - 8)`, 23차)이고, 거기서 위아래(`높이 + 6`, 23차 [`BUBBLE_STACK_GAP`]) · 좌우(폭의
 * [`BUBBLE_SIDE_SHIFT`]배)로 비킨 3×3 후보(위아래는 0 · 위 · 아래 순서, 각 줄은 가운데 · 왼쪽 · 오른쪽 — [`bubbleCandidates`]) 가운데 화면 안이고
 * 앞서 놓은 말풍선 · 모든 이름표와 겹치지 않는 첫 자리를 쓴다. 자리가 없으면 말풍선 줄로 보낸다. 보이는 말풍선
 * 넓이의 합이 화면 넓이의 [`LANE_AREA_RATIO`]를 넘으면 모두 말풍선 줄로 보낸다.
 */
export function placeBubbles(spots: readonly HeadSpot[], screen: Size): Placement {
  const hidden: string[] = [];
  const visible: HeadSpot[] = [];
  for (const spot of spots) {
    const onScreen = spot.x >= 0 && spot.x <= screen.width && spot.y >= 0 && spot.y <= screen.height;
    if (onScreen) visible.push(spot);
    else hidden.push(spot.agent);
  }
  const area = visible.reduce((sum, s) => sum + s.bubble.width * s.bubble.height, 0);
  if (area > screen.width * screen.height * LANE_AREA_RATIO) {
    return { placed: [], lane: visible.map((s) => s.agent), hidden };
  }
  const labels = spots.map((s) => s.label);
  const placed: { agent: string; rect: Rect }[] = [];
  const lane: string[] = [];
  for (const spot of visible) {
    const rect = bubbleCandidates(spot.x, spot.y, spot.bubble).find(
      (r) => insideScreen(r, screen) && !placed.some((p) => overlap(p.rect, r)) && !labels.some((l) => overlap(l, r)),
    );
    if (rect) placed.push({ agent: spot.agent, rect });
    else lane.push(spot.agent);
  }
  return { placed, lane, hidden };
}

/**
 * (23차 2번) 머리 위 말풍선의 3×3 후보(원본 `label-layout` 순서): 위아래 0 · 위로 `높이 + 6` · 아래로 `높이 + 6`,
 * 각 줄 안에서 좌우 0 · -0.55폭 · +0.55폭. 첫 자리는 가운데 맞춤, 위쪽 끝 `max(8, 머리 y - 높이 - 8)`.
 */
export function bubbleCandidates(headX: number, headY: number, size: Size): Rect[] {
  const { width, height } = size;
  const x0 = headX - width / 2;
  const y0 = Math.max(BUBBLE_TOP_MARGIN, headY - height - BUBBLE_HEAD_GAP);
  const shift = width * BUBBLE_SIDE_SHIFT;
  const step = height + BUBBLE_STACK_GAP;
  const out: Rect[] = [];
  for (const dy of [0, -step, step]) {
    for (const dx of [0, -shift, shift]) out.push({ x: x0 + dx, y: y0 + dy, width, height });
  }
  return out;
}

/** 사각형이 화면 안에 다 들어오는가. */
export function insideScreen(r: Rect, screen: Size): boolean {
  return r.x >= 0 && r.y >= 0 && r.x + r.width <= screen.width && r.y + r.height <= screen.height;
}

/** 두 사각형이 겹치는가(맞닿음은 겹침이 아니다). */
export function overlap(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/** 한 담당이 한 말(가벼운 화면의 최근 말). `at`은 밀리초. */
export interface Said {
  agent: string;
  text: string;
  at: number;
}

/**
 * 가벼운 화면(WebGL 없음)에서 담당 줄 아래 한 줄: 그 담당의 가장 최근 말이 [`LATEST_LIFE_MS`] 안이면
 * [`trimSpeech`]를 거친 글, 아니면 `null`.
 */
export function latestLine(said: readonly Said[], agent: string, now: number): string | null {
  let latest: Said | undefined;
  for (const one of said) {
    if (one.agent === agent && (!latest || one.at >= latest.at)) latest = one;
  }
  if (!latest || now - latest.at >= LATEST_LIFE_MS) return null;
  return trimSpeech(latest.text);
}

/** (23차 2번) 말풍선 글의 이름 구분자(원본: 이름을 따로 굵게 하지 않는 한 줄 글자). */
export const NAME_SEPARATOR = " · ";

/**
 * (23차 2번 · 3번) 말풍선에 보일 한 줄 글자: `이름 · 글`. 상태가 붙은 글이면 `이름 · 💭 글`, 글 없이 말하는 중이면
 * `이름 · ···`, 글 없는 상태 말풍선이면 `이름 · 💭`. `shown`은 [`SpeechBoard.shown`]이 낸 것이다.
 */
export function bubbleText(name: string, shown: Bubble): string {
  return `${name}${NAME_SEPARATOR}${shown.text}`;
}

/** 말풍선이 그려지는 곳: 머리 위 · 말풍선 줄 · 아무 데도. */
export type BubbleWhere = "head" | "rail" | "none";

/**
 * (23차 3번) 보이는 말풍선이 어디에 그려지나: 글 말풍선(대화 · "···" · 잡담)은 머리 위, 글 없는 상태 말풍선(생각 중 ·
 * 기다림 · 실패 표시만)은 말풍선 줄, 말풍선이 없으면(쉬는 중 포함) 아무 데도.
 */
export function bubbleWhere(shown: Bubble | undefined): BubbleWhere {
  if (!shown) return "none";
  return shown.kind === "status" ? "rail" : "head";
}
