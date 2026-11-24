// 누르기와 끌기 가르기(server 32차 5번): 원본 DeskRPG `pointer-gesture`와 같게 누른 뒤 포인터가 5px 이상 움직였으면
// 끌기(카메라 이동)이고 누르기(걷기 · 담당 누르기)로 치지 않는다(docs/research/metaverse-office-refs.md B절).

/** 이만큼(px) 이상 움직이면 끌기다. */
export const DRAG_THRESHOLD_PX = 5;

/** 포인터 하나의 누르기/끌기 판정. 누른 뒤 가장 멀리 간 거리로 가른다(돌아와도 끌기다). */
export class PointerGesture {
  /** 눌렀다(화면 px). */
  down(x: number, y: number): void {
    void x;
    void y;
    throw new Error("todo");
  }

  /** 움직였다. */
  move(x: number, y: number): void {
    void x;
    void y;
    throw new Error("todo");
  }

  /** 뗐다: 누르기면 `"tap"`, 끌기면 `"drag"`, 누르지 않았으면 `null`. */
  up(x: number, y: number): "tap" | "drag" | null {
    void x;
    void y;
    throw new Error("todo");
  }

  /** 지금 누른 채 끌고 있는가(문턱을 넘었는가). */
  dragging(): boolean {
    throw new Error("todo");
  }
}
