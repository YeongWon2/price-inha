#!/usr/bin/env bash
# 로컬 측정 · 감사 (GitHub CI 대신, 2026-11-24 사용자 "깃허브 ci는 하지말자 로컬에서 다해결하면돼").
# 판정(끝났다는 기준)은 여전히 scripts/verify.sh 뿐이다. 여기 것은 손으로 돌리고, 결과는 보고로만 쓴다.
#   scripts/nightly.sh audit                  보안 공지(cargo audit · cargo deny check advisories) + 모든 판 발행일 보고(supply.py --all)
#   scripts/nightly.sh mutants [-p 패키지...]  변이 시험(기본: pi-rules pi-paper pi-exec pi-gate). 보고만.
#   scripts/nightly.sh coverage               워크스페이스 줄 커버리지를 크레이트별 표로. 보고만.
#   scripts/nightly.sh stress <runs> [cargo test 인자...]   간헐 실패 찾기: 여러 번 돌려 실패 줄만 모은다.
#   scripts/nightly.sh e2e <runs>             브라우저 e2e(결정 23)를 여러 번 돌려 흔들리는지 · 걸린 시간을 본다.
#   scripts/nightly.sh all                    audit · mutants · coverage 를 차례로(오래 걸린다).
# 결과는 target/nightly/ 에 남는다(.gitignore). 네트워크가 필요한 것은 audit 뿐이다.
# 실제 증권사 · AI 서버를 부르지 않는다(테스트는 로컬 가짜 서버만 쓴다). 비밀값을 쓰지 않는다.
# 도구 판은 발행 14일 넘은 것만 고정한다(QMIT 결정 8 · 9): cargo-mutants 27.1.0, cargo-llvm-cov 0.9.1.
set -uo pipefail
cd "$(dirname "$0")/.."
. scripts/lib.sh
MUTANTS_VERSION="27.1.0"
LLVM_COV_VERSION="0.9.1"
OUT="target/nightly"
mkdir -p "$OUT"
PY=$(find_python)

need() { # 도구 이름 · 설치 안내
  if ! command -v "$1" >/dev/null 2>&1; then echo "✗ $1 가 없다 — 고치기: $2"; return 1; fi
}

run_audit() {
  local fail=0
  echo "== 보안 공지 (RustSec)"
  if need cargo-audit "cargo install cargo-audit --locked"; then cargo audit || fail=1; else fail=1; fi
  if need cargo-deny "cargo install cargo-deny --version $DENY_VERSION --locked"; then cargo deny check advisories || fail=1; else fail=1; fi
  echo "== 모든 판 발행일 (보고만)"
  if [ -z "$PY" ]; then echo "✗ Python 3.11 이상이 없다"; fail=1
  else "$PY" scripts/supply.py --all || true; fi
  return "$fail"
}

run_mutants() {
  need cargo-mutants "cargo install cargo-mutants --version $MUTANTS_VERSION --locked" || return 1
  local pkgs=("$@")
  [ "${#pkgs[@]}" -eq 0 ] && pkgs=(-p pi-rules -p pi-paper -p pi-exec -p pi-gate)
  echo "== 변이 시험 (보고만): ${pkgs[*]}"
  cargo mutants --no-shuffle --timeout 180 --build-timeout 600 --output "$OUT/mutants" "${pkgs[@]}"
  local o="$OUT/mutants/mutants.out"
  if [ -f "$o/outcomes.json" ] && [ -n "$PY" ]; then
    "$PY" - "$o/outcomes.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print(f"전체 {d.get('total_mutants')} · 잡음 {d.get('caught')} · 놓침 {d.get('missed')} · 시간 초과 {d.get('timeout')} · 빌드 불가 {d.get('unviable')}")
PY
  fi
  if [ -s "$o/missed.txt" ]; then echo "-- 놓친 변이 ($o/missed.txt)"; cat "$o/missed.txt"; else echo "놓친 변이 없음 (또는 측정 못 함)"; fi
  return 0 # 보고만
}

run_coverage() {
  need cargo-llvm-cov "cargo install cargo-llvm-cov --version $LLVM_COV_VERSION --locked && rustup component add llvm-tools-preview" || return 1
  echo "== 줄 커버리지 (보고만)"
  cargo llvm-cov --workspace --json --summary-only --output-path "$OUT/coverage.json" || return 0
  [ -n "$PY" ] || return 0
  "$PY" - "$OUT/coverage.json" <<'PY'
import json, re, sys, collections
d = json.load(open(sys.argv[1]))["data"][0]
per = collections.defaultdict(lambda: [0, 0])
for f in d["files"]:
    m = re.search(r"/crates/([^/]+)/", f["filename"])
    k = m.group(1) if m else "(그 밖)"
    per[k][0] += f["summary"]["lines"]["covered"]
    per[k][1] += f["summary"]["lines"]["count"]
print(f"{'크레이트':<12} {'덮은 줄':>8} {'전체 줄':>8} {'비율':>7}")
for k in sorted(per):
    c, n = per[k]
    print(f"{k:<12} {c:>8} {n:>8} {(100 * c / n) if n else 0:>6.1f}%")
t = d["totals"]["lines"]
print(f"{'전체':<12} {t['covered']:>8} {t['count']:>8} {t['percent']:>6.1f}%")
PY
  return 0
}

run_e2e() {
  local runs="${1:?몇 번 돌릴지 준다: scripts/nightly.sh e2e 3}" m w i fails=0 t0 t1
  for m in $MODULES; do
    for w in $(module_web "$m"); do
      web_deps "$w" || return 1
      web_e2e_browser "$w" || { echo "✗ 고치기: (cd $w && npx playwright install chromium)"; return 1; }
      for i in $(seq 1 "$runs"); do
        t0=$(date +%s)
        if web_e2e "$m" "$w" >"$OUT/e2e.txt" 2>&1; then t1=$(date +%s); echo "== $m ${i}번째 통과 ($((t1 - t0))초)"
        else
          t1=$(date +%s); fails=$((fails + 1)); echo "== $m ${i}번째 실패 ($((t1 - t0))초)"
          grep -E '✘|Error:' "$OUT/e2e.txt" | head -12
        fi
      done
    done
  done
  echo "실패: $fails / $runs (모듈마다)"
  [ "$fails" -eq 0 ]
}

run_stress() {
  local runs="${1:?몇 번 돌릴지 준다: scripts/nightly.sh stress 20 -p pi-ai}"; shift
  local sel=("$@")
  [ "${#sel[@]}" -eq 0 ] && sel=(--workspace)
  cargo test "${sel[@]}" --no-run -q || return 1
  local i fails=0
  for i in $(seq 1 "$runs"); do
    if ! cargo test "${sel[@]}" -q >"$OUT/stress.txt" 2>&1; then
      fails=$((fails + 1)); echo "== ${i}번째 실패"; grep -E 'FAILED|panicked|left:|right:' "$OUT/stress.txt" | head -12
    fi
  done
  echo "실패: $fails / $runs"
  [ "$fails" -eq 0 ]
}

cmd="${1:-}"; shift || true
case "$cmd" in
  audit) run_audit ;;
  mutants) run_mutants "$@" ;;
  coverage) run_coverage ;;
  stress) run_stress "$@" ;;
  e2e) run_e2e "$@" ;;
  all) a=0; run_audit || a=1; run_mutants; run_coverage; exit "$a" ;;
  *) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
