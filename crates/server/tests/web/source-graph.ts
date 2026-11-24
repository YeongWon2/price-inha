// 화면 소스의 import 따라가기(server 26차 시험 도우미): 화면 main.ts에서 시작해 상대 import(ts · css)를 따라간다.
// 빌드 결과를 보지 않고 소스 글자만 읽는다.

interface TextFs {
  readFileSync(path: URL, encoding: "utf8"): string;
}

/**
 * 이 폴더 기준 상대 경로의 파일 글. vitest는 CSS를 `?raw`로 불러도 빈 글로 바꾸므로(CSS 처리 끔) 파일을 직접 읽는다.
 * (이 저장소의 화면 의존성에는 Node 형 정의가 없어 내장 모듈을 이름으로 받는다.)
 */
export function readText(relative: string): string {
  const host = globalThis as unknown as { process: { getBuiltinModule(name: string): unknown } };
  const fs = host.process.getBuiltinModule("node:fs") as TextFs;
  return fs.readFileSync(new URL(relative, import.meta.url), "utf8");
}

/** 화면 CSS 글: 키는 "../../web/src/<폴더>/<파일>.css". */
export const CSS_SOURCES: Record<string, string> = Object.fromEntries(
  Object.keys(import.meta.glob("../../web/src/**/*.css")).map((key) => [key, readText(key)]),
);

/** 화면 소스(ts · css) 글: 키는 "../../web/src/<폴더>/<파일>". */
export const SOURCES: Record<string, string> = {
  ...import.meta.glob<string>("../../web/src/**/*.ts", { query: "?raw", import: "default", eager: true }),
  ...CSS_SOURCES,
};

const PREFIX = "../../web/src/";

/** 한 파일이 부르는 import 글자(정적 · 늦게 부르기 모두). */
function specifiers(text: string): string[] {
  const out: string[] = [];
  const patterns = [
    /\bimport\s+(?:[^"'`;]*?\s+from\s+)?["']([^"']+)["']/g,
    /\bexport\s+[^"'`;]*?\s+from\s+["']([^"']+)["']/g,
    /\bimport\s*\(\s*["']([^"']+)["']\s*\)/g,
  ];
  for (const pattern of patterns) for (const match of text.matchAll(pattern)) out.push(match[1]);
  return out;
}

/** "../../web/src/a/b.ts" 기준으로 상대 글자를 SOURCES의 키로 푼다. 못 풀면 null. */
function resolve(from: string, spec: string): string | null {
  const parts = from.slice(PREFIX.length).split("/");
  parts.pop();
  for (const piece of spec.split("/")) {
    if (piece === "" || piece === ".") continue;
    if (piece === "..") parts.pop();
    else parts.push(piece);
  }
  const base = PREFIX + parts.join("/");
  for (const candidate of [base, `${base}.ts`, `${base}/index.ts`]) {
    if (candidate in SOURCES) return candidate;
  }
  return null;
}

/** 시작 파일에서 닿는 화면 소스 파일들과, 그 사이에 부른 바깥 이름(예: "three"). */
export function reach(start: string): { files: Set<string>; bare: Set<string> } {
  const files = new Set<string>();
  const bare = new Set<string>();
  const queue = [start];
  while (queue.length > 0) {
    const file = queue.pop() as string;
    if (files.has(file) || !(file in SOURCES)) continue;
    files.add(file);
    for (const spec of specifiers(SOURCES[file])) {
      if (spec.startsWith(".")) {
        const next = resolve(file, spec);
        if (next) queue.push(next);
      } else {
        bare.add(spec);
      }
    }
  }
  return { files, bare };
}

/** 화면 이름 → main.ts의 SOURCES 키. */
export function mainOf(screen: string): string {
  return `${PREFIX}${screen}/main.ts`;
}
