// 카메라 따라가기(server 32차 6번): 기본은 내 캐릭터를 부드럽게 따라간다. 원본 DeskRPG와 같게 매 프레임 보는 점을
// (캐릭터 - 보는 점) × 0.08만큼 옮기고(카메라도 같은 만큼 — 거리 · 각도 그대로), 사용자가 끌기 · 돌리기 · 확대 ·
// 각도 바꾸기를 하면 따라가기가 풀린다. 도구 줄의 "따라가기"로 다시 켠다(docs/research/metaverse-office-refs.md B절).
// 따라가기가 켜져 있으면 회의 발언자 카메라는 끼어들지 않는다(`SpeakerCamera.setFollowing`). 장면 없이 시험하는 순수 모듈.

/** 한 프레임에 보는 점이 캐릭터 쪽으로 다가가는 몫(원본 0.08). */
export const FOLLOW_LERP = 0.08;

/** 따라가기를 푸는 사용자 조작. 끌기 · 돌리기(끌기 · 단추) · 확대 · 축소 · 각도(입체 · 위에서) · 전체 보기. */
export type CameraTouch = "drag" | "rotate" | "zoom" | "angle" | "overview";

/** x · z 평면의 점(세계 좌표). */
export interface Flat {
  x: number;
  z: number;
}

/** 따라가기 켜짐 · 꺼짐 상태. 기본 켜짐. */
export class FollowState {
  private following = true;
  private listeners = new Set<(on: boolean) => void>();

  /** 따라가는 중인가. */
  on(): boolean {
    return this.following;
  }

  /** 사용자가 카메라를 손으로 움직였다: 따라가기를 푼다. */
  touch(kind: CameraTouch): void {
    void kind;
    this.set(false);
  }

  /** "따라가기" 단추: 다시 켠다. */
  resume(): void {
    this.set(true);
  }

  /** 켜짐 · 꺼짐이 바뀔 때 부른다(도구 줄 단추 · 발언자 카메라에 알린다). 끊는 함수를 돌려준다. */
  onChange(listener: (on: boolean) => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private set(on: boolean): void {
    if (on === this.following) return;
    this.following = on;
    for (const listener of [...this.listeners]) listener(on);
  }
}

/** 한 프레임의 따라가기: 보는 점을 캐릭터 쪽으로 `factor`만큼 옮긴 새 점. */
export function followStep(target: Flat, player: Flat, factor: number = FOLLOW_LERP): Flat {
  return { x: target.x + (player.x - target.x) * factor, z: target.z + (player.z - target.z) * factor };
}
