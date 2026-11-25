// 화면 린트(결정 23): TS 정적 검사. verify.sh 의 "3l 화면 린트" 단계가 `npm run lint`로 돈다.
// 시험(../tests)도 읽으려고 린트는 crates/server 에서 이 파일을 -c 로 주고 돈다 — 아래 경로는 crates/server 기준이다.
// 검사를 끄는 주석(eslint-disable 등)은 쓰지 않는다 — 여기서 인라인 설정을 막고(noInlineConfig), verify.sh 9번이 그 주석도 잡는다.
// 규칙이 틀렸으면 주석으로 끄지 말고 멈춰 알린다(이 파일은 조정자가 고친다).
//
// typescript-eslint 8.72 는 TypeScript 6.1 아래만 읽는다(형 검사 tsc 는 7.0.2 그대로).
// 그래서 린트만 TypeScript 6.0.3(별칭 의존 `typescript-6`)으로 소스를 읽게 `typescript` 이름을 그쪽으로 돌린다.
// typescript-eslint 가 TypeScript 7 을 읽게 되면 이 돌림과 `typescript-6` · package.json overrides 를 지운다.
import { registerHooks } from "node:module";

registerHooks({
  resolve(specifier, context, nextResolve) {
    const to6 = specifier === "typescript" || specifier.startsWith("typescript/");
    return nextResolve(to6 ? specifier.replace(/^typescript/, "typescript-6") : specifier, context);
  },
});
const { default: tseslint } = await import("typescript-eslint");

// 화면에서 쓰지 않는 것(글자 검색 시험도 본다): HTML 을 글로 넣는 길 · 코드를 글로 실행 · 브라우저 저장소.
const HTML = "HTML 글을 그대로 넣지 않는다 — textContent 와 createElement 로 만든다";
const STORE = "브라우저 저장소를 쓰지 않는다 — 상태는 서버(API)에 둔다";
const EVAL = "코드를 글로 실행하지 않는다";
const safety = {
  "no-restricted-properties": [
    "error",
    { property: "innerHTML", message: HTML },
    { property: "outerHTML", message: HTML },
    { property: "insertAdjacentHTML", message: HTML },
    { object: "document", property: "write", message: HTML },
    { object: "document", property: "writeln", message: HTML },
    { object: "document", property: "cookie", message: STORE },
    { property: "localStorage", message: STORE },
    { property: "sessionStorage", message: STORE },
    { property: "indexedDB", message: STORE },
    { property: "eval", message: EVAL },
  ],
  "no-restricted-globals": [
    "error",
    { name: "localStorage", message: STORE },
    { name: "sessionStorage", message: STORE },
    { name: "indexedDB", message: STORE },
    { name: "eval", message: EVAL },
  ],
  "no-eval": "error",
  "no-new-func": "error",
  "@typescript-eslint/no-implied-eval": "error",
};

// 일부러 any 로 둔 이름(three 는 형 정의 @types/three 를 쓰지 않는다 — server 11차 1번 · src/office/three.d.ts,
// 설정 화면은 옛 한 장 화면을 그대로 옮긴 모듈 — server 11차). 그 파일에서도 any 는 그 별칭 하나에만 허락한다.
const ANY_ALIAS_FILES = [
  "office/actors3d.ts", "office/architecture.ts", "office/doll.ts", "office/framing.ts", "office/furniture.ts",
  "office/gltf.ts", "office/player-scene.ts", "office/player3d.ts", "office/scene3d.ts", "setup/main.ts",
].map((f) => `web/src/${f}`);

// 형 정보는 화면 tsconfig(src · ../tests/web · ../tests/e2e 를 품는다) 하나로 읽는다.
const typed = { parserOptions: { project: "./tsconfig.json", tsconfigRootDir: import.meta.dirname } };

export default tseslint.config(
  { ignores: ["web/dist/**", "web/node_modules/**"] },
  { linterOptions: { noInlineConfig: true, reportUnusedDisableDirectives: "error" } },
  {
    // 실행 코드: 형 정보를 쓰는 규칙(typescript-eslint recommendedTypeChecked) + 아래 규칙.
    files: ["web/src/**/*.ts"],
    extends: [tseslint.configs.recommendedTypeChecked],
    languageOptions: typed,
    rules: {
      ...safety,
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/no-misused-promises": "error",
      "@typescript-eslint/no-explicit-any": "error",
      eqeqeq: ["error", "always"],
      // 안 쓰는 지역 이름 · 가져오기는 tsc(noUnusedLocals)가 본다. 여기서는 겹치지 않게 안 쓰는 매개변수 · catch 이름만 본다.
      "@typescript-eslint/no-unused-vars": ["error", {
        vars: "all", varsIgnorePattern: "^", args: "after-used", argsIgnorePattern: "^_",
        caughtErrors: "all", caughtErrorsIgnorePattern: "^_",
      }],
      // 켜지 않은 것(지금 코드를 동작 그대로 고칠 수 없어서 — 결정 23 보고):
      // - no-unsafe-*: three 의 값이 모두 any(위 ANY_ALIAS_FILES 까닭)라 장면 코드 거의 모든 줄에 걸린다.
      // - no-base-to-string: 서버 JSON(unknown)을 String() 으로 바꾸는 곳 — 고치면 객체가 올 때의 글이 달라진다.
      // - no-var: 설정 화면(옛 한 장 화면을 옮긴 모듈)의 var 129곳 — let/const 로 바꾸면 범위가 바뀔 수 있다.
      "@typescript-eslint/no-unsafe-assignment": "off",
      "@typescript-eslint/no-unsafe-member-access": "off",
      "@typescript-eslint/no-unsafe-call": "off",
      "@typescript-eslint/no-unsafe-argument": "off",
      "@typescript-eslint/no-unsafe-return": "off",
      "@typescript-eslint/no-base-to-string": "off",
      "no-var": "off",
    },
  },
  {
    files: ANY_ALIAS_FILES,
    rules: {
      "@typescript-eslint/no-explicit-any": "off",
      "no-restricted-syntax": ["error", {
        selector: ":not(TSTypeAliasDeclaration[id.name=/^(Obj|Data)$/]) > TSAnyKeyword",
        message: "any 는 이 파일의 별칭(type Obj / type Data) 하나에만 쓴다",
      }],
    },
  },
  {
    // 시험(느슨한 묶음): 기다리지 않은 Promise(브라우저 e2e 에서 await 빠짐) · 느슨한 같음만 본다.
    // 위의 금지 속성(innerHTML · localStorage 등)은 보지 않는다 — 화면 시험이 jsdom 준비와 "쓰지 않음" 확인에 그 이름을 쓴다.
    files: ["tests/web/**/*.ts", "tests/e2e/**/*.ts"],
    extends: [tseslint.configs.base],
    languageOptions: typed,
    rules: {
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/no-misused-promises": "error",
      eqeqeq: ["error", "always"],
    },
  },
);
