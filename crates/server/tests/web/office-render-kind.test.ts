// 3D 장면을 처음부터 가볍게 그릴지(백로그 T 6번): WebGL이 소프트웨어 그리기(SwiftShader · llvmpipe 등)면 그림자를 끄고
// 해상도를 최저로 시작한다 — 그리기가 주 스레드를 막아 탭 누르기가 밀리지 않게, 나중에 셰이더를 다시 짓지 않게.
// 장면 없이 시험하는 순수 함수.
import { describe, expect, it } from "vitest";
import { softwareRenderer } from "../../web/src/office/render-kind";

describe("그리기 장치 가리기", () => {
  it("소프트웨어 그리기 이름을 알아본다", () => {
    for (const name of [
      "ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device (Subzero) (0x0000C0DE)), SwiftShader driver)",
      "Google SwiftShader",
      "llvmpipe (LLVM 15.0.7, 256 bits)",
      "Mesa softpipe",
      "Microsoft Basic Render Driver",
      "Software Rasterizer",
    ]) {
      expect(softwareRenderer(name), name).toBe(true);
    }
  });

  it("그래픽 장치 이름 · 모르는 이름은 소프트웨어로 보지 않는다", () => {
    for (const name of [
      "ANGLE (Apple, ANGLE Metal Renderer: Apple M2, Unspecified Version)",
      "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)",
      "Mali-G78",
      "WebKit WebGL",
      "",
    ]) {
      expect(softwareRenderer(name), name).toBe(false);
    }
  });
});
