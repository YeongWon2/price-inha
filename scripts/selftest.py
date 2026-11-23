#!/usr/bin/env python3
"""검사기 자체 시험.

깨끗한 가짜 저장소를 만들어 (1) 깨끗하면 통과하는지, (2) 규칙을 하나씩 어기면 그 규칙 이름으로
잡는지를 확인한다. 모두 맞으면 .pipeline/selftest.ok 에 지금 규칙의 지문을 적는다.
verify.sh 는 그 지문이 지금 규칙과 같을 때만 통과한다(규칙이나 검사기를 고치면 다시 돌려야 한다).

    python3.14 scripts/selftest.py
"""
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RULES = tomllib.loads((ROOT / "docs/dev/architecture.toml").read_text(encoding="utf-8"))
# 규칙 파일에 있는 크레이트의 모듈(폴더 이름). crates/server 는 server 모듈이다.
MODULES = [Path(rule["dir"]).name for rule in RULES["crates"].values()]
# 테스트에서 부르는 크레이트 이름. server(price-inha)는 실행 파일뿐이라 부를 라이브러리가 없다.
TEST_CRATE = {m: f"pi_{m}" for m in MODULES}
TEST_CRATE["server"] = None

# 금지 모양마다 "어긴 코드 한 줄"과 넣을 파일. architecture.toml 에 모양을 더하면 여기에도 더해야 한다.
PATTERN_SAMPLES = {
    "ARCH-MONEY-FLOAT": ("crates/market/src/lib.rs", "/// 값.\npub const PRICE: f64 = 1.0;"),
    "ARCH-ORDER-API": ("crates/market/src/lib.rs", '/// 값.\npub const P: &str = "/api/v1/orders";'),
    "ARCH-SECRET-FILES": ("crates/ai/src/lib.rs", '/// 값.\npub const F: &str = ".env";'),
    "ARCH-ENV-READ": ("crates/gate/src/lib.rs", '/// 값.\npub fn e() -> bool { std::env::var("X").is_ok() }'),
    "ARCH-CLOCK": ("crates/store/src/lib.rs", "/// 값.\npub fn t() -> std::time::SystemTime { std::time::SystemTime::now() }"),
    "ARCH-PAPER-PURE": ("crates/paper/src/lib.rs", "/// 값.\npub fn p() { let _ = std::fs::read(\"x\"); }"),
    "ARCH-NET-IN-STORE": ("crates/store/src/lib.rs", "/// 값.\npub fn n() { let _ = std::net::TcpStream::connect(\"127.0.0.1:1\"); }"),
    "CONV-BOX-ERROR": ("crates/ai/src/lib.rs", "/// 값.\npub fn b() -> Result<(), Box<dyn std::error::Error>> { Ok(()) }"),
    "CONV-SECRET-LOG": ("crates/ai/src/lib.rs", "/// 값.\npub fn l(api_key: &str) { tracing::info!(\"{}\", api_key); }"),
    "ARCH-RULES-PURE": ("crates/template/src/lib.rs", "/// 값.\npub fn p() { let _ = std::fs::read(\"x\"); }"),
    "ARCH-CLOCK-2": ("crates/trigger/src/lib.rs", "/// 값.\npub fn t() -> std::time::Instant { std::time::Instant::now() }"),
    "ARCH-NET-IN-CORE": ("crates/trigger/src/lib.rs", "/// 값.\npub fn n() { let _ = std::net::TcpStream::connect(\"127.0.0.1:1\"); }"),
    "ARCH-REAL-BROKER": ("crates/rules/src/lib.rs", "/// 값.\npub struct R;\nimpl pi_paper::Broker for R {}"),
    "CONV-BIND-ALL": ("crates/gate/src/lib.rs", '/// 값.\npub const B: &str = "0.0.0.0:8080";'),
}

results = []


def sh(cmd, cwd, env=None):
    e = dict(os.environ, PI_SELFTEST="1", CARGO_TERM_COLOR="never")
    e.update(env or {})
    p = subprocess.run(cmd, cwd=cwd, env=e, capture_output=True, text=True, encoding="utf-8", errors="replace")
    return p.returncode, p.stdout + p.stderr


def required_ids(module):
    text = (ROOT / f"docs/dev/modules/{module}.md").read_text(encoding="utf-8")
    part = text.split("## 필수 번호", 1)[1].split("\n## ", 1)[0]
    return part.split()


def build_fixture(fx):
    for f in ["Cargo.toml", "clippy.toml", ".gitignore"]:
        shutil.copy(ROOT / f, fx / f)
    shutil.copytree(ROOT / "scripts", fx / "scripts")
    (fx / "docs").mkdir()
    shutil.copytree(ROOT / "docs/dev", fx / "docs/dev")
    for name, rule in RULES["crates"].items():
        d = fx / rule["dir"]
        (d / "src").mkdir(parents=True)
        (d / "Cargo.toml").write_text(
            f'[package]\nname = "{name}"\nedition.workspace = true\nversion.workspace = true\n'
            "license.workspace = true\npublish.workspace = true\n\n[lints]\nworkspace = true\n\n"
            "[dependencies]\n\n[dev-dependencies]\n"
        )
        if name == "price-inha":
            (d / "src/main.rs").write_text("//! 가짜 실행 파일.\nfn main() {}\n")
            continue
        (d / "src/lib.rs").write_text(
            "//! 가짜 모듈.\npub mod error;\n\n/// 더한다.\npub fn add(a: i64, b: i64) -> i64 {\n    a + b\n}\n"
        )
        (d / "src/error.rs").write_text("//! 오류.\n\n/// 오류 종류.\n#[derive(Debug)]\npub enum Error {\n    /// 예.\n    Example,\n}\n")
    for m in MODULES:
        t = fx / f"crates/{m}/tests"
        t.mkdir()
        crate = TEST_CRATE[m]
        call = f"{crate}::add(1, 2)" if crate else "1 + 2"
        (t / "t.rs").write_text(
            f"// 검증: {', '.join(required_ids(m))}\n#[test]\nfn adds() {{\n    assert_eq!({call}, 3);\n}}\n"
        )
    sh(["git", "init", "-q", "-b", "main"], fx)
    sh(["git", "config", "user.email", "selftest@example.invalid"], fx)
    sh(["git", "config", "user.name", "selftest"], fx)
    sh(["cargo", "generate-lockfile"], fx)
    sh(["cargo", "fmt", "--all"], fx)
    sh(["git", "add", "-A"], fx)
    sh(["git", "commit", "-q", "-m", "base"], fx)
    sh(["git", "tag", "base"], fx)
    for m in MODULES:
        sh(["git", "tag", f"red/{m}"], fx)


def reset(fx):
    sh(["git", "checkout", "-q", "--", "."], fx)
    sh(["git", "clean", "-fdq", "-e", "target"], fx)
    # 앞 경우가 커밋하거나 태그를 옮겼어도 처음 상태로 되돌린다.
    sh(["git", "reset", "-q", "--hard", "base"], fx)
    for m in MODULES:
        sh(["git", "tag", "-f", f"red/{m}", "base"], fx)
    shutil.rmtree(fx / ".pipeline", ignore_errors=True)
    shutil.rmtree(fx.parent / "vendor", ignore_errors=True)


def append(fx, rel, text):
    p = fx / rel
    p.write_text(p.read_text(encoding="utf-8") + "\n" + text + "\n", encoding="utf-8")


def case(fx, name, mutate, cmd, expect, should_fail=True):
    reset(fx)
    if mutate:
        mutate()
    code, out = sh(cmd, fx)
    good = (code != 0) == should_fail and all(e in out for e in ([expect] if isinstance(expect, str) else expect))
    results.append((good, name))
    print(("✓ " if good else "✗ ") + name)
    if not good:
        print(f"    기대: {'실패' if should_fail else '통과'} + {expect!r} / 실제 종료 코드 {code}")
        print("    " + "\n    ".join(out.strip().splitlines()[-12:]))


def vendor(fx, name):
    d = fx.parent / "vendor" / name  # 저장소 밖: 워크스페이스 구성원이 아닌 바깥 의존
    (d / "src").mkdir(parents=True)
    (d / "Cargo.toml").write_text(f'[package]\nname = "{name}"\nversion = "0.0.0"\nedition = "2021"\n')
    (d / "src/lib.rs").write_text("")


def add_dep(fx, crate_dir, line):
    p = fx / crate_dir / "Cargo.toml"
    p.write_text(p.read_text().replace("[dependencies]\n", "[dependencies]\n" + line + "\n"))


def main():
    py = sys.executable
    arch = [py, "scripts/check_arch.py"]
    verify = ["bash", "scripts/verify.sh"]
    tmp = Path(tempfile.mkdtemp(prefix="pi-selftest-"))
    fx = tmp / "repo"
    fx.mkdir()
    try:
        build_fixture(fx)

        # --- 깨끗하면 통과한다
        case(fx, "깨끗한 저장소: 아키텍처 검사 통과", None, arch, "", should_fail=False)
        case(fx, "깨끗한 저장소: 전체 검사(G2) 통과", None, verify, "G2 통과", should_fail=False)
        case(fx, "깨끗한 저장소: 모듈 하나만 검사해도 통과", None, verify + ["store"], "G2 통과", should_fail=False)
        case(fx, "모듈 하나만 검사할 때 다른 모듈의 문제는 넘어간다",
             lambda: (fx / "crates/ai/src/error.rs").unlink(), verify + ["store"], "G2 통과", should_fail=False)
        case(fx, "모듈 하나만 검사할 때 그 모듈의 문제는 잡는다",
             lambda: (fx / "crates/store/src/error.rs").unlink(), verify + ["store"], "[CONV-ERROR-FILE]")

        # --- 아키텍처: 의존
        case(fx, "의존 방향을 어기면 잡는다 (ai → store)",
             lambda: add_dep(fx, "crates/ai", 'pi-store = { path = "../store" }'), arch, "[ARCH-DEP-DIR]")

        def ext():
            vendor(fx, "leftpad")
            add_dep(fx, "crates/store", 'leftpad = { path = "../../../vendor/leftpad" }')
        case(fx, "허용 목록에 없는 바깥 의존을 잡는다", ext, arch, "[ARCH-DEP-EXT]")

        def ban():
            vendor(fx, "openssl-sys")
            add_dep(fx, "crates/server", 'openssl-sys = { path = "../../../vendor/openssl-sys" }')
        case(fx, "금지된 의존을 잡는다 (openssl-sys)", ban, arch, "[ARCH-DEP-BAN]")

        def pinned():
            vendor(fx, "thiserror")
            add_dep(fx, "crates/store", 'thiserror = { path = "../../../vendor/thiserror", version = "0.0.0" }')
        case(fx, "크레이트에 판을 직접 적으면 잡는다 (workspace.dependencies)", pinned, arch,
             "[ARCH-DEP-WORKSPACE] crates/store/Cargo.toml:")

        def extra():
            shutil.copytree(fx / "crates/store", fx / "crates/extra")
            p = fx / "crates/extra/Cargo.toml"
            p.write_text(p.read_text().replace("pi-store", "pi-extra"))
        case(fx, "표에 없는 크레이트를 잡는다", extra, arch, "[ARCH-CRATE]")

        # --- 아키텍처 · 컨벤션: 금지 모양 (규칙 파일의 모든 모양에 시험이 있어야 한다)
        ids = [p["id"] for p in RULES.get("patterns", [])]
        for pid in ids:
            if pid not in PATTERN_SAMPLES:
                results.append((False, f"{pid}: 자체 시험 표본 없음"))
                print(f"✗ {pid}: scripts/selftest.py 의 PATTERN_SAMPLES 에 어긴 코드 표본을 더해야 한다")
                continue
            rel, line = PATTERN_SAMPLES[pid]
            case(fx, f"금지 모양을 잡는다: {pid}", lambda rel=rel, line=line: append(fx, rel, line), arch, f"[{pid}]")
        case(fx, "주석 안의 글자는 잡지 않는다",
             lambda: append(fx, "crates/market/src/lib.rs", "// f64 를 쓰지 않는다"), arch, "", should_fail=False)
        case(fx, "허용된 곳은 잡지 않는다 (secrets 의 키 파일 이름)",
             lambda: append(fx, "crates/secrets/src/lib.rs", '/// 값.\npub const F: &str = ".env";'), arch, "", should_fail=False)

        # --- 구조 · 크기
        case(fx, "라이브러리에 main.rs 가 있으면 잡는다",
             lambda: (fx / "crates/store/src/main.rs").write_text("fn main() {}\n"), arch, "[ARCH-ONE-BIN]")
        case(fx, "error.rs 가 없으면 잡는다",
             lambda: (fx / "crates/ai/src/error.rs").unlink(), arch, "[CONV-ERROR-FILE]")
        big = RULES["limits"]["max_src_lines"] + 5
        case(fx, "너무 긴 파일을 잡는다",
             lambda: append(fx, "crates/store/src/lib.rs", "\n".join(f"/// 값.\npub const C{i}: i64 = {i};" for i in range(big))),
             arch, "[CONV-FILE-SIZE]")
        long_fn = RULES["limits"]["max_fn_lines"] + 5
        case(fx, "너무 긴 함수를 잡는다",
             lambda: append(fx, "crates/store/src/lib.rs",
                            "/// 길다.\npub fn long() -> i64 {\n    let mut x = 0;\n" + "    x += 1;\n" * long_fn + "    x\n}"),
             arch, "[CONV-FN-SIZE]")

        # --- 전체 검사(G2)의 나머지 단계
        case(fx, "모양이 다르면 잡는다 (fmt)",
             lambda: append(fx, "crates/store/src/lib.rs", "/// 값.\npub fn   ugly( )->i64{1}"), verify, "✗ 1 모양")
        case(fx, "unwrap 을 잡는다 (clippy)",
             lambda: append(fx, "crates/store/src/lib.rs", '/// 값.\npub fn u() -> i64 {\n    "1".parse::<i64>().unwrap()\n}'),
             verify, "✗ 2a clippy")
        case(fx, "todo! 를 잡는다 (clippy)",
             lambda: append(fx, "crates/store/src/lib.rs", "/// 값.\npub fn td() -> i64 {\n    todo!()\n}"), verify, "✗ 2a clippy")
        case(fx, "문서 주석 없는 공개 항목을 잡는다",
             lambda: append(fx, "crates/store/src/lib.rs", "pub fn nodoc() -> i64 {\n    1\n}"), verify, "✗ 2a clippy")
        case(fx, "println! 을 잡는다",
             lambda: append(fx, "crates/store/src/lib.rs", '/// 값.\npub fn pr() {\n    println!("x");\n}'), verify, "✗ 2a clippy")
        case(fx, "깨진 테스트를 잡는다",
             lambda: (fx / "crates/store/src/lib.rs").write_text(
                 (fx / "crates/store/src/lib.rs").read_text().replace("a + b", "a - b")), verify, "✗ 3 테스트")
        case(fx, "잠긴 테스트를 고치면 잡는다",
             lambda: append(fx, "crates/store/tests/t.rs", "// 고침"), verify, "✗ 4 잠긴 테스트가 바뀜: store")
        case(fx, "잠긴 뒤 새 테스트 파일을 잡는다",
             lambda: (fx / "crates/store/tests/new.rs").write_text("#[test]\nfn n() {}\n"), verify, "✗ 4 잠긴 뒤 새 테스트 파일: store")

        def drop_ids():
            p = fx / "crates/secrets/tests/t.rs"
            p.write_text("\n".join(p.read_text().splitlines()[1:]) + "\n")
            sh(["git", "commit", "-qam", "x"], fx)
            sh(["git", "tag", "-f", "red/secrets"], fx)
        case(fx, "필수 번호가 빠지면 잡는다", drop_ids, verify, "✗ 5 필수 번호 빠짐 (secrets)")

        def drop_origin():
            p = fx / "docs/dev/modules/ai.md"
            t = p.read_text()
            a = t.index("## 원본 동작 근거")
            b = t.index("\n## ", a + 5)
            p.write_text(t[:a] + t[b + 1:])
        case(fx, "지시서에 원본 동작 근거가 없으면 잡는다", drop_origin, verify, "✗ 5b 원본 동작 근거 없음 (ai)")

        fake_key = "sk-" + "ant-" + "A" * 30
        case(fx, "키처럼 생긴 글자를 잡는다",
             lambda: (fx / "docs/note.md").write_text(f"key = {fake_key}\n"), verify, "✗ 7 비밀 글자")

        def envfile():
            (fx / "crates/store/.env").write_text("X=1\n")
            sh(["git", "add", "-f", "crates/store/.env"], fx)
        case(fx, "저장소에 들어온 .env 를 잡는다", envfile, verify, "✗ 7 비밀 글자")
        host = "https://api." + "anthropic" + ".com"
        case(fx, "테스트의 바깥 주소를 잡는다",
             lambda: append(fx, "crates/ai/tests/t.rs", f'const H: &str = "{host}";'), verify, "✗ 8 테스트에 바깥 주소: ai")
        case(fx, "#[allow] 로 검사를 끄면 잡는다",
             lambda: append(fx, "crates/store/src/lib.rs", "#[allow(dead_code)]\nfn hidden() {}"), verify, "✗ 9 검사를 끈 곳")
        case(fx, "#[ignore] 로 테스트를 끄면 잡는다",
             lambda: append(fx, "crates/store/src/lib.rs", "#[cfg(test)]\nmod t {\n    #[test]\n    #[ignore]\n    fn x() {}\n}"),
             verify, "✗ 9 검사를 끈 곳")
        case(fx, "red 태그 없이 모듈 검사를 하면 잡는다",
             lambda: sh(["git", "tag", "-d", "red/ai"], fx), verify + ["ai"], "✗ 4 red 태그 없음: ai")
        sh(["git", "tag", "-f", "red/ai"], fx)

        # --- 영향 범위 (QMIT 결정 6, scripts/affected.py)
        aff = f"{py} scripts/affected.py --base HEAD"

        def dep_then_touch():
            add_dep(fx, "crates/ai", 'pi-secrets = { path = "../secrets" }')
            sh(["git", "commit", "-qam", "ai 가 secrets 에 기댐"], fx)
            append(fx, "crates/secrets/src/lib.rs", "// 고침")
        case(fx, "영향 범위: 크레이트 하나를 고치면 그 크레이트와 기대는 크레이트", dep_then_touch,
             ["bash", "-c", f'test "$({aff})" = "secrets ai" && echo 맞음'], "맞음", should_fail=False)
        case(fx, "영향 범위: scripts 를 고치면 전체",
             lambda: append(fx, "scripts/lib.sh", "# 고침"),
             ["bash", "-c", f'test "$({aff})" = "ALL" && echo 맞음'], "맞음", should_fail=False)
        case(fx, "영향 범위: 문서만 고치면 검사할 모듈이 없다",
             lambda: append(fx, "docs/dev/roadmap.md", "고침"),
             verify + ["--affected", "--base", "HEAD"], "바뀐 코드 없음", should_fail=False)

        # --- G1 (red 확인)
        red = ["bash", "scripts/red.sh"]
        case(fx, "G1: 테스트가 이미 통과하면 red 가 아니라고 한다", None, red + ["store"], "이미 통과함")
        todo_lib = lambda: (fx / "crates/store/src/lib.rs").write_text(
            "//! 가짜 모듈.\npub mod error;\n\n/// 더한다.\npub fn add(_a: i64, _b: i64) -> i64 {\n    todo!()\n}\n")
        case(fx, "G1: 뼈대만 있고 테스트가 실패하면 통과", todo_lib, red + ["store"], "G1 통과", should_fail=False)

        def red_noids():
            todo_lib()
            (fx / "crates/store/tests/t.rs").write_text("#[test]\nfn adds() {\n    assert_eq!(pi_store::add(1, 2), 3);\n}\n")
        case(fx, "G1: 필수 번호가 빠지면 잡는다", red_noids, red + ["store"], "필수 번호 빠짐")
        def red_upper():
            todo_lib()
            append(fx, "crates/store/tests/t.rs", "#[test]\nfn uses_IP_name() {\n    assert_eq!(pi_store::add(1, 2), 3);\n}")
        case(fx, "G1: 테스트 이름이 clippy 에 걸리면 잡는다 (잠그기 전에)", red_upper, red + ["store"], "✗ 테스트 clippy")
        case(fx, "G1: 컴파일이 안 되면 잡는다",
             lambda: append(fx, "crates/store/tests/t.rs", "fn broken( {"), red + ["store"], "컴파일 안 됨")

        # --- 그래프 실행기
        if (ROOT / "scripts/pipeline.py").exists():
            pl = [py, "scripts/pipeline.py"]
            case(fx, "그래프: 처음에는 gate 를 시작할 수 없다 (앞 모듈이 안 끝남)", None, pl + ["start", "N1", "gate"], "아직")
            case(fx, "그래프: red 확인 없이 구현을 시작할 수 없다", None, pl + ["start", "N2", "store"], "아직")
            case(fx, "그래프: 처음에 시작할 수 있는 것은 N1 네 개", None, pl + ["next"],
                 ["N1 store", "N1 secrets", "N1 ai", "N1 market"], should_fail=False)

            def three_fails():
                for _ in range(3):
                    sh(pl + ["start", "N1", "store"], fx)
                    sh(pl + ["gate", "G1", "store"], fx)
            case(fx, "반복: 같은 실패 3번이면 사람에게 넘긴다", three_fails, pl + ["status"], "사람에게", should_fail=False)
            def red_autolock():
                todo_lib()
                sh(["git", "tag", "-d", "red/store"], fx)
                sh(pl + ["start", "N1", "store"], fx)
                sh(pl + ["gate", "G1", "store"], fx)
            case(fx, "그래프: G1 을 통과하면 바로 잠기고 작업 폴더가 깨끗하다", red_autolock,
                 ["bash", "-c", "test -z \"$(git status --porcelain)\" && git tag -l red/store && python3 -c 'import json;print(json.load(open(\".pipeline/state.json\"))[\"store\"][\"stage\"])'"],
                 ["red/store", "locked"], should_fail=False)

            def green_commit():
                for c in (["start", "N1", "store"],):
                    sh(pl + c, fx)
                st = fx / ".pipeline/state.json"
                import json as _j
                d = _j.loads(st.read_text()); d["store"]["stage"] = "n2"; st.write_text(_j.dumps(d))
                append(fx, "crates/store/src/lib.rs", "/// 뺀다.\npub fn sub(a: i64, b: i64) -> i64 {\n    a - b\n}")
                sh(pl + ["gate", "G2", "store"], fx)
            case(fx, "그래프: G2 를 통과한 구현은 바로 커밋되어 작업 폴더가 깨끗하다", green_commit,
                 ["bash", "-c", "test -z \"$(git status --porcelain)\" && git log -1 --format=%s"], "feat(store)", should_fail=False)
            case(fx, "권한: 그래프 밖의 조사 문서는 노드의 범위 검사에 걸리지 않는다",
                 lambda: (sh(pl + ["start", "N1", "store"], fx),
                          (fx / "docs/research").mkdir(exist_ok=True),
                          (fx / "docs/research/x.md").write_text("조사\n")),
                 pl + ["scope"], "모두 맡은 범위 안", should_fail=False)
            case(fx, "권한: 맡지 않은 곳을 고치면 잡는다",
                 lambda: (sh(pl + ["start", "N1", "store"], fx), append(fx, "crates/ai/src/lib.rs", "// 남의 모듈")),
                 pl + ["scope"], "맡지 않은 곳")

            # --- 대조 노드 R (QMIT 결정 1)
            brief = "docs/dev/modules/store.md"
            store_ids = ", ".join(required_ids("store"))

            def reopened(lock=False):
                sh(pl + ["reopen", "store"] + (["--lock"] if lock else []) + ["자체 시험"], fx)
                sh(pl + ["start", "R", "store"], fx)

            def table(*rows):
                append(fx, brief, "### 대조\n\n| 시험 · 문서 경로 | 판정 | 근거 |\n|---|---|---|\n"
                       + "\n".join(f"| {p} | {v} | {w} |" for p, v, w in rows))

            def to_n1(*rows):
                reopened()
                table(*rows)
                sh(pl + ["gate", "GR", "store"], fx)
                sh(pl + ["start", "N1", "store"], fx)
                todo_lib()

            case(fx, "대조: 다시 연 모듈은 R 없이 N1 을 시작할 수 없다",
                 lambda: sh(pl + ["reopen", "store", "자체 시험"], fx), pl + ["start", "N1", "store"], "먼저 R(대조)")
            case(fx, "대조: 대조 표가 없으면 GR 실패", reopened, pl + ["gate", "GR", "store"], "✗ 대조 표 없음")
            case(fx, "대조: 지움 줄에 근거가 없으면 GR 실패",
                 lambda: (reopened(), table(("`crates/store/tests/t.rs`", "지움", "낡아 보임"))),
                 pl + ["gate", "GR", "store"], "✗ 근거 없는 지움")
            case(fx, "대조: 지움 줄에 근거가 있으면 GR 통과하고 지시서를 커밋한다",
                 lambda: (reopened(), table(("`crates/store/tests/t.rs`", "지움", '사용자 "옛 시험은 버린다"'),
                                            ("`docs/dev/modules/store.md`", "열어 둠", "물음"))),
                 ["bash", "-c", f"{py} scripts/pipeline.py gate GR store && git log -1 --format=%s"],
                 ["GR 통과", "docs(store): 대조 표"], should_fail=False)

            def drop_fn_unlisted():
                to_n1(("`crates/store/tests/t.rs`", "유지", "이번 차수와 무관"))
                (fx / "crates/store/tests/t.rs").write_text(
                    f"// 검증: {store_ids}\n#[test]\nfn adds_again() {{\n    assert_eq!(pi_store::add(1, 2), 3);\n}}\n")
            case(fx, "대조: 표에 없는 시험 함수를 지우면 G1 실패", drop_fn_unlisted,
                 pl + ["gate", "G1", "store"], "✗ 대조 표에 없는 시험을 지웠다")

            def drop_file_listed():
                to_n1(("`crates/store/tests/t.rs`", "지움", "QMIT 결정 1 자체 시험"))
                (fx / "crates/store/tests/t.rs").unlink()
                (fx / "crates/store/tests/t2.rs").write_text(
                    f"// 검증: {store_ids}\n#[test]\nfn adds_again() {{\n    assert_eq!(pi_store::add(1, 2), 3);\n}}\n")
            case(fx, "대조: 표의 지움에 있는 시험 파일만 지우면 G1 통과", drop_file_listed,
                 pl + ["gate", "G1", "store"], "G1 통과", should_fail=False)

            # --- 동작 잠금 (QMIT 결정 2)
            case(fx, "동작 잠금: 시험이 지금 구현에서 모두 통과하면 G1 통과", None,
                 red + ["store", "--lock"], "G1 통과: store (동작 잠금)", should_fail=False)
            case(fx, "동작 잠금: 지금 구현에서 실패하는 시험이 있으면 G1 실패", todo_lib,
                 red + ["store", "--lock"], "✗ 동작 잠금인데")
            case(fx, "동작 잠금: reopen --lock 이 상태에 남고 status 에 보인다",
                 lambda: sh(pl + ["reopen", "store", "--lock", "옮기기"], fx), pl + ["status"],
                 "동작 잠금", should_fail=False)

            # --- 모델 승급 (QMIT 결정 3)
            case(fx, "승급: 같은 실패 2번이면 한 등급 위 모델로 다시 맡기라고 하고 상태에 남긴다",
                 lambda: (sh(pl + ["start", "N1", "store"], fx), sh(pl + ["gate", "G1", "store"], fx)),
                 ["bash", "-c", f"{py} scripts/pipeline.py gate G1 store; {py} -c "
                  "'import json;print(json.load(open(\".pipeline/state.json\"))[\"store\"][\"escalate_model\"])'"],
                 ["같은 실패 2번 — 이 노드를 한 등급 위 모델로", "True"], should_fail=False)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    bad = [n for ok, n in results if not ok]
    print(f"\n자체 시험 {len(results)}개 중 {len(results) - len(bad)}개 통과")
    stamp = ROOT / ".pipeline/selftest.ok"
    if bad:
        stamp.unlink(missing_ok=True)
        print("실패: " + " / ".join(bad))
        sys.exit(1)
    stamp.parent.mkdir(exist_ok=True)
    code, h = sh(["bash", "-c", ". scripts/lib.sh && rules_hash"], ROOT)
    stamp.write_text(h.strip() + "\n")
    print("검사기 자체 시험 통과 — 지문을 .pipeline/selftest.ok 에 적었다")


if __name__ == "__main__":
    main()
