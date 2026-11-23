// 화면 빌드(결정 1 · server 11차): 설정(`/`) · 키(`/keys`) · 사무실(`/office/`) · 폰(`/phone/`) 네 쪽의 다중 페이지.
// 경로는 이 폴더 기준이다(npm 스크립트는 이 폴더에서 돈다). 빌드 결과 `dist/`는 커밋하고 server가 실행 파일에 넣는다.
// 같은 소스에서 늘 같은 결과(이름의 해시는 내용 해시)가 나와야 한다(verify.sh 3w가 다시 빌드해 대조한다).
import { defineConfig } from "vitest/config";

export default defineConfig({
  base: "/",
  // 해시 없이 그대로 나갈 파일(폰의 sw.js · manifest · 아이콘, 사무실 지도 등)은 public/ 아래 같은 경로에 둔다.
  publicDir: "public",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // 옛 시험이 dist 안에서 같은 뜻의 글을 찾는다(server.md 11차 5번) — 줄이지 않는다.
    minify: false,
    // 모든 대상 브라우저가 modulepreload를 안다. 덧붙는 fetch 코드를 넣지 않는다.
    modulePreload: { polyfill: false },
    rollupOptions: {
      input: {
        setup: "index.html",
        keys: "keys.html",
        office: "office/index.html",
        phone: "phone/index.html",
      },
    },
  },
  // 화면 시험이 이 폴더 밖(../tests/web)에 있다.
  server: { fs: { allow: [".."] } },
  test: {
    // 화면 시험은 잠금이 덮는 crates/server/tests/web/ 에 둔다.
    include: ["../tests/web/**/*.test.ts"],
    environment: "jsdom",
  },
});
