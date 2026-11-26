// 다듬기 E 10번: 도구 줄의 "따라가기"가 켜져 있으면 "자동 카메라" 단추(`#auto-camera`)를 흐리게 하고(`data-resting="true"`)
// 도움말(title)로 까닭을 단다. 동작(눌림 · 켜고 끄기)은 그대로다 — `aria-pressed` · 글은 건드리지 않는다.
// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { AUTO_CAMERA_REST_HINT, mountToolbar } from "../../web/src/office/toolbar";
import { CSS_SOURCES } from "./source-graph";

function autoCamera(): HTMLButtonElement {
  const button = document.createElement("button");
  button.id = "auto-camera";
  button.setAttribute("aria-pressed", "true");
  button.textContent = "자동 카메라 켜짐";
  document.body.append(button);
  return button;
}

afterEach(() => {
  document.body.replaceChildren();
});

describe("자동 카메라와 따라가기", () => {
  it("따라가기가 켜져 있으면 자동 카메라를 흐리게 하고 도움말을 단다", () => {
    const button = autoCamera();
    const stage = document.createElement("div");
    document.body.append(stage);
    mountToolbar(stage, () => undefined, true);
    expect(button.dataset.resting).toBe("true");
    expect(button.title).toBe(AUTO_CAMERA_REST_HINT);
    expect(AUTO_CAMERA_REST_HINT).toBe("내 캐릭터를 따라가는 중에는 자동 카메라가 쉽니다");
    expect(button.getAttribute("aria-pressed"), "동작은 그대로").toBe("true");
    expect(button.textContent).toBe("자동 카메라 켜짐");
  });

  it("따라가기를 끄면 흐림과 도움말을 걷는다", () => {
    const button = autoCamera();
    const stage = document.createElement("div");
    document.body.append(stage);
    const toolbar = mountToolbar(stage, () => undefined, true);
    toolbar.setFollowing(false);
    expect(button.dataset.resting).toBe("false");
    expect(button.title).toBe("");
    toolbar.setFollowing(true);
    expect(button.dataset.resting).toBe("true");
  });

  it("자동 카메라 단추가 없어도 도구 줄은 선다", () => {
    const stage = document.createElement("div");
    document.body.append(stage);
    const toolbar = mountToolbar(stage, () => undefined, false);
    expect(() => toolbar.setFollowing(true)).not.toThrow();
  });

  it("흐린 자동 카메라는 사무실 모양에서 흐리게 그린다", () => {
    const css = CSS_SOURCES["../../web/src/office/office.css"];
    const rule = /#auto-camera\[data-resting="true"\]\s*\{([^}]*)\}/.exec(css);
    expect(rule, "흐림 규칙이 있다").not.toBeNull();
    expect(rule?.[1]).toMatch(/opacity:\s*0?\.\d+/);
  });
});
