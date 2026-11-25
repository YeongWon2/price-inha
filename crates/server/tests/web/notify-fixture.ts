// 설정 화면 알림 묶음 시험의 응답 모양(server 40차 4번): `GET /api/notify/settings`.
import type { NotifySettings } from "../../web/src/setup/notify-model";

/** 종류 여섯(서버 `NOTIFY_KIND_LABELS`의 차례)과 잠긴 종류. */
export const KINDS: readonly { kind: string; label: string; locked: boolean }[] = [
  { kind: "approval_request", label: "승인 요청", locked: true },
  { kind: "stall", label: "멈춤", locked: true },
  { kind: "anomaly", label: "이상", locked: true },
  { kind: "access_lock", label: "접속 잠금", locked: false },
  { kind: "backup_failure", label: "백업 실패", locked: false },
  { kind: "daily_summary", label: "하루 정리", locked: false },
];

/** 기본 설정(모두 켜짐 · 조용한 시간 꺼짐 · 이 컴퓨터). `overrides`로 칸을 바꾼다. */
export function notifySettings(overrides: Partial<NotifySettings> = {}): NotifySettings {
  return {
    kinds: KINDS.map((row) => ({ ...row, push: true, messenger: true })),
    quiet: { enabled: false, start: "22:00", end: "07:00" },
    editable: true,
    ...overrides,
  };
}
