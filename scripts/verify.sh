#!/usr/bin/env bash
# G2 기계 검사. 0으로 끝나야 "끝났다"고 말할 수 있다. docs/dev/pipeline.md 5장.
#   scripts/verify.sh            전체
#   scripts/verify.sh <모듈>...  그 모듈만 (다른 모듈이 아직 red일 때)
#   scripts/verify.sh --affected [--base <ref>]  바뀐 모듈과 그것에 기대는 모듈만 (scripts/affected.py)
set -uo pipefail
cd "$(dirname "$0")/.."
. scripts/lib.sh
fail=0
ok()  { printf '✓ %s\n' "$*"; }
bad() { printf '✗ %s\n' "$*"; fail=1; }
run() { # run <이름> <명령...>
  local name="$1"; shift
  if out=$("$@" 2>&1); then ok "$name"; else bad "$name"; printf '%s\n' "$out" | tail -60; fi
}
run_fix() { # run_fix <이름> <고치기> <명령...>: 실패하면 고치는 법을 함께 말한다
  local name="$1" fix="$2"; shift 2
  if out=$("$@" 2>&1); then ok "$name"; else bad "$name — 고치기: $fix"; printf '%s\n' "$out" | tail -60; fi
}

picked=0 # 모듈을 사람이 골랐는가(그때만 red 태그 없음을 실패로 본다)
full=0   # 전체 범위인가(모듈 밖의 검사 — 11 설치 스크립트 — 는 그때만 돈다)
if [ "${1:-}" = "--affected" ]; then
  shift
  PY=$(find_python)
  [ -n "$PY" ] || { echo "✗ --affected: Python 3.11 이상이 없다"; exit 2; }
  aff=$("$PY" scripts/affected.py "$@") || exit 2
  case "$aff" in
    ALL) set -- ;;
    "") echo "범위: 바뀐 코드 없음 (영향 범위) — 검사할 모듈이 없다"; exit 0 ;;
    # shellcheck disable=SC2086
    *) echo "영향 범위: $aff"; set -- $aff ;;
  esac
elif [ "$#" -gt 0 ]; then picked=1; fi

if [ "$#" -gt 0 ]; then
  mods="$*"; pkgs=""; only=""; srcdirs=""
  for m in $mods; do
    pkgs="$pkgs $(module_pkgs "$m")" || exit 2
    for d in $(module_dirs "$m"); do only="$only,$d"; srcdirs="$srcdirs $d"; done
  done
  fmtpkgs="$pkgs"
  echo "범위: $mods"
else
  mods="$MODULES"; pkgs="--workspace"; only=""; srcdirs="crates"; fmtpkgs="--all"; full=1
  echo "범위: 전체"
fi

# 0 검사기 자체 시험이 지금 규칙으로 통과했는가
if [ "${PI_SELFTEST:-0}" != "1" ]; then
  now=$(rules_hash)
  if [ "$(cat .pipeline/selftest.ok 2>/dev/null)" = "$now" ]; then ok "0 검사기 자체 시험 (지금 규칙으로 통과함)"
  else bad "0 검사기 자체 시험이 없거나 낡았다 — 고치기: python3.14 scripts/selftest.py 를 돌려 통과시킨다"; fi
  # 참고(실패 아님): 로컬 pre-push 관문(QMIT 결정 4)이 켜져 있는가
  if [ -z "${CI:-}" ] && [ "$(git config core.hooksPath 2>/dev/null)" != ".githooks" ]; then
    echo "- 참고: pre-push 관문이 꺼져 있다 — 켜기: git config core.hooksPath .githooks"
  fi
fi

# shellcheck disable=SC2086
run "1 모양 (cargo fmt)" cargo fmt $fmtpkgs --check
# shellcheck disable=SC2086
run "2a clippy (실행 코드, 엄격)" cargo clippy $pkgs --lib --bins -- -D warnings
# shellcheck disable=SC2086
run "2b clippy (테스트)" cargo clippy $pkgs --tests -- -D warnings \
  -A clippy::unwrap_used -A clippy::expect_used -A clippy::panic -A missing_docs
# shellcheck disable=SC2086
run "3 테스트" cargo test $pkgs

# 3w 화면(Vite): 의존성 · 형 검사 · vitest · 빌드한 dist 가 커밋된 것과 같은가
for m in $mods; do
  for w in $(module_web "$m"); do
    if ! command -v npm >/dev/null; then bad "3w 화면: npm 없음 ($m) — 고치기: Node.js LTS 를 설치한다"; continue; fi
    run "3w 화면 의존성 (npm ci): $m" web_deps "$w"
    run "3w 화면 형 검사 (tsc): $m" npm --prefix "$w" run --silent typecheck
    run "3w 화면 시험 (vitest): $m" npm --prefix "$w" test --silent
    run "3w 화면 빌드 · dist 대조: $m" web_dist_fresh "$w"
    # 3l 화면 린트 · 3e 브라우저 e2e (결정 23)
    run_fix "3l 화면 린트 (eslint): $m" \
      "아래 [규칙]이 가리킨 코드를 고친다(eslint-disable 같은 주석으로 끄지 않는다). 잠긴 시험이 걸리거나 규칙이 틀렸으면 멈추고 알린다" \
      web_lint "$w"
    if ! bout=$(web_e2e_browser "$w" 2>&1); then
      bad "3e 브라우저 e2e (playwright): $m — 브라우저 없음. $bout — 고치기: (cd $w && npx playwright install chromium)"
    else
      run_fix "3e 브라우저 e2e (playwright): $m" \
        "아래 실패한 시험을 본다(trace · 스크린샷: ${TMPDIR:-/tmp}/price-inha-e2e, 시연 서버 로그: ${TMPDIR:-/tmp}/price-inha-e2e-server.log). 잠긴 시험은 고치지 않고 화면 · 서버 코드를 고친다. 시험이 틀렸으면 멈추고 알린다" \
        web_e2e "$m" "$w"
    fi
  done
done

# 4 잠긴 테스트
for m in $mods; do
  if git rev-parse -q --verify "refs/tags/red/$m" >/dev/null; then
    # shellcheck disable=SC2046
    if ! git diff --quiet "red/$m" -- $(module_test_dirs "$m"); then
      bad "4 잠긴 테스트가 바뀜: $m — 고치기: git checkout red/$m -- <tests 폴더> 로 되돌리고 src 를 고친다. 테스트가 틀렸으면 멈추고 알린다"
      git diff --stat "red/$m" -- $(module_test_dirs "$m")
    elif [ -n "$(git ls-files -o --exclude-standard -- $(module_test_dirs "$m"))" ]; then
      bad "4 잠긴 뒤 새 테스트 파일: $m — 고치기: 그 파일을 지운다. 더 볼 것은 src 안의 단위 테스트로 쓴다"
      git ls-files -o --exclude-standard -- $(module_test_dirs "$m")
    else ok "4 잠긴 테스트 그대로: $m"; fi
  elif [ "$picked" -eq 1 ]; then
    bad "4 red 태그 없음: $m — 고치기: G1(scripts/red.sh $m)을 통과시키고 잠근 뒤에 구현한다"
  else
    printf -- '- 4 %s: red 태그 없음 (아직 G1 전)\n' "$m"
  fi
done

# 5 필수 번호
for m in $mods; do
  [ -f "docs/dev/modules/$m.md" ] || { bad "5 지시서 없음: $m"; continue; }
  miss=$(missing_ids "$m")
  if [ -n "$miss" ]; then bad "5 필수 번호 빠짐 ($m): $(echo $miss) — 고치기: 그 번호를 확인하는 테스트에 '// 검증: 번호' 주석을 단다"
  else ok "5 필수 번호: $m"; fi
done

# 5b 원본 동작 근거
for m in $mods; do
  [ -f "docs/dev/modules/$m.md" ] || continue
  if origin_ok "$m"; then ok "5b 원본 동작 근거: $m"
  else bad "5b 원본 동작 근거 없음 ($m) — 고치기: 지시서에 '## 원본 동작 근거' 절을 두고 따르는 조사 문서(docs/research/…)의 절을 적는다. 원본에 없는 기능이면 '원본에 없음(우리 고유)'이라고 적는다"; fi
done

# 6 아키텍처 · 컨벤션
PY=$(find_python)
if [ -z "$PY" ]; then bad "6 아키텍처 · 컨벤션: Python 3.11 이상이 없다"
elif out=$("$PY" scripts/check_arch.py --only "$only" 2>&1); then ok "6 아키텍처 · 컨벤션"
else bad "6 아키텍처 · 컨벤션"; printf '%s\n' "$out"; fi

# 7 비밀 글자
sec=$(git ls-files -co --exclude-standard | tr '\n' '\0' \
  | xargs -0 grep -nIE "$SECRET_RX" 2>/dev/null)
files=$(git ls-files -co --exclude-standard | grep -E '(^|/)(\.env(\..*)?|auth\.json)$' | grep -vE '\.env\.example$')
if [ -n "$sec" ] || [ -n "$files" ]; then
  bad "7 비밀 글자 — 고치기: 그 값을 지우고 진짜처럼 보이지 않는 시험용 글자로 바꾼다. 진짜 키였다면 멈추고 사용자에게 알린다(재발급 필요)"
  printf '%s\n%s\n' "$sec" "$files" | cut -c1-100
else ok "7 비밀 글자 없음"; fi

# 8 바깥 주소
oh_any=0
for m in $mods; do
  oh=$(outside_hosts "$m")
  if [ -n "$oh" ]; then bad "8 테스트에 바깥 주소: $m — 고치기: 로컬 가짜 서버 주소를 넘긴다"; printf '%s\n' "$oh"; oh_any=1; fi
done
[ "$oh_any" -eq 0 ] && ok "8 테스트에 바깥 주소 없음"

# 9 검사 끄기 (Rust 의 #[ignore] · #[allow], 화면 · 시험 TS 의 eslint-disable · @ts-ignore · .skip · .only — 결정 23)
off=$(grep -rnE '#!?\[(cfg_attr\([^]]*)?(ignore|allow)\b' $srcdirs --include='*.rs' 2>/dev/null)
offw=$(grep -rnE "$WEB_OFF_RX" $srcdirs --include='*.ts' --include='*.js' --include='*.mjs' --include='*.cjs' --include='*.html' \
  --exclude-dir=node_modules --exclude-dir=dist 2>/dev/null)
if [ -n "$off$offw" ]; then
  bad "9 검사를 끈 곳 — 고치기: #[ignore] · #[allow] · eslint-disable · @ts-ignore · .skip · .only 를 지우고 원인을 고친다"
  printf '%s\n' "$off" "$offw" | grep -v '^$'
else ok "9 검사를 끈 곳 없음"; fi

# 10 공급망 (cargo-deny, QMIT 결정 8): 라이선스 · 금지 크레이트 · 출처(crates.io 만). 오프라인으로 되는 것만.
# 보안 공지(advisories)와 새 판 발행일(scripts/supply.py)은 네트워크라 pre-push · scripts/nightly.sh audit 가 본다.
if ! cargo deny --version >/dev/null 2>&1; then
  bad "10 공급망 (cargo-deny): cargo-deny 가 없다 — 고치기: cargo install cargo-deny --version $DENY_VERSION --locked"
elif out=$(cargo deny --offline check licenses bans sources 2>&1); then ok "10 공급망 (cargo-deny)"
else bad "10 공급망 (cargo-deny) — 고치기: 아래 error 를 본다. 허용 밖 라이선스 · git 출처 · 금지 크레이트는 더하지 말고 멈춰 알린다(deny.toml 은 사용자 확인 뒤 조정자만 고친다)"
  printf '%s\n' "$out" | grep -E -A6 '^error' | head -60; fi

# 11 설치 스크립트 시험 (결정 27): 전체 범위에서만. 가짜 릴리스 묶음 · 임시 폴더만 쓰고 네트워크를 쓰지 않는다.
if [ "$full" -eq 1 ]; then
  run_fix "11 설치 스크립트 시험 (scripts/test-install.sh)" \
    "아래 ✗ 줄의 동작을 install.sh 에서 고친다(sha256 확인 · https 만 · sudo 없음 · 셸 설정 안 고침). 시험이 틀렸으면 멈추고 알린다" \
    bash scripts/test-install.sh
fi

echo
[ "$fail" -eq 0 ] && echo "G2 통과" || echo "G2 실패"
exit "$fail"
