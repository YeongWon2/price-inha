#!/usr/bin/env python3
"""공급망 발행일 검사 (QMIT 결정 8, docs/dev/pipeline.md 5장).

Cargo.lock 의 crates.io 패키지와 crates/*/web/package-lock.json 의 npm 패키지마다 발행 시각을 받아,
기준 판(--since, 기본 origin/main)에 없던 새 판이 min_age_days(architecture.toml [supply], 기본 14일)보다
새것이면 실패한다. 이미 있던 판은 보지 않는다. 막 나온 판은 탈취된 계정이 올린 것일 수 있어
며칠 기다려 다른 사람들이 먼저 걸러 내게 한다.
예외: architecture.toml [supply] security_fixes 에 사람이 적은 판(보안 권고를 고친 판)은 14일 안이어도 통과(결정 9).

    python3.14 scripts/supply.py                       새 판만 (실패하면 1)
    python3.14 scripts/supply.py --since <ref>          기준을 바꾼다
    python3.14 scripts/supply.py --head <ref>           작업 트리 대신 그 커밋의 lock 을 본다(pre-push)
    python3.14 scripts/supply.py --all                  모든 판을 보고만 한다(밤 실행, 늘 0)
    python3.14 scripts/supply.py --offline-fixture f.json  네트워크 없이 가짜 발행일 표로(자체 시험)

종료: 0 통과 · 1 너무 새 판이 있음 · 2 사용법/기준 오류 · 3 발행일을 받지 못한 판이 있음(네트워크).
네트워크가 필요하다. verify.sh(G2, 오프라인)에는 넣지 않는다 — pre-push 와 scripts/nightly.sh audit 가 부른다.
"""
import argparse
import json
import subprocess
import sys
import threading
import time
import tomllib
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timedelta, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES_IO = ("registry+https://github.com/rust-lang/crates.io-index", "sparse+https://index.crates.io/")
UA = "price-inha-supply-check"  # 일반 이름만 둔다(개인 정보 금지)
CACHE = ROOT / ".pipeline/supply-cache.json"
WORKERS = 4  # 동시 요청 4개 이하
PAUSE = 0.25  # 요청마다 쉬는 시간(초) — 레지스트리에 부담을 주지 않는다


def git_show(ref, rel):
    """그 커밋의 파일 내용. 파일이 없으면 None, 기준 자체가 없으면 예외."""
    p = subprocess.run(["git", "show", f"{ref}:{rel}"], cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
    if p.returncode == 0:
        return p.stdout
    if subprocess.run(["git", "rev-parse", "-q", "--verify", f"{ref}^{{commit}}"], cwd=ROOT,
                      capture_output=True).returncode != 0:
        raise LookupError(ref)
    return None


def read(ref, rel):
    if ref is None:
        p = ROOT / rel
        return p.read_text(encoding="utf-8") if p.exists() else None
    return git_show(ref, rel)


def npm_locks(ref):
    if ref is None:
        return sorted(p.relative_to(ROOT).as_posix() for p in ROOT.glob("crates/*/web/package-lock.json"))
    p = subprocess.run(["git", "ls-tree", "-r", "--name-only", ref, "--", "crates"], cwd=ROOT,
                       capture_output=True, text=True, encoding="utf-8")
    return sorted(f for f in p.stdout.splitlines() if f.endswith("/web/package-lock.json") and f.count("/") == 3)


def cargo_pkgs(text):
    if not text:
        return set()
    return {("cargo", p["name"], p["version"]) for p in tomllib.loads(text).get("package", [])
            if p.get("source", "").startswith(CRATES_IO)}


def npm_pkgs(text):
    if not text:
        return set()
    out = set()
    for key, p in json.loads(text).get("packages", {}).items():
        if not key or p.get("link") or "version" not in p:
            continue
        resolved = p.get("resolved", "")
        if resolved and not resolved.startswith("https://registry.npmjs.org/"):
            continue  # 레지스트리 밖(파일 · git)은 발행일이 없다
        name = p.get("name") or key.rsplit("node_modules/", 1)[-1]
        out.add(("npm", name, p["version"]))
    return out


def packages(ref, lock_paths):
    pk = cargo_pkgs(read(ref, "Cargo.lock"))
    for rel in lock_paths:
        pk |= npm_pkgs(read(ref, rel))
    return pk


def parse_time(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00")).astimezone(timezone.utc)


def get_json(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)


def fetch(kind, name, ver):
    """발행 시각(ISO 문자열). 받지 못하면 예외."""
    time.sleep(PAUSE)
    if kind == "cargo":
        d = get_json(f"https://crates.io/api/v1/crates/{urllib.parse.quote(name)}/{urllib.parse.quote(ver)}")
        return d["version"]["created_at"]
    d = get_json("https://registry.npmjs.org/" + urllib.parse.quote(name, safe="@"))
    return d["time"][ver]


class Times:
    """발행 시각 표. 발행 시각은 바뀌지 않으므로 캐시는 지우지 않는다."""

    def __init__(self, fixture):
        self.fixture = fixture
        self.lock = threading.Lock()
        self.cache = {}
        if fixture is None and CACHE.exists():
            try:
                self.cache = json.loads(CACHE.read_text(encoding="utf-8"))
            except ValueError:
                self.cache = {}

    def get(self, pkg):
        kind, name, ver = pkg
        key = f"{kind}:{name}@{ver}"
        if self.fixture is not None:
            return self.fixture.get(kind, {}).get(f"{name}@{ver}")
        with self.lock:
            if key in self.cache:
                return self.cache[key]
        try:
            t = fetch(kind, name, ver)
        except (urllib.error.URLError, OSError, KeyError, ValueError):
            return None
        with self.lock:
            self.cache[key] = t
        return t

    def save(self):
        if self.fixture is None:
            CACHE.parent.mkdir(exist_ok=True)
            CACHE.write_text(json.dumps(self.cache, sort_keys=True, indent=0) + "\n", encoding="utf-8")


def supply_rules():
    return tomllib.loads((ROOT / "docs/dev/architecture.toml").read_text(encoding="utf-8")).get("supply", {})


def min_age_days():
    return int(supply_rules().get("min_age_days", 14))


def security_fixes():
    """사람이 정한 보안 수정 판 예외(결정 9): (cargo, 이름, 판) → 권고 번호. 14일 안이어도 통과시킨다."""
    out = {}
    for f in supply_rules().get("security_fixes", []):
        out[("cargo", f["crate"], f["version"])] = f["advisory"]
    return out


def main():
    ap = argparse.ArgumentParser(description="새 의존 판의 발행일 검사")
    ap.add_argument("--since", default="origin/main", help="기준 판(이 판에 없던 것만 본다)")
    ap.add_argument("--head", default=None, help="볼 판(기본: 작업 트리)")
    ap.add_argument("--all", action="store_true", help="모든 판을 보고만 한다(늘 0)")
    ap.add_argument("--days", type=int, default=None, help="최소 나이(일). 기본 architecture.toml [supply]")
    ap.add_argument("--offline-fixture", default=None, help="가짜 발행일 표 JSON(네트워크 없이 시험)")
    a = ap.parse_args()

    days = a.days if a.days is not None else min_age_days()
    fixture = None
    now = datetime.now(timezone.utc)
    if a.offline_fixture:
        fixture = json.loads(Path(a.offline_fixture).read_text(encoding="utf-8"))
        if "now" in fixture:
            now = parse_time(fixture["now"])

    try:
        head = packages(a.head, npm_locks(a.head))
        if a.all:
            todo = head
            print(f"공급망 발행일: 모든 판 {len(todo)}개 (보고만, 기준 {days}일)")
        else:
            base = packages(a.since, npm_locks(a.since))
            todo = head - base
            print(f"공급망 발행일: 기준 {a.since} 에 없던 새 판 {len(todo)}개 (기준 {days}일)")
    except LookupError as e:
        print(f"✗ 기준 판 {e} 를 찾을 수 없다 — 고치기: git fetch origin 하거나 --since <ref> 를 준다")
        return 2

    times = Times(fixture)
    order = sorted(todo)
    with ThreadPoolExecutor(max_workers=WORKERS) as ex:
        got = list(ex.map(times.get, order))
    times.save()

    fixes = security_fixes()
    young, unknown = [], []
    for pkg, t in zip(order, got):
        if t is None:
            unknown.append(pkg)
            continue
        age = now - parse_time(t)
        if age < timedelta(days=days):
            if pkg in fixes:
                print(f"✓ [SUPPLY-SECFIX] {pkg[0]} {pkg[1]} {pkg[2]} — {t[:10]} 발행, {age.days}일 지남. "
                      f"{fixes[pkg]} 을 고친 판이라 통과(architecture.toml [supply] security_fixes, 결정 9)")
                continue
            young.append((pkg, t[:10], age.days))
    for (kind, name, ver), when, age in young:
        print(f"✗ [SUPPLY-YOUNG] {kind} {name} {ver} — {when} 발행, {age}일 지남 (기준 {days}일)")
    for kind, name, ver in unknown:
        print(f"? [SUPPLY-UNKNOWN] {kind} {name} {ver} — 발행일을 받지 못했다(네트워크 · 레지스트리)")
    if a.all:
        print(f"\n{days}일 안의 판 {len(young)}개, 확인 못한 판 {len(unknown)}개 (보고만)")
        return 0
    if young:
        print(f"\n공급망 실패: {days}일이 안 된 새 판 {len(young)}개.\n"
              f"    왜: 막 나온 판은 탈취된 계정이 올린 것일 수 있다(QMIT 결정 8).\n"
              f"    고치기: {days}일이 지난 판으로 내려 고정한다(cargo update -p <이름> --precise <판>, "
              f"npm install <이름>@<판>). 꼭 필요하면 멈추고 사용자 승인을 받는다.")
        return 1
    if unknown:
        print("\n발행일을 확인하지 못한 판이 있다 — 네트워크가 되는 곳에서 다시 돌린다.")
        return 3
    print("공급망 발행일 통과")
    return 0


if __name__ == "__main__":
    sys.exit(main())
