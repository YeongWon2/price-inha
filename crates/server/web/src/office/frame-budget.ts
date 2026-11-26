// (백로그 T 6번) 그리기 예산.

/** 이보다 오래 걸린 장을 아주 느린 장으로 본다(밀리초). */
export const SLOW_FRAME_MS = 100;
/** 아주 느린 장이 이만큼 이어지면 가볍게 한다. */
export const SLOW_FRAME_RUN = 5;

export interface FrameBudget {
  note(frameMs: number): "lighten" | null;
  lightened(): boolean;
}

export function createFrameBudget(): FrameBudget {
  return {
    note: () => null,
    lightened: () => false,
  };
}
