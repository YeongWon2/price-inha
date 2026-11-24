// 머리 위 말풍선(server 22차 3번 · 5번, 원본 DeskRPG 동작 — docs/research/deskrpg.md 10절).
// 장면 없이 시험할 수 있게 글 다듬기 · 폭 · 말풍선 판(수명 · 우선순위) · 자리 고르기 · 가벼운 화면의 최근 말을
// 순수 함수와 작은 클래스로 둔다. 시각은 부르는 쪽이 넘기는 밀리초(`now`)다.

/** 말풍선 글의 최대 글자 수. 넘으면 [`SPEECH_CUT`]자 + "..."(원본 `speech-previews`). */
export const SPEECH_MAX = 240;
/** 넘친 글을 자르는 글자 수. */
export const SPEECH_CUT = 237;
/** 글 말풍선이 마지막으로 바뀐 뒤 보이는 시간(밀리초). */
export const SPEECH_LIFE_MS = 12_000;
/** 글 없이 "말하는 중"만 알 때의 표시 글. */
export const TYPING_TEXT = "···";
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
/** 말풍선 넓이의 합이 화면 넓이의 이 비율을 넘으면 모두 말풍선 줄로 보낸다. */
export const LANE_AREA_RATIO = 0.24;

/** 말풍선 종류. 우선순위: 대화(논의 · 답) > 상태 > 잡담. */
export type SpeechKind = "talk" | "status" | "smalltalk";

/** 담당의 상태(원본 표의 뜻만 같게). */
export type SpeechState = "thinking" | "writing" | "waiting" | "failed";

/** 상태 표시: 생각 중 💭 · 쓰는 중 💬 · 기다림 ⏳ · 실패 ❗. 글 앞에 붙인다. */
export const STATE_ICONS: Readonly<Record<SpeechState, string>> = {
  thinking: "💭",
  writing: "💬",
  waiting: "⏳",
  failed: "❗",
};

/** 빈칸(줄바꿈 포함)을 하나로 접고 앞뒤를 깎은 뒤, [`SPEECH_MAX`]자를 넘으면 [`SPEECH_CUT`]자 + "..."로 자른다. */
export function trimSpeech(text: string): string {
  throw new Error("todo");
}

/** 말풍선 폭(px): `max(80, min(190, 화면폭 - 16))`. */
export function bubbleWidth(screenWidth: number): number {
  throw new Error("todo");
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
  /**
   * 대화 말풍선(논의 글 · 담당 답)을 둔다. 같은 담당의 대화 말풍선은 통째로 바꾼다(스트리밍 중에는 지금까지의 글
   * 전체를 넘긴다 — 덧붙이지 않는다). 수명은 `now`부터 [`SPEECH_LIFE_MS`]. `state`가 있으면 그 표시를 글 앞에 붙인다.
   */
  say(agent: string, text: string, now: number, state?: SpeechState): void {
    throw new Error("todo");
  }

  /** 글 없이 "말하는 중"만 알 때: 대화 말풍선을 [`TYPING_TEXT`]로 [`TYPING_LIFE_MS`] 동안 둔다. */
  typing(agent: string, now: number): void {
    throw new Error("todo");
  }

  /** 상태 말풍선(표시만)을 둔다. 지울 때까지 보인다. */
  status(agent: string, state: SpeechState, now: number): void {
    throw new Error("todo");
  }

  /** 잡담 말풍선을 `until`까지 둔다. */
  smalltalk(agent: string, text: string, now: number, until: number): void {
    throw new Error("todo");
  }

  /** 그 담당의 말풍선을 지운다(`kind`가 없으면 모두). */
  clear(agent: string, kind?: SpeechKind): void {
    throw new Error("todo");
  }

  /** `now`에 그 담당에게 보이는 말풍선(우선순위가 가장 높은 하나). 없으면 `undefined`. */
  shown(agent: string, now: number): Bubble | undefined {
    throw new Error("todo");
  }

  /** `now`에 대화나 상태 말풍선이 있는가(있으면 잡담하지 않는다). */
  busy(agent: string, now: number): boolean {
    throw new Error("todo");
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
 * `머리 y - 높이 - 8`)이고, 거기서 위아래 · 좌우(폭의 [`BUBBLE_SIDE_SHIFT`]배)로 비킨 3×3 후보 가운데 화면 안이고
 * 앞서 놓은 말풍선 · 모든 이름표와 겹치지 않는 첫 자리를 쓴다. 자리가 없으면 말풍선 줄로 보낸다. 보이는 말풍선
 * 넓이의 합이 화면 넓이의 [`LANE_AREA_RATIO`]를 넘으면 모두 말풍선 줄로 보낸다.
 */
export function placeBubbles(spots: readonly HeadSpot[], screen: Size): Placement {
  throw new Error("todo");
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
  throw new Error("todo");
}
