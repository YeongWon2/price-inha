# 출처 (DeskRPG 원본 자산)

- 원본 저장소: dandacompany/deskrpg (GitHub)
- 원본 커밋: 5d80963
- 들인 날짜: 2026-11-24
- 들인 까닭: 결정 13 (`docs/dev/decisions.md` 13번 — "사무실 자산은 원본 DeskRPG 것을 그대로"), server 20차 1번
- 들인 것: 원본 `public/assets/` 가운데 `characters/office/*.glb`(외형 50종) · `characters/LICENSES.md` ·
  `environments/` · `furniture/` · `shared/`, 그리고 원본 저장소 꼭대기의 `LICENSE.md`.
  파일은 고치지 않고 같은 상대 경로에 바이트 그대로 두었다(해시 표: `crates/server/tests/round23/deskrpg_assets.txt`).
- 들이지 않은 것: 원본 상표 `brand/`, 비교용 `characters/eunchae-mpfb.glb`(CC BY 표기 의무).

## 라이선스 조건 요약 (원문은 같은 폴더의 `LICENSE.md` · `characters/LICENSES.md`)

- 원본 저장소는 Sustainable Use License 1.0이다: 자기 내부 업무 · 개인 · 비상업 용도로만 쓰고 고칠 수 있고,
  남에게 줄 때는 비상업 목적으로 무료로만 줄 수 있다. 라이선스 · 저작권 표시를 지우거나 가리면 안 된다.
- 원본 안의 제3자 구성 요소는 그 소유자의 라이선스를 따른다. 외형 GLB는 Quaternius의 CC0 1.0 바탕 모델을
  원본이 고친 것이다(`characters/LICENSES.md`). 폴더별 README의 출처 표기도 그대로 둔다.
- 이 프로젝트를 상업 서비스로 바꾸면 이 자산을 바꿔야 한다(그때 다시 정한다 — 결정 13).
