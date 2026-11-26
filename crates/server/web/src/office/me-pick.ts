// 외형 고르기의 "나" 줄(다듬기 E 9번, server 32차 1번 C17): 내 이름과 외형을 바꾼다.
// 바꾸기: `POST API.officeMe` `{"name"?, "look"?}`(JSON, 같은 출처) → 200 `{"name", "look", "x", "y", "dir"}`.
// 서버가 거절하면(400 이름 길이 · 모르는 외형 등) 던진다 — 외형 고르기가 안내만 한다. 이 파일은 3D를 부르지 않는다.

import { API } from "../paths";

/** 내 이름 · 외형. */
export interface MeLook {
  name: string;
  look: string;
}

/** 외형 고르기가 "나"에 쓰는 것. */
export interface MePick {
  /** 지금 이름 · 외형(모르면 `null`). */
  current(): MeLook | null;
  /** 서버에 바꾸기를 보낸다(빠진 칸은 그대로). 성공하면 서버가 준 이름 · 외형, 실패면 거부. */
  save(change: Partial<MeLook>): Promise<MeLook>;
  /** 바뀐 이름 · 외형을 화면(3D)에 알린다. */
  applied(me: MeLook): void;
}

/** "나" 탭 글. */
export const ME_TAB_LABEL = "나";

/** 답 몸통 → 이름 · 외형. 모양이 틀리면 `null`. */
function parseMeLook(body: unknown): MeLook | null {
  if (typeof body !== "object" || body === null) return null;
  const { name, look } = body as Record<string, unknown>;
  return typeof name === "string" && typeof look === "string" ? { name, look } : null;
}

/** `fetcher`로 서버에 보내는 "나" 줄. `current`는 지금 값, `keep`은 바뀐 값을 화면에 알린다. */
export function createMePick(
  fetcher: (url: string, init?: RequestInit) => Promise<Response>,
  current: () => MeLook | null,
  keep: (me: MeLook) => void,
): MePick {
  return {
    current,
    async save(change: Partial<MeLook>): Promise<MeLook> {
      const reply = await fetcher(API.officeMe, {
        method: "POST",
        credentials: "same-origin",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(change),
      });
      if (!reply.ok) throw new Error(`나 바꾸기 거절 ${reply.status}`);
      const me = parseMeLook(await reply.json());
      if (me === null) throw new Error("나 바꾸기 답의 모양이 틀립니다");
      return me;
    },
    applied: keep,
  };
}
