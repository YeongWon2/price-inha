// 사무실 이름표 · 말풍선 · 말풍선 줄의 자리(server 23차 1 · 2 · 4 · 5번, 원본 DeskRPG `label-layout` 수치 —
// docs/research/deskrpg.md 11절). 장면 없이 시험할 수 있게 투영한 화면 점(px)만 받는 순수 함수로 둔다.
import type { Rect, Size } from "./speech";

/** 이름표 윗변이 발 투영점에서 내려오는 거리(px). */
export const TAG_FOOT_GAP = 10;
/** 이름표 가로 자리의 화면 여백(px). */
export const TAG_SCREEN_MARGIN = 8;
/** 겹칠 때 아래로 내려가는 한 단의 덧붙임(px): 한 단 = `높이 + 5`. */
export const TAG_STACK_GAP = 5;
/** 아래로 내려가는 단 수(첫 자리 포함). */
export const TAG_ROWS = 3;
/** 좌우 후보가 비켜나는 거리(이름표 폭의 배수). */
export const TAG_SIDE_SHIFT = 0.4;
/** 이름표끼리 · 다른 것과 두는 간격(px). 이보다 가까우면 겹친 것으로 본다. */
export const TAG_PADDING = 4;
/** 이름표 최대 폭(px, 넘으면 말줄임). */
export const TAG_MAX_WIDTH = 150;

/** 자리 잡는 우선순위: 쓰는 중이거나 글 있음. */
export const PRIORITY_SPEAKING = 80;
/** 자리 잡는 우선순위: 일하는 중. */
export const PRIORITY_WORKING = 50;
/** 자리 잡는 우선순위: 그 밖. */
export const PRIORITY_OTHER = 0;

/** 말풍선 줄의 오른쪽 여백(px): x = 화면폭 - 말풍선폭 - 18. */
export const RAIL_RIGHT_INSET = 18;
/** 말풍선 줄의 위 여백(px). */
export const RAIL_TOP = 8;
/** 말풍선 줄 폭 = 말풍선폭 + 10. */
export const RAIL_EXTRA_WIDTH = 10;
/** 말풍선 줄 높이의 상한(px)과 화면 높이에 대한 비율: min(168, 화면높이 × 0.28). */
export const RAIL_MAX_HEIGHT = 168;
export const RAIL_HEIGHT_RATIO = 0.28;
/** 말풍선 줄 안 세로 간격(px). */
export const RAIL_GAP = 6;

/** 말풍선 줄의 모서리. 이름표와 겹치면 오른쪽 위 → 왼쪽 위 → 왼쪽 아래 → 오른쪽 아래 순서로 옮긴다. */
export type RailCorner = "top-right" | "top-left" | "bottom-left" | "bottom-right";
export const RAIL_CORNERS: readonly RailCorner[] = ["top-right", "top-left", "bottom-left", "bottom-right"];

/** 이름표를 놓을 담당 하나: 발(월드 y 0, 앉았으면 좌석)을 투영한 화면 점과 이름표 크기 · 우선순위. */
export interface FootSpot {
  agent: string;
  x: number;
  y: number;
  width: number;
  height: number;
  priority: number;
}

/** 자리를 잡은 것 하나. */
export interface Placed {
  agent: string;
  rect: Rect;
}

/**
 * (23차 1번) 이름표 자리. 우선순위 내림차순(같으면 id 글자 순)으로 놓는다. 첫 자리는 발 투영점에서 10px 아래가
 * 윗변, 가로 가운데 맞춤 뒤 화면 여백 8px 안으로 민 자리다. 앞서 놓은 이름표 · `taken`과 [`TAG_PADDING`]보다
 * 가까우면 아래로 `높이 + 5`씩 3단 × 좌우 0 · -0.4폭 · +0.4폭의 9후보 가운데 처음 빈 자리를 쓴다.
 * 결과는 놓은 순서다.
 */
export function placeNameTags(feet: readonly FootSpot[], screen: Size, taken: readonly Rect[] = []): Placed[] {
  void feet;
  void screen;
  void taken;
  throw new Error("todo");
}

/** (23차 1번) 이름표 안 글자: 이름뿐(💭 · 💤 같은 아이콘을 붙이지 않는다). */
export function nameTagText(name: string, work: string, place: string): string {
  void name;
  void work;
  void place;
  throw new Error("todo");
}

/**
 * (23차 4번) 자리 잡는 우선순위: 머리 위 말풍선(글 · "···")이 있으면 [`PRIORITY_SPEAKING`], 일하는 중(`work`가
 * `idle`이 아님)이면 [`PRIORITY_WORKING`], 그 밖 [`PRIORITY_OTHER`].
 */
export function actorPriority(hasSpeech: boolean, work: string): number {
  void hasSpeech;
  void work;
  throw new Error("todo");
}

/**
 * (23차 5번) 말풍선 줄의 자리: 그 모서리에 폭 = 말풍선폭 + 10, 높이 = min(168, 화면높이 × 0.28). 오른쪽 위는
 * (화면폭 - 말풍선폭 - 18, 8)이고, 다른 모서리도 화면 가장자리에서 같은 여백(8px)이다.
 */
export function railRect(corner: RailCorner, screen: Size, bubbleWidth: number): Rect {
  void corner;
  void screen;
  void bubbleWidth;
  throw new Error("todo");
}

/** 한 프레임에 자리를 잡을 담당 하나. */
export interface OfficeActor {
  agent: string;
  /** 투영 좌표가 화면 안인가. 아니면 이름표 · 말풍선 모두 숨긴다. */
  visible: boolean;
  /** 발 투영점(px). */
  foot: { x: number; y: number };
  /** 머리 투영점(px, 월드 y 2.7 · 앉으면 2.0을 우리 키에 맞춘 점). */
  head: { x: number; y: number };
  /** 이름표 크기. */
  tag: Size;
  /** 머리 위 말풍선 크기(없으면 `null`). */
  bubble: Size | null;
  /** 말풍선 줄에 보낼 글 없는 상태 말풍선이 있는가. */
  railed: boolean;
  /** [`actorPriority`]. */
  priority: number;
}

/** 한 프레임의 자리 잡기 결과. */
export interface OfficeLayout {
  /** 이름표(놓은 순서). */
  tags: Placed[];
  /** 머리 위 말풍선(놓은 순서). */
  bubbles: Placed[];
  /**
   * 말풍선 줄(줄에 보낸 것이 없으면 `null`). `agents`는 줄 안 순서(위부터): 글 없는 상태로 줄에 간 것(자리 잡는
   * 순서) 다음에 ③에서 자리가 없어 넘친 것(자리 잡는 순서).
   */
  rail: { corner: RailCorner; rect: Rect; agents: string[] } | null;
  /** 화면 밖이라 숨긴 담당. */
  hidden: string[];
}

/**
 * (23차 4번) 한 프레임의 자리 잡기. 보이는 담당을 우선순위 내림차순(같으면 id 순)으로 두고
 * ① 우선순위 80 이상의 이름표 → ② 말풍선 줄 모서리(①의 이름표와 안 겹치는 첫 모서리) → ③ 머리 위 말풍선
 * (22차 후보 · 간격 `높이 + 6` · 첫 자리 `max(8, 머리 y - 높이 - 8)`, ①의 이름표 · 쓰는 말풍선 줄 · 앞선 말풍선과
 * 안 겹치는 첫 자리, 없으면 줄로; 넓이 합이 화면의 24%를 넘으면 모두 줄로) → ④ 나머지 이름표(앞서 놓은 것 모두를
 * 피한다). 말풍선폭은 `bubbleWidth(화면폭)`이다.
 */
export function layoutOffice(actors: readonly OfficeActor[], screen: Size): OfficeLayout {
  void actors;
  void screen;
  throw new Error("todo");
}
