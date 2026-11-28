#!/usr/bin/env python3
"""Rust 시험을 모듈마다 나눠 돌린다(verify.sh 3번 · red.sh). docs/dev/pipeline.md 5장.

    cargo_tests.py [--out <파일>] <모듈>[:<대상>,<대상>…] …

  - 빌드는 한 번: 모든 모듈의 시험을 한 cargo 호출로 짓는다(--no-run). 그다음 모듈마다 `cargo test -p <패키지>`.
    (cargo 는 시험 실행 파일을 어차피 하나씩 돌리므로 모듈마다 나눠도 더 걸리지 않고, 어느 모듈이 떨어졌는지 나뉜다.)
  - <대상>을 주면 그 시험 대상만(`--test <이름>` · `lib`). 동작 잠금 G1 이 바뀐 시험만 돌릴 때 쓴다.
  - 흔들림: 실패한 시험이 몇 개(FLAKY_MAX 이하)뿐이면 그 시험 하나만 곧바로 혼자 한 번 다시 돌린다.
    통과하면 "흔들림: <이름> — 혼자 다시 돌려 통과"로 적고 통과로 친다(.pipeline/flaky.json 에 횟수를 쌓는다).
    다시 돌려도 실패하면 실패다. 전체를 다시 돌리지 않는다.
종료 코드: 0 모두 통과 · 1 실패가 있음. --out 파일에는 통과한 모듈 이름을 한 줄에 하나씩 적는다.
"""
import json
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FLAKY = ROOT / ".pipeline/flaky.json"
FLAKY_MAX = 5  # 이보다 많이 떨어지면 흔들림이 아니라 진짜 실패로 보고 다시 돌리지 않는다


def pkg(m):
    return "price-inha" if m == "server" else f"pi-{m}"


def run(cmd):
    # stdout(시험 결과)과 stderr(cargo 의 "Running <대상>")를 한 흐름으로: 어느 대상의 시험인지 차례로 읽는다.
    p = subprocess.run(cmd, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                       encoding="utf-8", errors="replace")
    return p.returncode, p.stdout


def target_args(m, t):
    return ["--lib"] if t == "lib" else ["--bins"] if t == "bins" else ["--test", t]


def failures(out):
    """[(대상 인자, 시험 이름)] — 실패한 시험. 대상을 모르는 실패(실행 파일이 통째로 죽음 등)가 있으면 None."""
    cur, found, bad_bins = None, [], 0
    for line in out.splitlines():
        s = line.strip()
        m = re.match(r"^Running (unittests )?(\S+)", s)
        if m:
            path = m.group(2)
            if m.group(1):
                cur = ["--lib"] if path.endswith("lib.rs") else ["--bins"]
            else:
                tm = re.match(r"^tests/([^/]+?)(?:\.rs|/main\.rs)$", path)
                cur = ["--test", tm.group(1)] if tm else None
            continue
        if s.startswith("Doc-tests "):
            cur = ["--doc"]
            continue
        fm = re.match(r"^test (.+) \.\.\. FAILED$", s)
        if fm:
            if cur is None or cur == ["--doc"]:
                return None
            found.append((cur, fm.group(1)))
        if re.match(r"^test result: FAILED", s):
            bad_bins += 1
    # cargo 가 실패했다고 적은 대상(`--test x` · `--lib` …)마다 FAILED 시험이 있어야 한다.
    # 실행 파일이 통째로 죽었거나(신호 등) 컴파일이 깨진 대상이 섞이면 다시 돌리지 않는다(진짜 실패).
    named = {("--bins",) if a.startswith("--bin") else tuple(a.split())
             for a in re.findall(r"`(?:-p \S+ )?(--test \S+|--lib|--bins|--bin \S+|--doc)`", out)}
    have = {tuple(t) for t, _ in found}
    if not found or not named or not named <= have or bad_bins < len(have):
        return None
    return found


def note_flaky(m, name):
    try:
        d = json.loads(FLAKY.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        d = {}
    k = f"{pkg(m)} {name}"
    e = d.get(k, {"count": 0})
    e["count"] += 1
    e["last"] = time.strftime("%Y-%m-%dT%H:%M:%S")
    d[k] = e
    FLAKY.parent.mkdir(exist_ok=True)
    FLAKY.write_text(json.dumps(d, ensure_ascii=False, indent=1), encoding="utf-8")


def one(m, targets):
    """한 모듈의 시험. (통과?, 출력 꼬리)"""
    sel = [a for t in targets for a in target_args(m, t)]
    code, out = run(["cargo", "test", "-p", pkg(m), *sel, "--no-fail-fast"])
    if code == 0:
        n = sum(int(x) for x in re.findall(r"test result: ok\. (\d+) passed", out))
        return True, f"통과 {n}개", []
    fails = failures(out)
    tail = "\n".join(out.strip().splitlines()[-60:])
    if not fails or len(fails) > FLAKY_MAX:
        return False, tail, []
    flaky = []
    for args, name in fails:
        c2, o2 = run(["cargo", "test", "-p", pkg(m), *args, "--", "--exact", name])
        ran = re.search(r"test result: ok\. ([1-9]\d*) passed", o2)
        if c2 != 0 or not ran:
            return False, tail + f"\n(혼자 다시 돌려도 실패: {name})", []
        flaky.append(name)
    return True, "통과(흔들림 다시 돌림)", flaky


def main():
    args = sys.argv[1:]
    out_file = None
    if args[:1] == ["--out"]:
        out_file, args = Path(args[1]), args[2:]
    mods = []
    for a in args:
        m, _, t = a.partition(":")
        mods.append((m, [x for x in t.split(",") if x]))
    if not mods:
        print("쓰는 법: cargo_tests.py [--out <파일>] <모듈>[:<대상>,…] …", file=sys.stderr)
        sys.exit(2)
    build = ["cargo", "test", "--no-run"]
    for m, _ in mods:
        build += ["-p", pkg(m)]
    code, out = run(build)
    ok_mods, fail = [], 0
    if code != 0:
        print("✗ 3 테스트: 컴파일 안 됨 — 고치기: 아래 오류를 고친다")
        print("\n".join(out.strip().splitlines()[-40:]))
        sys.exit(1)
    for m, targets in mods:
        good, msg, flaky = one(m, targets)
        scope = f" ({' · '.join(targets)})" if targets else ""
        if good:
            print(f"✓ 3 테스트: {m}{scope} — {msg}")
            for name in flaky:
                print(f"  흔들림: {name} — 혼자 다시 돌려 통과 (.pipeline/flaky.json 에 쌓음)")
                note_flaky(m, name)
            ok_mods.append(m)
        else:
            fail = 1
            print(f"✗ 3 테스트: {m}{scope} — 고치기: 아래 실패한 시험을 본다. 잠긴 시험은 고치지 않고 src 를 고친다")
            print(msg)
    if out_file:
        out_file.write_text("".join(f"{m}\n" for m in ok_mods), encoding="utf-8")
    sys.exit(fail)


if __name__ == "__main__":
    main()
