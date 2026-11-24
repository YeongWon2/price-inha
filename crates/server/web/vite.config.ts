// 화면 빌드(결정 1 · server 11차 · 26차): 설정(`/`) · 키(`/keys`) · 사무실(`/office/`) · 폰(`/phone/`)과
// 관리 화면 넷(`/account/` · `/templates/` · `/rules/` · `/history/`)의 다중 페이지.
// 경로는 이 폴더 기준이다(npm 스크립트는 이 폴더에서 돈다). 빌드 결과 `dist/`는 커밋하고 server가 실행 파일에 넣는다.
// 같은 소스에서 늘 같은 결과(이름의 해시는 내용 해시)가 나와야 한다(verify.sh 3w가 다시 빌드해 대조한다).
import { defineConfig, type Plugin } from "vitest/config";

/** three(MIT)의 라이선스 글을 묶음과 함께 dist에 둔다(server 11차 2번). npm 판의 LICENSE 그대로. */
function threeLicense(): Plugin {
  let root = ".";
  return {
    name: "three-license",
    apply: "build",
    configResolved(config) {
      root = config.root;
    },
    async generateBundle() {
      const source = await this.fs.readFile(`${root}/node_modules/three/LICENSE`, { encoding: "utf8" });
      this.emitFile({ type: "asset", fileName: "office/three-LICENSE.txt", source });
    },
  };
}

export default defineConfig({
  base: "/",
  plugins: [threeLicense()],
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
        // 관리 화면 넷(server 26차 4번): 3D 없이 메뉴 · 공통 부품만 싣는다.
        account: "account/index.html",
        templates: "templates/index.html",
        rules: "rules/index.html",
        history: "history/index.html",
      },
      output: {
        // 이름의 해시는 내용 해시. `-`가 해시 안에 끼지 않게 글자를 좁힌다(`/assets/<이름>-<해시>.js`).
        hashCharacters: "base36",
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
