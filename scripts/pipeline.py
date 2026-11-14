#!/usr/bin/env python3
"""그래프 실행기. docs/dev/graph.toml 을 읽어 순서 · 권한 · 반복 한도를 지키게 한다.

    pipeline.py status                 모듈마다 지금 어느 단계인가
    pipeline.py next                   지금 시작할 수 있는 노드 (여러 개면 병렬)
    pipeline.py prompt <노드> <모듈>   그 노드에 줄 프롬프트 (고정 틀 + 지시서)
    pipeline.py start <노드> <모듈>    노드 시작 (앞 단계가 안 끝났으면 거부)
    pipeline.py gate <G1|G2> <모듈>    기계 검사를 돌리고 결과를 기록 (같은 실패 3번이면 사람에게)
    pipeline.py lock <모듈>            G1 통과 뒤 테스트를 커밋하고 red/<모듈> 태그로 잠근다
    pipeline.py review <모듈> <통과|되돌림> [메모]   AI 검토 판정 기록
    pipeline.py scope                  진행 중인 노드가 맡지 않은 곳을 고쳤는지
    pipeline.py resume <모듈>          사람에게 넘어간 모듈을 사람이 다시 연다

상태는 .pipeline/state.json 에 둔다.
"""
import fnmatch
import hashlib
import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GRAPH = tomllib.loads((ROOT / "docs/dev/graph.toml").read_text(encoding="utf-8"))
STATE_FILE = ROOT / ".pipeline/state.json"
DIRS = {"gate": ["crates/gate", "crates/server"]}
# 단계 순서: 할 일 → N1 중 → red 확인됨 → 잠김 → N2 중 → green → N3 중 → 검토 통과
ORDER = ["todo", "n1", "red_ok", "locked", "n2", "green", "n3", "reviewed"]
LABEL = {
    "todo": "할 일", "n1": "N1 테스트 작성 중", "red_ok": "red 확인됨 (잠그기 전)", "locked": "테스트 잠김",
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


def run(cmd):
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace")
    return p.returncode, p.stdout + p.stderr


def runnable(st):
    out = []
    for m, spec in GRAPH["modules"].items():
        s = mod(st, m)["stage"]
        if s == "todo" and all(at_least(st, n, "green") for n in spec["needs"]):
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
    d = mod(st, m)
    want = {"N1": "todo", "N2": "locked", "N3": "green"}[node]
    if d["stage"] == "escalated":
        sys.exit(f"아직 안 된다: {m} 은 사람에게 넘어갔다. 사람이 pipeline.py resume {m} 을 해야 한다")
    if node == "N1":
        late = [n for n in GRAPH["modules"][m]["needs"] if not at_least(st, n, "green")]
        if late:
            sys.exit(f"아직 안 된다: {m} 은 {', '.join(late)} 이(가) green 이 된 뒤에 시작한다")
    if d["stage"] != want:
        sys.exit(f"아직 안 된다: {node} {m} 은 '{LABEL[want]}' 단계에서 시작한다 (지금: {LABEL[d['stage']]})")
    d["stage"] = node.lower()
    d["fails"], d["last_fail"] = 0, ""
    d["log"].append(f"{node} 시작")
    save(st)
    print(f"{node} {m} 시작. 고칠 수 있는 곳: {', '.join(allowed(node, m))}")


def allowed(node, m):
    return [w.replace("{dir}", x) for w in GRAPH["nodes"][node]["writes"] for x in (dirs(m) if "{dir}" in w else [""])]


def changed_files():
    _, out = run(["git", "status", "--porcelain", "--untracked-files=all"])
    return [l[3:].split(" -> ")[-1].strip('"') for l in out.splitlines() if l.strip()]


def scope_problems(st):
    active = [(d["stage"].upper(), m) for m, d in st.items() if d["stage"] in ("n1", "n2")]
    globs = [g for n, m in active for g in allowed(n, m)]
    return [f for f in changed_files() if not any(fnmatch.fnmatch(f, g) for g in globs)]


def cmd_scope(st, _):
    bad = scope_problems(st)
    if bad:
        print("✗ 맡지 않은 곳을 고쳤다 — 고치기: 그 변경을 되돌린다 (진행 중인 노드는 자기 모듈 폴더만 고친다)")
        for f in bad:
            print("   " + f)
        sys.exit(1)
    print("✓ 고친 곳이 모두 맡은 범위 안이다")


def cmd_gate(st, a):
    gate, m = a[0], a[1]
    d = mod(st, m)
    need = {"G1": "n1", "G2": "n2"}[gate]
    if d["stage"] != need:
        sys.exit(f"아직 안 된다: {gate} {m} 은 '{LABEL[need]}' 단계에서 돌린다 (지금: {LABEL[d['stage']]})")
    code, out = run(["bash", "scripts/red.sh", m] if gate == "G1" else ["bash", "scripts/verify.sh", m])
    bad = scope_problems(st)
    if bad:
        code = 1
        out += "\n✗ 맡지 않은 곳을 고쳤다: " + ", ".join(bad) + " — 고치기: 그 변경을 되돌린다\n"
    print(out)
    if code == 0:
        d["stage"] = "red_ok" if gate == "G1" else "green"
        d["fails"], d["last_fail"] = 0, ""
        d["log"].append(f"{gate} 통과")
        save(st)
        return
    sig = hashlib.sha256("\n".join(sorted(l.split(" — ")[0] for l in out.splitlines() if l.startswith("✗"))).encode()).hexdigest()
    d["fails"] = d["fails"] + 1 if sig == d["last_fail"] else 1
    d["last_fail"] = sig
    d["log"].append(f"{gate} 실패 ({d['fails']}번째 같은 실패)")
    if d["fails"] >= GRAPH["max_same_failure"]:
        d["stage"] = "escalated"
        print(f"■ 같은 실패가 {d['fails']}번 되풀이됐다. 멈추고 사람에게 넘긴다.")
    save(st)
    sys.exit(1)


def cmd_lock(st, a):
    m = a[0]
    d = mod(st, m)
    if d["stage"] != "red_ok":
        sys.exit(f"아직 안 된다: 잠그기는 G1 을 통과한 뒤에 한다 (지금: {LABEL[d['stage']]})")
    run(["git", "add", "--", *dirs(m), "Cargo.lock"])
    code, out = run(["git", "commit", "-q", "-m", f"test({m}): 테스트와 뼈대 (red)\n\nG1 통과. 이 뒤로 tests/ 는 잠긴다.", "--", *dirs(m), "Cargo.lock"])
    if code != 0:
        sys.exit("커밋 실패:\n" + out)
    run(["git", "tag", "-f", f"red/{m}"])
    d["stage"] = "locked"
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


def cmd_resume(st, a):
    d = mod(st, a[0])
    if d["stage"] != "escalated":
        sys.exit("사람에게 넘어간 모듈이 아니다")
    back = "locked" if (ROOT / ".git/refs/tags/red" / a[0]).exists() else "todo"
    d.update(stage=back, fails=0, last_fail="", rounds=0)
    d["log"].append("사람이 다시 엶")
    save(st)
    print(f"{a[0]}: {LABEL[back]}")


def main():
    cmds = {k[4:]: v for k, v in globals().items() if k.startswith("cmd_")}
    if len(sys.argv) < 2 or sys.argv[1] not in cmds:
        sys.exit(__doc__)
    st = load()
    for m in GRAPH["modules"]:
        mod(st, m)
    try:
        cmds[sys.argv[1]](st, sys.argv[2:])
    except (IndexError, KeyError):
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
