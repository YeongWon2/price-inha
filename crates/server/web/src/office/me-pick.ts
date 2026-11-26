// 외형 고르기의 "나" 줄(다듬기 E 9번). 뼈대.

/** 내 이름 · 외형. */
export interface MeLook {
  name: string;
  look: string;
}

/** 외형 고르기가 "나"에 쓰는 것. */
export interface MePick {
  current(): MeLook | null;
  save(change: Partial<MeLook>): Promise<MeLook>;
  applied(me: MeLook): void;
}

/** "나" 탭 글. */
export const ME_TAB_LABEL = "나";

export function createMePick(
  fetcher: (url: string, init?: RequestInit) => Promise<Response>,
  current: () => MeLook | null,
  keep: (me: MeLook) => void,
): MePick {
  void fetcher; void current; void keep;
  throw new Error("todo");
}
