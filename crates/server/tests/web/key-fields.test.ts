// 키 칸 규칙(server 11차 2번): type=password · autocomplete=new-password · spellcheck=false ·
// 성공하면 비움 · 서버 답으로 다시 채우지 않음 · 브라우저 저장소에 비밀 없음.
import { describe, expect, it, vi } from "vitest";
import { applyKeyFieldRules, keyInput, submitKeyFields } from "../../web/src/keys/field";

const entryPages = import.meta.glob<string>(["../../web/index.html", "../../web/keys.html"], {
  query: "?raw",
  import: "default",
  eager: true,
});
const sources = import.meta.glob<string>("../../web/src/**/*.ts", { query: "?raw", import: "default", eager: true });

function expectRules(input: HTMLInputElement) {
  expect(input.type).toBe("password");
  expect(input.getAttribute("autocomplete")).toBe("new-password");
  expect(input.getAttribute("spellcheck")).toBe("false");
}

function filled(values: Record<string, string>): Record<string, HTMLInputElement> {
  const out: Record<string, HTMLInputElement> = {};
  for (const [name, value] of Object.entries(values)) {
    const input = keyInput(`k-${name}`);
    input.value = value;
    out[name] = input;
  }
  return out;
}

describe("키 칸", () => {
  it("새 키 칸은 가린 칸이고 저장 · 맞춤법을 권하지 않는다", () => {
    // 검증: V46 C3
    const input = keyInput("k-test");
    expect(input.id).toBe("k-test");
    expectRules(input);
  });

  it("있는 칸에도 같은 규칙을 건다", () => {
    // 검증: V46 C3
    const input = document.createElement("input");
    input.type = "text";
    applyKeyFieldRules(input);
    expectRules(input);
  });

  it("등록에 성공하면 모든 칸을 비운다", async () => {
    // 검증: V46 C3 C16
    const fields = filled({ client_id: "id-value", client_secret: "secret-value" });
    const send = vi.fn(async () => true);
    await expect(submitKeyFields(fields, send)).resolves.toBe(true);
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith({ client_id: "id-value", client_secret: "secret-value" });
    expect(Object.values(fields).map((f) => f.value)).toEqual(["", ""]);
  });

  it("거절되거나 실패하면 칸을 그대로 둔다(다시 고칠 수 있게)", async () => {
    // 검증: V46 C3 E4
    const fields = filled({ api_key: "typed" });
    await expect(submitKeyFields(fields, async () => false)).resolves.toBe(false);
    expect(fields.api_key.value).toBe("typed");
    await expect(
      submitKeyFields(fields, async () => {
        throw new Error("network");
      }),
    ).resolves.toBe(false);
    expect(fields.api_key.value).toBe("typed");
  });

  it("설정 · 키 화면의 가린 칸은 모두 규칙을 지킨다", () => {
    // 검증: V46 C3
    expect(Object.keys(entryPages).length).toBe(2);
    for (const [name, html] of Object.entries(entryPages)) {
      const doc = new DOMParser().parseFromString(html, "text/html");
      const hidden = [...doc.querySelectorAll<HTMLInputElement>('input[type="password"]')];
      expect(hidden.length, name).toBeGreaterThanOrEqual(3);
      for (const input of hidden) {
        expect(input.getAttribute("autocomplete"), `${name} #${input.id}`).toBe("new-password");
        expect(input.getAttribute("spellcheck"), `${name} #${input.id}`).toBe("false");
      }
    }
  });

  it("화면 코드는 브라우저 저장소 · 쿠키 · 주소에 값을 남기지 않고 HTML로 글을 넣지 않는다", () => {
    // 검증: V46 N8
    expect(Object.keys(sources).length).toBeGreaterThan(0);
    for (const [name, text] of Object.entries(sources)) {
      for (const banned of [
        "localStorage",
        "sessionStorage",
        "indexedDB",
        "document.cookie",
        "innerHTML",
        "outerHTML",
        "insertAdjacentHTML",
        "document.write",
        "eval(",
        "new Function(",
      ]) {
        expect(text.includes(banned), `${name}: ${banned}`).toBe(false);
      }
    }
  });
});
