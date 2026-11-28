#!/usr/bin/env python3
"""영향 범위 자동 선택 (QMIT 결정 6 · 2026-11-28 영향 시험). docs/dev/pipeline.md 5장.

    affected.py [--base <ref>]          돌릴 Rust 시험 모듈을 한 줄로 (ALL · 빈 줄 · 모듈들)
    affected.py --plan [--base <ref>] [--focus a,b] [--wip c,d]
                                        verify.sh 가 읽는 계획(셸 대입문)
    affected.py --record <단위> <지문> [항목…]   통과한 검사 단위를 .pipeline/passed.json 에 적는다
    affected.py --tree-fp <경로…>       작업 트리(커밋 안 된 것 포함)에서 그 경로들의 지문

기준(기본 origin/main, 없으면 HEAD~1)과 작업 트리(커밋 안 된 것 · 새 파일 포함)의 차이로 고른다.
  - Rust 시험: 코드가 바뀐 크레이트 + 그것에 (전이적으로) 기대는 크레이트(cargo metadata 의 path 의존),
    시험(tests/)만 바뀐 크레이트는 그 크레이트만, 다른 크레이트의 파일을 글로 읽는 크레이트(파일 참조),
    돈 · 안전 크레이트(RISK)는 자기나 자기가 기대는 크레이트(Cargo.toml 을 따로 읽어 셈)의 코드가 바뀌면 늘.
  - 화면: 바뀐 화면 파일로 vitest(import 그래프 · 파일을 읽는 시험) · 빌드 · e2e(바뀐 화면에 대응하는 spec) 범위.
  - ALL: 그래프가 못 따라가는 변경(루트 Cargo.toml · 판이 바뀐 Cargo.lock · scripts/ · 규칙 · 모르는 곳).
지문(--record · DONE_*): 한 검사 단위가 읽는 파일 · 규칙 · 도구 판이 지난번 통과 때와 같으면 다시 돌리지 않는다.
"""
import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib
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
# 돈 · 안전 크레이트(모듈 이름). 이것이나 이것이 기대는 크레이트의 코드가 바뀌면 이 크레이트의 시험은 늘 돈다.
# 규칙 파일(architecture.toml · graph.toml)은 사람만 고치므로 이 목록은 여기 둔다. 2026-11-28 사용자 요청.
RISK = ("store", "rules", "approval", "exec", "paper", "gate", "secrets")
# Rust 시험 단위의 지문에 넣는 루트 파일(있는 것만).
ROOT_INPUTS = ("Cargo.toml", "Cargo.lock", "clippy.toml", "rust-toolchain", "rust-toolchain.toml", ".cargo")
RULE_INPUTS = ("scripts", "docs/dev/architecture.toml", "docs/dev/graph.toml", "deny.toml")
PASSED = ROOT / ".pipeline/cache/passed.json"  # .pipeline/ 은 git 이 무시한다. 지우기: pipeline.py cache clear
KEEP = 40  # 단위마다 남기는 통과 지문 수

# 화면: 이 페이지 폴더(web/src/<폴더>)와 주소. 여기 없는 곳(ui/ · paths.ts · 설정)이 바뀌면 e2e 전체.
PAGES = {"setup": "/", "keys": "/keys", "office": "/office/", "phone": "/phone/", "account": "/account/",
         "templates": "/templates/", "rules": "/rules/", "history": "/history/"}
HTML_PAGE = {"index.html": "setup", "keys.html": "keys"}
# 바뀌면 vitest 전체를 도는 화면 소스(공통).
WEB_COMMON = ("src/ui/", "src/paths.ts")
# server 의 HTTP 경로 파일: 바뀌면 e2e 전체(라우터 · *_http.rs · http.rs · page.rs · reach/** · 시연 서버 · 조립).
HTTP_RX = re.compile(r"^crates/server/src/(.*(^|/)(page|reach|app|demo|main|lib)(\.rs$|/)|.*http.*\.rs$|.*demo.*\.rs$)")
FS_RX = re.compile(r"readFileSync|readdirSync|existsSync|statSync|node:fs|from [\"']fs[\"']|import\.meta\.glob|\?raw")


def git(*args, env=None):
    p = subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, encoding="utf-8",
                       env=dict(os.environ, **(env or {})))
    return p.returncode, p.stdout


def ok_commit(ref):
    return git("rev-parse", "-q", "--verify", f"{ref}^{{commit}}")[0] == 0


def module_order():
    text = (ROOT / "scripts/lib.sh").read_text(encoding="utf-8")
    return re.search(r'^ALL_MODULES="([^"]*)"', text, re.M).group(1).split()


def existing_modules():
    return [m for m in module_order() if (ROOT / f"crates/{m}/Cargo.toml").is_file()]


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
    """크레이트 폴더 이름(모듈) → 그것에 기대는 모듈들. path 의존만 본다(모든 종류: 보통 · dev · build)."""
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


def forward_graph(mods):
    """모듈 → 기대는 내부 모듈들. cargo metadata 와 따로, 각 Cargo.toml 을 직접 읽는다(돈 · 안전 규칙의 근거)."""
    names = {("price-inha" if m == "server" else f"pi-{m}"): m for m in mods}
    fwd = {}
    for m in mods:
        try:
            t = tomllib.loads((ROOT / f"crates/{m}/Cargo.toml").read_text(encoding="utf-8"))
        except (OSError, tomllib.TOMLDecodeError):
            return None
        tables = [t.get(k, {}) for k in ("dependencies", "dev-dependencies", "build-dependencies")]
        for tgt in t.get("target", {}).values():
            tables += [tgt.get(k, {}) for k in ("dependencies", "dev-dependencies", "build-dependencies")]
        deps = set()
        for tab in tables:
            for key, val in tab.items():
                name = val.get("package", key) if isinstance(val, dict) else key
                if name in names:
                    deps.add(names[name])
        fwd[m] = deps
    return fwd


def closure(start, graph):
    seen, todo = set(start), list(start)
    while todo:
        for n in graph.get(todo.pop(), ()):
            if n not in seen:
                seen.add(n)
                todo.append(n)
    return seen


def file_refs(mods):
    """모듈 → 그 모듈의 Rust 코드가 글로 가리키는(파일을 읽는) 다른 모듈들. 예: gate 시험이 store/tests/fixtures 를 연다."""
    alt = "|".join(re.escape(m) for m in mods)
    rx = re.compile(rf"(?:\.\./|crates/)({alt})/")
    refs = {}
    for m in mods:
        for sub in ("src", "tests", "build.rs"):
            base = ROOT / f"crates/{m}/{sub}"
            files = [base] if base.is_file() else (base.rglob("*.rs") if base.is_dir() else [])
            for f in files:
                try:
                    text = f.read_text(encoding="utf-8", errors="replace")
                except OSError:
                    continue
                for other in rx.findall(text):
                    if other != m:
                        refs.setdefault(m, set()).add(other)
    return refs


# --- 지문
def work_tree():
    """작업 트리(커밋 안 된 것 · 새 파일 포함, .gitignore 밖)를 git 트리로 적는다. 진짜 색인은 건드리지 않는다."""
    _, idx = git("rev-parse", "--git-path", "index")
    real = Path(idx.strip())
    real = real if real.is_absolute() else ROOT / real
    with tempfile.TemporaryDirectory() as d:
        tmp = Path(d) / "index"
        if real.exists():
            shutil.copy(real, tmp)
        env = {"GIT_INDEX_FILE": str(tmp)}
        if not real.exists():
            git("read-tree", "HEAD", env=env)
        git("add", "-A", "--", ".", env=env)
        code, tree = git("write-tree", env=env)
    return tree.strip() if code == 0 else ""


_IDS = {}


def _prefetch(tree, parent):
    _, out = git("ls-tree", "-z", tree, *([parent + "/"] if parent else []))
    for ent in out.split("\0"):
        if "\t" in ent:
            meta, name = ent.split("\t", 1)
            _IDS[(tree, name)] = meta.split()[2]
    _IDS[(tree, ("<dir>", parent))] = "1"


def path_ids(tree, paths):
    """트리 안 경로마다 객체 id(없으면 '-'). 같은 폴더의 것은 ls-tree 한 번으로 읽는다."""
    out = []
    for p in paths:
        parent = p.rsplit("/", 1)[0] if "/" in p else ""
        if (tree, ("<dir>", parent)) not in _IDS:
            _prefetch(tree, parent)
        out.append(f"{p}={_IDS.get((tree, p), '-')}")
    return out


_TOOLS = {}


def tool(*cmd):
    """도구 판(지문에 넣는다). 없으면 '-'."""
    if cmd not in _TOOLS:
        try:
            p = subprocess.run(list(cmd), cwd=ROOT, capture_output=True, text=True)
            _TOOLS[cmd] = p.stdout.strip()
        except OSError:
            _TOOLS[cmd] = "-"
    return _TOOLS[cmd]


def digest(parts):
    return hashlib.sha256("\n".join(parts).encode()).hexdigest()[:20]


def rust_fp(tree, m, fwd, refs):
    deps = sorted(closure([m], fwd) | refs.get(m, set()))
    parts = path_ids(tree, [f"crates/{d}" for d in deps] + list(ROOT_INPUTS) + list(RULE_INPUTS))
    return digest(parts + [tool("rustc", "-vV"), tool("cargo", "-V")])


def web_fp(tree, m):
    parts = path_ids(tree, [f"crates/{m}/web", f"crates/{m}/tests/web", f"crates/{m}/tests/e2e"] + list(RULE_INPUTS))
    return digest(parts + [tool("node", "-v")])


def tree_fp(paths):
    tree = work_tree()
    return digest(path_ids(tree, paths) + path_ids(tree, list(RULE_INPUTS)))


def load_passed():
    try:
        return json.loads(PASSED.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def record(unit, fp, items):
    d = load_passed()
    lst = [e for e in d.get(unit, []) if not (e["fp"] == fp and e.get("items") == items)]
    lst.append({"fp": fp, "items": items, "at": time.strftime("%Y-%m-%dT%H:%M:%S")})
    d[unit] = lst[-KEEP:]
    PASSED.parent.mkdir(parents=True, exist_ok=True)
    PASSED.write_text(json.dumps(d, ensure_ascii=False, indent=1), encoding="utf-8")


NO_CACHE = False


def passed(unit, fp, items=None):
    """같은 지문으로 통과한 때(글) 또는 None. items 가 있으면 그때 돈 항목이 지금 필요한 것을 다 덮어야 한다('*' 는 전부).
    실패는 적지 않으므로(통과만 --record) 실패한 결과를 캐시로 넘기는 일은 없다."""
    if NO_CACHE:
        return None
    for e in reversed(load_passed().get(unit, [])):
        if e["fp"] != fp:
            continue
        got = e.get("items") or []
        if items is None or "*" in got or set(items) <= set(got):
            return e.get("at", "?")
    return None


# --- 화면
def ts_imports(web):
    """web/src 의 .ts 파일 → 그 파일이 import 하는 web/src 안 파일들(상대 경로만)."""
    rx = re.compile(r"""(?:import|export)\s[^'"]*?from\s*['"]([^'"]+)['"]|import\s*\(?\s*['"]([^'"]+)['"]""")
    src = web / "src"
    graph = {}
    for f in src.rglob("*"):
        if f.suffix not in (".ts", ".css") or not f.is_file():
            continue
        text = f.read_text(encoding="utf-8", errors="replace") if f.suffix == ".ts" else ""
        deps = set()
        for a, b in rx.findall(text):
            spec = (a or b).split("?")[0]
            if not spec.startswith("."):
                continue
            t = (f.parent / spec).resolve()
            for cand in (t, t.with_suffix(".ts"), Path(str(t)[:-3] + ".ts") if str(t).endswith(".js") else None,
                         t / "index.ts"):
                if cand and cand.is_file():
                    deps.add(cand.relative_to(web.resolve()).as_posix())
                    break
        graph[f.resolve().relative_to(web.resolve()).as_posix()] = deps
    return graph


def pages_of(web, changed_src):
    """바뀐 web/src 파일들을 가져다 쓰는 페이지 폴더들. 모르는 곳(공통)에 닿으면 None(= 전체)."""
    graph = ts_imports(web)
    rev = {}
    for f, deps in graph.items():
        for d in deps:
            rev.setdefault(d, set()).add(f)
    users = closure(changed_src, rev)
    pages = set()
    for f in users:
        parts = f.split("/")
        if len(parts) < 3 or parts[1] not in PAGES:
            return None
        pages.add(parts[1])
    return pages


def specs_for(e2e_dir, pages):
    """페이지들에 대응하는 e2e spec. 주소를 글로 갖지 않거나 변수로 여는(goto(path)) spec 은 모르는 것이라 늘 넣는다."""
    out = []
    for spec in sorted(e2e_dir.glob("*.spec.ts")):
        text = spec.read_text(encoding="utf-8", errors="replace")
        lits = set(re.findall(r"""goto\(\s*['"]([^'"]*)['"]""", text))
        dynamic = re.search(r"goto\(\s*[^'\"\s)]", text)
        mentions = {p for p, url in PAGES.items() if (url != "/" and url in text) or (url == "/" and "/" in lits)}
        if dynamic or not lits or mentions & pages:
            out.append(spec.name)
    return out


def web_plan(m, files, notes):
    """화면 모듈 m 의 계획: vitest(none · related · tests · all) · 빌드 · e2e(none · specs · all)."""
    web = ROOT / f"crates/{m}/web"
    pre_w, pre_t, pre_e = f"crates/{m}/web/", f"crates/{m}/tests/web/", f"crates/{m}/tests/e2e/"
    wf = [f[len(pre_w):] for f in files if f.startswith(pre_w)]
    tf = [f[len(pre_t):] for f in files if f.startswith(pre_t)]
    ef = [f[len(pre_e):] for f in files if f.startswith(pre_e)]
    http = [f for f in files if HTTP_RX.match(f)] if m == "server" else []
    plan = {"vitest": "none", "vfiles": [], "build": bool(wf), "e2e": "none", "specs": []}
    # vitest: 공통 소스 · CSS · 설정 · 시험 도움 파일이면 전체, 아니면 import 그래프 + 파일을 읽는 시험 + 바뀐 시험
    helpers = [f for f in tf if not f.endswith(".test.ts")]
    src = [f for f in wf if f.startswith("src/")]
    src_like = [f for f in wf if f.startswith(("src/", "dist/", "public/")) or f.endswith(".html")]
    config = [f for f in wf if "/" not in f and not f.endswith(".html")]
    if config or helpers or [f for f in src if f.startswith(WEB_COMMON) or f.endswith(".css")]:
        plan["vitest"] = "all"
        notes.append(f"  화면 시험 범위: {m} — 전체 (공통 · 설정 · 시험 도움 파일 · CSS 가 바뀜)")
    elif src_like or tf:
        tests = [f"../tests/web/{f}" for f in tf if f.endswith(".test.ts")]
        fs_tests = [f"../tests/web/{p.name}" for p in sorted((ROOT / pre_t).glob("*.test.ts"))
                    if FS_RX.search(p.read_text(encoding="utf-8", errors="replace"))
                    or any(h.stem in p.read_text(encoding="utf-8", errors="replace")
                           for h in (ROOT / pre_t).glob("*.ts") if not h.name.endswith(".test.ts")
                           and FS_RX.search(h.read_text(encoding="utf-8", errors="replace")))] if src_like else []
        plan["vitest"] = "related" if src else "tests"
        plan["vfiles"] = sorted(set(src)) + sorted(set(tests + fs_tests))
        notes.append(f"  화면 시험 범위: {m} — " + (f"import 그래프로 이어진 시험(vitest related {len(src)}개 파일) + "
                                                 if src else "") + f"시험 {len(set(tests + fs_tests))}개(바뀐 시험 · 파일을 읽는 시험)")
    # e2e
    if http or [f for f in wf if not (f.startswith("src/") or f.endswith(".html"))] or \
            [f for f in ef if not f.endswith(".spec.ts")]:
        plan["e2e"] = "all"
    elif wf or ef:
        pages = set()
        for f in wf:
            if f.endswith(".html"):
                pg = HTML_PAGE.get(f) or (f.split("/")[0] if f.split("/")[0] in PAGES else None)
                if pg is None:
                    pages = None
                    break
                pages.add(pg)
        if pages is not None and src:
            got = pages_of(web, [f for f in src])
            pages = None if got is None else pages | got
        if pages is None:
            plan["e2e"] = "all"
        else:
            specs = set(specs_for(ROOT / pre_e, pages)) if pages else set()
            specs |= {f for f in ef if f.endswith(".spec.ts")}
            plan["e2e"], plan["specs"] = "specs", sorted(specs)
    if plan["e2e"] == "all":
        notes.append(f"  e2e 범위: {m} — 전체 (공통 화면 · 설정 · HTTP 경로 · 시험 도움 파일이 바뀜, 또는 대응을 모름)")
    elif plan["e2e"] == "specs":
        notes.append(f"  e2e 범위: {m} — 바뀐 화면에 대응하는 spec {len(plan['specs'])}개: {' '.join(plan['specs'])}")
    return plan


# --- 계획
def classify(base, focus, wip):
    """바뀐 파일을 나눈다. ('ALL', 까닭) 또는 (None, 자료)."""
    if not ok_commit(base):
        return "ALL", f"기준 {base} 이 커밋이 아니다"
    start, files = changed_files(base)
    order = module_order()
    existing = existing_modules()
    code_seed, test_seed, ignored, used = set(), set(), {}, []
    for f in files:
        if f.startswith(ALL_PREFIXES):
            return "ALL", f"그래프가 못 따라가는 파일이 바뀜: {f}"
        if f == "Cargo.lock":
            if lock_versions_changed(start):
                return "ALL", "Cargo.lock 의 판이 바뀜"
            continue
        mm = re.match(r"^crates/([^/]+)/(.*)$", f)
        if mm:
            c, rest = mm.group(1), mm.group(2)
            if c not in existing:
                return "ALL", f"크레이트가 아닌 곳이거나 지운 크레이트: {f}"
            if c in wip:
                ignored.setdefault(c, "다른 노드가 작업 중 — 그 모듈의 G2 가 본다")
                continue
            if focus and c not in focus:
                ignored.setdefault(c, "다른 모듈의 변경 — 그 모듈의 G2 와 push 전 검사가 본다")
                continue
            used.append(f)
            (test_seed if rest.startswith("tests/") else code_seed).add(c)
            continue
        if re.match(r"^docs/dev/modules/[^/]+\.md$", f):
            continue  # 지시서: 정적 검사(5 · 5b)는 늘 모든 모듈에 돈다
        if f.startswith(NO_CODE_PREFIXES) or f in NO_CODE_FILES or ("/" not in f and f.endswith(".md")):
            continue
        return "ALL", f"모르는 곳이 바뀜: {f}"
    return None, {"start": start, "files": used, "code": code_seed, "test": test_seed, "ignored": ignored,
                  "order": order, "existing": existing}


def full_plan(why_all, focus, wip):
    """전체 범위: 모든 모듈 · 화면 전부. 캐시(같은 지문)만 쓴다."""
    existing = existing_modules()
    data = {"files": None, "code": set(existing), "test": set(), "ignored": {}, "order": module_order(),
            "existing": existing}
    return data, why_all


def plan(base, focus, wip, full=False):
    notes, out = [], {}
    why_all = "전체 검사" if full else None
    if not full:
        kind, data = classify(base, focus, wip)
        if kind == "ALL":
            why_all = data
    if why_all:
        data, _ = full_plan(why_all, focus, wip)
        notes.append(f"범위: 전체 — {why_all}")
    order, existing = data["order"], data["existing"]
    rev = reverse_graph()
    fwd = forward_graph(existing)
    if rev is None or fwd is None:
        if not why_all:
            return plan(base, focus, wip, full=True)
        rev, fwd = rev or {}, fwd or {m: set(existing) for m in existing}
    refs = file_refs(existing)
    ref_users = {}
    for user, targets in refs.items():
        for t in targets:
            ref_users.setdefault(t, set()).add(user)
    why = {}
    for c in data["code"]:
        why[c] = why_all and "전체" or "코드가 바뀜"
    for c in data["test"]:
        why.setdefault(c, "시험만 바뀜(그 크레이트만)")
    for c in closure(data["code"], rev) - set(why):
        src = sorted(x for x in data["code"] if c in closure([x], rev))
        why[c] = f"기댐({', '.join(src)})"
    for c in sorted(data["code"] | data["test"]):
        for u in ref_users.get(c, ()):
            why.setdefault(u, f"{c} 의 파일을 읽음")
    for r in RISK:
        if r in existing and r not in wip and r not in data["code"]:
            hit = sorted(closure([r], fwd) & data["code"])
            if hit:
                why[r] = f"돈 · 안전 — {', '.join(hit)} 에 기댐(늘 돈다)"
    for f in focus:
        why.setdefault(f, "이 노드의 모듈")
    for w in wip:
        why.pop(w, None)
    tests = [m for m in order if m in why]
    tree = work_tree()
    for c, reason in sorted(data["ignored"].items()):
        notes.append(f"- 범위 밖: {c} — {reason}")
    run_tests, run_clippy = [], []
    for m in tests:
        fp = rust_fp(tree, m, fwd, refs)
        out[f"FP_{m}"] = fp
        for unit, lst, label in (("test", run_tests, "3 테스트"), ("clippy", run_clippy, "2 clippy")):
            at = passed(f"{unit}:{m}", fp)
            if at:
                notes.append(f"캐시: {label} {m} — 같은 입력으로 {at} 통과 (지문 {fp[:8]}, {why[m]})")
            else:
                lst.append(m)
        if m in run_tests:
            notes.append(f"  시험 {m}: {why[m]}")
    for m in existing:
        if m not in why and m not in wip:
            notes.append(f"건너뜀: 3 테스트 {m} — 까닭(영향 없음)")
    fmt_fp = digest(path_ids(tree, ["crates"] + list(RULE_INPUTS)) + [tool("rustfmt", "-V")])
    fmt_at = passed("fmt", fmt_fp)
    if fmt_at:
        notes.append(f"캐시: 1 모양 (cargo fmt) — 같은 입력으로 {fmt_at} 통과")
    out.update(MODE="full" if why_all else "affected", TEST_MODS=" ".join(run_tests),
               CLIPPY_MODS=" ".join(run_clippy), ALL_TEST_MODS=" ".join(tests),
               STATIC_MODS=" ".join(m for m in existing if m not in wip), WIP=" ".join(wip),
               FOCUS=" ".join(focus), FP_FMT=fmt_fp, FMT="done" if fmt_at else "run")
    install_fp = digest(path_ids(tree, ["install.sh", "install.ps1"] + list(RULE_INPUTS)))
    out["FP_INSTALL"] = install_fp
    out["INSTALL"] = "none" if not why_all else ("done" if passed("install", install_fp) else "run")
    if out["INSTALL"] == "done":
        notes.append(f"캐시: 11 설치 스크립트 시험 — 같은 입력으로 {passed('install', install_fp)} 통과")
    webs = [m for m in existing if (ROOT / f"crates/{m}/web/package.json").is_file() and m not in wip]
    out["WEB_MODS"] = " ".join(webs)
    for m in webs:
        if why_all:
            wp = {"vitest": "all", "vfiles": [], "build": True, "e2e": "all", "specs": []}
        else:
            wp = web_plan(m, data["files"], notes)
        fpw = web_fp(tree, m)
        fpe = digest([fpw, rust_fp(tree, m, fwd, refs)])
        out[f"FPW_{m}"], out[f"FPE_{m}"] = fpw, fpe
        for unit, label in (("tsc", "3w 화면 형 검사 (tsc)"), ("lint", "3l 화면 린트 (eslint)")):
            at = passed(f"{unit}:{m}", fpw)
            out[f"{unit.upper()}_{m}"] = "done" if at else "run"
            if at:
                notes.append(f"캐시: {label}: {m} — 같은 입력으로 {at} 통과")
        v = wp["vitest"]
        at = passed(f"vitest:{m}", fpw, ["*"] if v == "all" else wp["vfiles"]) if v != "none" else None
        if at:
            notes.append(f"캐시: 3w 화면 시험 (vitest): {m} — 같은 입력으로 {at} 통과")
            v = "done"
        out[f"VITEST_{m}"], out[f"VFILES_{m}"] = v, " ".join(wp["vfiles"])
        b = "run" if wp["build"] else "none"
        at = passed(f"build:{m}", fpw) if b == "run" else None
        if at:
            notes.append(f"캐시: 3w 화면 빌드 · dist 대조: {m} — 같은 입력(소스 · 커밋된 dist)으로 {at} 통과")
            b = "done"
        out[f"BUILD_{m}"] = b
        e = wp["e2e"]
        if e == "specs" and not wp["specs"]:
            e = "none"
        at = passed(f"e2e:{m}", fpe, ["*"] if e == "all" else wp["specs"]) if e != "none" else None
        if at:
            notes.append(f"캐시: 3e 브라우저 e2e: {m} — 같은 입력(화면 · 서버)으로 {at} 통과")
            e = "done"
        out[f"E2E_{m}"], out[f"SPECS_{m}"] = e, " ".join(wp["specs"])
        if v == "none":
            notes.append(f"건너뜀: 3w 화면 시험 (vitest): {m} — 까닭(영향 없음: 화면 · 화면 시험 파일이 안 바뀜)")
        if b == "none":
            notes.append(f"건너뜀: 3w 화면 빌드 · dist 대조: {m} — 까닭(영향 없음: web/ 이 안 바뀜)")
        if e == "none":
            notes.append(f"건너뜀: 3e 브라우저 e2e: {m} — 까닭(영향 없음: 화면 · HTTP 경로 · e2e 시험이 안 바뀜)")
    return out, notes


def main():
    args = sys.argv[1:]
    if args[:1] == ["--record"] and len(args) >= 3:
        # (보안 검토 2) 시험이 도는 사이 작업 트리가 바뀌었으면(계획 때의 PLAN_TREE와 다르면) 적지 않는다 —
        # 바뀐 트리로 통과한 결과를 계획 때 지문으로 굳히지 않는다.
        planned = os.environ.get("PLAN_TREE", "")
        if planned and work_tree() != planned:
            print(f"캐시에 적지 않음: {args[1]} — 시험이 도는 사이 작업 트리가 바뀜", file=sys.stderr)
            return
        record(args[1], args[2], args[3:] or ["*"])
        return
    if args[:1] == ["--deps"] and len(args) == 2:
        ex = existing_modules()
        fwd = forward_graph(ex)
        if fwd is None:
            print("ALL")
            return
        refs = file_refs(ex)
        print(" ".join(sorted((closure([args[1]], fwd) | refs.get(args[1], set())) - {args[1]})))
        return
    if args[:1] == ["--tree-fp"]:
        print(tree_fp(args[1:]))
        return
    global NO_CACHE
    flags = {a for a in args if a in ("--plan", "--full", "--no-cache")}
    args = [a for a in args if a not in flags]
    NO_CACHE = "--no-cache" in flags
    opts = {"--base": None, "--focus": "", "--wip": ""}
    while args:
        k = args.pop(0)
        if k not in opts or not args:
            print("쓰는 법: affected.py [--plan] [--full] [--no-cache] [--base <ref>] [--focus a,b] [--wip c,d]",
                  file=sys.stderr)
            sys.exit(2)
        opts[k] = args.pop(0)
    base = opts["--base"] or ("origin/main" if ok_commit("origin/main") else "HEAD~1")
    focus = [x for x in re.split(r"[,\s]+", opts["--focus"]) if x]
    wip = [x for x in re.split(r"[,\s]+", opts["--wip"]) if x and x not in focus]
    if "--plan" not in flags:
        # 옛 쓰임: 돌릴 Rust 시험 모듈 한 줄(캐시는 보지 않는다)
        NO_CACHE = True
        kind, _ = classify(base, focus, wip)
        if kind == "ALL":
            print("ALL")
            return
        p, _ = plan(base, focus, wip)
        print(p.get("ALL_TEST_MODS", ""))
        return
    p, notes = plan(base, focus, wip, full="--full" in flags)
    p["NOTES"] = "\n".join(notes)
    for k, v in p.items():
        print(f"{k}={shlex.quote(str(v))}")


if __name__ == "__main__":
    main()
