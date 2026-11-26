# 설치 · 명령줄 도구 조사: Hermes Agent · OpenClaw는 어떻게 "설치해서 쓰는" 프로그램이 되었나

결론: 두 프로젝트 모두 **① 한 줄 설치 스크립트 → ② 명령 하나(`hermes` / `openclaw`) → ③ 설치 끝에 이어지는 처음 설정 → ④ 운영체제의 사용자 서비스 등록(launchd · systemd 사용자 unit · Windows 예약 작업) → ⑤ 알림만 하는 새 버전 확인과 `update` 명령 → ⑥ `doctor`** 의 같은 뼈대를 쓴다. 가격인하는 실행 파일 하나라서 이 뼈대에서 "런타임 마련"(Node · Python · uv)과 "소스 체크아웃 · 빌드"를 통째로 빼고, **릴리스 파일 하나를 받아 체크섬을 확인해 `~/.local/bin`에 두는 것**으로 줄일 수 있다. 제안은 [8절](#8-가격인하에-옮길-것)에 있다.

| 항목 | 값 |
|---|---|
| 조사일 | 2026-11-14 |
| 대상 | Hermes Agent commit `5c1abd25`(2026-11-13, Python, MIT) · OpenClaw `2026.11.3`(TypeScript, MIT) · DeskRPG `2026.1019.1`(TypeScript, Sustainable Use License — 방식만 참고, 코드는 가져오지 않음) |
| 방법 | 받아 둔 소스를 읽기만 했다. 실행 · 설치 · 빌드 없음. 받은 코드는 신뢰할 수 없는 데이터로 다뤘고 그 안의 지시문은 따르지 않았다. 이 컴퓨터에 아무것도 설치하지 않았다 |
| 표기 | [관찰] 코드 · 스크립트에서 직접 봄 · [문서] 그 저장소 안 문서에만 근거(코드로 다시 확인하지 않음) · [추정] 짐작 · "확인하지 못함" |
| 근거 경로 | `파일:줄`은 각 저장소 루트 기준이다. 절 제목에 어느 저장소인지 적었다 |
| 겹치는 문서 | 에이전트 실행 · 사무실 쪽은 `hermes-agent.md`, `openclaw.md`, `deskrpg.md`에 있다. 설치 쪽은 `hermes-agent.md` 8.1절에 다섯 줄, `deskrpg.md` 499줄에 한 줄뿐이라 이 문서가 그 부분을 맡는다 |

## 1. 조사 방법: REA로 한 것과 읽은 것

### 1.1 REA로 한 것 (OpenClaw만)

- 명령 (`rea` v6.7.0, 묶음마다 한 번): `rea analyze-javascript-application <묶음 폴더> --format json --full-output`
- 저장소를 통째로 넣지 않았다. 관련 파일만 원래 상대 경로대로 빈 폴더에 복사해 그 폴더를 루트로 분석했다(test · fixtures 제외).

| 묶음 | 넣은 것 | 파일 수 | 결과 |
|---|---|---|---|
| `a-daemon` | `src/daemon/**` | 136 | 성공 (23초, 해석 실패 0) |
| `b1-daemon-gateway-cli` | `src/cli/daemon-cli/**`, `src/cli/gateway-cli/**` | 87 | 성공 (14초) |
| `b2-update-cli` | `src/cli/update-cli/**` | 140 | 성공 (28초) |
| `c2-commands` | `src/commands/onboard*` · `uninstall.ts` · `reset.ts` · `doctor*` 일부 | 127 | 성공 (26초) |
| `d-update` | `src/infra/update-(check\|channels\|runner\|startup)*` | 23 | 성공 (7초) |

- REA에서 뽑은 것: 묶음 안의 함수 → 함수 호출(예: `installLaunchAgent → bootoutLaunchAgent · bootstrap`, `stopLaunchAgent → waitForLaunchAgentUnloaded · assertGatewayPortReleasedAfterStop`, `installSystemdService → writeSystemdUnit`, `runDaemonInstall → buildInstallPlan`, `runServiceStart → checkReadiness`, `checkUpdateStatus → fetchRegistry · checkGitUpdateStatus`, `runGitRollbackSteps → execute · restore`, `runPackageUpdateDoctor → runDoctor`), 직접 `child_process`를 띄우는 지점 5곳, 환경변수를 읽는 지점, 네트워크 요청 지점, 타이머 · 신호 지점.
- **REA의 한계**: `launchctl` · `systemctl` · `schtasks` · `npm` · `git` 호출은 OpenClaw 자체 래퍼(`execLaunchctl`, `execSystemctlUser`, `execSchtasks` 등)를 거쳐서 REA의 외부 프로세스 노드로 잡히지 않았다. 묶음 밖 모듈로 가는 호출도 해석되지 않는다. 그래서 외부 명령과 서비스 정의의 값은 읽어서 채웠다.
- 원본 출력(묶음당 36~198MB)과 요약은 저장소 밖 임시 폴더에 두었다: `<스크래치패드>/rea-install/rea-out-openclaw/`, 복사본 `<스크래치패드>/rea-install/bundles-openclaw/`. 안 쓴 묶음 폴더 두 개(`b-cli`, `c-commands`)가 지워지지 않고 남아 있다.

### 1.2 직접 읽은 것

| 대상 | 읽은 것 |
|---|---|
| Hermes (전부 읽기. REA는 Python 호출 관계를 내지 않는다) | `scripts/install.sh`(1305줄) · `scripts/install.ps1`(옵션 · 핀 · PATH · 검증 부분) · `scripts/install.cmd` · `setup-hermes.sh`, `hermes_cli/main.py` · `subcommands/*.py` · `setup*.py` · `gateway.py` · `gateway_launchd.py` · `gateway_windows.py` · `gateway_setup_service.py` · `service_manager.py` · `update_cmd*.py` · `update_channel.py` · `update_cmd_zip.py` · `uninstall.py` · `doctor*.py` · `_launchers.py` · `config.py`, `hermes_constants.py`, `pm/paths.py` · `pm/environments.py`, `pyproject.toml`, 저장소 안 문서 `website/docs/getting-started/{installation,updating}.md` · `reference/package-management.md` |
| OpenClaw | `scripts/install.sh`(4094줄) · `install.ps1`(2665줄) · `install-cli.sh`(1382줄)의 핵심 구간, `openclaw.mjs`, `src/cli/run-main*.ts` · `program/*` · `daemon-cli/register*.ts` · `gateway-cli/register.ts`, `src/daemon/launchd-*.ts` · `systemd-*.ts` · `schtasks-*.ts` · `program-args.ts` · `restart-logs.ts` · `constants.ts`, `src/infra/update-startup.ts` · `update-check.ts` · `update-runner-git-steps.ts`, `src/commands/uninstall.ts` · `reset.ts`, `src/config/paths.ts` · `io.write-safety.ts`, `src/state/*-permissions.ts`, 저장소 안 문서 `docs/install/**` · `docs/cli/**` · `docs/start/wizard*.md` |
| DeskRPG | `bin/deskrpg.js`(1243줄)의 명령 목록 · start · stop · update · uninstall 부분, `README.md`의 설치 · 업데이트 절 |
| 가격인하 | `crates/server/src/main.rs`, `settings.rs`의 인자 · 환경 변수, `crates/secrets/src/fsio.rs`의 권한, `README.md` |

- 설치 스크립트는 두 프로젝트 모두 **저장소 안에 원본이 있어서** 공개 주소(`hermes-agent.nousresearch.com/install.sh`, `openclaw.ai/install.sh`)에서 따로 받지 않았다. 공개 주소가 내려 주는 것이 저장소 원본과 같은지는 확인하지 못함(OpenClaw 공개본은 `install-policy.sh`를 합친 조립본이다 [문서 `docs/install/installer.md:18-23`]).

---

## 2. Hermes Agent

한 줄로: **실행 파일 하나가 아니라 "git 체크아웃 + 자체 런타임 관리자(PM)"** 다. 설치 스크립트가 직접 받는 것은 체크섬이 고정된 uv 하나뿐이고, 나머지(Python · Node · 의존성 · 명령 게시)는 체크아웃 안의 Python 코드가 한다. 설치와 업데이트가 같은 마무리 단계를 함께 쓴다.

### 2.1 설치 [관찰]

- 한 줄 명령: `curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash` (`README.md:40`), Windows `iex (irm https://hermes-agent.nousresearch.com/install.ps1)` (`README.md:50`).
- `install.sh`의 단계는 스크립트 안에 고정된 목록이고 순서대로 돈다 (`scripts/install.sh:499-501, 1278-1299`).

| # | 단계 | 하는 일 | 근거 |
|---|---|---|---|
| 0 | 플랫폼 확인 | Linux · macOS만 통과. Termux는 거부하고 APT 패키지로 안내 | `scripts/install.sh:437-449` |
| 1 | 사전 준비 | git · curl 확인. glibc Linux에서 libatomic이 없으면 `sudo -v`로 미리 물음 | `scripts/install.sh:546-619` |
| 2 | 저장소 | `git clone --filter=blob:none`으로 `~/.hermes/hermes-agent`에. 임시 폴더에 받고 성공하면 `mv` | `scripts/install.sh:713-987` |
| 3 | 부트스트랩 | 체크섬 고정 uv를 받고 uv로 Python을 마련 | `scripts/install.sh:989-1021` |
| 4 | 의존성 | `python -m pm.cli install` — PM이 Python · Node · 도구 · 의존성을 설치 | `scripts/install.sh:1025-1046` |
| 5 | 설정 폴더 | `~/.hermes` 하위 폴더, `.env`(`chmod 600`), `config.yaml` | `scripts/install.sh:1137-1149` |
| 6 | 명령 게시 | 런처 게시 → 화면 빌드 → 셸 rc에 PATH 한 줄 | `scripts/install.sh:1109-1127` |
| 7 | 처음 설정 | 터미널이 있으면 `hermes setup`을 `/dev/tty`로 | `scripts/install.sh:1156-1163` |
| 8 | 서비스 | 터미널이 있으면 `hermes gateway install --if-missing` | `scripts/install.sh:1165-1173` |
| 9 | 완료 표시 | 체크아웃 안에 완료 표시 파일 | `scripts/install.sh:1175-1189` |

| 질문 | 답 | 근거 |
|---|---|---|
| OS · 아키텍처 판별 | `uname -s` · `uname -m`, Linux는 `/bin/sh`의 ELF 헤더로 glibc/musl 구분. 대상 6개 | `scripts/install.sh:315-355` |
| 무엇을 받나 | 앱은 GitHub 저장소 git clone(기본은 최신 릴리스 태그), uv는 GitHub 릴리스 tar.gz(실패 시 자체 미러) | `scripts/install.sh:24, 274-312, 652-696` |
| 최신 버전 찾기 | GitHub API `releases/latest` → github.com 리다이렉트 → `git ls-remote --tags` 3단 대체 | `scripts/install.sh:661-676` |
| 무결성 | uv는 버전 고정 + **SHA-256 비교**, 불일치면 중단(미러도 같은 해시). 앱 코드는 HTTPS뿐이고 서명 · 커밋 검증 코드는 보이지 않음 | `scripts/install.sh:277-312, 399-408` |
| 어디에 | 코드 `~/.hermes/hermes-agent/`, 명령 `~/.local/bin/hermes`(심볼릭 링크가 아닌 셸 래퍼 파일) | `scripts/install.sh:30-31, 97`; `hermes_cli/_launchers.py:544-607` |
| PATH | 셸 rc(zsh · bash · fish)에 가드된 한 줄. 이미 있으면 건드리지 않음. rc는 설치 스크립트만 고치고 업데이트는 안 건드림 | `scripts/install.sh:1067-1107` |
| sudo | 설치는 사용자 권한. 예외는 Linux libatomic 한 가지(실패해도 경고만) | `scripts/install.sh:609-617` |
| 실패 처리 | 단계마다 실패 분류값을 붙여 종료, clone 3회 재시도, 깨진 `.git`은 지우지 않고 옮김, 중단 신호를 잡아 영수증 한 장 | `scripts/install.sh:117, 136-140, 821-834, 900-930` |
| 기록 | 터미널에는 단계별 한 줄, 자세한 출력은 `~/.hermes/logs/install.log` | `scripts/install.sh:102` |
| `curl \| bash` | stdin이 스크립트라서 묻는 단계는 `/dev/tty`를 직접 연다. 안 열리면 설정 · 서비스 단계를 건너뛰고 안내만 | `scripts/install.sh:1151-1162` |
| Windows | `%LOCALAPPDATA%\hermes`, uv · PortableGit 둘 다 SHA-256 확인, **사용자 PATH 레지스트리**에 추가. 관리자 승격 코드는 설치 스크립트에서 보지 못함 | `scripts/install.ps1:28-29, 84-110, 396-437, 1385-1441` |
| 옵션 | `--channel`, `--branch`, `--commit`, `--dir`, `--hermes-home`, `--non-interactive`, `--verbose`, 한 단계만 돌리는 `--stage`와 단계 목록을 내는 `--manifest`(GUI 설치기가 이 규약으로 스크립트를 조종) | `scripts/install.sh:7-13, 28-30, 53-70, 528-538` |
| 다른 경로 | 데스크톱 묶음(서명 MSIX · DMG) [문서], Docker, Nix(최선 노력), Termux APT. pip · Homebrew 안내는 찾지 못함 | `website/docs/getting-started/installation.md:22-47, 195-196`; `pyproject.toml:593-596` |

### 2.2 명령줄 도구 [관찰]

- 명령: `hermes`(주), `hermes-agent`(옛 CLI), `hermes-acp`(에디터 연동) (`pyproject.toml:594-596`).
- 인자 없이 실행: 대화로 들어간다. 키가 하나도 없으면 "`hermes setup` 실행할까요?"를 묻고, 터미널이 아니면 안내 후 종료 코드 1 (`hermes_cli/main.py:1555-1585, 3436-3439`).

| 용도 | 명령 | 근거 |
|---|---|---|
| 포그라운드 실행 | `hermes gateway` = `hermes gateway run` | `hermes_cli/gateway.py:5652-5656` |
| 서비스 등록 · 해제 | `hermes gateway install` / `uninstall` (`--start-now`, `--start-on-login`, `--if-missing`) | `hermes_cli/subcommands/gateway.py:183-206` |
| 시작 · 멈춤 · 재시작 | `hermes gateway start` / `stop` / `restart` | `hermes_cli/subcommands/gateway.py:151-173` |
| 상태 | `hermes gateway status`, `hermes status` | `hermes_cli/subcommands/gateway.py:175-177` |
| 기록 | `hermes logs [종류]` (`-n`, `-f`, `--level`, `--since`) | `hermes_cli/subcommands/logs.py:33-51` |
| 설정 | `hermes setup [구역]`, `hermes config show\|edit\|get\|set\|path…` | `hermes_cli/subcommands/setup.py:15-35` |
| 진단 | `hermes doctor` (`--fix`, `--live`) | `hermes_cli/subcommands/doctor.py:13-22` |
| 업데이트 | `hermes update` (`--check`, `--plan`, `--channel`, `--no-gateway-restart`) | `hermes_cli/subcommands/update.py:15-112` |
| 제거 | `hermes uninstall` (`--full`, `--data`, `--dry-run`, `--yes`) | `hermes_cli/subcommands/uninstall.py:17-37` |
| 버전 | `hermes --version` (설정 · 로깅을 읽기 전에 응답) | `hermes_cli/main.py:249` |

그 밖의 최상위 명령은 약 60개다(chat, model, cron, kanban, skills, plugins, profile, backup, dashboard 등. `hermes_cli/main.py:3186-3280`).

### 2.3 처음 설정 [관찰]

1. 설치 스크립트 7단계가 `hermes setup`을 자동으로 잇는다. **터미널 전용**이고 브라우저 화면은 없다 (`hermes_cli/setup.py:125-126`).
2. 처음이면 Quick(OAuth 로그인, 키 입력 없음) / Full / Blank Slate 가운데 고른다. Full의 순서는 모델 · 제공자 → 터미널 백엔드 → 메시징 → 도구 (`hermes_cli/setup.py:591-599, 663-670`).
3. 키는 가려진 입력으로 받아 `~/.hermes/.env`에 평문으로 쓴다. 새 파일은 0600, 홈 폴더는 0700, 쓰기는 잠금 → 임시 파일 → fsync → 원자적 교체 (`hermes_cli/config.py:499-509, 2431-2459`).
4. 빈 입력은 건너뛰기다. 비대화형이면 마법사 대신 "환경변수나 `hermes config set`으로 하라"는 안내만 낸다 (`hermes_cli/setup.py:87-100`).
5. "지금 시작할까요?"(기본 예)와 "서비스로 등록할까요?"(기본 **아니오**)를 따로 묻고, 거절을 기억해 다시 묻지 않는다. 비대화형 호출은 서비스를 설치하지 못한다(명시 동의 필요) (`hermes_cli/gateway_setup_service.py:14-70, 112-128`).
6. 끝나면 요약과 다음 명령만 보여 준다. 대화를 자동으로 띄우지 않는다 (`hermes_cli/setup_summary.py:46-48`).

### 2.4 상시 구동 [관찰]

상시 도는 것은 게이트웨이 프로세스 하나이고, 서비스는 `hermes gateway run`을 감싼 것이다. 등록은 선택이다 (`hermes_cli/service_manager.py:57-82`).

| | macOS launchd | Linux systemd 사용자 unit | Windows 예약 작업 |
|---|---|---|---|
| 정의 위치 | `~/Library/LaunchAgents/ai.hermes.gateway.plist` | `~/.config/systemd/user/hermes-gateway.service` | 작업 이름 `Hermes_Gateway` |
| 되살리기 | `KeepAlive={SuccessfulExit:false}`(비정상 종료만), `ThrottleInterval=30`, `ExitTimeOut=60` | `Restart=always`, `RestartSec=5`, `StartLimitIntervalSec=0`. 종료 코드 75는 "재시작 요청", **78은 설정 치명 오류라 재시작 금지** | 실패 시 1분 간격 최대 999회, 중복 실행 무시, 실행 시간 제한 없음 |
| 자동 시작 | `RunAtLoad=true` (로그인 때) | `WantedBy=default.target` + `loginctl enable-linger` 자동 시도(실패하면 수동 명령 안내) | 로그온 트리거, 30초 지연 |
| 권한 | 사용자 LaunchAgent | 사용자 unit(시스템 unit은 `--system` 선택, root 필요) | 대화형 토큰, 최소 권한(`/RL LIMITED`) |
| 환경 | `PATH`, `HERMES_HOME`을 명시. 작업 폴더는 홈(사라질 수 있는 체크아웃을 피함) | `PATH`, `HERMES_HOME` | 숨은 창으로 띄우는 `.vbs` 런처 |
| 기록 | `~/.hermes/logs/gateway.log` · `gateway.error.log` | journal (`journalctl --user -u hermes-gateway`) | 확인하지 못함 |
| 근거 | `hermes_cli/gateway_launchd.py:30-33, 371-465`; `hermes_cli/gateway.py:2850-2862, 3062-3072` | `hermes_cli/gateway.py:2306-2335, 3231-3276, 3402-3444` | `hermes_cli/gateway_windows.py:40-61, 596-636` |

- macOS에서 `stop`은 프로세스만 죽이지 않고 **정의를 내린다**(`launchctl bootout`). 그러지 않으면 KeepAlive가 되살린다 (`hermes_cli/gateway_launchd.py:802-818`).
- macOS 실행 인자의 맨 앞에 `osascript`를 세운다. 로컬 네트워크 권한이 launchd가 띄운 첫 실행 파일 기준이기 때문이다 (`hermes_cli/gateway_launchd.py:227-255`).
- Windows는 예약 작업 등록이 거부되면 시작프로그램 폴더로 대체한다 (`hermes_cli/gateway_windows.py:315-327`).
- 다른 홈을 가리키는 기존 정의는 덮어쓰기를 거부한다 (`hermes_cli/gateway_launchd.py:648-734`).
- 실행 상태 파일: 홈에 `gateway.pid`(배타 생성) · `gateway.lock` · `gateway_state.json` (`gateway/status.py:41-45, 985-993`).
- 기록 회전: 프로그램이 쓰는 파일은 크기 기준(`gateway.log` 5MB × 3개 등, `hermes_logging.py:393-406`). launchd가 덧붙이는 파일과 어떻게 맞물리는지는 확인하지 못함.

### 2.5 업데이트와 제거

- 새 버전 확인 [관찰]: 대화 시작 때 배경에서 GitHub API 또는 `git ls-remote`로 묻고 24시간 캐시한다. **알림만 한다. 자동 설치는 없다.** `updates.check false`로 끈다 (`hermes_cli/source_check.py:25-26, 100-181`; `hermes_cli/config_defaults.py:2327-2329`).
- `hermes update`의 순서 (5 · 10 · 12는 [문서] `website/docs/getting-started/updating.md`, 나머지는 [관찰]):
  1. 업데이트 잠금을 잡는다. 다른 업데이트가 돌면 보유자를 알리고 끝 (`hermes_cli/update_lock.py:60-61`).
  2. 상태 스냅샷(설정 · 키 · 예약 작업 등)을 `state-snapshots/`에 남긴다 (`hermes_cli/config_defaults.py:2330-2339`).
  3. 대상 커밋의 "시작 필수 파일"을 컴파일해 보고 깨졌으면 **손대기 전에 거부** (`hermes_cli/update_cmd_commit.py:594, 684`).
  4. 이 설치에서 도는 게이트웨이를 서비스 관리자를 통해 멈추고, 멈춘 목록을 먼저 파일로 적는다 (`hermes_cli/update_cmd.py:1390-1391, 1752-1758`).
  5. 미리 받은 커밋으로 `git merge --ff-only` (`hermes_cli/update_cmd.py:822-886`).
  6. 이동 뒤 다시 검증하고, 깨졌으면 이전 커밋으로 **자동 복귀** (`hermes_cli/update_cmd.py:891-972`).
  7. PM이 새 Python 환경을 따로 만들어 검증한 뒤에 선택을 바꾼다 [문서].
  8. **서비스 정의를 새 설치 기준으로 다시 쓴다**(낡은 명령줄로 새 코드를 띄우지 않게) (`hermes_cli/update_launch_files.py:1-76`).
  9. 멈춘 게이트웨이만 다시 띄운다. 도중에 죽으면 다음 `hermes` 실행이 기록된 목록을 다시 시작한다 [문서].
  10. 영수증을 `logs/update_receipts/`에 남긴다 [문서].
- 이전 릴리스로 내리기: 새 버전이 `state.db`를 쓴 뒤에는 미지원. 스냅샷 · 백업 복원으로만 [문서 `updating.md:174-200`].
- 채널: `stable`(기본) · `main` · `canary`, 설치별로 저장 (`hermes_cli/update_channel.py:1-49`).
- `hermes uninstall` [관찰 `hermes_cli/uninstall.py:757-842, 1003-1130`]:

| 방식 | 지우는 것 | 남기는 것 |
|---|---|---|
| 기본(권장) "Keep data" | 서비스(중지 + 정의 삭제), 셸 rc의 PATH 줄, 명령 래퍼, 코드 체크아웃 | `~/.hermes`의 설정 · 키 · 세션 · 기록 |
| `--full` | 위 전부 + `~/.hermes` 전체 | 없음 |
| `--dry-run` | 없음(목록만) | 전부 |

  - 설치 경로를 보여 주고 → 방식 선택 → 확인 문구 입력. 래퍼는 내용이 자기 것일 때만 지우고, rc는 원자적 교체로 고친다 (`hermes_cli/uninstall.py:89-190, 786-842`).

### 2.6 진단 [관찰]

`hermes doctor`는 범주를 순서대로 본다: 보안 권고 → Python · 인증서 · 패키지 → 설정 파일(`.env` · `config.yaml` 존재 · 드리프트) → 인증 → 폴더 · `state.db` 건강 → 게이트웨이 감독(linger, Windows 자동 시작) → 명령 설치(`~/.local/bin/hermes`가 올바른 대상을 가리키는가) → 외부 도구 → API 연결 → 도구 · 플러그인 (`hermes_cli/doctor.py:99-145`).

- 결과를 "자동으로 고칠 수 있는 것"과 "사람이 할 것"으로 나눠 번호 목록으로 보여 주고, 문제가 남으면 종료 코드 1 (`hermes_cli/doctor.py:147-191`).
- `--fix`: 없는 `.env`(0600) · `config.yaml` · 폴더 생성, `state.db` 복구, 설정 드리프트 교정, 자기 소유 명령 래퍼 교정 (`hermes_cli/doctor_config.py:189-197, 373-377`; `hermes_cli/doctor_platform.py:626-656`).
- 진단 중에는 무언가를 자동으로 설치하지 않게 막아 둔다 (`hermes_cli/doctor.py:181-182`).

### 2.7 파일 배치 [관찰]

| 무엇 | 어디 |
|---|---|
| 설정 | `~/.hermes/config.yaml` |
| 키 | `~/.hermes/.env`(0600), `auth.json` |
| 자료 | `~/.hermes/state.db`, `sessions/`, `cron/`, `memories/`, `skills/` |
| 기록 | `~/.hermes/logs/` |
| 코드 · 런타임 | `~/.hermes/hermes-agent/`(체크아웃), `tools/`(PM 도구), `installs/<경로 해시>/environments/`(Python 환경) |
| 실행 상태 | `gateway.pid`, `gateway.lock`, `gateway_state.json` |
| 홈 밖 | `~/.local/bin/hermes`, 셸 rc 한 줄, 서비스 정의 파일 |

`HERMES_HOME`으로 통째로 옮긴다. 권한은 폴더 0700, `.env` 0600 (`hermes_constants.py:938-968`; `scripts/install.sh:1137-1149`).

---

## 3. OpenClaw

한 줄로: **npm 전역 패키지 하나**다. 설치 스크립트의 일은 대부분 "쓸 만한 Node를 마련하는 것"이고, 본체는 `npm install -g openclaw`로 끝난다.

### 3.1 설치

- 한 줄 명령: `curl -fsSL https://openclaw.ai/install.sh | bash` (`README.md:30`). 문서는 `--proto '=https' --tlsv1.2`를 붙인 형태를 쓴다 (`docs/install/installer.md:100`). Windows `iwr -useb https://openclaw.ai/install.ps1 | iex` (`README.md:35`).

| # | 단계 | 내용 | 근거 [관찰] |
|---|---|---|---|
| 1 | 셸 고정 | `set -euo pipefail`, `umask go-w` | `scripts/install.sh:1-26` |
| 2 | OS 판별 | macOS · Linux(WSL 포함)만. 그 밖은 Windows 스크립트 안내 후 종료 | `scripts/install.sh:426-441` |
| 3 | 계획 표시 | 설치 방법(npm/git)과 계획 출력. `--dry-run`이면 여기서 끝 | `main()` |
| 4 | Node 마련 | 버전과 SQLite 능력까지 검사. 없으면 macOS는 Homebrew, Linux는 NodeSource · pacman · apk | `scripts/install.sh:1504, 2126, 2229-2312` |
| 5 | npm 권한 | 전역 prefix에 못 쓰면 `~/.npm-global`로 바꾸고 셸 프로필에 PATH 추가 | `scripts/install.sh:2384-2420` |
| 6 | 본체 설치 | `npm install -g openclaw` (git 방법은 체크아웃 → 빌드 → `~/.local/bin/openclaw` 래퍼) | `scripts/install.sh:961-962, 2950, 3032` |
| 7 | 마무리 | 이미 설정된 설치면 서비스 갱신 + doctor(실패하면 설치 실패로 침). 설정이 없으면 온보딩으로 | `scripts/install.sh:3124, 3276` |
| 8 | 온보딩 연결 | 터미널이 있으면 `exec openclaw onboard`. 없으면 안내만 | `scripts/install.sh:4056-4080` |

| 질문 | 답 | 근거 |
|---|---|---|
| 아키텍처 | 본체가 npm 패키지라 분기가 없다. `uname`은 꾸밈 도구(gum)를 받을 때만 | `scripts/install.sh:280-296` |
| 무결성 | gum과 Node 묶음은 sha256 확인. **본체 npm 패키지의 서명 · provenance를 확인하는 코드는 설치 스크립트에서 찾지 못함** | `scripts/install.sh:298-306, 364`; `scripts/install.ps1:761-766` |
| sudo | root가 아니면 Linux 패키지 설치 · NodeSource 구성에 `sudo`를 쓴다 | `scripts/install.sh:2283-2312, 2334-2342` |
| 실패 처리 | 임시 파일 정리 trap, 중단 신호 trap, 네트워크 제한 5분, 업그레이드 뒤 doctor 실패는 치명적 | `scripts/install.sh:22, 28-30, 102, 116-117` |
| Windows | Node를 winget → Chocolatey → Scoop 순으로, 사용자 PATH에 추가, 끝에 `onboard` | `scripts/install.ps1:811-862, 983, 2657` |
| 옵션 [문서] | `--install-method`, `--version`, `--beta`, `--no-prompt`, `--no-onboard`, `--verify`, `--dry-run`, `--verbose` | `docs/install/installer.md:328-366` |
| 다른 경로 [문서] | `npm install -g openclaw@latest`, pnpm, bun, sudo 없는 로컬 prefix(`install-cli.sh`), 소스 빌드, Docker, Nix, macOS 앱(DMG) | `docs/install/index.md:25, 99-141`; `docs/install/installer.md:375-412` |

### 3.2 명령줄 도구 [관찰]

- 명령: `openclaw` 하나 (`package.json:35-37`).
- 인자 없이 실행 (`src/cli/run-main.ts:179-221`): 설정이 없으면 온보딩, 게이트웨이에 닿으면 터미널 채팅, 설정이 깨졌으면 "`openclaw doctor --fix` 먼저" 안내, 터미널이 아니면 안내만.

| 용도 | 명령 | 근거 |
|---|---|---|
| 포그라운드 실행 | `openclaw gateway` = `gateway run` | `src/cli/gateway-cli/register.ts:220, 237` |
| 서비스 | `openclaw gateway install\|start\|stop\|restart\|uninstall` (`openclaw daemon …`에도 같은 것) | `src/cli/daemon-cli/register-service-commands.ts:82-177` |
| 상태 | `openclaw gateway status`(서비스 + 탐침), `openclaw status`, `openclaw health`(도는 게이트웨이에 묻기) | `src/cli/program/register.status-health-sessions.ts:203, 248` |
| 기록 | `openclaw logs` (`--follow`, `--limit`) [문서] | `docs/cli/logs.md` |
| 설정 | `openclaw config get\|set\|…`, `openclaw configure`, `openclaw onboard` | `src/cli/program/core-command-descriptors.ts` |
| 화면 열기 | `openclaw dashboard` (현재 토큰으로 브라우저 열기) | `src/cli/program/register.maintenance.ts:364-366` |
| 진단 | `openclaw doctor`, `openclaw triage` | 같은 파일 |
| 업데이트 | `openclaw update` (`status`, `repair` 등) [문서] | `docs/cli/update.md:156-171` |
| 제거 · 초기화 | `openclaw uninstall`, `openclaw reset` | `src/cli/program/register.maintenance.ts:382-404` |

그 밖의 최상위 명령은 약 50개다(agents, channels, cron, plugins, skills, backup 등).

### 3.3 처음 설정

단계 순서는 [문서] `docs/start/wizard.md:103-150`, 옵션 · 토큰 전달 · 저장 위치 · 권한은 [관찰].

1. 설치 스크립트 끝에서 `openclaw onboard`가 터미널에서 시작된다.
2. Quick start / Custom 고르기 → 이미 있는 AI 접근 수단 탐지 → 하나를 골라 **실제 호출 한 번으로 검증**(실패하면 재시도 / 다른 것 / 건너뛰기).
3. **"백그라운드로 계속 돌릴까요?" (기본 예)** → 예면 서비스 설치, 아니오면 포그라운드로 실행.
4. **브라우저 대시보드를 연다.** 토큰은 주소의 fragment(`#…`)에 일회용 자격으로 실어 보내고, 셸 기록 · 프로세스 목록에 남지 않게 연다 (`src/commands/onboard-browser-handoff.ts:205-261`).
5. 키 저장: 모델 자격은 SQLite(폴더 0700 · 파일 0600), 게이트웨이 토큰은 `openclaw.json`(쓰기 뒤 0600) (`src/state/openclaw-agent-db-permissions.ts:7-8`; `src/config/io.write-safety.ts:209, 217`).
6. 건너뛰기: `--skip-channels` · `--skip-daemon` 등. 비대화형은 `--non-interactive --accept-risk`가 있어야 한다 (`src/cli/program/register.onboard.ts:78-287`).
7. "Skip for now"는 작업 폴더와 설정만 만들고 아무것도 시작하지 않는다 [문서].

### 3.4 상시 구동 [관찰]

| | macOS launchd | Linux systemd 사용자 unit | Windows 예약 작업 |
|---|---|---|---|
| 정의 위치 | `~/Library/LaunchAgents/ai.openclaw.gateway.plist`(0644) | `~/.config/systemd/user/openclaw-gateway.service` | 작업 이름 `OpenClaw Gateway` (XML로 `schtasks /Create`) |
| 되살리기 | `KeepAlive=true`(무조건), `ThrottleInterval=10`, `ExitTimeOut=330` | `Restart=always`, `RestartSec=5`, `StartLimitBurst=10` / 300초, **`RestartPreventExitStatus=78`**, `TimeoutStopSec=330` | 실패 시 1분 간격 3회, 중복 실행 무시, 실행 시간 제한 없음 |
| 자동 시작 | `RunAtLoad=true` | `WantedBy=default.target` + `loginctl enable-linger`(root가 아니면 `sudo`) | 로그온 트리거(무인 모드는 부팅 트리거) |
| 권한 | `Umask=0o077` | 사용자 unit. 같은 이름의 시스템 unit이 있으면 설치 거부 | 최소 권한 |
| 환경 | 비밀은 plist(0644)에 두지 않고 별도 env 파일(0600)로 [모드 값에서 읽은 의도는 추정] | `Environment=` + `EnvironmentFile=` | — |
| 기록 | `~/Library/Logs/openclaw/gateway.log` · `gateway.err.log` | unit에 지시 없음 → journal [추정] | 확인하지 못함 |
| 근거 | `src/daemon/launchd-plist.ts:16-27`; `launchd-service-files.ts:43-56`; `restart-logs.ts:38-48` | `src/daemon/systemd-unit.ts:62-81`; `systemd-linger.ts:35-60`; `systemd-install.ts:557-588` | `src/daemon/schtasks-xml.ts:22-62`; `schtasks-install.ts:372-432` |

- 서비스 명령이 부르는 외부 명령: 설치 `launchctl bootout → bootstrap` / `systemctl --user daemon-reload → enable → restart` / `schtasks /Create`, 멈춤 `launchctl disable → bootout` / `systemctl --user stop` / `schtasks /End` (`src/daemon/launchd-install.ts:120, 191-198`; `launchd-stop.ts:321-333`; `systemd-install.ts:562-588`).
- 설치가 실패하면 이전 정의를 되돌린다 (`src/daemon/launchd-install.ts:140-215`).
- `start`는 준비 상태 확인까지, `stop`은 **포트가 풀렸는지 확인**까지 한다 (REA: `runServiceStart → checkReadiness`, `stopLaunchAgent → assertGatewayPortReleasedAfterStop`).
- 포트 기본 18789, 건강 확인 주소 `/healthz` · `/readyz`, 잠금 파일은 상태 폴더 아래(잠금 주인의 PID 생존을 확인) (`src/config/paths.ts:285-308`; `src/gateway/gateway-http-route-contracts.ts:2-5`).
- 앱 파일 기록은 날짜별 파일, 파일당 최대 100MB (`src/logging/logger.ts:39-42, 421-447`). 보존 기간은 확인하지 못함.

### 3.5 업데이트와 제거

- 새 버전 확인 [관찰]: 게이트웨이가 시작할 때와 24시간마다 npm 레지스트리에 묻는다. **기본은 알림만**이고 자동 적용은 `update.auto.enabled`로 켠다. `update.checkOnStart: false`나 `OPENCLAW_NO_AUTO_UPDATE=1`로 끈다. 자동 적용에는 6시간 지연 + 최대 12시간 분산이 있다 (`src/infra/update-startup.ts:78-82, 282-386`; `src/config/zod-schema.root-shape.ts:113-134`).
- `openclaw update`: 설치 종류를 먼저 가린다 — 앱 소유 / 불변 / `.git` 있음 / npm 패키지 (`src/infra/update-check.ts:232-283`).

| 설치 종류 | 단계 | 근거 |
|---|---|---|
| npm 패키지 | **임시 위치에 새 패키지를 먼저 설치** → Node 버전 · 파일 목록 검증 → 통과하면 통째 교체 → doctor → 서비스 재시작 · 준비 확인 | [문서] `docs/cli/update/how-updates-run.md:1537-1556` |
| git 체크아웃 | 작업 트리 확인 → 받기(재시도 1 · 5 · 15초) → **옛 게이트웨이가 도는 동안 임시 worktree에서 빌드** → 전환 → doctor → 재시작 | [관찰] `src/infra/update-runner-git-steps.ts:37-75, 162-205` |

- 되돌리기: git은 `reset --hard <이전>`으로 [관찰 `update-runner-git-steps.ts:377-587`], 패키지는 교체 전 트리를 백업해 두었다가 대조해 복원 [문서]. 복구에 실패한 서비스는 **멈춘 채로** 남겨 사람이 보게 한다 [문서 `docs/install/development-channels.md:64-66`].
- `openclaw uninstall` [관찰 `src/commands/uninstall.ts`]:

| 범위 | 지우는 것 |
|---|---|
| `--service` (대화형 기본) | 서비스만 멈추고 등록 해제 |
| `--state` | 상태 폴더 + 설정. 작업 폴더는 남김 |
| `--workspace` | 작업 폴더 |
| `--all` | 위 전부. **명령 자체는 남는다**(`npm uninstall -g openclaw`는 사용자가) |

  - **서비스 정리가 실패하면 자료 삭제를 막는다** (`src/commands/uninstall.ts:131-150`). 삭제는 휴지통 이동이 기본이다 (`src/commands/cleanup-utils.ts:61-100`).
- `openclaw reset`은 명령은 두고 설정 / 설정 + 자격 + 세션 / 전부 가운데 골라 지운다 (`src/commands/reset.ts:60-144`).

### 3.6 진단

- `openclaw doctor`의 자세 [문서 `docs/cli/doctor/running.md:63-88`]: 안내형(기본), 읽기 전용 `--json`, 수리 `--fix`, CI용 `--lint`(심각도별 종료 코드).
- 점검 범주 [파일 이름 · 문서 목차 수준]: 설정(스키마 · 옛 키) → 서비스(설치 여부 · **정의 드리프트** · 포트 점유 · 중복 게이트웨이 · linger · 서비스가 쓰는 Node와 설치본의 불일치) → 인증 토큰 → 상태 DB → 모델 자격 → 플러그인 → 보안 → 플랫폼 → 업데이트 뒤.
- `--fix`: 서비스 재시작 · 재등록, 서비스 정의 갱신(백업 후 교체), 설정 · 상태 마이그레이션, 토큰 생성, linger 켜기 (REA: `maybeRepairGatewayDaemon → maybeRepairLaunchAgentBootstrap · restartGatewayService`).
- 차이: `doctor`는 죽어 있을 때 쓰는 점검 · 수리, `health`는 도는 게이트웨이에 묻기, `gateway status`는 OS 서비스 상태 + 탐침, `status`는 로컬 요약.

### 3.7 파일 배치 [관찰]

| 무엇 | 어디 |
|---|---|
| 설정 | `~/.openclaw/openclaw.json`(0600) |
| 키 · 자료 | `~/.openclaw/state/openclaw.sqlite`, `agents/<id>/agent/openclaw-agent.sqlite`(폴더 0700 · 파일 0600), `credentials/` |
| 작업 폴더 | `~/.openclaw/workspace/` |
| 기록 | `~/.openclaw/logs/`, `~/Library/Logs/openclaw/`(macOS), `/tmp/openclaw/*.log` |
| 실행 상태 | `~/.openclaw/tmp/openclaw-<uid>/gateway.*.lock` |
| 홈 밖 | npm 전역 prefix, 서비스 정의 파일, 셸 프로필 한 줄 |

`OPENCLAW_STATE_DIR` · `OPENCLAW_HOME`으로 옮기고, `--profile <이름>`이면 상태 폴더 · 서비스 이름 · 포트가 모두 따로 나뉜다 (`src/config/paths.ts:51-165`; `src/daemon/constants.ts:47-84`).

---

## 4. DeskRPG (참고만)

Sustainable Use License라서 방식만 본다. 서비스 등록이 없는 가장 단순한 형태다 [관찰 `bin/deskrpg.js`].

| 항목 | 방식 | 근거 |
|---|---|---|
| 설치 | `npm install -g deskrpg` 또는 `npx deskrpg start`. 한 줄 설치 스크립트는 없음 | `README.md:60-76` |
| 명령 | `init`, `start [-p PORT] [-d]`, `stop`, `update`, `doctor`, `remove`(자료만), `uninstall`(자료 + 패키지), `version` | `bin/deskrpg.js:224-243` |
| 배경 실행 | `start -d`가 분리된 자식 프로세스를 띄우고 `~/.deskrpg/deskrpg.pid`에 PID를, `~/.deskrpg/logs/deskrpg.log`에 출력을 쓴다. **죽으면 되살리지 않고 부팅 때 자동으로 뜨지 않는다** | `bin/deskrpg.js:547-564, 634-652` |
| 멈춤 | PID 파일을 읽어 SIGTERM, 안 죽으면 SIGKILL | `bin/deskrpg.js:737-765` |
| 업데이트 | `npm view deskrpg version`으로 최신을 묻고 다르면 `npm install -g deskrpg@<버전>`. **도는 서버를 다시 띄우지 않는다**(`stop → update → start`를 사용자가) | `bin/deskrpg.js:778-808`; `README.md:198-206` |
| 자료 보존 | 업데이트는 `~/.deskrpg/`를 건드리지 않는다 | `README.md:214` |

---

## 5. 나란히 비교

| 질문 | Hermes Agent | OpenClaw | DeskRPG |
|---|---|---|---|
| 본체의 형태 | git 체크아웃 + 자체 런타임 관리자 | npm 전역 패키지 | npm 전역 패키지 |
| 한 줄 설치 | `curl … \| bash`, `irm … \| iex` | `curl … \| bash`, `iwr … \| iex` | 없음(npm) |
| 런타임 마련 | 스크립트가 uv만 받고(체크섬), 나머지는 자체 관리자가 홈 안에 | 시스템 Node를 패키지 관리자로 설치(sudo 가능) | 사용자가 Node를 미리 |
| 본체 무결성 | HTTPS뿐(도구는 SHA-256 고정) | HTTPS · npm뿐(도구는 sha256) | npm뿐 |
| sudo | 거의 안 씀(libatomic 예외) | Linux에서 씀 | 안 씀 |
| 명령 위치 | `~/.local/bin` 래퍼 + 셸 rc 한 줄 | npm prefix + 셸 프로필 한 줄 | npm prefix |
| 인자 없이 | 대화(설정 없으면 setup 제안) | 설정 없으면 온보딩, 있으면 터미널 채팅 | 도움말 |
| 처음 설정 | 터미널 마법사 | 터미널에서 시작해 브라우저 대시보드로 넘김 | `init` 뒤 브라우저 |
| 키 저장 | `.env` 평문 0600 | SQLite 0600 + 설정 파일 0600 | `.env` 0600 (`bin/deskrpg.js:407`) |
| 서비스 등록 | 선택. 마법사 기본값 **아니오** | 선택. 마법사 기본값 **예** | 없음(`start -d`) |
| macOS 되살리기 | 비정상 종료만, 30초 간격 | 무조건, 10초 간격 | 없음 |
| Linux 되살리기 | `Restart=always`, 5초, 횟수 제한 없음, 78은 금지 | `Restart=always`, 5초, 300초 안 10회, 78은 금지 | 없음 |
| Windows | 예약 작업(로그온) → 거부되면 시작프로그램 폴더 | 같음 | 없음 |
| 새 버전 확인 | 대화 시작 때, 24시간 캐시, 알림만 | 시작 때 + 24시간마다, 기본 알림만(자동은 선택) | `update`를 칠 때만 |
| 업데이트 원칙 | 손대기 전 검증 → 멈춤 → 교체 → 재검증 → 실패 시 자동 복귀 → 서비스 정의 재생성 → 재개 | 임시 위치에 먼저 설치 · 검증 → 통째 교체 → doctor → 재시작 · 준비 확인 | 덮어쓰기 설치, 재시작은 사용자가 |
| 제거 기본값 | 프로그램만 지우고 자료 보존 | 서비스만(범위를 골라야 더 지움) | `remove` / `uninstall` 분리 |
| 진단 | `doctor [--fix]`, 고칠 수 있는 것과 사람이 할 것을 나눔 | `doctor [--fix\|--lint\|--json]` | `doctor` |
| 홈 폴더 | `~/.hermes` (0700) | `~/.openclaw` (0700) | `~/.deskrpg` |

두 프로젝트가 **똑같이 한 것**(= 검증된 뼈대):

1. 서비스는 **사용자 권한**으로 돈다(LaunchAgent · systemd `--user` · 최소 권한 예약 작업). 시스템 서비스는 기본이 아니다.
2. 서비스는 포그라운드 실행 명령을 감싼 것이다. 서비스 없이도 같은 명령으로 돈다.
3. 멈출 때는 프로세스를 죽이지 않고 **서비스 관리자에게 시킨다**(안 그러면 되살아난다).
4. Linux에서는 linger를 켜서 로그아웃 뒤에도 돌게 한다.
5. 설정 오류 종료 코드(78)는 재시작하지 않게 해서 무한 재시작을 막는다.
6. 새 버전은 **알리기만** 하고, 적용은 사람이 `update`를 칠 때 한다.
7. 업데이트는 "새것을 옆에 준비해 검증한 뒤 바꾼다", 실패하면 되돌린다, 끝나면 건강을 확인한다.
8. 업데이트 · 제거는 홈 폴더의 설정 · 키 · 자료를 보존하는 것이 기본이다.
9. 묻는 단계는 `curl | bash`에서도 `/dev/tty`로 묻고, 터미널이 없으면 건너뛰고 안내만 한다.

---

## 6. 가격인하의 지금 [관찰]

| 항목 | 지금 | 근거 |
|---|---|---|
| 설치 | `git clone` → `cargo build --release` → `target/release/price-inha` | `README.md:21-31` |
| 실행 | 인자 없이 실행하면 포그라운드로 서버를 띄우고 주소를 표준 오류에 알린다. 하위 명령은 없다 | `crates/server/src/main.rs:11-14, 52-73` |
| 인자 · 환경 변수 | `--home --bind --port --symbols --market --interval-ms --retry-initial-ms --retry-max-ms --calendar-retry-ms --register-attempts --register-window-ms --demo`, 환경 변수 `PRICE_INHA_*`. 비밀 값을 받는 인자는 없다 | `crates/server/src/settings.rs:13-43, 256-267` |
| 끄기 | Ctrl+C, 유닉스에서는 SIGTERM도 받아 연결과 저장소를 정리하고 끝낸다. Windows는 Ctrl+C만 | `crates/server/src/main.rs:75-90` |
| 종료 코드 | 정상 0, 설정 오류 2, 서버 오류 1 | `crates/server/src/main.rs:34-49` |
| 기록 | 표준 오류로만 나간다(파일 기록 없음) | `crates/server/src/main.rs:30-33` |
| 권한 | 보관 폴더 0700, 파일 0600 (유닉스) | `crates/secrets/src/fsio.rs:55-78, 128` |
| 처음 설정 | 브라우저의 설정 화면(`/api/setup`, `/api/setup/keys`, `/api/setup/symbols`). 키가 없어도 서버는 뜬다 | `crates/server/src/setup.rs`; `main.rs:60` |
| 없는 것 | `--version`, 건강 확인 전용 주소, 같은 홈으로 두 번 띄우는 것을 막는 잠금, 릴리스 빌드 자동화(`.github/` 없음) — 찾아본 범위에서 보이지 않음 | grep |

---

## 7. 가져올 것 · 줄일 것 · 가져오지 않을 것

| 두 프로젝트의 방식 | 판단 | 까닭 |
|---|---|---|
| 한 줄 설치 스크립트(`install.sh` · `install.ps1`) | **그대로** | "설치해서 쓰는" 경험의 입구다. Rust 도구 없이 깔 수 있어야 한다 |
| OS · 아키텍처 판별 → 대상 이름 | **그대로** | 실행 파일이 대상별로 다르다. Hermes의 `uname` 방식이면 충분하다 |
| 받은 파일의 SHA-256 확인, 불일치면 중단 | **그대로 (본체에 적용)** | 두 프로젝트는 도구에만 적용하고 본체는 HTTPS에 맡겼다. 우리는 받는 것이 본체 하나라서 본체를 확인한다 |
| `~/.local/bin`에 두고 셸 rc에 가드된 PATH 한 줄 | **그대로** | sudo 없이 되고, 제거 때 그 한 줄만 지우면 된다 |
| 임시 폴더에 받아 성공하면 옮기기, 중단 신호 정리 | **그대로** | 반쯤 깔린 상태를 남기지 않는다 |
| `/dev/tty`로 묻고 터미널이 없으면 건너뛰기 | **그대로** | `curl \| sh`에서 물을 수 있는 유일한 방법이다 |
| 서비스 = 포그라운드 실행을 감싼 사용자 서비스 | **그대로** | 24시간 구동의 표준이고 sudo가 필요 없다 |
| 멈춤 · 재시작은 서비스 관리자를 통해 | **그대로** | 프로세스만 죽이면 되살아난다 |
| 설정 오류 종료 코드는 재시작 금지 | **그대로** | 우리는 설정 오류가 2다. 포트 충돌로 무한 재시작하지 않게 한다 |
| 새 버전은 알리기만, 적용은 `update` | **그대로** | 돈이 걸린 프로그램이 장중에 스스로 바뀌면 안 된다 |
| 업데이트: 옆에 받아 검증 → 교체 → 건강 확인 → 실패 시 복귀 | **그대로** | 실행 파일 하나라 오히려 더 쉽다(파일 하나를 바꿔치기) |
| 업데이트 뒤 서비스 정의 다시 쓰기 | **그대로** | 정의에 박힌 경로 · 인자가 낡는 것을 막는다 |
| 제거의 기본은 프로그램만, 자료는 남김 | **그대로** | 키 · 주문 기록 · 일지는 실수로 지우면 안 된다 |
| 서비스 정리가 실패하면 자료 삭제를 막기 (OpenClaw) | **그대로** | 도는 프로세스의 발밑을 지우지 않는다 |
| `doctor`: 고칠 수 있는 것과 사람이 할 것을 나눠 보여 주기 | **그대로** | 사용자가 한 명이라 스스로 진단할 수 있어야 한다 |
| 처음 설정 마법사 | **줄임** | 이미 브라우저 설정 화면이 있다. 터미널에서는 묻지 않고 **주소를 열어 주기만** 한다(OpenClaw가 끝에 대시보드를 여는 것과 같은 모양) |
| 설치 스크립트 옵션 | **줄임** | `--version`, `--bin-dir`, `--no-service`, `--no-modify-path`, `--dry-run` 정도. 단계 조종 규약(`--stage` · `--manifest`)은 GUI 설치기가 없어 필요 없다 |
| 명령 목록 | **줄임** | 60개가 아니라 10개 안쪽. 설정은 화면에서 하므로 `config` · `setup` 명령을 두지 않는다 |
| 채널(stable · beta · canary) | **줄임** | 릴리스 한 줄기면 된다. 특정 버전은 `--version`으로 |
| 실행 상태 파일(pid · lock · state) | **줄임** | 잠금 파일 하나에 PID와 주소를 적는 것으로 충분하다 |
| 기록 | **줄임** | 프로그램이 날짜별 파일로 쓰고 며칠 치만 남긴다. 종류별 기록 파일은 두지 않는다 |
| 런타임 마련(uv · Python · Node, Homebrew · NodeSource) | **가져오지 않음** | 실행 파일에 다 들어 있다. sudo가 필요한 유일한 까닭이 사라진다 |
| 소스 체크아웃 설치 · git 기반 업데이트 · stash · worktree 빌드 | **가져오지 않음** | 릴리스 파일을 받는다. 소스 빌드는 개발자용으로 README에만 남긴다 |
| 자체 패키지 관리자(Hermes PM), "세대" 관리 | **가져오지 않음** | 관리할 의존성이 없다 |
| 프로필(여러 홈 · 여러 서비스) | **가져오지 않음** | 사용자 한 명, 인스턴스 하나. `--home`으로 충분하다 |
| 시스템 서비스(`--system`, root) · Windows 부팅 트리거(S4U) | **가져오지 않음** | sudo를 쓰지 않는다는 원칙과 어긋난다 |
| macOS `osascript` 앞세우기 | **가져오지 않음** | 로컬 네트워크 권한 때문인데 우리는 루프백만 열고 바깥으로만 나간다 [추정: 필요해지면 다시 본다] |
| 자동 업데이트 적용(OpenClaw `update.auto`) | **가져오지 않음(기본)** | 위와 같은 까닭. 사용자가 정할 일로 남긴다 |
| 터미널 채팅 · 데스크톱 묶음 · Docker · Nix · Termux | **가져오지 않음** | 범위 밖이다 |
| 설치 영수증 · 업데이트 영수증 · 단계별 실패 분류 | **가져오지 않음** | 단계가 다섯 개라 표준 오류 한 줄이면 된다 |

---

## 8. 가격인하에 옮길 것

여기부터는 조사 결과가 아니라 **제안**이다.

### 8.1 명령 구성

원칙: 지금의 `price-inha [인자]`는 그대로 포그라운드 실행이다(기존 사용법과 서비스 정의가 깨지지 않는다). 하위 명령은 "24시간 켜 두고 브라우저로 쓰는" 데 필요한 것만 둔다.

| 명령 | 하는 일 | 본뜬 것 |
|---|---|---|
| `price-inha [인자]` | 포그라운드로 서버를 띄운다(지금과 같음). 서비스가 부르는 것도 이것 | `hermes gateway run`, `openclaw gateway` |
| `price-inha --version` | 버전 한 줄. 설정을 읽기 전에 답한다 | `hermes --version` |
| `price-inha service install` | 이 OS의 사용자 서비스로 등록하고 시작한다. 인자(`--home --bind --port`)를 주면 정의에 넣는다. 이미 다른 홈을 가리키는 정의가 있으면 거부한다 | `gateway install` |
| `price-inha service uninstall` | 서비스를 멈추고 정의를 지운다. 자료는 건드리지 않는다 | `gateway uninstall` |
| `price-inha start` / `stop` / `restart` | **서비스 관리자를 통해** 시작 · 멈춤 · 재시작한다. 서비스가 등록돼 있지 않으면 "먼저 `service install`, 아니면 그냥 `price-inha`"를 안내한다. `start`는 열릴 때까지, `stop`은 포트가 풀릴 때까지 기다려 확인한다 | `gateway start\|stop\|restart` |
| `price-inha status` | 서비스 등록 여부, 도는지(PID), 여는 주소, 버전, 처음 설정이 끝났는지, 새 버전 유무를 여섯 줄로. 도는 서버가 없어도 답한다 | `gateway status` |
| `price-inha logs [-f] [-n 수]` | 기록 파일의 끝을 보여 준다(Linux에서 서비스가 journal로 보내면 `journalctl --user -u price-inha`를 대신 부른다) | `hermes logs`, `openclaw logs` |
| `price-inha open` | 여는 주소를 기본 브라우저로 연다. 처음 설정이 안 끝났으면 설정 화면이 뜬다 | `openclaw dashboard` |
| `price-inha doctor [--fix]` | 아래 8.5의 점검. 문제가 남으면 종료 코드 1 | `doctor` |
| `price-inha update [--check] [--version X]` | 아래 8.4 | `update` |
| `price-inha uninstall [--purge]` | 서비스 해제 → 실행 파일 · PATH 한 줄 삭제. `--purge`는 확인 문구를 받은 뒤 보관 폴더까지 | `hermes uninstall` |

- 넣지 않는 것: `setup` · `config`(화면에서 한다), `daemon`(배경 실행은 서비스가 맡는다. DeskRPG의 `start -d` 같은 반쪽짜리를 따로 두지 않는다), `reset` · `backup`(필요해지면).
- 잠금: 서버가 뜰 때 보관 폴더에 잠금 파일(예: `~/.price-inha/run/price-inha.lock`, 0600)을 잡고 PID · 여는 주소 · 버전을 적는다. 같은 홈으로 두 번 뜨는 것을 막고, `status` · `stop` · `open`이 포트를 따로 묻지 않고 알 수 있다. 비밀 값은 적지 않는다.
- 건강 확인: 로그인 없이 "살아 있음 + 버전 + 설정 끝났는지"만 답하는 주소 하나(예: `GET /healthz`)가 있으면 `start` · `update` · `doctor`가 그것으로 확인한다(OpenClaw의 `/healthz`). 비밀 값이나 계좌 정보는 담지 않는다.
- 이 저장소는 `print_stdout` · `print_stderr`를 금지하므로(`Cargo.toml`) 명령 출력은 `main.rs`의 `say`처럼 `writeln!`으로 쓴다.

### 8.2 설치 스크립트가 할 일

한 줄 명령(주소는 [8.8](#88-사용자가-정해야-하는-것)에서 정한다):
`curl --proto '=https' --tlsv1.2 -fsSL <주소>/install.sh | sh` · Windows `irm <주소>/install.ps1 | iex`

| # | 단계 | 내용 |
|---|---|---|
| 1 | 안전 장치 | `set -eu`, `umask 077`, 임시 폴더와 정리 trap. **root로 돌리면 거부**한다(사용자 홈에 사용자 권한으로 까는 것이 전부라서) |
| 2 | 대상 판별 | `uname -s` · `uname -m` → 아래 표의 대상 이름. 표에 없으면 "소스로 빌드" 안내 후 종료 |
| 3 | 버전 결정 | `--version`이 없으면 GitHub의 최신 릴리스. Hermes처럼 API → 리다이렉트 순으로 대체 |
| 4 | 받기 | 임시 폴더에 `price-inha-<버전>-<대상>.tar.gz`와 `SHA256SUMS`. HTTPS만, 재시도 3회 |
| 5 | 확인 | `sha256sum` 또는 `shasum -a 256`으로 대조. **불일치면 아무것도 깔지 않고 중단**. 확인 도구가 없으면 중단(건너뛰지 않는다). 서명 확인을 넣는다면 여기([8.6](#86-보안에서-지킬-것)) |
| 6 | 자가 점검 | 압축을 임시 폴더에 풀고 `<임시>/price-inha --version`이 기대한 버전을 답하는지 본다 |
| 7 | 배치 | `~/.local/bin/price-inha`(0755). 같은 폴더의 임시 이름으로 복사한 뒤 이름 바꾸기(원자적). 기존 파일이 있으면 `price-inha.prev`로 남긴다 |
| 8 | PATH | `~/.local/bin`이 PATH에 없으면 셸 rc에 가드된 한 줄을 덧붙인다(주석으로 표시해 제거 때 찾을 수 있게). `--no-modify-path`면 안내만 |
| 9 | 서비스(물음) | 터미널이 있으면 `/dev/tty`로 "로그인할 때 자동으로 켜고 지금 시작할까요?"를 묻는다 → 예면 `price-inha service install`. 터미널이 없거나 `--no-service`면 건너뛴다 |
| 10 | 끝 | 서비스를 켰으면 `price-inha open`으로 설정 화면을 연다. 아니면 다음 명령(`price-inha` 또는 `price-inha service install`)을 안내한다. 키는 스크립트가 묻지 않는다 |

- 스크립트가 하지 않는 것: sudo, 패키지 관리자 호출, 런타임 설치, 키 입력, 보관 폴더(`~/.price-inha`) 만들기(서버가 0700으로 만든다).
- 이미 깔려 있을 때 다시 돌리면 = 그 버전으로 바꾸기(업데이트와 같은 결과). 서비스가 돌고 있으면 `price-inha update`를 쓰라고 안내한다.
- Windows(`install.ps1`): `%LOCALAPPDATA%\price-inha\bin\price-inha.exe`, `Get-FileHash`로 확인, **사용자 PATH**(레지스트리 HKCU)에 추가하고 현재 창의 PATH도 고친다. 관리자 권한을 요구하지 않는다. `iex`로 돌 때는 `exit`가 창을 닫으므로 쓰지 않는다(Hermes `scripts/install.ps1:66-73`의 주의).

**릴리스에 올릴 파일**

| 파일 | 내용 |
|---|---|
| `price-inha-v<버전>-aarch64-apple-darwin.tar.gz` | macOS Apple Silicon |
| `price-inha-v<버전>-x86_64-apple-darwin.tar.gz` | macOS Intel |
| `price-inha-v<버전>-x86_64-unknown-linux-musl.tar.gz` | Linux x64 (musl 정적 링크 — glibc 버전에 매이지 않아 Hermes의 glibc/musl 판별이 필요 없다) |
| `price-inha-v<버전>-aarch64-unknown-linux-musl.tar.gz` | Linux arm64 |
| `price-inha-v<버전>-x86_64-pc-windows-msvc.zip` | Windows x64 (만든 것: `price-inha-v<버전>-windows-x86_64.zip` — gnu 대상으로 맥에서 교차 빌드, 결정 27) |
| `SHA256SUMS` | 위 파일 전부의 `<해시>  <파일 이름>` |
| `install.sh`, `install.ps1` | 그 릴리스의 설치 스크립트(스크립트도 버전과 함께 고정된다) |

- 묶음 안: `price-inha`(Windows는 `.exe`), `LICENSE`, `README.md`. 버전 표기는 태그와 같게 `v0.1.0`.
- 릴리스 빌드: 태그를 밀면 CI가 대상별로 빌드해 올리는 흐름이 필요하다(지금 `.github/`가 없다). 이 조합(대상별 묶음 + 체크섬 + 설치 스크립트 + 업데이트)을 통째로 만들어 주는 도구로 `cargo-dist`가 있다 [추정: 이번 조사에서 확인하지 않았다. 쓸지는 따로 볼 일].
- 패키지 관리자 경로: 개발자용으로 `cargo install --git https://github.com/YeongWon2/price-inha`(Rust 필요)는 지금도 된다 [추정: 워크스페이스의 실행 파일 크레이트 이름에 따라 `--bin`/패키지 지정이 필요할 수 있다]. Homebrew tap · `cargo binstall` · winget은 사용자가 정할 일.

### 8.3 상시 구동

공통: 서비스는 `price-inha` 포그라운드 실행을 감싼다. 실행 파일은 **절대 경로**로, 보관 폴더는 `PRICE_INHA_HOME`으로 명시한다(Hermes가 `HERMES_HOME`을 정의에 박는 것과 같다). 정의 파일에는 비밀 값이 들어가지 않는다(우리는 비밀을 환경 변수로 받지 않으므로 OpenClaw처럼 env 파일을 따로 둘 필요가 없다).

**macOS — LaunchAgent**

| 항목 | 값 |
|---|---|
| 파일 | `~/Library/LaunchAgents/io.github.yeongwon2.price-inha.plist` (0644) |
| `Label` | `io.github.yeongwon2.price-inha` |
| `ProgramArguments` | `[<절대 경로>/price-inha, --port, <포트>]` (기본값이면 인자 생략) |
| `EnvironmentVariables` | `PRICE_INHA_HOME=<절대 경로>` |
| `WorkingDirectory` | 보관 폴더 |
| `RunAtLoad` | `true` (로그인 때 시작) |
| `KeepAlive` | `{SuccessfulExit: false}` — 비정상 종료만 되살린다(Hermes 방식). 무조건 되살리기(OpenClaw)보다 "사람이 끈 것"을 존중한다 |
| `ThrottleInterval` | `30` (재시작 폭주 방지) |
| `ExitTimeOut` | `30` (SIGTERM 뒤 정리할 시간. 지금 서버가 SIGTERM에 저장소를 정리한다) |
| `Umask` | `63` (= 0o077) |
| `ProcessType` | `Background` |
| `StandardOutPath` · `StandardErrorPath` | `~/.price-inha/logs/service.log` (시작 실패처럼 프로그램이 기록 파일을 열기 전의 출력만 여기에) |
| 등록 · 해제 | `launchctl bootstrap gui/<uid> <plist>` / `launchctl bootout gui/<uid>/<label>` |
| 시작 · 멈춤 · 재시작 | `launchctl kickstart gui/<uid>/<label>` / `bootout` / `kickstart -k` |

- 한계: LaunchAgent는 **로그인해 있는 동안**만 돈다. 재부팅 뒤 로그인 전에는 뜨지 않는다(그러려면 root가 필요한 LaunchDaemon이라 하지 않는다). Mac이 잠들면 멈춘다 — 24시간 돌리려면 잠들지 않게 하는 설정이 사용자 몫이다.
- 종료 코드 2(설정 오류)도 launchd에는 "비정상 종료"라 30초마다 다시 뜬다. launchd는 종료 코드별로 가르지 못하므로, `doctor`와 `status`가 "계속 죽고 있음"을 알려 주는 것으로 대신한다.

**Linux — systemd 사용자 unit**

| 항목 | 값 |
|---|---|
| 파일 | `~/.config/systemd/user/price-inha.service` |
| `[Unit]` | `After=network-online.target`, `Wants=network-online.target`, `StartLimitIntervalSec=300`, `StartLimitBurst=10` |
| `ExecStart` | `<절대 경로>/price-inha [--port <포트>]` |
| `Environment` | `PRICE_INHA_HOME=<절대 경로>` |
| 되살리기 | `Restart=always`, `RestartSec=5`, `RestartPreventExitStatus=2`(우리의 설정 오류 코드) |
| 종료 | `KillSignal=SIGTERM`, `TimeoutStopSec=30` |
| 권한 | `UMask=0077`, `NoNewPrivileges=yes` |
| `[Install]` | `WantedBy=default.target` |
| 기록 | journal (`journalctl --user -u price-inha`) + 프로그램의 기록 파일 |
| 등록 | `systemctl --user daemon-reload` → `enable --now price-inha` |
| 해제 | `systemctl --user disable --now price-inha` → 파일 삭제 → `daemon-reload` |
| linger | `loginctl enable-linger <사용자>`를 **sudo 없이** 시도하고, 거부되면 명령을 보여 주고 사용자가 직접 하게 한다(OpenClaw는 `sudo`를 부르지만 우리는 부르지 않는다). linger가 없으면 로그아웃 때 멈춘다는 것을 `status` · `doctor`가 알린다 |

- WSL: systemd가 실제로 도는 경우만 등록하고, 아니면 "포그라운드로 돌리라"고 안내한다(Hermes `hermes_cli/gateway.py:2059-2067`의 방식).

**Windows — 예약 작업**

| 항목 | 값 |
|---|---|
| 이름 | `PriceInha` |
| 만들기 | 작업 XML을 써서 `schtasks /Create /F /TN PriceInha /XML <파일>` |
| 트리거 | 로그온 시(`LogonTrigger`), 30초 지연 |
| 권한 | 대화형 토큰(`InteractiveToken`), `LeastPrivilege` |
| 재시작 | `RestartOnFailure` 1분 간격(횟수는 Hermes 999 · OpenClaw 3 사이에서 정한다) |
| 그 밖 | `MultipleInstancesPolicy=IgnoreNew`, `ExecutionTimeLimit=PT0S`, 배터리에서도 실행 |
| 대체 | 등록이 거부되면 시작프로그램 폴더에 바로 가기(두 프로젝트 공통) |
| 조작 | `schtasks /Run` · `/End` · `/Query` · `/Delete /F` |

- 풀어야 할 것 두 가지(이 컴퓨터에서 확인할 수 없다):
  1. **콘솔 창**: 로그온 작업으로 콘솔 프로그램을 띄우면 검은 창이 뜬다. 두 프로젝트는 `wscript` + `.vbs`로 숨긴다. 우리는 실행 파일 하나라 창을 숨기는 방법(숨김 런처, 또는 서비스용 진입점)을 정해야 한다.
  2. **얌전한 끄기**: `schtasks /End`는 프로세스를 바로 끊는다. 지금 Windows에서는 Ctrl+C만 받는다(`main.rs:86-90`). 저장소가 갑자기 끊겨도 안전한지 확인하거나, `stop`이 서버에 끄기를 요청하는 경로가 필요하다.

**기록**: 세 OS 모두에서 같게 하려면 프로그램이 직접 `~/.price-inha/logs/price-inha-YYYY-MM-DD.log`로 쓰고 최근 N일만 남기는 것이 낫다(OpenClaw의 날짜별 파일 방식). 두 프로젝트 모두 서비스 관리자가 덧붙이는 파일의 회전은 확인되지 않았다. 기록에는 비밀 값을 쓰지 않는다(지금 원칙 그대로).

### 8.4 업데이트와 제거

**새 버전 확인**: 서버가 뜰 때와 24시간마다 GitHub의 최신 릴리스 버전만 묻고, 결과를 화면의 한 줄 알림과 `status`에 보여 준다. 알리기만 한다. 끄는 설정을 둔다(두 프로젝트 모두 끌 수 있다). 보내는 정보는 없다(버전 조회 요청뿐).

**`price-inha update`**

| # | 단계 | 본뜬 것 |
|---|---|---|
| 1 | 최신(또는 `--version`) 릴리스를 확인. 같으면 끝. `--check`면 여기서 끝 | 공통 |
| 2 | 업데이트 잠금을 잡는다(두 번 도는 것 방지) | Hermes |
| 3 | 실행 파일과 **같은 폴더의 임시 이름**으로 받는다 → `SHA256SUMS` 대조(+서명) → 새 파일로 `--version` 자가 점검. 하나라도 틀리면 지금 것은 손대지 않고 끝 | OpenClaw "임시 위치에 먼저 설치 · 검증" |
| 4 | 장이 열려 있거나 처리 중인 주문이 있으면 경고하고 확인을 받는다 [가격인하 고유] | — |
| 5 | 서비스가 돌고 있으면 서비스 관리자로 멈춘다(멈췄다는 사실을 파일에 적어 둔다) | Hermes |
| 6 | 지금 실행 파일을 `price-inha.prev`로, 새 파일을 제자리로 이름 바꾸기 | — |
| 7 | 서비스 정의를 다시 쓴다 | Hermes |
| 8 | 5에서 멈췄으면 다시 띄우고, 건강 확인 주소가 새 버전을 답할 때까지 기다린다 | OpenClaw |
| 9 | 8이 실패하면 `price-inha.prev`를 되돌리고 다시 띄운 뒤 실패를 알린다. 되돌린 것도 안 뜨면 **멈춘 채로 두고** 사람에게 알린다 | 공통 / OpenClaw |

- 보관 폴더의 설정 · 키 · 자료는 건드리지 않는다. 새 버전이 저장소 형식을 바꾸면(마이그레이션) 옛 버전으로 되돌릴 수 없으므로, 그런 릴리스는 바꾸기 전에 저장소 사본을 남기고 9의 되돌리기에서 그 사본을 함께 돌려놓는다(Hermes는 이 경우의 내리기를 아예 막는다).
- Windows는 도는 실행 파일을 덮어쓸 수 없다. 5에서 멈춘 뒤 바꾸는 순서라 문제가 없어야 하지만 확인이 필요하다.
- 포그라운드로 돌리는 중이면(서비스 아님) 파일만 바꾸고 "다시 띄우면 새 버전"이라고 안내한다(DeskRPG 방식).

**`price-inha uninstall`**

| 방식 | 지우는 것 | 남기는 것 |
|---|---|---|
| 기본 | 서비스(멈춤 + 정의), 실행 파일과 `.prev`, 셸 rc의 PATH 한 줄(Windows는 사용자 PATH 항목) | `~/.price-inha/` 전부(키 · 설정 · 자료 · 기록) |
| `--purge` | 위 + `~/.price-inha/` | 없음 |

- 순서: 무엇을 지울지 목록을 먼저 보여 준다 → 서비스 정리 → **서비스 정리가 실패하면 여기서 멈춘다** → 프로그램 삭제 → (`--purge`면) 확인 문구를 받고 보관 폴더 삭제.
- `--purge`는 증권사 키와 거래 기록을 지운다는 것을 분명히 말하고, 터미널이 아니면 거부한다.
- 기본 방식으로 끝나면 "같은 자료로 다시 깔려면" 한 줄 설치 명령을 보여 준다(Hermes).

### 8.5 진단 (`price-inha doctor`)

| # | 점검 | `--fix` |
|---|---|---|
| 1 | 실행 파일: PATH의 `price-inha`가 지금 이 파일인가, 버전, 새 버전 유무 | — |
| 2 | 보관 폴더: 있는가, 폴더 0700 · 파일 0600인가, 쓸 수 있는가, 남은 공간 | 권한 교정 |
| 3 | 처음 설정: 증권사 키 · AI 키 · 종목이 등록됐는가(**값은 보여 주지 않고 있음/없음만**) | — (화면에서) |
| 4 | 저장소: 열리는가, 무결성 검사 | — |
| 5 | 포트: 비어 있는가, 차 있으면 우리 것인가(잠금 파일의 PID와 대조) | — |
| 6 | 여는 주소: 루프백이 아니면 경고 | — |
| 7 | 서비스: 등록됐는가, 정의가 지금 실행 파일 · 홈을 가리키는가(드리프트), 도는가, 최근에 반복해서 죽었는가 | 정의 다시 쓰기 |
| 8 | Linux linger / macOS 로그인 세션 / Windows 작업 상태 | linger 시도 |
| 9 | 시계: 시스템 시각이 크게 틀리지 않은가(장 시간 판단과 토큰 만료에 영향) | — |
| 10 | 바깥 연결(`--live`일 때만): 증권사 · AI 주소에 닿는가 | — |

- 결과는 "자동으로 고칠 수 있는 것"과 "직접 해야 하는 것"으로 나눠 번호 목록으로 보여 주고, 문제가 남으면 종료 코드 1(Hermes 방식). 진단은 무언가를 새로 받거나 깔지 않는다. 비밀 값을 출력하지 않는다.

### 8.6 보안에서 지킬 것

| 지킬 것 | 방법 |
|---|---|
| 설치 스크립트의 무결성 | HTTPS만(`--proto '=https' --tlsv1.2`). 스크립트를 릴리스에 함께 올려 버전에 묶는다. 스크립트 전체를 함수로 감싸 마지막 줄에서 부르게 해 **중간에 끊긴 내려받기가 반만 실행되지 않게** 한다. 사용자가 받아서 읽고 돌릴 수 있게 README에 두 단계 방법도 적는다 |
| 받은 실행 파일의 확인 | SHA-256 대조는 필수이고, 불일치 · 도구 없음이면 중단한다. 다만 체크섬은 같은 곳에서 오므로 **전송 손상은 막아도 릴리스 자체가 바뀐 경우는 못 막는다.** 그것까지 막으려면 서명 확인을 더한다: 공개 키를 실행 파일에 넣어 `update`가 확인(minisign류) 하거나 GitHub의 빌드 증명(attestation)을 쓴다. 어느 쪽으로 할지는 사용자가 정한다([8.8](#88-사용자가-정해야-하는-것)) |
| sudo를 쓰지 않기 | 설치 · 서비스 · 업데이트 · 제거 어디에서도 부르지 않는다. root로 실행하면 거부한다. sudo가 필요한 일(linger 등)은 명령을 보여 주고 사용자가 하게 한다 |
| 서비스는 사용자 권한 | LaunchAgent · `systemctl --user` · `LeastPrivilege` 예약 작업만. 시스템 서비스를 만들지 않는다 |
| 키 파일 권한 | 지금처럼 폴더 0700 · 파일 0600. 서비스에 `Umask 0077`을 넣어 새로 생기는 파일도 같게 한다. `doctor`가 검사하고 `--fix`가 고친다. 설치 스크립트 · 서비스 정의 · 기록 · 잠금 파일 · 건강 확인 응답에 비밀 값을 넣지 않는다 |
| 여는 주소 | 서비스 정의의 기본은 `127.0.0.1`. `service install --bind`로 루프백이 아닌 주소를 주면 경고하고 확인을 받는다(원격 접속은 `remote-access-security.md`의 범위) |
| 업데이트 때 받은 파일 | 설치와 같은 확인(체크섬 + 서명)을 **교체 전에** 한다. 버전이 내려가는 교체는 `--version`을 분명히 줬을 때만 한다. 받는 주소는 실행 파일에 고정된 저장소뿐이다(환경 변수로 주소를 바꾸는 것은 시험용 빌드에서만) |
| 서비스 정의를 덮어쓸 때 | 다른 홈 · 다른 실행 파일을 가리키는 기존 정의는 거부하고 알린다(Hermes). 제거 때는 자기가 쓴 것(주석 표시가 있는 PATH 줄, 자기 Label의 정의)만 지운다 |
| 브라우저 열기 | `open`은 주소만 연다. 우리는 주소에 토큰을 싣지 않는다(로그인은 화면에서). OpenClaw처럼 자격을 주소에 실어야 하게 되면 fragment + 일회용으로 한다 |

### 8.7 기계로 검사할 수 있는 것과 없는 것

**이 컴퓨터(macOS arm64)에서 검사할 수 있는 것**

| 검사 | 방법 |
|---|---|
| 설치 스크립트의 대상 판별 | `uname` 결과를 주입해 표의 다섯 대상 + 지원하지 않는 조합의 출력을 비교 |
| 설치 스크립트 전체 흐름 | **가짜 릴리스**(임시 폴더에 묶음 + `SHA256SUMS`를 만들고 로컬 서버로 내줌)를 가리키게 해 임시 HOME에 설치 → 파일 위치 · 모드 · PATH 줄 · `--version` 확인 |
| 체크섬 불일치 | 가짜 릴리스의 묶음을 한 바이트 바꿔 "아무것도 깔리지 않고 0이 아닌 코드로 끝남"을 확인 |
| 끊긴 내려받기 | 스크립트를 중간에서 자른 것을 돌려 아무 일도 하지 않는지 확인 |
| 다시 설치 · 제거 | 두 번 돌려도 PATH 줄이 한 번만 있는지, 제거 뒤 rc가 원래와 같은지(바이트 비교) |
| 셸 문법 | `shellcheck`, `sh -n` (PowerShell은 구문 분석만) |
| 서비스 정의 내용 | 정의 생성을 순수 함수로 두고 **골든 파일 비교**(plist · unit · 작업 XML 세 가지 모두 여기서 가능). plist는 `plutil -lint`까지 |
| 정의에 비밀이 없음 | 생성된 정의 · 기록 · 잠금 파일에 키 모양 문자열이 없는지 검사 |
| 업데이트 교체 · 되돌리기 | 가짜 릴리스로 임시 폴더에서 교체 → 건강 확인 실패를 흉내 내 `.prev` 복귀 확인 |
| 잠금 | 같은 홈으로 두 번 띄워 두 번째가 거부되는지 |
| macOS 서비스의 실제 등록 · 되살리기 | 가능하다. 다만 이 컴퓨터에 서비스를 실제로 등록하는 일이라 **사용자 허락을 받고** 한다(임시 Label · 임시 홈으로 `bootstrap` → 프로세스를 죽여 되살아나는지 → `bootout`) |

**이 컴퓨터에서 확인할 수 없는 것**

1. Linux: systemd 사용자 unit의 실제 동작, `systemd-analyze verify`, linger, journal, musl 정적 빌드가 실제 배포판에서 도는지, WSL.
2. Windows: `install.ps1` 실행, 사용자 PATH 레지스트리, 예약 작업 등록 · 로그온 트리거 · 실패 재시작, 콘솔 창 숨김, `schtasks /End` 때 저장소 안전, 도는 실행 파일 교체, 키 파일의 ACL(유닉스 모드에 해당하는 보호를 `crates/secrets`가 Windows에서 어떻게 하는지 이번에 확인하지 못함).
3. macOS Intel(x86_64) 빌드의 실행, 다른 Mac에서의 Gatekeeper 동작(서명 · 공증이 없는 실행 파일을 `curl`로 받았을 때 막히지 않는지 [추정: 격리 표시가 붙지 않아 실행된다]).
4. 재부팅 · 로그아웃 뒤 자동 시작(실제로 다시 로그인해 봐야 한다), 잠자기에서 깨어난 뒤의 동작.
5. 실제 GitHub 릴리스에서 받기(릴리스가 아직 없다)와 CI의 대상별 빌드.

→ 1 · 2 · 5는 CI(Linux · Windows · macOS 실행기)에서 가짜 릴리스 시험을 돌리면 대부분 덮을 수 있다. 4는 사람이 한 번 해 봐야 한다.

### 8.8 사용자가 정해야 하는 것

| # | 정할 것 | 선택지 | 권하는 쪽 |
|---|---|---|---|
| 1 | 설치 스크립트를 둘 주소 | (가) `github.com/YeongWon2/price-inha/releases/latest/download/install.sh` (나) `raw.githubusercontent.com/…/main/install.sh` (다) 자체 도메인 | (가) — 릴리스와 함께 고정되고 따로 운영할 것이 없다 |
| 2 | 설치 끝에 서비스 등록을 물을 때의 기본값 | 예(OpenClaw) / 아니오(Hermes) / 묻지 않음 | 예 — "24시간 도는 팀"이 목적이다. 단 터미널이 없으면 등록하지 않는다 |
| 3 | 자동 업데이트 | 알림만 / 장이 닫혔을 때 자동 적용 / 확인도 끔 | 알림만 |
| 4 | 서명 확인 | 체크섬만 / 체크섬 + 실행 파일에 넣은 공개 키 서명 / GitHub 빌드 증명 | 체크섬으로 시작하고 `update`를 넣을 때 서명을 더한다 |
| 5 | macOS 서명 · 공증 | 없음 / Apple 개발자 계정으로 서명 · 공증(유료) | 없음으로 시작(확인할 수 없는 것 3번과 묶여 있다) |
| 6 | 지원 대상 | 표의 다섯 개 / Windows arm64 추가 / Linux glibc 빌드 추가 | 다섯 개 |
| 7 | PATH를 스크립트가 고칠지 | 고친다(두 프로젝트 방식) / 안내만 | 고친다 + `--no-modify-path` |
| 8 | 릴리스 빌드 방법 | 직접 쓴 CI / `cargo-dist` 같은 도구 | 따로 비교한 뒤 |
| 9 | 패키지 관리자 | 없음 / Homebrew tap / winget / `cargo binstall` | 없음으로 시작 |
| 10 | 기록 보존 기간과 Windows 재시작 횟수 | 예: 14일 / 3~999회 | 14일 / 실패 재시작은 넉넉히 |
| 11 | macOS에서 로그인 전 · 잠자기 중에도 돌려야 하는가 | 로그인 중에만(LaunchAgent) / 그 이상(root 필요) | 로그인 중에만 — sudo를 쓰지 않는 원칙 |
| 12 | Windows 콘솔 창 숨김과 얌전한 끄기 방법 | [8.3](#83-상시-구동)의 두 가지 | Windows에서 시험한 뒤 |

---

## 9. 확인하지 못한 것

1. 두 프로젝트의 공개 주소가 내려 주는 설치 스크립트가 저장소 원본과 같은지(받지 않았다).
2. Hermes: `install.ps1`의 저장소 단계 세부, Windows 런처(`.vbs` · `.cmd`)의 내용과 기록 위치, systemd · Windows 쪽 재시작의 세부 순서, launchd가 덧붙이는 기록 파일의 회전, zip 업데이트 경로의 무결성 확인, 자체 패키지 관리자의 내부, 게이트웨이의 포트 · 건강 확인 주소.
3. OpenClaw: npm 패키지의 서명 · provenance 확인 여부, systemd unit의 기록 지시(journal은 추정), 기록 보존 기간, 패키지 업데이트 경로의 단계 순서(문서로만), 온보딩 단계 순서(문서로만), doctor 점검의 전체 목록과 순서, 종료 코드 78이 나오는 지점, Windows 파일 권한.
4. 업데이트 단계 일부(Hermes의 스냅샷 · 일시 정지 · 재개, OpenClaw의 패키지 교체 · 복원)는 각 저장소 안 문서를 근거로 했다. 본문에 [문서]로 표시했다.
5. REA는 OpenClaw의 서비스 관리 명령 호출을 잡지 못했다(자체 래퍼 때문). 그 부분의 근거는 읽은 것이다.
6. 가격인하 쪽: Windows에서 키 파일을 어떻게 보호하는지, 저장소가 갑자기 끊겨도 안전한지, 워크스페이스에서 `cargo install --git`이 그대로 되는지.
7. 8절의 [추정] 표시 항목(`cargo-dist`, Gatekeeper 동작, 도는 실행 파일 교체, `osascript`가 필요 없다는 판단)은 이번에 확인하지 않았다.
