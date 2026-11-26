// 다듬기 E 2번: 옛 "자료 키"(item "data")는 화면에 칸이 없고, 키 화면(`/keys`)은 자료 키를 DART · ECOS · FRED · 네이버
// 칸으로 넣는다는 한 줄을 보인다. 서버가 옛 항목을 `retired_item`으로 거절하면 두 화면이 그 까닭을 글로 보인다.
import { describe, expect, it } from "vitest";

const pages = import.meta.glob<string>(["../../web/index.html", "../../web/keys.html"], {
  query: "?raw",
  import: "default",
  eager: true,
});
const screens = import.meta.glob<string>(["../../web/src/keys/main.ts", "../../web/src/setup/main.ts"], {
  query: "?raw",
  import: "default",
  eager: true,
});

/** 자료 키 안내 한 줄. */
const GUIDE = "자료 키는 DART · ECOS · FRED · 네이버 칸으로 넣습니다";

describe("옛 자료 키 안내", () => {
  it("키 화면은 자료 키를 새 칸으로 넣는다고 한 줄 안내한다", () => {
    const keys = pages["../../web/keys.html"];
    expect(keys).toContain(GUIDE);
  });

  it("두 화면 모두 옛 자료 키 칸(item \"data\")이 없다", () => {
    for (const [path, page] of Object.entries(pages)) {
      expect(page, path).not.toMatch(/item["']?\s*[:=]\s*["']data["']/);
    }
  });

  it("두 화면 모두 retired_item 거절을 글로 옮긴다", () => {
    for (const [path, source] of Object.entries(screens)) {
      expect(source, path).toMatch(/"?retired_item"?:\s*"[^"]*DART[^"]*"/);
    }
  });
});
