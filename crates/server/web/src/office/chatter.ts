// 사무실 팀의 대화(server 22차 1 · 2 · 3 · 5번): 서버 이벤트(`office.talk` · `office.chat` · `office.agent`)를 받아
// 말풍선 판 · 가벼운 화면의 최근 말 · 팀 탭의 논의 줄로 나눠 둔다. 그리기는 부르는 쪽(main.ts · 3D 장면)이 한다.
// 첫 화면의 최근 줄은 사무실 스냅샷의 `talk` 칸으로, 그 뒤는 실시간 연결의 `office.talk`로 받는다(새 경로 없음).

import { caption, speakingLine, type TalkMark } from "./caption";
import { SpeechBoard, latestLine, type Said, type SpeechState } from "./speech";
import { talkEntry, type FeedEntry, type Talk } from "./team-feed";

/** 서버 이벤트 이름. */
export const KIND_OFFICE_TALK = "office.talk";
/** 팀 탭에 쥐는 논의 줄 수(채팅 줄 정리와 같은 200줄). */
const KEEP_TALKS = 200;
/** 가벼운 화면을 위해 쥐는 최근 말 수. */
const KEEP_SAID = 60;

/** 방 로그의 쓴 쪽 id → 사무실 담당 id(뉴스 공시 시세 담당은 방 로그에서 `scout`, 사무실에서 `news`). */
export function officeAgent(id: string): string {
  return id === "scout" ? "news" : id;
}

/** `office.talk` 내용을 읽는다(모양이 틀리면 `null`). 담당 id는 사무실 id로 바꾼다. */
export function readTalk(payload: unknown): Talk | null {
  if (typeof payload !== "object" || payload === null) return null;
  const p = payload as Record<string, unknown>;
  if (typeof p.agent !== "string" || typeof p.text !== "string" || typeof p.at !== "number") return null;
  return {
    room: typeof p.room === "string" ? p.room : "",
    agent: officeAgent(p.agent),
    text: p.text,
    at: p.at,
    topic: typeof p.topic === "string" ? p.topic : "",
  };
}

/** 담당 답(`office.chat`)에서 말풍선이 보는 부분. */
export interface ChatLike {
  agent: string;
  status: string;
  content?: string;
}

/** 팀의 대화 상태. */
export class Chatter {
  /** 머리 위 말풍선 판(3D 장면이 그린다). */
  readonly board = new SpeechBoard();
  private readonly said: Said[] = [];
  private readonly talks: FeedEntry[] = [];
  /** (23차 7번 (가)) 실시간으로 받은 논의 글의 담당 · 받은 시각(발언 중 표시). */
  private readonly marks: TalkMark[] = [];
  /** 팀 탭 줄의 처음 본 시각(내 글 · 담당 답). */
  private readonly stamps = new Map<string, number>();

  /** `office.talk` 하나. `live`면(실시간으로 온 것) 말풍선 · 최근 말에도 올린다. */
  talk(payload: unknown, seq: number, live: boolean, now: number = Date.now()): boolean {
    const talk = readTalk(payload);
    if (!talk) return false;
    const entry = talkEntry(talk, seq);
    if (this.talks.some((t) => t.key === entry.key)) return false;
    this.talks.push(entry);
    if (this.talks.length > KEEP_TALKS) this.talks.splice(0, this.talks.length - KEEP_TALKS);
    if (live) {
      this.board.say(talk.agent, talk.text, now);
      this.remember(talk.agent, talk.text, now);
      this.marks.push({ agent: talk.agent, at: now });
      if (this.marks.length > KEEP_SAID) this.marks.splice(0, this.marks.length - KEEP_SAID);
    }
    return true;
  }

  /** 담당 답의 상태: 쓰는 중이면 지금까지의 글 전체로 바꾸고, 끝나면 글로, 실패면 ❗. */
  chat(resp: ChatLike, now: number = Date.now()): void {
    const content = resp.content ?? "";
    if (resp.status === "complete") {
      this.board.clear(resp.agent, "status");
      if (content.trim()) {
        this.board.say(resp.agent, content, now);
        this.remember(resp.agent, content, now);
      }
    } else if (resp.status === "failed") {
      this.board.say(resp.agent, "답하지 못했습니다", now, "failed");
    } else if (content.trim()) {
      this.board.say(resp.agent, content, now, "writing");
    } else if (resp.status === "thinking") {
      this.board.typing(resp.agent, now);
    }
  }

  /** 담당의 일(`office.agent`의 `work`)을 상태 말풍선으로. */
  work(agent: string, work: string, now: number = Date.now()): void {
    const states: Record<string, SpeechState> = { queued: "waiting", thinking: "thinking", failed: "failed" };
    const state = states[work];
    if (state) this.board.status(agent, state, now);
    else this.board.clear(agent, "status");
  }

  private remember(agent: string, text: string, at: number): void {
    this.said.push({ agent, text, at });
    if (this.said.length > KEEP_SAID) this.said.splice(0, this.said.length - KEEP_SAID);
  }

  /** 가벼운 화면에서 그 담당 줄 아래 한 줄(없으면 `null`). */
  latest(agent: string, now: number = Date.now()): string | null {
    return latestLine(this.said, agent, now);
  }

  /** 팀 탭 줄(내 글 · 담당 답)의 시각: 처음 본 때. */
  stamp(key: string, now: number = Date.now()): number {
    const at = this.stamps.get(key);
    if (at !== undefined) return at;
    this.stamps.set(key, now);
    return now;
  }

  /** (23차 6번) 화면 아래 자막: 가장 최근 말 `이름: 글`(12초), 없으면 `null`. */
  caption(names: Readonly<Record<string, string>>, now: number = Date.now()): string | null {
    return caption(this.said, names, now);
  }

  /** (23차 7번 (가)) 팀 탭 위 `○○ 발언 중…`(마지막 논의 글 뒤 4초), 없으면 `null`. */
  speaking(names: Readonly<Record<string, string>>, now: number = Date.now()): string | null {
    return speakingLine(this.marks, names, now);
  }

  /** 팀 탭에 섞을 논의 줄들. */
  talkEntries(): readonly FeedEntry[] {
    return this.talks;
  }
}
