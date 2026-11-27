// (백로그 T 6번) 그리기 장치 가리기: WebGL이 소프트웨어 그리기(그래픽 장치 없이 CPU로 그림 — 헤드리스 브라우저 ·
// 가상 머신 · 그래픽 드라이버가 막힌 기기)면 3D 장면을 처음부터 가볍게(그림자 끄기 · 해상도 최저) 그린다. 그래야
// 그리기가 주 스레드를 막아 탭 누르기 같은 입력이 밀리지 않고, 나중에 가볍게 바꾸며 셰이더를 다시 짓지도 않는다.

/** 소프트웨어 그리기 장치 이름에 드는 낱말(대소문자 무시). */
const SOFTWARE_NAMES = /swiftshader|llvmpipe|softpipe|software|basic render/i;

/** WebGL 그리기 장치 이름이 소프트웨어 그리기인가. 모르는 이름 · 빈 이름은 아니다. */
export function softwareRenderer(name: string): boolean {
  return SOFTWARE_NAMES.test(name);
}

/** WebGL 맥락의 그리기 장치 이름(가려진 이름을 알 수 없으면 보이는 이름). 읽지 못하면 빈 글. */
export function rendererName(gl: WebGLRenderingContext): string {
  try {
    const info = gl.getExtension("WEBGL_debug_renderer_info");
    const name: unknown = info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER);
    return typeof name === "string" ? name : "";
  } catch {
    return "";
  }
}
