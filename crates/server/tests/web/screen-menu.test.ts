// 화면 여덟 개 위의 같은 메뉴(server 26차 3번 · R18-1 · V75): HTML마다 메뉴 자리(`#menu[data-screen]`)가 위에 있고
// 그 화면의 main 스크립트를 부르며, main이 메뉴를 단다. 설정 · 키 · 폰은 jsdom에서 main을 띄워 메뉴가 달린 것을 본다
// (사무실은 3D 때문에 소스 글자로 본다). 모든 화면은 토큰 파일을 (직접이든 부품을 거쳐서든) 불러온다.
import { afterEach, describe, expect, it, vi } from "vitest";
import { API } from "../../web/src/paths";
import { SOURCES, mainOf, reach } from "./source-graph";

const PAGES = import.meta.glob<string>(
  [
    "../../web/index.html",
    "../../web/keys.html",
    "../../web/office/index.html",
    "../../web/phone/index.html",
    "../../web/account/index.html",
    "../../web/templates/index.html",
    "../../web/rules/index.html",
    "../../web/history/index.html",
  ],
  { query: "?raw", import: "default", eager: true },
);

/** 화면 이름 → HTML 파일. */
const SCREENS: ReadonlyArray<readonly [string, string]> = [
  ["setup", "../../web/index.html"],
  ["keys", "../../web/keys.html"],
  ["office", "../../web/office/index.html"],
  ["phone", "../../web/phone/index.html"],
  ["account", "../../web/account/index.html"],
  ["templates", "../../web/templates/index.html"],
  ["rules", "../../web/rules/index.html"],
  ["history", "../../web/history/index.html"],
];

const LABELS = ["사무실", "계좌", "템플릿", "규칙", "히스토리", "설정"];

const MAINS = import.meta.glob(["../../web/src/setup/main.ts", "../../web/src/keys/main.ts", "../../web/src/phone/main.ts"]);

function parse(file: string): Document {
  const html = PAGES[file];
  expect(html, `${file}이 있다`).toBeTypeOf("string");
  return new DOMParser().parseFromString(html, "text/html");
}

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

class QuietSocket {
  onopen: (() => void) | null = null;
  onmessage: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  close(): void {}
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("화면 HTML의 메뉴 자리", () => {
  it("여덟 화면의 HTML이 있다", () => {
    // 검증: V75 F12
    expect(Object.keys(PAGES).sort()).toEqual(SCREENS.map(([, file]) => file).sort());
  });

  it("HTML마다 메뉴 자리가 하나이고 data-screen이 그 화면 이름이다", () => {
    // 검증: V75
    for (const [screen, file] of SCREENS) {
      const doc = parse(file);
      const slots = doc.querySelectorAll("#menu");
      expect(slots.length, `${screen}: #menu 하나`).toBe(1);
      expect(slots[0].getAttribute("data-screen"), screen).toBe(screen);
    }
  });

  it("메뉴 자리는 화면 내용(제목 · 본문 · 구역)보다 위에 있다", () => {
    // 검증: V75
    for (const [screen, file] of SCREENS) {
      const doc = parse(file);
      const slot = doc.getElementById("menu");
      expect(slot, screen).not.toBeNull();
      for (const element of doc.body.querySelectorAll("main, section, h1, h2")) {
        const after = (slot as HTMLElement).compareDocumentPosition(element) & Node.DOCUMENT_POSITION_FOLLOWING;
        expect(after, `${screen}: 메뉴가 <${element.tagName.toLowerCase()}>보다 위`).toBeTruthy();
        expect((slot as HTMLElement).contains(element), `${screen}: 메뉴 자리 안에 내용이 없다`).toBe(false);
      }
    }
  });

  it("HTML마다 그 화면의 main 스크립트를 모듈로 부른다", () => {
    // 검증: V75 D11
    for (const [screen, file] of SCREENS) {
      const doc = parse(file);
      const scripts = [...doc.querySelectorAll('script[type="module"]')].map((s) => s.getAttribute("src"));
      expect(scripts, screen).toContain(`/src/${screen}/main.ts`);
      expect(mainOf(screen) in SOURCES, `${screen}: main.ts가 있다`).toBe(true);
    }
  });
});

describe("main이 메뉴와 토큰을 단다", () => {
  it("여덟 화면의 main은 메뉴 부품을 불러 메뉴를 달고, 토큰 파일에 닿는다", () => {
    // 검증: V75 V76
    for (const [screen] of SCREENS) {
      const main = SOURCES[mainOf(screen)] ?? "";
      expect(/\bmountMenu\s*\(/.test(main), `${screen}: main이 mountMenu를 부른다`).toBe(true);
      const { files } = reach(mainOf(screen));
      expect(files.has("../../web/src/ui/menu.ts"), `${screen}: 메뉴 부품에 닿는다`).toBe(true);
      expect(files.has("../../web/src/ui/tokens.css"), `${screen}: 토큰 파일에 닿는다`).toBe(true);
    }
  });

  it("사무실 main은 메뉴를 사무실 화면으로 단다(소스 글자)", () => {
    // 검증: V75
    const main = SOURCES[mainOf("office")] ?? "";
    expect(/\bmountMenu\s*\([^;]*["']office["']\s*\)/.test(main)).toBe(true);
  });

  for (const [screen, file, label] of [
    ["setup", "../../web/index.html", "설정"],
    ["keys", "../../web/keys.html", null],
    ["phone", "../../web/phone/index.html", null],
  ] as const) {
    it(`${screen} 화면을 띄우면 메뉴 자리에 여섯 항목 메뉴가 달린다`, async () => {
      // 검증: V75
      vi.resetModules();
      document.body.innerHTML = parse(file).body.innerHTML;
      vi.stubGlobal("WebSocket", QuietSocket);
      vi.stubGlobal(
        "fetch",
        vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
          const method = (init?.method ?? "GET").toUpperCase();
          if (method === "POST" && String(url) === API.session) return new Response("{}", { status: 200 });
          return new Response("{}", { status: 404 });
        }),
      );
      await MAINS[`../../web/src/${screen}/main.ts`]();
      await settle();
      const nav = document.querySelector("#menu nav");
      expect(nav, `${screen}: #menu 안의 nav`).not.toBeNull();
      expect([...(nav as HTMLElement).querySelectorAll("a")].map((a) => a.textContent?.trim())).toEqual(LABELS);
      const current = [...(nav as HTMLElement).querySelectorAll('[aria-current="page"]')].map((a) => a.textContent?.trim());
      expect(current).toEqual(label ? [label] : []);
    });
  }
});
