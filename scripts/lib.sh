#!/usr/bin/env bash
# 파이프라인 스크립트가 함께 쓰는 표. docs/dev/pipeline.md 6장과 같아야 한다.
# 모든 모듈(그래프 docs/dev/graph.toml 과 같은 순서). 폴더는 crates/<모듈>, 패키지는 pi-<모듈>(server 는 price-inha).
ALL_MODULES="store secrets ai market paper gate server template rules approval exec data trigger agent journal knowledge backup office notify channel"

module_dirs() { # 모듈 → 폴더들
  case " $ALL_MODULES " in
    *" $1 "*) echo "crates/$1" ;;
    *) echo "모르는 모듈: $1" >&2; return 1 ;;
  esac
}

module_pkgs() { # 모듈 → cargo -p 인자
  case "$1" in
    server) echo "-p price-inha" ;;
    *) echo "-p pi-$1" ;;
  esac
}

# 검사하는 모듈: 크레이트가 이미 있는 것만(새 모듈은 그 크레이트를 처음 만드는 커밋부터 들어온다).
MODULES=""
for _m in $ALL_MODULES; do
  [ -f "crates/$_m/Cargo.toml" ] && MODULES="$MODULES $_m"
done
MODULES="${MODULES# }"
unset _m

module_test_dirs() {
  local d out=""
  for d in $(module_dirs "$1"); do out="$out $d/tests"; done
  echo "$out"
}

# 모듈의 화면 프로젝트(Vite). crates/<모듈>/web/package.json 이 있으면 그 폴더. 결정 1(2026-11-23 자리 조정).
# 화면 시험(vitest)은 crates/<모듈>/tests/web/ 에 둔다 — 잠금(red/<모듈>)이 Rust 시험과 같이 덮는다.
module_web() {
  local d
  for d in $(module_dirs "$1"); do [ -f "$d/web/package.json" ] && echo "$d/web"; done
}

web_deps() { # node_modules 를 package-lock 그대로 맞춘다(없거나 lock 이 더 새로우면 npm ci)
  local w="$1"
  if [ ! -f "$w/node_modules/.package-lock.json" ] || [ "$w/package-lock.json" -nt "$w/node_modules/.package-lock.json" ]; then
    (cd "$w" && npm ci --no-audit --no-fund --loglevel=error)
  fi
}

dist_hash() { # 빌드 결과 폴더의 지문(파일 이름 + 내용)
  if [ -d "$1" ]; then (cd "$1" && find . -type f | LC_ALL=C sort | xargs shasum -a 256 2>/dev/null | shasum -a 256 | cut -d' ' -f1)
  else echo none; fi
}

web_dist_fresh() { # 지금 dist 가 소스로 다시 빌드한 것과 같은가
  local w="$1" before after
  before=$(dist_hash "$w/dist")
  (cd "$w" && npm run --silent build) || return 1
  after=$(dist_hash "$w/dist")
  [ "$before" = "$after" ] || { echo "빌드하니 dist 가 바뀌었다 — 고치기: npm --prefix $w run build 결과를 함께 커밋한다"; return 1; }
}

# 화면 린트(결정 23): ESLint + typescript-eslint. 규칙은 <web>/eslint.config.js(src 는 엄격, 시험은 느슨한 묶음).
web_lint() { npm --prefix "$1" run --silent lint; }

# 브라우저 e2e(결정 23)가 쓸 Playwright chromium(headless shell)이 받아져 있는가. 받지 않는다(용량 · 네트워크) — 없으면 실패.
web_e2e_browser() {
  (cd "$1" && node -e '
const fs = require("fs"), os = require("os"), path = require("path");
const list = JSON.parse(fs.readFileSync("node_modules/playwright-core/browsers.json", "utf8")).browsers;
const shell = list.find((b) => b.name === "chromium-headless-shell");
const env = process.env.PLAYWRIGHT_BROWSERS_PATH;
const home = os.homedir();
const base = env && env !== "0" ? env
  : env === "0" ? path.join("node_modules", "playwright-core", ".local-browsers")
  : process.platform === "darwin" ? path.join(home, "Library", "Caches", "ms-playwright")
  : process.platform === "win32" ? path.join(process.env.LOCALAPPDATA || home, "ms-playwright")
  : path.join(process.env.XDG_CACHE_HOME || path.join(home, ".cache"), "ms-playwright");
const dir = path.join(base, `chromium_headless_shell-${shell.revision}`);
if (!fs.existsSync(path.join(dir, "INSTALLATION_COMPLETE"))) {
  console.log(`Playwright chromium(headless shell ${shell.revision})이 없다: ${dir}`);
  process.exit(1);
}')
}

# 브라우저 e2e(결정 23): 시연 서버를 띄워 화면을 기계가 누른다. 실행 파일을 먼저 맞춘다(바뀐 것이 없으면 그대로).
web_e2e() { # web_e2e <모듈> <web 폴더>
  # shellcheck disable=SC2046
  cargo build -q $(module_pkgs "$1") --bins || return 1
  npm --prefix "$2" run --silent e2e
}

# 검사를 끈 곳(화면 · 시험 TS): 린트 · 형 검사를 끄는 주석과 시험 건너뛰기 · 하나만 돌리기. verify.sh 9번.
WEB_OFF_RX='(//|/\*)[[:space:]]*(eslint-disable|eslint-enable|eslint[[:space:]]|@ts-(ignore|nocheck|expect-error))|\b(it|test|describe)\.(only|skip|fixme)\(|\bx(it|test|describe)\('

# 지시서의 "## 필수 번호" 아래 줄에서 번호를 뽑는다.
required_ids() {
  awk '/^## 필수 번호/{p=1;next} /^## /{p=0} p' "docs/dev/modules/$1.md" \
    | grep -oE '\b[A-Z]{1,2}[0-9]+(-[0-9]+)?\b' | sort -u
}

# 테스트의 "// 검증:" 주석에 적힌 번호.
covered_ids() {
  # shellcheck disable=SC2046
  grep -rhE '^\s*// 검증:' $(module_test_dirs "$1") 2>/dev/null \
    | grep -oE '\b[A-Z]{1,2}[0-9]+(-[0-9]+)?\b' | sort -u
}

missing_ids() { comm -23 <(required_ids "$1") <(covered_ids "$1"); }

outside_hosts() { # 테스트가 실제 바깥 주소를 쓰는가
  # shellcheck disable=SC2046
  grep -rnE 'tossinvest\.com|anthropic\.com|openai\.com|googleapis\.com' $(module_test_dirs "$1") 2>/dev/null
}

# 키처럼 생긴 글자. 이 줄 자체가 걸리지 않게 모양을 쪼개 쓴다.
SECRET_RX='sk-ant''-[A-Za-z0-9_-]{20,}|sk''-[A-Za-z0-9]{32,}|gh[pousr]''_[A-Za-z0-9]{30,}|AGE-SECRET''-KEY-1[A-Z0-9]{20,}|-----BEGIN [A-Z ]*PRIVATE'' KEY|eyJ[A-Za-z0-9_-]{20,}\.eyJ[A-Za-z0-9_-]{20,}\.'

find_python() { # tomllib가 있는 Python
  local c
  for c in python3.14 python3.13 python3.12 python3.11 python3; do
    if command -v "$c" >/dev/null && "$c" -c 'import tomllib' 2>/dev/null; then echo "$c"; return; fi
  done
}

# 검사 규칙 전체의 지문. 규칙이 바뀌면 자체 시험을 다시 돌려야 한다.
rules_hash() {
  cat scripts/*.sh scripts/*.py docs/dev/architecture.toml docs/dev/graph.toml Cargo.toml clippy.toml deny.toml 2>/dev/null | shasum -a 256 | cut -d' ' -f1
}

# 공급망 검사기 cargo-deny 의 고정 판(QMIT 결정 8). 발행 14일이 지난 판. GitHub CI 는 쓰지 않는다(2026-11-24 사용자).
DENY_VERSION="0.20.2"

# 지시서에 "## 원본 동작 근거" 절이 있고, 조사 문서를 가리키거나 "원본에 없음"이라고 적었는가.
origin_ok() {
  awk '/^## 원본 동작 근거/{p=1;next} /^## /{p=0} p' "docs/dev/modules/$1.md" \
    | grep -qE 'docs/research/[a-z-]+\.md|원본에 없음'
}
