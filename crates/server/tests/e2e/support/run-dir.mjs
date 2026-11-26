// 브라우저 e2e 실행마다 따로 쓰는 임시 폴더(기능 H 3번, playwright.config.mjs 의 PI_E2E_RUN_DIR)를 정리하는 보고기.
// - 모두 통과하면 지운다(trace · 스크린샷 · 시연 서버 로그). Playwright 가 보고를 마친 뒤에도 그 폴더에
//   `.last-run.json`을 쓰므로 지우기는 프로세스가 끝날 때(exit) 한다.
// - 실패 · 중단이면 남기고 그 경로를 알린다(실패한 시험의 trace 와 시연 서버 로그가 그 안에 있다).
import { existsSync, rmSync } from "node:fs";

export default class RunDirReporter {
  printsToStdio() {
    return false;
  }

  onEnd(result) {
    const dir = process.env.PI_E2E_RUN_DIR;
    if (!dir || !existsSync(dir)) return;
    if (result.status === "passed") {
      process.once("exit", () => rmSync(dir, { recursive: true, force: true }));
      return;
    }
    console.log(`\ne2e 임시 폴더(남김 — trace · 스크린샷: results/, 시연 서버 로그: server.log): ${dir}\n`);
  }
}
