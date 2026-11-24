// 화면 아래 자막 · 회의 "발언 중" 표시(server 23차 6번 · 7번 (가), 원본 DeskRPG 화면 — docs/research/deskrpg.md 11절).
// 시각은 부르는 쪽이 넘기는 밀리초(`now`)다.
import type { Said } from "./speech";

/** 자막이 보이는 시간(밀리초): 가장 최근 말 뒤 12초. */
export const CAPTION_LIFE_MS = 12_000;
/** "○○ 발언 중…"이 보이는 시간(밀리초): 마지막 논의 글 뒤 4초. */
export const SPEAKING_LIFE_MS = 4_000;

/**
 * 장면 아래 가운데 한 줄 자막: 모든 담당의 말 가운데 가장 최근 것(같은 시각이면 뒤에 넘긴 것)이 [`CAPTION_LIFE_MS`]
 * 안이면 `이름: 글`(글은 `trimSpeech`를 거친 것), 아니면 `null`(숨긴다). 이름은 `names`에서 찾고 없으면 id.
 */
export function caption(said: readonly Said[], names: Readonly<Record<string, string>>, now: number): string | null {
  void said;
  void names;
  void now;
  throw new Error("todo");
}

/** 논의 글 하나(`office.talk`에서 담당과 시각만). */
export interface TalkMark {
  agent: string;
  at: number;
}

/**
 * 팀 탭 위 한 줄: 마지막 논의 글이 [`SPEAKING_LIFE_MS`] 안이면 `○○ 발언 중…`(그 글의 담당 이름), 아니면 `null`.
 */
export function speakingLine(
  talks: readonly TalkMark[],
  names: Readonly<Record<string, string>>,
  now: number,
): string | null {
  void talks;
  void names;
  void now;
  throw new Error("todo");
}
