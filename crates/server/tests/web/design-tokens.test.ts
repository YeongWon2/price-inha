// 디자인 토큰 한 벌(server 26차 1번 · 5번 · R18-2 · N21 · V76): 토큰 파일에 라이트(:root)와 다크
// (`prefers-color-scheme: dark`) 값이 모두 있고, 라이트 색은 원본 DeskRPG 계열(docs/research/deskrpg.md 11절),
// 본문 글자 대비는 4.5:1 이상. 화면 CSS는 날 색 값(#rrggbb · rgb( · hsl()을 쓰지 않고 토큰(var(--…))만 쓴다.
// 사무실(office.css)은 위 줄(#bar · .conn)만 본다 — 3D 층의 원본 고정 색은 예외다.
import { describe, expect, it } from "vitest";
import { CSS_SOURCES as CSS } from "./source-graph";
const TOKENS_FILE = "../../web/src/ui/tokens.css";
const OFFICE_FILE = "../../web/src/office/office.css";

/** 색 토큰: 바탕 · 면 · 글자 · 흐린 글자 · 테두리 · 강조 · 위험 · 경고 · 성공. */
const COLORS = ["bg", "surface", "text", "text-muted", "border", "accent", "danger", "warning", "success"];
/** 글자 크기 · 간격 단계 · 둥글기. */
const SCALES = [
  "font-size-sm",
  "font-size-md",
  "font-size-lg",
  "font-size-xl",
  "space-1",
  "space-2",
  "space-3",
  "space-4",
  "space-5",
  "radius-sm",
  "radius-md",
];
/** 그림자. */
const SHADOWS = ["shadow-sm", "shadow-md"];
const ALL_TOKENS = [...COLORS, ...SCALES, ...SHADOWS];

const strip = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, "");

/** 다크 블록 안 글과 그 밖의 글(라이트)로 나눈다. */
function split(css: string): { light: string; dark: string } {
  const text = strip(css);
  const at = text.search(/@media[^{]*prefers-color-scheme\s*:\s*dark[^{]*\{/);
  if (at < 0) return { light: text, dark: "" };
  const open = text.indexOf("{", at);
  let depth = 0;
  let end = open;
  for (; end < text.length; end++) {
    if (text[end] === "{") depth++;
    if (text[end] === "}" && --depth === 0) break;
  }
  return { light: text.slice(0, at) + text.slice(end + 1), dark: text.slice(open + 1, end) };
}

/** `:root { … }` 블록 안의 사용자 속성(--이름: 값). */
function rootVars(text: string): Map<string, string> {
  const out = new Map<string, string>();
  for (const block of text.matchAll(/:root\s*\{([^{}]*)\}/g)) {
    for (const decl of block[1].matchAll(/--([\w-]+)\s*:\s*([^;]+);?/g)) out.set(decl[1], decl[2].trim());
  }
  return out;
}

function tokens(): { light: Map<string, string>; dark: Map<string, string> } {
  const css = CSS[TOKENS_FILE];
  expect(css, "web/src/ui/tokens.css가 있다").toBeTypeOf("string");
  const { light, dark } = split(css);
  return { light: rootVars(light), dark: rootVars(dark) };
}

function rgb(hex: string): [number, number, number] {
  const match = /^#([0-9a-f]{6})$/i.exec(hex.trim());
  expect(match, `${hex}는 #rrggbb`).not.toBeNull();
  const n = parseInt((match as RegExpExecArray)[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** WCAG 상대 휘도. */
function luminance(hex: string): number {
  const [r, g, b] = rgb(hex).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** 가장 안쪽 규칙 블록들: [선택자, 선언 글]. */
function rules(css: string): Array<[string, string]> {
  return [...strip(css).matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => [m[1].trim(), m[2]]);
}

/** 선언 값 안의 날 색 값. */
function rawColors(declarations: string): string[] {
  const out: string[] = [];
  for (const decl of declarations.split(";")) {
    const colon = decl.indexOf(":");
    if (colon < 0) continue;
    const value = decl.slice(colon + 1);
    for (const m of value.matchAll(/#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?)\s*\(/g)) out.push(`${decl.trim()} (${m[0]})`);
  }
  return out;
}

describe("디자인 토큰 파일", () => {
  it("라이트(:root)에 색 · 글자 크기 · 간격 · 둥글기 · 그림자 토큰이 모두 있다", () => {
    // 검증: V76 N21
    const { light } = tokens();
    for (const name of ALL_TOKENS) {
      expect(light.get(name)?.length ?? 0, `라이트 --${name}`).toBeGreaterThan(0);
    }
  });

  it("다크(prefers-color-scheme: dark)에 색 · 그림자 토큰이 같은 이름으로 모두 있다", () => {
    // 검증: V76 N21
    const { dark } = tokens();
    for (const name of [...COLORS, ...SHADOWS]) {
      expect(dark.get(name)?.length ?? 0, `다크 --${name}`).toBeGreaterThan(0);
    }
  });

  it("라이트 색은 원본 DeskRPG 계열이다(크림 바탕 · 연녹색 테두리 · 녹색 글자와 강조)", () => {
    // 검증: V76 N21
    const { light } = tokens();
    const value = (name: string) => (light.get(name) ?? "").toLowerCase();
    expect(["#fcfcf4", "#fffdf4"]).toContain(value("bg"));
    expect(["#fcfcf4", "#fffdf4"]).toContain(value("surface"));
    expect(["#344b3c", "#3e513f"]).toContain(value("text"));
    expect(["#cfd9c8", "#cdd7c7"]).toContain(value("border"));
    expect(value("accent")).toBe("#4e745b");
  });

  it("색 토큰은 #rrggbb이고, 다크는 바탕이 어둡고 글자가 밝다", () => {
    // 검증: V76
    const { light, dark } = tokens();
    for (const name of COLORS) {
      rgb(light.get(name) ?? "");
      rgb(dark.get(name) ?? "");
    }
    expect(luminance(dark.get("bg") ?? "")).toBeLessThan(luminance(dark.get("text") ?? ""));
    expect(luminance(light.get("bg") ?? "")).toBeGreaterThan(luminance(light.get("text") ?? ""));
    expect((dark.get("bg") ?? "").toLowerCase()).not.toBe((light.get("bg") ?? "").toLowerCase());
  });

  it("본문 글자 · 흐린 글자는 바탕 · 면 위에서 대비 4.5:1 이상이다(라이트 · 다크)", () => {
    // 검증: V76
    const { light, dark } = tokens();
    for (const [mode, set] of [
      ["라이트", light],
      ["다크", dark],
    ] as const) {
      for (const fg of ["text", "text-muted"]) {
        for (const bg of ["bg", "surface"]) {
          const ratio = contrast(set.get(fg) ?? "", set.get(bg) ?? "");
          expect(ratio, `${mode} --${fg} / --${bg} = ${ratio.toFixed(2)}`).toBeGreaterThanOrEqual(4.5);
        }
      }
    }
  });
});

describe("화면 CSS는 토큰만 쓴다", () => {
  it("설정 · 키 · 폰 · 관리 화면 · 공통 부품 CSS에 날 색 값이 없다", () => {
    // 검증: V76 N21
    const checked = Object.keys(CSS).filter((file) => file !== TOKENS_FILE && file !== OFFICE_FILE);
    for (const must of ["setup/setup.css", "keys/keys.css", "phone/phone.css"]) {
      expect(checked.some((file) => file.endsWith(must)), must).toBe(true);
    }
    for (const file of checked) {
      const found = rules(CSS[file]).flatMap(([, body]) => rawColors(body));
      expect(found, file).toEqual([]);
    }
  });

  it("화면 CSS는 토큰 이름을 다시 정의하지 않는다(사무실 포함)", () => {
    // 검증: V76
    for (const [file, css] of Object.entries(CSS)) {
      if (file === TOKENS_FILE) continue;
      const defined = [...strip(css).matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1].slice(2));
      expect(defined.filter((name) => ALL_TOKENS.includes(name)), file).toEqual([]);
    }
  });

  it("사무실 위 줄(#bar · .conn)은 날 색 값 없이 토큰 이름만 쓴다(3D 층은 예외)", () => {
    // 검증: V76
    const css = CSS[OFFICE_FILE];
    expect(css, "office.css가 있다").toBeTypeOf("string");
    const top = rules(css).filter(([selector]) => /#bar\b|\.conn\b/.test(selector));
    expect(top.length, "위 줄 규칙이 있다").toBeGreaterThan(0);
    for (const [selector, body] of top) {
      expect(rawColors(body), selector).toEqual([]);
      const used = [...body.matchAll(/var\(\s*--([\w-]+)/g)].map((m) => m[1]);
      for (const name of used) expect(ALL_TOKENS, `${selector}: var(--${name})`).toContain(name);
    }
  });
});
