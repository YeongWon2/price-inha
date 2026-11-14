#!/usr/bin/env bash
# G2 기계 검사. 0으로 끝나야 "끝났다"고 말할 수 있다. docs/dev/pipeline.md 5장.
#   scripts/verify.sh            전체
#   scripts/verify.sh <모듈>...  그 모듈만 (다른 모듈이 아직 red일 때)
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

if [ "$#" -gt 0 ]; then
  mods="$*"; pkgs=""; only=""; srcdirs=""
  for m in $mods; do
    pkgs="$pkgs $(module_pkgs "$m")" || exit 2
    for d in $(module_dirs "$m"); do only="$only,$d"; srcdirs="$srcdirs $d"; done
  done
  fmtpkgs="$pkgs"
  echo "범위: $mods"
else
  mods="$MODULES"; pkgs="--workspace"; only=""; srcdirs="crates"; fmtpkgs="--all"
  echo "범위: 전체"
fi

# 0 검사기 자체 시험이 지금 규칙으로 통과했는가
if [ "${PI_SELFTEST:-0}" != "1" ]; then
  now=$(rules_hash)
  if [ "$(cat .pipeline/selftest.ok 2>/dev/null)" = "$now" ]; then ok "0 검사기 자체 시험 (지금 규칙으로 통과함)"
  else bad "0 검사기 자체 시험이 없거나 낡았다 — 고치기: python3.14 scripts/selftest.py 를 돌려 통과시킨다"; fi
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
  elif [ "$#" -gt 0 ]; then
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

# 9 검사 끄기
off=$(grep -rnE '#!?\[(cfg_attr\([^]]*)?(ignore|allow)\b' $srcdirs --include='*.rs' 2>/dev/null)
if [ -n "$off" ]; then bad "9 검사를 끈 곳 — 고치기: #[ignore] · #[allow]를 지우고 원인을 고친다"; printf '%s\n' "$off"
else ok "9 검사를 끈 곳 없음"; fi

echo
[ "$fail" -eq 0 ] && echo "G2 통과" || echo "G2 실패"
exit "$fail"
