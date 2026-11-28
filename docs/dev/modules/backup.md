# backup 모듈 (T19)

크레이트 `pi-backup`, 폴더 `crates/backup/`. 구성 요소 A14. `pi-store`(11차의 스냅샷 API · `mark_restored`)와 `pi-secrets`(6차의 바이트 검사 · GitHub 토큰)를 쓴다.
근거: `dev.md` D7 · C13 · E15 · E39 · N8 · V48 · V49 · V51 · V50, `spec.md` F9, `principles.md` P22.

## 원본 동작 근거

- SQLite 백업 API(WAL · SHM 빼기), manifest와 검증(경로 탈출 · 빠진 것 · 온전함), 깨지면 아무것도 쓰지 않음, 보존 일 7 · 주 4 · 월 12, 올리기 순서, 빈 폴더에만 되살리기: `docs/research/triggers-approval-memory-deep.md` 6.1 · 6.2 · 6.3, `docs/research/hermes-agent.md` 7.1 · 7.5, `docs/research/deskrpg.md` 5-1 · 5-2.
- 의도해서 다름: age 암호화와 비밀 빼기(D7 · `docs/research/remote-access-security.md` 6.3), 되살린 뒤 멈춘 상태(F9).

## 만드는 것 (1차)

1. 스냅샷을 임시 폴더에 뜬다.
2. manifest(판 번호 · 파일 · SHA-256)를 만든다.
3. 등록된 비밀 값(그대로 · URL · Base64)이 들어 있으면 올리지 않는다(E39). 키 파일은 처음부터 대상이 아니다.
4. age 수신자 공개키로 암호화한다.
5. GitHub contents API로 올린다(기본 주소는 인자) → 크기를 확인한다 → 임시 파일을 지운다. 비공개 저장소 이름과 fine-grained 토큰은 사용자가 화면에서 등록한다(결정 2026-11-16 23번).
6. 보존 규칙을 적용한다(로컬과 원격 같게 7 · 4 · 12).
7. 실패하면 기록하고, 다음 마감에 다시 하고, 알림 요구를 낸다(E15).
8. 되살리기(V51)
   - 빈 폴더에만 한다.
   - 쓰기 전에 검증한다.
   - 비밀키 identity는 인자로만 받고 남기지 않는다.
   - 끝나면 `mark_restored`를 불러 팀을 멈추고 기다리던 승인을 만료한다.
9. 마지막 성공 시각을 기록한다.

## 하지 않는 것

일정(trigger), 키 다시 등록 화면(server), git CLI.

## 테스트 주의

GitHub는 로컬 가짜 서버로만 한다. age 키는 테스트 안에서 만든다. 시험용 키 글자를 DB에 일부러 넣어 거부되는지 본다(V49 · E39).

## 필수 번호

D7 C13 E15 E39 N8 V48 V49 V51 P22

## 1차 검토 참고 (다음 차수 후보)
- 끝남: sqlite.rs is_sound가 평문 스냅샷 사본을 설정한 temp_root가 아니라 시스템 임시 폴더에 쓴다(0700 · drop 때 지움이라 fail-o… — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/backup.md`

## 2차 작업 (1차 검토 참고에서)
- 끝남: sqlite.rs is_sound의 평문 스냅샷 사본은 설정한 temp_root 안(0700)에 두고 끝나면 지운다. 시험: 검증 중에도 시스템 임시 폴더에 파… — 커밋 d0b11a1(red) · 98b892e(green) · 전문: `git show 1a73576:docs/dev/modules/backup.md`

## 3차 작업 (후보 일괄 처리)
- 끝남: 되살리기에도 임시 폴더를 넘긴다: restore_in(암호문, identity, 대상, temp_root, 시각). 검증 사본은 temp_root 안(0700)… — 커밋 4f465b0(red) · 53df1f7(green) · 전문: `git show 1a73576:docs/dev/modules/backup.md`

## 4차 작업 (마무리)
- 끝남: GitHub 토큰 확인 함수 verify_token(api_base, &SecretString) -> Result<(), VerifyFailure>(공개). G… — 커밋 f278a84(red) · 022572e(green) · 전문: `git show 1a73576:docs/dev/modules/backup.md`

## 5차 작업 (2026-11-27 조정자, 결정 31 — 사용자 "경량화 작업도 해야 해")
- 끝남: 동작 잠금 차수다. 별도 작업 폴더(결정 28 방식)에서 red → green으로 만든 것을 main에 합쳤고, 이 차수는 합친 구현에서 시험이 모두 통과하는지… — 커밋 f967e37(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/backup.md`

## 6차 작업 (2026-11-27 조정자, 백로그 P — 사용자 "백로그 다 실행해" · "남은 것들만 다 실행해 key 제외하고")

동작 잠금 차수다. 별도 작업 폴더(결정 28 방식)에서 red → green으로 만든 것을 main에 합쳤고, 이 차수는 합친 구현에서 시험이 모두 통과하는지 보고 잠근다.

1. **설정 파일 담기**: `with_config_files(home, names)` — 이름 그대로 담고 되살리면 제자리로. 비밀 파일(`pi_secrets::BACKUP_EXCLUDED_FILES`) · 저장소 스냅샷 · 보관 앞붙이 · 점 파일 · 폴더 나눔 이름은 줘도 담지 않는다. 심볼릭 링크는 따라가지 않는다. 담은 파일도 E39 비밀 검사를 거친다.

- 원본 동작 근거: 결정 31(보관 파일은 백업에 들어간다) · 백로그 묶음 D(설정 파일이 백업에 드는지), E39 그대로. gzip(결정 33)은 미뤘다.
- 시험으로 확인할 것: `tests/suite/config_files.rs`가 합친 구현에서 통과하고, 기존 시험도 모두 통과한다.

### 대조

| 시험 · 문서 경로 | 판정 | 근거 |
|---|---|---|
| 새 시험 `tests/suite/config_files.rs` | 유지 | 작업 폴더에서 red 커밋으로 먼저 쓰고 green 커밋으로 통과 |
| `tests/suite/main.rs` | 고침 | 새 시험 모듈을 잇는 줄 · 문서 한 줄만 더함(단언은 그대로) |
| 그 밖의 backup 시험 | 유지 | 동작이 바뀌지 않는다 |
