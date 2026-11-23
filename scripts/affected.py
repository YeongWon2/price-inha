#!/usr/bin/env python3
"""영향 범위 자동 선택 (QMIT 결정 6). docs/dev/pipeline.md 5장.

    affected.py                 기준 origin/main(없으면 HEAD~1)
    affected.py --base <ref>    기준을 바꾼다

기준과 작업 트리(커밋 안 된 것 · 새 파일 포함)의 차이로 돌릴 모듈을 한 줄로 찍는다.
  - 모듈 이름들(공백 구분): 바뀐 파일이 든 crates/<모듈>/ + 그 크레이트에 (전이적으로) 기대는 크레이트
  - ALL: 그래프가 못 따라가는 변경(루트 Cargo.toml · 판이 바뀐 Cargo.lock · scripts/ · 규칙 · CI 등)
  - 빈 줄: 검사할 코드가 없다(문서만 바뀜)
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# 바뀌면 전체를 도는 곳(앞부분이 같으면).
ALL_PREFIXES = (
    "Cargo.toml", "clippy.toml", "scripts/", ".github/", ".cargo/",
    "docs/dev/architecture.toml", "docs/dev/graph.toml",
    "rust-toolchain", "deny.toml",
)
# 코드 검사와 상관없는 곳. 여기 말고 crates/ 밖에서 모르는 파일이 바뀌면 안전하게 전체를 돈다.
NO_CODE_PREFIXES = ("docs/", ".githooks/", ".pipeline/")
NO_CODE_FILES = {".gitignore", "LICENSE"}


def git(*args):
    p = subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
    return p.returncode, p.stdout


def ok_commit(ref):
    return git("rev-parse", "-q", "--verify", f"{ref}^{{commit}}")[0] == 0


def module_order():
    text = (ROOT / "scripts/lib.sh").read_text(encoding="utf-8")
    return re.search(r'^ALL_MODULES="([^"]*)"', text, re.M).group(1).split()


def changed_files(base):
    code, mb = git("merge-base", base, "HEAD")
    start = mb.strip() if code == 0 and mb.strip() else base
    _, diff = git("diff", "--name-only", start)
    _, new = git("ls-files", "-o", "--exclude-standard")
    return start, sorted(set(diff.split("\n") + new.split("\n")) - {""})


def lock_versions_changed(start):
    _, d = git("diff", "-U0", start, "--", "Cargo.lock")
    return any(re.match(r"^[+-](version|checksum|source) = ", line) for line in d.splitlines())


def reverse_graph():
    """크레이트 폴더 이름(모듈) → 그것에 기대는 모듈들. path 의존만 본다."""
    p = subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1"],
                       cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
    if p.returncode != 0:
        return None
    meta = json.loads(p.stdout)
    rev = {}
    for pkg in meta["packages"]:
        me = Path(pkg["manifest_path"]).parent.name
        for dep in pkg["dependencies"]:
            if dep.get("path"):
                rev.setdefault(Path(dep["path"]).name, set()).add(me)
    return rev


def affected(base):
    if not ok_commit(base):
        return "ALL"
    start, files = changed_files(base)
    order = module_order()
    existing = {m for m in order if (ROOT / f"crates/{m}/Cargo.toml").is_file()}
    hit = set()
    for f in files:
        if f.startswith(ALL_PREFIXES):
            return "ALL"
        if f == "Cargo.lock":
            if lock_versions_changed(start):
                return "ALL"
            continue
        m = re.match(r"^crates/([^/]+)/", f)
        if m:
            if m.group(1) not in existing:
                return "ALL"  # 크레이트가 아닌 곳이거나 지운 크레이트
            hit.add(m.group(1))
            continue
        m = re.match(r"^docs/dev/modules/([^/]+)\.md$", f)
        if m:
            if m.group(1) in existing:
                hit.add(m.group(1))  # 지시서: 그 모듈의 필수 번호 · 원본 근거 검사 대상
            continue
        if f.startswith(NO_CODE_PREFIXES) or f in NO_CODE_FILES or ("/" not in f and f.endswith(".md")):
            continue
        return "ALL"
    if not hit:
        return ""
    rev = reverse_graph()
    if rev is None:
        return "ALL"
    todo = list(hit)
    while todo:
        for user in rev.get(todo.pop(), ()):
            if user not in hit:
                hit.add(user)
                todo.append(user)
    return " ".join(m for m in order if m in hit)


def main():
    args = sys.argv[1:]
    if args[:1] == ["--base"] and len(args) == 2:
        base = args[1]
    elif not args:
        base = "origin/main" if ok_commit("origin/main") else "HEAD~1"
    else:
        print("쓰는 법: affected.py [--base <ref>]", file=sys.stderr)
        sys.exit(2)
    print(affected(base))


if __name__ == "__main__":
    main()
