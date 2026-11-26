// 브라우저 e2e 전역 준비(결정 23): 이미 빌드된 실행 파일로 시연 서버를 띄운다.
// - 임시 폴더 하나(HOME 과 --home 모두 그 안)를 만들고, 빈 포트(7899 는 쓰지 않는다)의 127.0.0.1 에 --demo 로 띄운다.
// - AI · 증권사 키처럼 보이는 환경 변수는 넘기지 않는다(ANTHROPIC_API_KEY 등). 시연 모드는 가짜 시세 · 가짜 AI · 모의계좌다.
// - 바깥 요청 기록: 서버의 HTTP(S)_PROXY 를 이 프로세스의 기록용 프록시로 돌려, 루프백 밖으로 나가려던 요청을
//   (프록시 설정을 따르는 HTTP 클라이언트라면) 막고 적는다. 하나라도 있으면 끝낼 때 실패로 알린다.
// - 끝나면 서버를 끄고 임시 폴더를 지운다. 서버 로그는 실행마다 따로 쓰는 저장소 밖 임시 폴더
//   (PI_E2E_RUN_DIR/server.log — 통과하면 support/run-dir.mjs 가 지우고, 실패하면 남겨 경로를 알린다)에 남긴다.
import { spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdtempSync, openSync, readFileSync, rmSync } from "node:fs";
import http from "node:http";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../../../..");
const READY_MS = 30_000;
const SECRET_ENV = /(API_KEY|TOKEN|SECRET|PASSWORD|CLIENT_ID)/i;

function binary() {
  if (process.env.PI_E2E_BIN) return process.env.PI_E2E_BIN;
  const target = process.env.CARGO_TARGET_DIR ?? path.join(ROOT, "target");
  return path.join(target, "debug", "price-inha");
}

/** 127.0.0.1 의 빈 포트(7899 는 다른 용도라 피한다). */
async function freePort() {
  for (;;) {
    const port = await new Promise((resolve, reject) => {
      const s = net.createServer();
      s.once("error", reject);
      s.listen(0, "127.0.0.1", () => {
        const p = s.address().port;
        s.close(() => resolve(p));
      });
    });
    if (port !== 7899) return port;
  }
}

/** 바깥으로 나가려는 요청을 막고 적는 프록시. */
async function recordingProxy(seen) {
  const proxy = http.createServer((req, res) => {
    seen.push(`${req.method} ${req.url}`);
    res.writeHead(403).end();
  });
  proxy.on("connect", (req, socket) => {
    seen.push(`CONNECT ${req.url}`);
    socket.end("HTTP/1.1 403 Forbidden\r\n\r\n");
  });
  await new Promise((resolve) => proxy.listen(0, "127.0.0.1", resolve));
  return proxy;
}

function tail(file, lines = 30) {
  try {
    return readFileSync(file, "utf8").trimEnd().split("\n").slice(-lines).join("\n");
  } catch {
    return "(로그 없음)";
  }
}

async function waitReady(base, child, log) {
  const until = Date.now() + READY_MS;
  while (Date.now() < until) {
    if (child.exitCode !== null) throw new Error(`시연 서버가 바로 끝났다(종료 코드 ${child.exitCode}):\n${tail(log)}`);
    try {
      const res = await fetch(`${base}/`);
      if (res.ok) return;
    } catch {
      // 아직 듣지 않음
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`시연 서버가 ${READY_MS / 1000}초 안에 뜨지 않았다:\n${tail(log)}`);
}

export default async function globalSetup() {
  const bin = binary();
  if (!existsSync(bin)) {
    throw new Error(`실행 파일이 없다: ${bin} — 고치기: cargo build -p price-inha (verify.sh 는 e2e 앞에서 빌드한다)`);
  }
  const tmp = mkdtempSync(path.join(os.tmpdir(), "pi-e2e-"));
  const home = path.join(tmp, "home");
  const log = path.join(tmp, "server.log");
  const seen = [];
  const proxy = await recordingProxy(seen);
  const proxyUrl = `http://127.0.0.1:${proxy.address().port}`;
  const port = await freePort();
  const base = `http://127.0.0.1:${port}`;

  const env = {};
  for (const [k, v] of Object.entries(process.env)) if (!SECRET_ENV.test(k)) env[k] = v;
  Object.assign(env, {
    HOME: tmp,
    HTTP_PROXY: proxyUrl, HTTPS_PROXY: proxyUrl, ALL_PROXY: proxyUrl,
    http_proxy: proxyUrl, https_proxy: proxyUrl, all_proxy: proxyUrl,
    NO_PROXY: "127.0.0.1,localhost,::1", no_proxy: "127.0.0.1,localhost,::1",
  });
  const out = openSync(log, "a");
  const child = spawn(bin, ["run", "--home", home, "--bind", "127.0.0.1", "--port", String(port), "--demo"], {
    cwd: tmp, env, stdio: ["ignore", out, out],
  });
  const kill = () => { if (child.exitCode === null) child.kill("SIGKILL"); };
  process.on("exit", kill);

  try {
    await waitReady(base, child, log);
  } catch (e) {
    kill();
    proxy.close();
    rmSync(tmp, { recursive: true, force: true });
    throw e;
  }
  process.env.PI_E2E_BASE_URL = base;

  return async function globalTeardown() {
    if (child.exitCode === null) {
      const exited = new Promise((r) => child.once("exit", r));
      child.kill("SIGTERM");
      await Promise.race([exited, new Promise((r) => setTimeout(r, 5_000))]);
      kill();
    }
    proxy.close();
    try {
      // 실행마다 따로 쓰는 폴더(playwright.config.mjs 의 PI_E2E_RUN_DIR — 기능 H 3번)에 남긴다.
      const runDir = process.env.PI_E2E_RUN_DIR ?? mkdtempSync(path.join(os.tmpdir(), "price-inha-e2e-"));
      copyFileSync(log, path.join(runDir, "server.log"));
    } catch {
      // 로그를 못 남겨도 시험 결과와는 무관
    }
    rmSync(tmp, { recursive: true, force: true });
    if (seen.length > 0) {
      throw new Error(`시연 서버가 바깥 주소로 요청을 보내려 했다(기록 프록시): ${seen.join(" · ")}`);
    }
  };
}
