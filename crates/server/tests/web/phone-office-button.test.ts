// 폰 "3D 사무실" 단추(server 15차 1번): 누르면 사무실 `/office/`로 간다(설정 화면 `/`가 아니다).
import { beforeAll, describe, expect, it, vi } from "vitest";

const pages = import.meta.glob<string>("../../web/phone/index.html", { query: "?raw", import: "default", eager: true });

const place = { pathname: "/phone/", hash: "", href: "http://localhost/phone/" };

beforeAll(async () => {
  const html = Object.values(pages)[0];
  document.body.innerHTML = new DOMParser().parseFromString(html, "text/html").body.innerHTML;
  vi.stubGlobal("location", place);
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response("{}", { status: 404 })),
  );
  await import("../../web/src/phone/main");
});

describe("폰 3D 사무실 단추", () => {
  it("누르면 /office/로 간다", () => {
    // 검증: V10
    expect(Object.keys(pages).length).toBe(1);
    (document.querySelector('[data-action="open-office"]') as HTMLElement).click();
    expect(place.href).toBe("/office/");
  });
});
