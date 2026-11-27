// (백로그 T 6번) 그리기 예산: 3D 장면 한 장을 그리는 데 아주 오래 걸리는 것이 이어지면(소프트웨어 그리기 · 느린 기기)
// 곧바로 한 번 "가볍게"(그림자 끄기 · 해상도 최저)를 낸다.
//
// 잰 것(e2e 시연 서버 · 헤드리스 Chromium의 소프트웨어 WebGL): 장면이 뜬 뒤 한 장에 약 160ms(6fps)가 걸려 주 스레드가
// 거의 쉬지 않았고, 탭 누르기가 처리되기까지 4초 넘게 밀렸다. 장면 불러오기(모델 · 셰이더 준비)는 뜨기 전 1~3초였다 —
// 막는 것은 불러오기보다 이어지는 그리기다. 그림자를 끄면 한 장이 절반 아래로 줄고 탭 누르기가 1초 안에 처리됐다.
// 3초 창 두 개를 보는 해상도 낮춤(scene3d.ts)은 그대로 두고, 그보다 먼저 이 예산이 한 번만 가볍게 한다.

/** 이보다 오래 걸린 장을 아주 느린 장으로 본다(밀리초). 60fps의 여섯 배쯤 — 보통 기기에서는 나지 않는다. */
export const SLOW_FRAME_MS = 100;
/** 아주 느린 장이 이만큼 이어지면 가볍게 한다(처음 그리기 준비 같은 한 번 긴 장은 넘긴다). */
export const SLOW_FRAME_RUN = 5;

export interface FrameBudget {
  /** 한 장의 걸린 시간을 넣는다. 이번에 가볍게 해야 하면 `"lighten"`(한 번만). */
  note(frameMs: number): "lighten" | null;
  /** 이미 가볍게 했는가. */
  lightened(): boolean;
}

export function createFrameBudget(): FrameBudget {
  let run = 0;
  let done = false;
  return {
    note(frameMs: number): "lighten" | null {
      if (done) { return null; }
      run = frameMs > SLOW_FRAME_MS ? run + 1 : 0;
      if (run < SLOW_FRAME_RUN) { return null; }
      done = true;
      return "lighten";
    },
    lightened: () => done,
  };
}
