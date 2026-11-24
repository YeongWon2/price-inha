// 누르기와 끌기 가르기(server 32차 5번): 원본 DeskRPG `pointer-gesture`와 같게 누른 뒤 포인터가 5px 이상 움직였으면
// 끌기(카메라 이동)이고 누르기(걷기 · 담당 누르기)로 치지 않는다(docs/research/metaverse-office-refs.md B절).

/** 이만큼(px) 이상 움직이면 끌기다. */
export const DRAG_THRESHOLD_PX = 5;

/** 포인터 하나의 누르기/끌기 판정. 누른 뒤 가장 멀리 간 거리로 가른다(돌아와도 끌기다). */
export class PointerGesture {
  private start: { x: number; y: number } | null = null;
  private far = 0;

  /** 눌렀다(화면 px). */
  down(x: number, y: number): void {
    this.start = { x, y };
    this.far = 0;
  }

  /** 움직였다. */
  move(x: number, y: number): void {
    if (!this.start) return;
    this.far = Math.max(this.far, Math.hypot(x - this.start.x, y - this.start.y));
  }

  /** 뗐다: 누르기면 `"tap"`, 끌기면 `"drag"`, 누르지 않았으면 `null`. */
  up(x: number, y: number): "tap" | "drag" | null {
    if (!this.start) return null;
    this.move(x, y);
    const kind = this.far >= DRAG_THRESHOLD_PX ? "drag" : "tap";
    this.start = null;
    this.far = 0;
    return kind;
  }

  /** 지금 누른 채 끌고 있는가(문턱을 넘었는가). */
  dragging(): boolean {
    return this.start !== null && this.far >= DRAG_THRESHOLD_PX;
  }
}
