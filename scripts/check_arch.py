#!/usr/bin/env python3
"""아키텍처 · 컨벤션 기계 검사. 규칙은 docs/dev/architecture.toml 에 있다.

어긴 곳마다 [규칙 번호] 파일:줄 · 왜 · 고치는 법을 찍는다. 하나라도 있으면 1로 끝난다.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10 이하
    print("✗ [TOOL] Python 3.11 이상이 필요하다 (tomllib)")
    sys.exit(2)

ROOT = Path(sys.argv[sys.argv.index("--root") + 1]).resolve() if "--root" in sys.argv else Path(__file__).resolve().parent.parent
RULES = tomllib.loads((ROOT / "docs/dev/architecture.toml").read_text(encoding="utf-8"))
problems = []


def report(rule, where, why, fix):
    problems.append(f"✗ [{rule}] {where}\n    왜: {why}\n    고치기: {fix}")


def rs_files(base, tests=True):
    base = ROOT / base
    if not base.exists():
        return []
    out = []
    for p in sorted(base.rglob("*.rs")):
        rel = p.relative_to(ROOT).as_posix()
        if "/target/" in rel:
            continue
        if not tests and "/tests/" in rel:
            continue
        out.append(p)
    return out


def strip_comment(line):
    i = line.find("//")
    return line if i < 0 else line[:i]


# 1. 의존 방향과 바깥 의존
_m = subprocess.run(["cargo", "metadata", "--format-version", "1"], cwd=ROOT, capture_output=True, text=True)
if _m.returncode != 0:
    print("✗ [ARCH-MANIFEST] Cargo.toml\n    왜: cargo 가 워크스페이스를 읽지 못한다\n    고치기: 아래 cargo 오류를 고친다\n"
          + "\n".join("    " + l for l in _m.stderr.strip().splitlines()[-8:]))
    sys.exit(1)
meta = json.loads(_m.stdout)
members = set(meta["workspace_members"])
local = {p["name"]: p for p in meta["packages"] if p["id"] in members}
crates = RULES["crates"]
for name, pkg in local.items():
    rule = crates.get(name)
    manifest = Path(pkg["manifest_path"]).relative_to(ROOT).as_posix()
    if rule is None:
        report("ARCH-CRATE", manifest, "규칙 표에 없는 크레이트다",
               "docs/dev/architecture.toml 의 [crates]에 더할지 사용자에게 묻는다")
        continue
    for dep in pkg["dependencies"]:
        if dep.get("kind") == "dev":
            continue  # 테스트용 의존은 방향 검사에서 뺀다
        d = dep["name"]
        if d in local:
            if d not in rule["internal"]:
                report("ARCH-DEP-DIR", manifest,
                       f"{name} → {d} 는 허용되지 않은 방향이다 ({name}: {rule['component']})",
                       f"{d} 를 쓰지 말고 이 모듈 안의 trait으로 받거나, 조립은 price-inha에서 한다. "
                       f"허용: {rule['internal'] or '없음'}")
        elif "*" not in rule["external"] and d not in rule["external"]:
            report("ARCH-DEP-EXT", manifest,
                   f"{name} 가 허용 목록에 없는 바깥 의존 {d} 를 쓴다",
                   f"허용된 것({', '.join(rule['external'])})으로 바꾸거나, 꼭 필요하면 멈추고 "
                   "조정자에게 architecture.toml 에 더해 달라고 알린다")
for name in crates:
    if name not in local:
        report("ARCH-CRATE", "Cargo.toml", f"규칙 표의 {name} 크레이트가 없다", "크레이트를 만들거나 표에서 뺀다")

all_pkgs = {p["name"] for p in meta["packages"]}
for b in RULES.get("banned_deps", []):
    if b["name"] in all_pkgs:
        report("ARCH-DEP-BAN", "Cargo.lock", f"{b['name']} 가 의존 나무에 있다 — {b['why']}", b["fix"])

# 2. 금지 모양
for pat in RULES.get("patterns", []):
    rx = re.compile(pat["regex"])
    excepts = pat.get("except", [])
    for scope in pat["scope"]:
        for f in rs_files(scope, tests=not pat.get("except_tests", False)):
            rel = f.relative_to(ROOT).as_posix()
            if any(rel.startswith(e + "/") for e in excepts):
                continue
            in_test_mod = False
            for n, line in enumerate(f.read_text(encoding="utf-8").splitlines(), 1):
                if pat.get("except_tests") and re.match(r"\s*#\[cfg\(test\)\]", line):
                    in_test_mod = True  # 파일 끝의 단위 테스트 모듈부터는 뺀다
                if in_test_mod:
                    continue
                if rx.search(strip_comment(line)):
                    report(pat["id"], f"{rel}:{n}", pat["why"], pat["fix"])

# 3. 구조
st = RULES.get("structure", {})
for name, rule in crates.items():
    d = rule["dir"]
    is_bin = name == "price-inha"
    if st.get("lib_crates_have_no_main") and not is_bin and (ROOT / d / "src/main.rs").exists():
        report("ARCH-ONE-BIN", f"{d}/src/main.rs", "실행 파일은 price-inha 하나다",
               "main.rs 를 지우고 필요한 것은 lib 의 공개 함수로 둔다")
    if st.get("lib_crates_have_error_rs") and not is_bin:
        src = ROOT / d / "src"
        has_code = any(
            len([l for l in p.read_text(encoding="utf-8").splitlines() if l.strip() and not l.strip().startswith("//")]) > 0
            for p in src.rglob("*.rs")
        ) if src.exists() else False
        if has_code and not (src / "error.rs").exists():
            report("CONV-ERROR-FILE", f"{d}/src/", "라이브러리의 오류 타입은 src/error.rs 에 둔다",
                   "오류 enum(thiserror)을 src/error.rs 로 옮기고 lib.rs 에서 pub use 한다")

# 4. 크기
lim = RULES.get("limits", {})
fn_rx = re.compile(r"^\s*(pub(\([^)]*\))?\s+)?(const\s+)?(async\s+)?(unsafe\s+)?fn\s+(\w+)")
for name, rule in crates.items():
    for f in rs_files(rule["dir"] + "/src"):
        rel = f.relative_to(ROOT).as_posix()
        lines = f.read_text(encoding="utf-8").splitlines()
        cut = next((i for i, l in enumerate(lines) if re.match(r"\s*#\[cfg\(test\)\]", l)), len(lines))
        body = lines[:cut]
        if len(body) > lim.get("max_src_lines", 10**9):
            report("CONV-FILE-SIZE", rel, f"{len(body)}줄이다 (한도 {lim['max_src_lines']}줄, 단위 테스트 제외)",
                   "책임별로 파일을 나눈다")
        i = 0
        while i < len(body):
            m = fn_rx.match(body[i])
            if not m:
                i += 1
                continue
            depth, started, j = 0, False, i
            while j < len(body):
                code = strip_comment(body[j])
                depth += code.count("{") - code.count("}")
                if "{" in code:
                    started = True
                if (started and depth <= 0) or (not started and code.rstrip().endswith(";")):
                    break
                j += 1
            length = j - i + 1
            if started and length > lim.get("max_fn_lines", 10**9):
                report("CONV-FN-SIZE", f"{rel}:{i + 1}",
                       f"함수 {m.group(6)} 가 {length}줄이다 (한도 {lim['max_fn_lines']}줄)",
                       "단계별로 작은 함수로 나눈다")
            i = j + 1

# --only 폴더,폴더 : 그 폴더의 문제만 남긴다 (모듈 하나만 검사할 때). 전체에 걸친 문제는 늘 남긴다.
if "--only" in sys.argv:
    only = [d for d in sys.argv[sys.argv.index("--only") + 1].split(",") if d]
    if only:
        def mine(p):
            where = p.split("] ", 1)[1].split("\n", 1)[0]
            return where.startswith(("Cargo.", "docs/")) or any(where.startswith(d + "/") for d in only)
        problems = [p for p in problems if mine(p)]
for p in problems:
    print(p)
if problems:
    print(f"\n어긴 곳 {len(problems)}개. 규칙: docs/dev/architecture.toml")
sys.exit(1 if problems else 0)
