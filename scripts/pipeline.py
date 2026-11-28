#!/usr/bin/env python3
"""그래프 실행기. docs/dev/graph.toml 을 읽어 순서 · 권한 · 반복 한도를 지키게 한다.

    pipeline.py status                 모듈마다 지금 어느 단계인가
    pipeline.py next                   지금 시작할 수 있는 노드 (여러 개면 병렬)
    pipeline.py prompt <노드> <모듈>   그 노드에 줄 프롬프트 (고정 틀 + 지시서)
    pipeline.py start <노드> <모듈> [--lock]   노드 시작 (앞 단계가 안 끝났으면 거부). N1 --lock: 동작 잠금 모드
    pipeline.py gate <GR|G1|G2> <모듈>  기계 검사를 돌리고 결과를 기록 (같은 실패 2번이면 모델 승급, 3번이면 사람에게)
    pipeline.py lock <모듈>            G1 통과 뒤 테스트를 커밋하고 red/<모듈> 태그로 잠근다
    pipeline.py review <모듈> <통과|되돌림> [메모]   AI 검토 판정 기록
    pipeline.py scope                  진행 중인 노드가 맡지 않은 곳을 고쳤는지
    pipeline.py resume <모듈>          사람에게 넘어간 모듈을 사람이 다시 연다
    pipeline.py reopen <모듈> [--lock] <이유>   조정자가 다시 연다. R(대조)부터 돈다. --lock: 동작 잠금 모드
    pipeline.py amend <모듈> <이유>    뜻이 안 바뀌는 테스트 결함(이름 · 모양)을 조정자가 바로잡고 다시 잠근다
    pipeline.py gate-batch <GR|G1|G2> <모듈…>   여러 모듈의 관문을 한 번에(빌드 · 검사는 한 번, 결과는 모듈마다)
    pipeline.py tidy [--all] [--dry-run] [<모듈…>]   끝난 차수의 지시서 절을 한 줄로 줄이고 끝난 문서를 정리한다
    pipeline.py flaky                  흔들린 시험(혼자 다시 돌려 통과한 것) 횟수 — 고칠 후보
    pipeline.py cache clear            검사 캐시(.pipeline/cache)를 지운다

상태는 .pipeline/state.json 에 둔다.
"""
import fnmatch
import hashlib
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import affected  # noqa: E402  같은 폴더의 영향 범위 · 지문 계산
GRAPH = tomllib.loads((ROOT / "docs/dev/graph.toml").read_text(encoding="utf-8"))
STATE_FILE = ROOT / ".pipeline/state.json"
# 실행기가 만드는 커밋 끝에 붙는 줄. 바꿀 때는 이 한 곳만 고친다.
COAUTHOR = "\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
# 모듈 → 폴더. 기본은 crates/<모듈>(server 는 crates/server). 2026-11-16 조립을 gate 에서 server 로 떼어 냄.
DIRS = {}
# 단계 순서: 할 일 → (다시 연 모듈만: R 대조 중 → 대조 확인됨) → N1 중 → red 확인됨 → 잠김 → N2 중 → green → N3 중 → 검토 통과
ORDER = ["todo", "r", "r_ok", "n1", "red_ok", "locked", "n2", "green", "n3", "reviewed"]
LABEL = {
    "todo": "할 일", "r": "R 대조 중", "r_ok": "대조 확인됨", "n1": "N1 테스트 작성 중", "red_ok": "red 확인됨 (잠그기 전)", "locked": "테스트 잠김",
    "n2": "N2 구현 중", "green": "green (기계 검사 통과)", "n3": "N3 AI 검토 중", "reviewed": "검토 통과",
    "escalated": "사람에게 넘김",
}


def dirs(m):
    return DIRS.get(m, [f"crates/{m}"])


def load():
    if STATE_FILE.exists():
        return json.loads(STATE_FILE.read_text(encoding="utf-8"))
    return {}


def save(st):
    STATE_FILE.parent.mkdir(exist_ok=True)
    STATE_FILE.write_text(json.dumps(st, ensure_ascii=False, indent=2), encoding="utf-8")


def mod(st, m):
    if m not in GRAPH["modules"]:
        sys.exit(f"모르는 모듈: {m} (있는 것: {', '.join(GRAPH['modules'])})")
    return st.setdefault(m, {"stage": "todo", "fails": 0, "last_fail": "", "rounds": 0, "log": []})


def at_least(st, m, stage):
    s = mod(st, m)["stage"]
    return s in ORDER and ORDER.index(s) >= ORDER.index(stage)


def run(cmd, env=None):
    import os
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace",
                       env=dict(os.environ, **(env or {})))
    return p.returncode, p.stdout + p.stderr


BUSY = ("r", "r_ok", "n1", "red_ok", "locked", "n2")


def busy(st, exclude=()):
    """다른 노드가 작업 중인 모듈(시험이 아직 red 일 수 있다). G2 의 영향 범위에서 뺀다(그 모듈의 G2 가 본다)."""
    return [m for m, d in st.items() if m not in exclude and m in GRAPH["modules"]
            and (d["stage"] in BUSY or (d["stage"] == "todo" and d.get("reconcile")))]


def round_base(d):
    """이 차수가 열린 커밋: 다시 연 때(reopen_at) 또는 처음 N1 을 시작한 때(opened_at)."""
    for k in ("reopen_at", "opened_at"):
        b = d.get(k)
        if b and run(["git", "rev-parse", "-q", "--verify", f"{b}^{{commit}}"])[0] == 0:
            return b
    return None


def head():
    return run(["git", "rev-parse", "HEAD"])[1].strip()


def runnable(st):
    out = []
    for m, spec in GRAPH["modules"].items():
        d = mod(st, m)
        s = d["stage"]
        if s == "todo" and all(at_least(st, n, "green") for n in spec["needs"]):
            out.append(("R" if d.get("reconcile") else "N1", m))
        elif s == "r_ok":
            out.append(("N1", m))
        elif s == "locked":
            out.append(("N2", m))
        elif s == "green":
            out.append(("N3", m))
    return out


def cmd_status(st, _):
    for m, spec in GRAPH["modules"].items():
        d = mod(st, m)
        extra = f" · 같은 실패 {d['fails']}번" if d["fails"] else ""
        extra += f" · 검토 되돌림 {d['rounds']}번" if d["rounds"] else ""
        extra += " · 동작 잠금" if d.get("mode") == "lock" and d["stage"] != "reviewed" else ""
        extra += " · 모델 승급" if d.get("escalate_model") else ""
        extra += " · 대조 먼저" if d.get("reconcile") and d["stage"] == "todo" else ""
        needs = f" (먼저: {', '.join(spec['needs'])})" if spec["needs"] else ""
        print(f"{m:8} {LABEL[d['stage']]}{extra}{needs}")
    todo = [f"pipeline.py lock {m}" for m in GRAPH["modules"] if st[m]["stage"] == "red_ok"]
    nxt = [f"{n} {m}" for n, m in runnable(st)]
    print("다음: " + (" / ".join(todo + nxt) or "없음"))


def cmd_next(st, _):
    for n, m in runnable(st):
        print(f"{n} {m}")


def cmd_prompt(st, a):
    node, m = a[0], a[1]
    mod(st, m)
    tpl = (ROOT / GRAPH["nodes"][node]["prompt"]).read_text(encoding="utf-8")
    d = " · ".join(f"`{x}/`" for x in dirs(m))
    print(tpl.replace("{module}", m).replace("{dirs}", d).replace("{brief}", f"docs/dev/modules/{m}.md"))
    print(f"\n저장소: `{ROOT}`")


def cmd_start(st, a):
    node, m = a[0], a[1]
    lock = "--lock" in a[2:]
    d = mod(st, m)
    want = {"R": "todo", "N1": "r_ok" if d["stage"] == "r_ok" else "todo", "N2": "locked", "N3": "green"}[node]
    if d["stage"] == "escalated":
        sys.exit(f"아직 안 된다: {m} 은 사람에게 넘어갔다. 사람이 pipeline.py resume {m} 을 해야 한다")
    if node in ("R", "N1"):
        late = [n for n in GRAPH["modules"][m]["needs"] if not at_least(st, n, "green")]
        if late:
            sys.exit(f"아직 안 된다: {m} 은 {', '.join(late)} 이(가) green 이 된 뒤에 시작한다")
    if node == "R" and not d.get("reconcile"):
        sys.exit(f"아직 안 된다: R(대조)은 다시 연(reopen) 모듈만 돈다. {m} 은 N1 부터 한다")
    if node == "N1" and d.get("reconcile"):
        sys.exit(f"아직 안 된다: {m} 은 다시 열렸다. 먼저 R(대조)을 하고 gate GR 을 통과한다 (pipeline.py start R {m})")
    if lock and node != "N1":
        sys.exit("--lock 은 N1 에만 쓴다 (동작 잠금 모드)")
    if d["stage"] != want:
        sys.exit(f"아직 안 된다: {node} {m} 은 '{LABEL[want]}' 단계에서 시작한다 (지금: {LABEL[d['stage']]})")
    d["stage"] = node.lower()
    d["fails"], d["last_fail"], d["escalate_model"] = 0, "", False
    if node == "N1" and not d.get("reopen_at"):
        d["opened_at"] = head()
    if lock:
        d["mode"] = "lock"
    d["log"].append(f"{node} 시작" + (" (동작 잠금)" if lock else ""))
    save(st)
    print(f"{node} {m} 시작. 고칠 수 있는 곳: {', '.join(allowed(node, m))}")
    if node == "N1" and d.get("mode") == "lock":
        print("동작 잠금 모드: G1 은 시험이 지금 구현에서 모두 통과해야 통과한다 (src 는 고치지 않는다)")


def allowed(node, m):
    return [w.replace("{dir}", x).replace("{module}", m)
            for w in GRAPH["nodes"][node]["writes"] for x in (dirs(m) if "{dir}" in w else [""])]


def changed_files():
    _, out = run(["git", "status", "--porcelain", "--untracked-files=all"])
    return [l[3:].split(" -> ")[-1].strip('"') for l in out.splitlines() if l.strip()]


def scope_problems(st):
    active = [(d["stage"].upper(), m) for m, d in st.items() if d["stage"] in ("r", "n1", "n2")]
    globs = [g for n, m in active for g in allowed(n, m)] + GRAPH.get("outside_graph", [])
    return [f for f in changed_files() if not any(fnmatch.fnmatch(f, g) for g in globs)]


def cmd_scope(st, _):
    bad = scope_problems(st)
    if bad:
        print("✗ 맡지 않은 곳을 고쳤다 — 고치기: 그 변경을 되돌린다 (진행 중인 노드는 자기 모듈 폴더만 고친다)")
        for f in bad:
            print("   " + f)
        sys.exit(1)
    print("✓ 고친 곳이 모두 맡은 범위 안이다")


# --- 대조(R) 표 (QMIT 결정 1). 지시서의 그 차수 절 아래 "### 대조" 표: 시험 · 문서 경로 | 판정 | 근거
VERDICTS = {"유지", "고침", "지움", "열어둠"}
# "지움"의 근거: 사용자 말(따옴표로 옮김) · 결정 번호 · 지시서 확정 항목(차수) 가운데 하나.
EVIDENCE = re.compile(r'사용자[^|"“]*["“][^"”]+["”]|결정\s*\d+|지시서\s*\d+차')
TEST_FN = re.compile(r'#\[(?:tokio::)?test[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)')


def brief_path(m):
    return f"docs/dev/modules/{m}.md"


def reconcile_tables(text):
    """지시서 글에서 "### 대조" 표들을 [(줄 목록)] 으로. 줄은 [경로, 판정, 근거] 칸."""
    tables, lines = [], text.splitlines()
    for i, line in enumerate(lines):
        if line.strip() != "### 대조":
            continue
        rows, j = [], i + 1
        # 머리와 표 사이의 빈 줄 · 설명 글은 건너뛴다(다음 머리 전까지).
        while j < len(lines) and not lines[j].lstrip().startswith(("|", "#")):
            j += 1
        while j < len(lines) and lines[j].lstrip().startswith("|"):
            cells = [c.strip() for c in lines[j].strip().strip("|").split("|")]
            if not all(re.fullmatch(r":?-+:?", c) for c in cells if c):
                rows.append(cells)
            j += 1
        tables.append(rows[1:])  # 첫 줄은 머리
    return tables


def round_sections(text):
    """지시서의 절(## 머리)들: [(머리 줄, 시작 줄 번호, 끝 줄 번호(다음 머리), 차수 절인가)]. 코드 블록 안은 머리로 보지 않는다."""
    lines, heads, fence = text.split("\n"), [], False
    for i, line in enumerate(lines):
        if line.startswith("```"):
            fence = not fence
        if not fence and line.startswith("## "):
            heads.append(i)
    out = []
    for k, i in enumerate(heads):
        end = heads[k + 1] if k + 1 < len(heads) else len(lines)
        out.append((lines[i], i, end, bool(ROUND_RX.match(lines[i]))))
    return out


ROUND_RX = re.compile(r"^## \d+차")
STUB = "- 끝남:"


def stub_headings(text):
    """tidy 가 한 줄로 줄인 차수 절의 머리들."""
    lines = text.split("\n")
    out = set()
    for h, i, end, is_round in round_sections(text):
        body = [l for l in lines[i + 1:end] if l.strip()]
        if is_round and body and body[0].startswith(STUB):
            out.add(h)
    return out


def tables_with_heading(text):
    """[(그 표가 든 ## 절의 머리, 표 줄들)]."""
    secs = round_sections(text)
    lines = text.split("\n")
    out, k = [], 0
    rows_all = reconcile_tables(text)
    for i, line in enumerate(lines):
        if line.strip() == "### 대조":
            h = next((hh for hh, s0, e0, _ in secs if s0 < i < e0), "")
            out.append((h, rows_all[k]))
            k += 1
    return out


def check_reconcile(m, d):
    """GR: 이번 차수의 대조 표가 있고, 판정이 넷 가운데 하나이고, "지움" 줄마다 근거가 적혔는가 (형식 검사).

    "표가 하나 늘었다"는 다시 연 때(reopen_at)의 지시서와 센다. 그 사이 tidy 가 한 줄로 줄인 옛 차수 절의 표는
    (지금 지시서에서 그 절이 '- 끝남:' 한 줄이면) 옛 표 수에서 뺀다 — 그래야 tidy 뒤에도 새 표 하나로 통과한다.
    옛 표를 고쳐 쓰는 것은 여전히 새 표로 세지 않는다(그 절이 줄여진 절이 아니므로 옛 수에 그대로 든다).
    """
    text = (ROOT / brief_path(m)).read_text(encoding="utf-8")
    tables = reconcile_tables(text)
    before = 0
    if d.get("reopen_at"):
        _, old = run(["git", "show", f"{d['reopen_at']}:{brief_path(m)}"])
        stubs = stub_headings(text)
        before = sum(1 for h, _ in tables_with_heading(old) if h not in stubs)
    if len(tables) <= before or not tables[-1]:
        return 1, (f"✗ 대조 표 없음 — 고치기: {brief_path(m)} 의 이번 차수 절 아래 '### 대조' 표"
                   "(시험 · 문서 경로 | 판정 | 근거)를 쓴다. 다시 연(reopen) 때의 지시서보다 표가 하나 늘어야 한다 — "
                   "reopen 전에 써서 커밋한 표는 이번 차수 표로 세지 않으니 reopen 뒤에 쓴다\nGR 실패: " + m)
    out, bad = [], False
    for row in tables[-1]:
        path, verdict, why = (row + ["", "", ""])[:3]
        v = verdict.replace(" ", "")
        if v not in VERDICTS:
            out.append(f"✗ 판정이 넷 가운데 하나가 아니다 — {path}: '{verdict}' (유지 · 고침 · 지움 · 열어 둠)")
            bad = True
        elif v == "지움" and not EVIDENCE.search(why):
            out.append(f"✗ 근거 없는 지움 — {path}: 근거 칸에 사용자 \"…\" · 결정 <번호> · 지시서 <N>차 가운데 하나를 적는다")
            bad = True
    out.append(f"{'GR 실패' if bad else 'GR 통과'}: {m} (대조 {len(tables[-1])}줄)")
    return (1 if bad else 0), "\n".join(out)


def test_inventory(files):
    """{시험 파일: 시험 함수 이름 집합}. files: {경로: 글}."""
    return {f: set(TEST_FN.findall(body)) if f.endswith(".rs") else set() for f, body in files.items()}


def unlisted_deletions(m, base):
    """red 태그 이전 판(base) 대비 지운 시험 파일 · 시험 함수 가운데 대조 표의 "지움"에 없는 것."""
    tdirs = [x + "/tests" for x in dirs(m)]
    _, names = run(["git", "-c", "core.quotePath=false", "ls-tree", "-r", "-z", "--name-only", base, "--", *tdirs])
    old = test_inventory({f: run(["git", "show", f"{base}:{f}"])[1] for f in names.split("\0") if f})
    now = test_inventory({str(f.relative_to(ROOT)): f.read_text(encoding="utf-8", errors="replace")
                          for x in tdirs for f in sorted((ROOT / x).rglob("*")) if f.is_file()})
    tables = reconcile_tables((ROOT / brief_path(m)).read_text(encoding="utf-8"))
    listed = [r[0] for r in (tables[-1] if tables else []) if len(r) > 1 and r[1].replace(" ", "") == "지움"]
    gone_files = [f for f in old if f not in now]
    left = set().union(*now.values()) if now else set()
    gone_fns = sorted({fn for f, fns in old.items() if f in now for fn in fns} - left)
    miss = [f for f in gone_files if not any(f in c for c in listed)]
    miss += [fn for fn in gone_fns if not any(re.search(rf"(?<!\w){re.escape(fn)}(?!\w)", c) for c in listed)]
    return miss


NEED = {"GR": "r", "G1": "n1", "G2": "n2"}


def red_cmd(m, d):
    """G1(red.sh) 명령과 환경. 동작 잠금이면 다시 연 때의 판을 기준으로 바뀐 · 새 시험만 돌린다."""
    lock = d.get("mode") == "lock"
    env = {"PI_RED_BASE": d["reopen_at"]} if lock and d.get("reopen_at") else {}
    return ["bash", "scripts/red.sh", m] + (["--lock"] if lock else []), env


def g1_fp(m):
    return affected.tree_fp(["crates", "Cargo.toml", "Cargo.lock"])


def gate_check(st, gate, m):
    d = st[m]
    if gate == "GR":
        code, out = check_reconcile(m, d)
    elif gate == "G1":
        cmd, env = red_cmd(m, d)
        code, out = run(cmd, env)
        miss = unlisted_deletions(m, d["base"]) if d.get("base") else []
        if miss:
            code = 1
            out += ("\n✗ 대조 표에 없는 시험을 지웠다 — 고치기: 되살리거나, R(대조)이 근거와 함께 '지움'으로 적은 것만 지운다\n"
                    + "\n".join("   " + x for x in miss) + "\n")
    else:
        code, out = run(verify_cmd(st, [m]))
    bad = scope_problems(st)
    if bad:
        code = 1
        out += "\n✗ 맡지 않은 곳을 고쳤다: " + ", ".join(bad) + " — 고치기: 그 변경을 되돌린다\n"
    return code, out


def verify_cmd(st, mods):
    """G2: 그 차수가 열린 커밋부터의 영향 범위(이 노드의 모듈 + 기대는 크레이트 + 돈 · 안전, 작업 중인 모듈은 뺌)."""
    bases = [round_base(st[m]) for m in mods]
    if not all(bases):
        return ["bash", "scripts/verify.sh", *mods]
    # 가장 오래된 기준(HEAD 에서 가장 먼 것)
    base = max(bases, key=lambda b: int(run(["git", "rev-list", "--count", f"{b}..HEAD"])[1].strip() or 0))
    cmd = ["bash", "scripts/verify.sh", "--affected", "--base", base, "--focus", ",".join(mods)]
    wip = busy(st, exclude=mods)
    if wip:
        cmd += ["--wip", ",".join(wip)]
    return cmd


def gate_pass(st, gate, m):
    d = st[m]
    d["stage"] = {"GR": "r_ok", "G1": "red_ok", "G2": "green"}[gate]
    d["fails"], d["last_fail"], d["escalate_model"] = 0, "", False
    d["log"].append(f"{gate} 통과")
    if gate == "GR":
        # 대조 표는 바로 커밋한다. 남겨 두면 N1 의 범위 검사가 "맡지 않은 곳"으로 본다.
        d["reconcile"] = False
        run(["git", "add", "--", brief_path(m)])
        c, _ = run(["git", "commit", "-q", "-m", f"docs({m}): 대조 표 (R)\n\nGR 통과." + COAUTHOR, "--", brief_path(m)])
        print("커밋했다: docs(%s) 대조 표" % m if c == 0 else "커밋할 변경 없음")
    if gate == "G1":
        # red 를 확인하면 바로 잠근다. 남겨 두면 그 사이에 테스트가 바뀌거나 다른 모듈의 범위 검사에 걸린다.
        d["g1_fp"] = g1_fp(m)
        save(st)
        cmd_lock(st, [m])
        return
    if gate == "G2":
        # 통과한 구현은 바로 커밋한다. 남겨 두면 다른 모듈의 범위 검사가 "맡지 않은 곳"으로 본다.
        run(["git", "add", "--", *dirs(m), "Cargo.lock"])
        c, o = run(["git", "commit", "-q", "-m", f"feat({m}): 구현 (green)\n\nG2 기계 검사 통과." + COAUTHOR, "--", *dirs(m), "Cargo.lock"])
        print("커밋했다: feat(%s)" % m if c == 0 else "커밋할 변경 없음")
    save(st)


def gate_fail(st, gate, m, out):
    d = st[m]
    sig = hashlib.sha256("\n".join(sorted(l.split(" — ")[0] for l in out.splitlines() if l.startswith("✗"))).encode()).hexdigest()
    d["fails"] = d["fails"] + 1 if sig == d["last_fail"] else 1
    d["last_fail"] = sig
    d["log"].append(f"{gate} 실패 ({d['fails']}번째 같은 실패)")
    if d["fails"] >= GRAPH["max_same_failure"]:
        d["stage"] = "escalated"
        print(f"■ {m}: 같은 실패가 {d['fails']}번 되풀이됐다. 멈추고 사람에게 넘긴다.")
    elif d["fails"] >= GRAPH.get("escalate_model_at", GRAPH["max_same_failure"]):
        d["escalate_model"] = True
        d["log"].append("모델 승급")
        print(f"▲ 같은 실패 {d['fails']}번 — 이 노드를 한 등급 위 모델로 다시 맡긴다(Haiku→Sonnet→Opus→Fable…) ({m})")
    save(st)


def cmd_gate(st, a):
    gate, m = a[0], a[1]
    d = mod(st, m)
    need = NEED[gate]
    if d["stage"] != need:
        sys.exit(f"아직 안 된다: {gate} {m} 은 '{LABEL[need]}' 단계에서 돌린다 (지금: {LABEL[d['stage']]})")
    code, out = gate_check(st, gate, m)
    print(out)
    if code == 0:
        gate_pass(st, gate, m)
        return
    gate_fail(st, gate, m, out)
    sys.exit(1)


MOD_LINE = [re.compile(r"^✗ 3 테스트: (\w+)"), re.compile(r"^✗ [48] [^:—]*: (\w+)"),
            re.compile(r"^✗ 5b? [^(]*\((\w+)\)"), re.compile(r"^✗ 3[wle] [^:]*: (\w+)")]


def attribute(out, mods):
    """한 번 돈 verify 출력을 모듈마다 나눈다: {모듈: [그 모듈에 걸린 ✗ 줄과 그 아래 줄]}.
    모듈 이름이 적힌 ✗ 는 그 모듈에, 모듈 밖 검사(모양 · clippy · 아키텍처 등)나 묶음 밖 모듈(기대는 크레이트)의 ✗ 는
    아래 줄에 적힌 crates/<모듈>/ 에(묶음 모듈이 안 적혔으면 묶음 전부에) 매긴다."""
    res = {m: [] for m in mods}
    blocks, cur = [], None
    for line in out.splitlines():
        if line.startswith(("✓ ", "✗ ", "건너뜀:", "캐시:", "G2 ")):
            cur = [line] if line.startswith("✗ ") else None
            if cur is not None:
                blocks.append(cur)
        elif cur is not None:
            cur.append(line)
    for b in blocks:
        who = []
        for rx in MOD_LINE:
            mm = rx.match(b[0])
            if mm:
                who = [mm.group(1)] if mm.group(1) in res else []
                break
        if not who:
            named = sorted({x for x in re.findall(r"crates/(\w+)/", "\n".join(b)) if x in res})
            who = named or list(mods)
        for m in who:
            res[m] += b
    return res


def cmd_gate_batch(st, a):
    """여러 모듈의 관문을 한 번에. GR 은 모듈마다, G1 은 시험 빌드를 한 번 하고 모듈마다 red.sh,
    G2 는 verify 를 한 번(묶음의 영향 범위) 돌리고 결과를 모듈마다 나눠 기록 · 커밋한다."""
    if len(a) < 2 or a[0] not in NEED:
        sys.exit("쓰는 법: pipeline.py gate-batch <GR|G1|G2> <모듈…>")
    gate, mods = a[0], a[1:]
    for m in mods:
        d = mod(st, m)
        if d["stage"] != NEED[gate]:
            sys.exit(f"아직 안 된다: {gate} {m} 은 '{LABEL[NEED[gate]]}' 단계에서 돌린다 (지금: {LABEL[d['stage']]})")
    failed = []
    if gate in ("GR", "G1"):
        if gate == "G1":
            pk = [x for m in mods for x in ("-p", "price-inha" if m == "server" else f"pi-{m}")]
            run(["cargo", "test", "--no-run", *pk])  # 빌드는 한 번(오류는 모듈마다 red.sh 가 보인다)
        for m in mods:
            code, out = gate_check(st, gate, m)
            print(f"=== {gate} {m}\n{out}")
            if code == 0:
                try:
                    gate_pass(st, gate, m)
                except SystemExit as e:
                    print(e)
                    failed.append(m)
            else:
                gate_fail(st, gate, m, out)
                failed.append(m)
    else:
        code, out = run(verify_cmd(st, mods))
        print(out)
        bad = scope_problems(st)
        per = attribute(out, mods)
        if code != 0 and not any(per.values()):
            per = {m: ["✗ G2 실패(어느 모듈인지 나누지 못함)"] for m in mods}
        for m in mods:
            if bad:
                per[m] = per[m] + ["✗ 맡지 않은 곳을 고쳤다: " + ", ".join(bad)]
            if per[m]:
                print(f"✗ G2 {m}: " + " / ".join(l for l in per[m] if l.startswith("✗")))
                gate_fail(st, gate, m, "\n".join(per[m]))
                failed.append(m)
            else:
                print(f"✓ G2 {m}")
                gate_pass(st, gate, m)
    print(f"\n묶음 {gate}: 통과 {len(mods) - len(failed)} / {len(mods)}" + (f" — 실패: {', '.join(failed)}" if failed else ""))
    if failed:
        sys.exit(1)


def cmd_lock(st, a):
    m = a[0]
    d = mod(st, m)
    if d["stage"] != "red_ok":
        sys.exit(f"아직 안 된다: 잠그기는 G1 을 통과한 뒤에 한다 (지금: {LABEL[d['stage']]})")
    # G1 뒤에 고친 것이 있을 수 있으니 잠그기 직전에 확인한다. G1 이 같은 트리(지문)를 이미 확인했으면 다시 돌리지 않는다.
    if d.get("g1_fp") and d["g1_fp"] == g1_fp(m):
        print(f"G1 이 같은 트리(지문 {d['g1_fp'][:8]})를 방금 확인했다 — red.sh 를 다시 돌리지 않는다")
    else:
        cmd, env = red_cmd(m, d)
        code, out = run(cmd, env)
        if code != 0:
            d["stage"] = "n1"
            save(st)
            sys.exit(out + "\n잠그지 않았다: G1 을 다시 통과해야 한다 (N1 로 되돌림)")
    run(["git", "add", "--", *dirs(m), "Cargo.lock"])
    staged = run(["git", "diff", "--cached", "--quiet", "--", *dirs(m), "Cargo.lock"])[0] != 0
    if staged:
        code, out = run(["git", "commit", "-q", "-m", f"test({m}): 테스트와 뼈대 (red)\n\nG1 통과. 이 뒤로 tests/ 는 잠긴다." + COAUTHOR, "--", *dirs(m), "Cargo.lock"])
        if code != 0:
            sys.exit("커밋 실패:\n" + out)
    else:
        # 바뀐 시험이 이미 커밋돼 있다(동작 잠금 등): 빈 커밋 없이 태그만 옮긴다. 태그가 가리킬 시험이 지금 시험과 같아야 한다.
        tdirs = [x + "/tests" for x in dirs(m)]
        same = run(["git", "diff", "--quiet", "HEAD", "--", *tdirs])[0] == 0 \
            and not run(["git", "ls-files", "-o", "--exclude-standard", "--", *tdirs])[1].strip()
        if not same:
            sys.exit("잠그지 않았다: 커밋할 것이 없는데 작업 트리의 시험이 HEAD 와 다르다")
        print("커밋할 변경 없음 — 시험이 이미 커밋돼 있어 빈 커밋 없이 태그만 옮긴다 (시험 내용 = HEAD)")
    run(["git", "tag", "-f", f"red/{m}"])
    d["stage"] = "locked"
    d.pop("base", None)
    d.pop("g1_fp", None)
    d["log"].append("테스트 잠금")
    save(st)
    print(f"잠갔다: red/{m}")


def cmd_review(st, a):
    m, verdict = a[0], a[1]
    d = mod(st, m)
    if d["stage"] != "n3":
        sys.exit(f"아직 안 된다: 판정은 N3 가 도는 중에 기록한다 (지금: {LABEL[d['stage']]})")
    note = " ".join(a[2:])
    if verdict == "통과":
        d["stage"] = "reviewed"
    elif verdict == "되돌림":
        d["rounds"] += 1
        d["stage"] = "escalated" if d["rounds"] > GRAPH["max_review_rounds"] else "locked"
    else:
        sys.exit("판정은 통과 또는 되돌림")
    d["log"].append(f"N3 {verdict}: {note}")
    save(st)
    print(f"{m}: {LABEL[d['stage']]}")
    if verdict == "통과":
        # 검토를 통과하면 그 모듈 지시서의 옛 차수 절을 한 줄로 줄인다(지금 차수는 그대로). 바뀌면 커밋한다.
        tidy_briefs(st, [m], dry=False, commit=True)


def cmd_resume(st, a):
    d = mod(st, a[0])
    if d["stage"] != "escalated":
        sys.exit("사람에게 넘어간 모듈이 아니다")
    back = "locked" if (ROOT / ".git/refs/tags/red" / a[0]).exists() else "todo"
    d.update(stage=back, fails=0, last_fail="", rounds=0)
    d["log"].append("사람이 다시 엶")
    save(st)
    print(f"{a[0]}: {LABEL[back]}")


def cmd_reopen(st, a):
    """조정자가 모듈을 다시 연다(새 차수 · 테스트 결함). 잠금 태그를 지우고 R(대조)부터 돌게 한다.

    잠금 태그가 가리키던 판을 base 로 남긴다: G1 이 그 판 대비 지운 시험이 대조 표의 "지움"에 다 있는지 본다.
    --lock 이면 그 차수를 동작 잠금 모드로 둔다(G1 이 "지금 구현에서 모두 통과"를 본다).
    """
    m = a[0]
    d = mod(st, m)
    lock = "--lock" in a[1:]
    reason = " ".join(x for x in a[1:] if x != "--lock")
    c, base = run(["git", "rev-parse", "-q", "--verify", f"refs/tags/red/{m}^{{commit}}"])
    _, head = run(["git", "rev-parse", "HEAD"])
    run(["git", "tag", "-d", f"red/{m}"])
    d.update(stage="todo", fails=0, last_fail="", escalate_model=False, reconcile=True,
             mode="lock" if lock else "red", reopen_at=head.strip())
    if c == 0:
        d["base"] = base.strip()
    else:
        d.pop("base", None)
    d["log"].append("다시 엶 (R 대조부터" + (", 동작 잠금" if lock else "") + "): " + reason)
    save(st)
    print(f"{m}: {LABEL['todo']} (R 대조부터 다시{' · 동작 잠금' if lock else ''}) — pipeline.py start R {m}")


def cmd_amend(st, a):
    """잠긴 테스트의 결함(이름 · 모양처럼 뜻이 바뀌지 않는 것)을 조정자가 바로잡고 다시 잠근다.

    뜻이 바뀌는 고침은 reopen 으로 N1 부터 다시 한다. 기계가 보는 것: 테스트 수와 필수 번호가 그대로인가.
    """
    m = a[0]
    d = mod(st, m)
    if len(a) < 2:
        sys.exit("이유를 적는다: pipeline.py amend <모듈> <이유>")
    tdirs = [x + "/tests" for x in dirs(m)]
    count = lambda ref: sum(
        len([l for l in run(["git", "show", f"{ref}:{f}"])[1].splitlines() if l.strip().startswith(("#[test]", "#[tokio::test"))])
        for f in run(["git", "ls-tree", "-r", "--name-only", ref, "--", *tdirs])[1].split())
    before = count(f"red/{m}")
    run(["git", "add", "--", *tdirs])
    code, out = run(["git", "commit", "-q", "-m", f"test({m}): 잠긴 테스트 바로잡기\n\n{' '.join(a[1:])}" + COAUTHOR, "--", *tdirs])
    if code != 0:
        sys.exit("커밋 실패:\n" + out)
    after = count("HEAD")
    code, miss = run(["bash", "-c", f". scripts/lib.sh && missing_ids {m}"])
    if before != after or miss.strip():
        run(["git", "reset", "-q", "--soft", "HEAD~1"])
        sys.exit(f"거부: 테스트 수가 바뀌었거나({before} → {after}) 필수 번호가 빠졌다({miss.strip()}). 뜻이 바뀌는 고침은 reopen 으로 한다")
    run(["git", "tag", "-f", f"red/{m}"])
    d["log"].append("잠긴 테스트 바로잡음: " + " ".join(a[1:]))
    save(st)
    print(f"바로잡고 다시 잠갔다: red/{m} (테스트 {after}개 그대로)")


# --- 문서 정리 (tidy, 2026-11-28 사용자 "코드베이스로 알 수 있는 문서들은 다 제거해" · "하네스에도 문서 정리하는 거 만들어서")
# 지우지 않는 곳: 설계서 · 결정 · 조사 · 대조표 · 토스 요약 · 키 목록 · 규칙 파일 · 파이프라인 설명.
KEEP_DOCS = ("docs/product/", "docs/dev/decisions.md", "docs/dev/conformance.md", "docs/dev/toss-api-notes.md",
             "docs/dev/keys-needed.md", "docs/dev/pipeline.md", "docs/dev/modules/", "docs/dev/prompts/")
TIDY_OK = ("todo", "reviewed")  # 차수 사이(검토 통과 뒤 · 아직 연 적 없음)에만 지시서를 줄인다


def _commit_index():
    """[(짧은 해시, 제목)] 옛것부터, 그리고 해시 → 순번."""
    _, out = run(["git", "log", "--reverse", "--format=%h%x09%s"])
    rows = [l.split("\t", 1) for l in out.splitlines() if "\t" in l]
    return rows, {h: i for i, (h, _) in enumerate(rows)}


def _heading_added(path):
    """지시서의 '## ' 머리 줄 → 그 줄을 처음 더한 커밋(짧은 해시)."""
    _, out = run(["git", "log", "--reverse", "--format=@@C %h", "-U0", "--", path])
    added, cur = {}, None
    for line in out.splitlines():
        if line.startswith("@@C "):
            cur = line[4:].strip()
        elif line.startswith("+## ") and cur:
            added.setdefault(line[1:], cur)
    return added


def _summary(body):
    for line in body:
        t = line.strip()
        if not t or t.startswith(("#", "|", "```", ">", "<")):
            continue
        t = re.sub(r"^([-*]|\d+[.)])\s+", "", t).replace("**", "").replace("`", "")
        return t if len(t) <= 90 else t[:89] + "…"
    return "(본문 없음)"


def _fixed_parts(text):
    """tidy 가 바꾸면 안 되는 것: 차수 절이 아닌 절 전부(원본 동작 근거 · 필수 번호 · 만드는 것 …)와 머리 줄 순서."""
    lines = text.split("\n")
    secs = round_sections(text)
    pre = "\n".join(lines[:secs[0][1]]) if secs else text
    return pre, [(h, "\n".join(lines[i:e]) if not r else "") for h, i, e, r in secs]


def tidy_text(text, m, path, commits, index, added, head_hash):
    """옛 차수 절(마지막 차수 절 말고)의 본문을 '- 끝남: …' 한 줄로. (새 글, 줄인 절 수)."""
    lines = text.split("\n")
    secs = round_sections(text)
    rounds = [s for s in secs if s[3]]
    if len(rounds) < 2:
        return text, 0
    last = rounds[-1]
    order = sorted(index[added[h]] for h, *_ in rounds if added.get(h) in index)
    out, pos, n = [], 0, 0
    for h, i, e, is_round in secs:
        if not is_round or (h, i) == (last[0], last[1]):
            continue
        body = lines[i + 1:e]
        nonblank = [l for l in body if l.strip()]
        if nonblank and nonblank[0].startswith(STUB):
            continue
        red = green = "—"
        at = index.get(added.get(h, ""), None)
        if at is not None:
            nxt = min([o for o in order if o > at] + [len(commits)])
            for k in range(at, nxt):
                hh, subj = commits[k]
                if red == "—" and subj.startswith(f"test({m})"):
                    red = hh
                if green == "—" and subj.startswith(f"feat({m})"):
                    green = hh
        stub = f"{STUB} {_summary(body)} — 커밋 {red}(red) · {green}(green) · 전문: `git show {head_hash}:{path}`"
        out += lines[pos:i + 1] + [stub, ""]
        pos = e
        n += 1
    out += lines[pos:]
    new = "\n".join(out)
    if _fixed_parts(new) != _fixed_parts(text):
        raise RuntimeError(f"tidy 가 차수 절 밖을 바꾸려 했다: {path} — 멈춘다")
    return new, n


def tidy_briefs(st, mods, dry, commit):
    """모듈 지시서를 줄인다. 바뀐 경로 목록."""
    commits, index = _commit_index()
    head_hash = run(["git", "rev-parse", "--short", "HEAD"])[1].strip()
    changed = []
    for m in mods:
        d = mod(st, m)
        path = brief_path(m)
        if not (ROOT / path).is_file():
            continue
        if d["stage"] not in TIDY_OK or d.get("reconcile"):
            print(f"건너뜀: tidy {m} — 까닭(차수 진행 중: {LABEL[d['stage']]}{' · 대조 먼저' if d.get('reconcile') else ''})")
            continue
        text = (ROOT / path).read_text(encoding="utf-8")
        new, n = tidy_text(text, m, path, commits, index, _heading_added(path), head_hash)
        if n == 0:
            continue
        before, after = text.count("\n"), new.count("\n")
        print(f"tidy {m}: 옛 차수 절 {n}개를 한 줄로 — {before} → {after}줄")
        if not dry:
            (ROOT / path).write_text(new, encoding="utf-8")
            changed.append(path)
            if commit:
                run(["git", "add", "--", path])
                run(["git", "commit", "-q", "-m", f"docs({m}): 지시서 정리\n\n옛 차수 절 {n}개를 한 줄로 줄였다(전문은 git 기록)."
                     + COAUTHOR, "--", path])
                print(f"커밋했다: docs({m}): 지시서 정리")
    return changed


def tidy_backlog(dry):
    """docs/dev/backlog.md: 끝난 줄(- [x])과 '진행 기록' 절을 지우고(git 이 기록), 남은 - [ ] 이 없는 절도 지운다."""
    p = ROOT / "docs/dev/backlog.md"
    if not p.is_file():
        return []
    text = p.read_text(encoding="utf-8")
    lines, out, skip_indent, log_sec, keep_indent = text.split("\n"), [], None, False, None
    for line in lines:
        if line.startswith("## "):
            log_sec = line.startswith("## 진행 기록")
            skip_indent = keep_indent = None
            out.append(line)
            continue
        ind = len(line) - len(line.lstrip())
        if skip_indent is not None:
            if line.strip() and ind > skip_indent:
                continue
            skip_indent = None
        if re.match(r"^\s*- \[[xX]\]", line):
            skip_indent = ind
            continue
        if log_sec:
            # 진행 기록 절: 기록 글은 지우고(git 이 기록), 그 안의 남은 일(- [ ])과 그 밑줄만 둔다.
            if re.match(r"^\s*- \[ \]", line):
                keep_indent = ind
                out.append(line)
            elif keep_indent is not None and line.strip() and ind > keep_indent:
                out.append(line)
            else:
                keep_indent = None
            continue
        out.append(line)
    # 남은 - [ ] 이 없는 절은 지운다(머리말 앞부분은 둔다)
    text2 = "\n".join(out)
    secs = round_sections(text2)
    lines2 = text2.split("\n")
    keep = lines2[:secs[0][1]] if secs else lines2
    for h, i, e, _ in secs:
        part = lines2[i:e]
        if any(re.match(r"^\s*- \[ \]", l) for l in part):
            keep += part
    new = re.sub(r"\n{3,}", "\n\n", "\n".join(keep)).rstrip("\n") + "\n"
    if new == text:
        return []
    print(f"tidy backlog: {text.count(chr(10))} → {new.count(chr(10))}줄 (끝난 줄 · 진행 기록 지움)")
    if not dry:
        p.write_text(new, encoding="utf-8")
    return ["docs/dev/backlog.md"]


def tidy_empty_docs(dry):
    """이름만 남은 · 빈 문서(머리 줄 말고 글이 없는 .md)를 지운다. 지키는 곳(KEEP_DOCS)과 스크립트가 읽는 것은 두다."""
    gone = []
    scripts_text = "\n".join(f.read_text(encoding="utf-8", errors="replace") for f in (ROOT / "scripts").glob("*")
                             if f.is_file())
    for f in sorted((ROOT / "docs").rglob("*.md")):
        rel = f.relative_to(ROOT).as_posix()
        if rel.startswith(KEEP_DOCS):
            continue
        body = [l for l in f.read_text(encoding="utf-8", errors="replace").splitlines() if l.strip() and not l.startswith("#")]
        if body or rel in scripts_text:
            continue
        print(f"tidy 빈 문서 지움: {rel}")
        if not dry:
            f.unlink()
        gone.append(rel)
    return gone


def cmd_tidy(st, a):
    """pipeline.py tidy [--all] [--dry-run] [<모듈…>]: 끝난 차수의 지시서 절을 한 줄로 줄인다(검토 통과 뒤 · 차수 사이만).
    --all: 모든 모듈 지시서 + backlog(끝난 줄 · 진행 기록) + 빈 문서. 바뀐 것은 한 커밋으로(--dry-run 이면 보이기만)."""
    dry = "--dry-run" in a
    mods = [x for x in a if not x.startswith("--")]
    if "--all" in a:
        mods = list(GRAPH["modules"])
    if not mods:
        sys.exit("쓰는 법: pipeline.py tidy [--all] [--dry-run] [<모듈…>]")
    changed = tidy_briefs(st, mods, dry, commit=False)
    if "--all" in a:
        changed += tidy_backlog(dry) + tidy_empty_docs(dry)
    if dry or not changed:
        print("바뀐 문서 없음" if not changed else "(--dry-run: 쓰지 않았다)")
        return
    run(["git", "add", "-A", "--", *changed])
    c, o = run(["git", "commit", "-q", "-m", "docs: 문서 정리 (pipeline.py tidy)\n\n끝난 차수 절을 한 줄로 줄이고 끝난 기록을 지웠다(전문은 git 기록)."
                + COAUTHOR, "--", *changed])
    print("커밋했다: docs: 문서 정리" if c == 0 else "커밋 실패:\n" + o)


def cmd_flaky(st, a):
    """흔들린 시험(혼자 다시 돌려 통과한 것) 횟수 — 고칠 후보."""
    p = ROOT / ".pipeline/flaky.json"
    try:
        d = json.loads(p.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        print("흔들린 시험 기록 없음")
        return
    for k, v in sorted(d.items(), key=lambda kv: -kv[1]["count"]):
        print(f"{v['count']:3}번  {k}  (마지막 {v.get('last', '?')})")


def cmd_cache(st, a):
    """pipeline.py cache clear: 검사 캐시(같은 입력 지문으로 통과한 기록)를 지운다."""
    if a[:1] != ["clear"]:
        sys.exit("쓰는 법: pipeline.py cache clear")
    import shutil
    shutil.rmtree(ROOT / ".pipeline/cache", ignore_errors=True)
    print("검사 캐시를 지웠다 (.pipeline/cache)")


def main():
    cmds = {k[4:].replace("_", "-"): v for k, v in globals().items() if k.startswith("cmd_")}
    if len(sys.argv) < 2 or sys.argv[1] not in cmds:
        sys.exit(__doc__)
    # 병렬 에이전트가 동시에 불러도 상태가 어긋나지 않게 한 번에 하나만 돈다.
    import fcntl
    STATE_FILE.parent.mkdir(exist_ok=True)
    guard = open(STATE_FILE.parent / "lock", "w")
    fcntl.flock(guard, fcntl.LOCK_EX)
    st = load()
    for m in GRAPH["modules"]:
        mod(st, m)
    try:
        cmds[sys.argv[1]](st, sys.argv[2:])
    except (IndexError, KeyError):
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
