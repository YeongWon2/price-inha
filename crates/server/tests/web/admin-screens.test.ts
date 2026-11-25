// 관리 화면 넷(server 26차 4번 · 6번 · 30차 2번 · F12 · R18-3 · N21 · V77): 계좌 · 템플릿 · 규칙 · 히스토리 화면은
// 메뉴 · 제목과 내용을 그린다("준비 중" 틀은 30차로 남지 않는다). 관리 화면 소스는 3D(three · scene3d)를 부르지 않고,
// 새 npm 의존성이 없다.
import { afterEach, describe, expect, it, vi } from "vitest";
import { API } from "../../web/src/paths";
import { SOURCES, mainOf, reach } from "./source-graph";

const PAGES = import.meta.glob<string>(
  ["../../web/account/index.html", "../../web/templates/index.html", "../../web/rules/index.html", "../../web/history/index.html"],
  { query: "?raw", import: "default", eager: true },
);
const MAINS = import.meta.glob([
  "../../web/src/account/main.ts",
  "../../web/src/templates/main.ts",
  "../../web/src/rules/main.ts",
  "../../web/src/history/main.ts",
]);
const PACKAGE = import.meta.glob<string>("../../web/package.json", { query: "?raw", import: "default", eager: true });

const ADMIN = [
  ["account", "계좌", "/account/"],
  ["templates", "템플릿", "/templates/"],
  ["rules", "규칙", "/rules/"],
  ["history", "히스토리", "/history/"],
] as const;

const LABELS = ["사무실", "계좌", "템플릿", "규칙", "히스토리", "설정"];

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

afterEach(() => {
  vi.unstubAllGlobals();
});

// 27차 3번 · 28차 2번 · 29차 2번 · 30차 2번: 관리 화면 넷 모두 내용이 생긴다(template-screen · rules-screen ·
// history-screen · account-screen 시험) — "준비 중" 틀은 남는 화면이 없다. 메뉴와 제목(h1)은 그대로다.
describe("관리 화면 틀", () => {
  for (const [screen, label, href] of ADMIN) {
    it(`${label} 화면은 메뉴 · 제목을 그리고 "준비 중"이 아닌 내용을 그린다`, async () => {
      // 검증: V75 F12
      const html = PAGES[`../../web/${screen}/index.html`];
      expect(html, `${screen}/index.html이 있다`).toBeTypeOf("string");
      vi.resetModules();
      document.body.innerHTML = new DOMParser().parseFromString(html, "text/html").body.innerHTML;
      const calls: string[] = [];
      vi.stubGlobal(
        "fetch",
        vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
          calls.push(String(url));
          const method = (init?.method ?? "GET").toUpperCase();
          if (method === "POST" && String(url) === API.session) return new Response("{}", { status: 200 });
          return new Response("{}", { status: 404 });
        }),
      );
      await MAINS[`../../web/src/${screen}/main.ts`]();
      await settle();

      const nav = document.querySelector("#menu nav");
      expect(nav, `${screen}: 메뉴`).not.toBeNull();
      expect([...(nav as HTMLElement).querySelectorAll("a")].map((a) => a.textContent?.trim())).toEqual(LABELS);
      const current = [...(nav as HTMLElement).querySelectorAll('[aria-current="page"]')];
      expect(current.map((a) => a.textContent?.trim())).toEqual([label]);
      expect(current[0].getAttribute("href")).toBe(href);

      const titles = [...document.querySelectorAll("h1")];
      expect(titles.length, `${screen}: 제목 하나`).toBe(1);
      expect(titles[0].textContent).toContain(label);

      // "준비 중" 빈 상태는 어느 화면에도 없다.
      expect(document.body.textContent ?? "", `${screen}: "준비 중"이 없다`).not.toContain("준비 중");
      // 내용이 있다: 본문 자리에 제목 · 안내 줄 밖의 그림이 있고, 세션 밖의 서버 경로를 읽는다
      // (이 가짜 서버는 세션 밖을 404로 답하므로 그 내용은 오류 줄일 수 있다).
      const main = document.getElementById("screen") as HTMLElement;
      const drawn = [...main.children].filter((node) => node.tagName !== "H1" && !node.classList.contains("admin-lead"));
      expect(drawn.length, `${screen}: 본문에 그린 것이 있다`).toBeGreaterThan(0);
      expect(
        calls.some((url) => url !== API.session),
        `${screen}: 세션 밖의 경로를 읽는다`,
      ).toBe(true);
    });
  }
});

describe("관리 화면은 가볍다", () => {
  it("관리 화면 main은 three · 사무실 3D(scene3d) · 사무실 소스를 부르지 않는다(소스 글자)", () => {
    // 검증: V77 N21
    for (const [screen] of ADMIN) {
      expect(mainOf(screen) in SOURCES, `${screen}: main.ts가 있다`).toBe(true);
      const { files, bare } = reach(mainOf(screen));
      expect([...bare].filter((name) => name === "three" || name.startsWith("three/")), screen).toEqual([]);
      expect([...files].filter((file) => file.includes("scene3d") || file.includes("/src/office/")), screen).toEqual([]);
      // 메뉴 부품을 거쳐야 이 검사가 뜻을 가진다.
      expect(files.has("../../web/src/ui/menu.ts"), `${screen}: 메뉴 부품에 닿는다`).toBe(true);
    }
  });

  it("사무실 main은 3D를 부른다(위 검사가 뜻을 가지는지)", () => {
    // 검증: V77
    const { files, bare } = reach(mainOf("office"));
    expect([...files].some((file) => file.includes("scene3d")) || bare.has("three")).toBe(true);
  });

  it("새 npm 의존성을 더하지 않는다", () => {
    // 검증: N21
    const pkg = JSON.parse(Object.values(PACKAGE)[0]) as {
      dependencies?: Record<string, string>;
      devDependencies?: Record<string, string>;
    };
    expect(Object.keys(pkg.dependencies ?? {}).sort()).toEqual(["three"]);
    // 36차(결정 23): 린트 · 브라우저 e2e 개발 의존성 넷이 더해졌다. 실행 의존성은 위처럼 three 하나다.
    expect(Object.keys(pkg.devDependencies ?? {}).sort()).toEqual([
      "@playwright/test",
      "eslint",
      "jsdom",
      "typescript",
      "typescript-6",
      "typescript-eslint",
      "vite",
      "vitest",
    ]);
  });
});
