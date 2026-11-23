// 지도 격자와 길 찾기. 원본 DeskRPG의 동작을 따른다(docs/research/office-deep.md 2-3절):
// A* 8방향, 대각선은 양옆 두 직교 칸이 모두 걸을 수 있을 때만, 비용 직선 1 · 대각선 √2,
// 휴리스틱 유클리드, 탐색 한도 4096회, 몸통 반지름 0.22칸을 지키며 꺾이는 점만 남긴다.

export const SEARCH_LIMIT = 4096;
export const ACTOR_RADIUS = 0.22;

/** 칸 사각형(벽 · 가구). */
export interface Rect { x: number; y: number; w: number; h: number; blocks?: boolean }
/** 지도 정의 중 길 찾기에 쓰는 부분. */
export interface NavMap { cols: number; rows: number; walls?: Rect[]; furniture?: Rect[] }
/** 점(칸 또는 실수 좌표). */
export interface Point { x: number; y: number }
/** 걸을 수 있는 칸의 표. */
export interface Grid { cols: number; rows: number; walkable(x: number, y: number): boolean }

interface Node { x: number; y: number; g: number; f: number }

/** 지도 정의에서 걸을 수 없는 칸의 표를 만든다. */
export function buildGrid(map: NavMap): Grid {
  const cols = map.cols;
  const rows = map.rows;
  const blocked = new Uint8Array(cols * rows);
  const mark = (r: Rect) => {
    for (let y = r.y; y < r.y + r.h; y++) {
      for (let x = r.x; x < r.x + r.w; x++) {
        if (x >= 0 && y >= 0 && x < cols && y < rows) { blocked[y * cols + x] = 1; }
      }
    }
  };
  for (const w of map.walls || []) { mark(w); }
  for (const f of map.furniture || []) { if (f.blocks) { mark(f); } }
  return {
    cols,
    rows,
    walkable(x: number, y: number) {
      return x >= 0 && y >= 0 && x < cols && y < rows && blocked[y * cols + x] === 0;
    },
  };
}

class Heap {
  items: Node[] = [];
  get size() { return this.items.length; }
  push(node: Node) {
    const a = this.items;
    a.push(node);
    let i = a.length - 1;
    while (i > 0) {
      const p = (i - 1) >> 1;
      if (a[p].f <= a[i].f) { break; }
      [a[p], a[i]] = [a[i], a[p]];
      i = p;
    }
  }
  pop(): Node {
    const a = this.items;
    const top = a[0];
    const last = a.pop() as Node;
    if (a.length > 0) {
      a[0] = last;
      let i = 0;
      for (;;) {
        const l = i * 2 + 1;
        const r = l + 1;
        let m = i;
        if (l < a.length && a[l].f < a[m].f) { m = l; }
        if (r < a.length && a[r].f < a[m].f) { m = r; }
        if (m === i) { break; }
        [a[m], a[i]] = [a[i], a[m]];
        i = m;
      }
    }
    return top;
  }
}

const STEPS: [number, number, number][] = [
  [1, 0, 1], [-1, 0, 1], [0, 1, 1], [0, -1, 1],
  [1, 1, Math.SQRT2], [1, -1, Math.SQRT2], [-1, 1, Math.SQRT2], [-1, -1, Math.SQRT2],
];

/**
 * 칸 `from`에서 칸 `to`로 가는 경로(칸 중심 좌표 목록, 실수)를 찾는다. 못 찾으면 null.
 * 출발 칸은 언제나 걸을 수 있는 것으로 친다. 도착 칸이 막혔으면 실패.
 */
export function findPath(grid: Grid, from: Point, to: Point): Point[] | null {
  if (!grid.walkable(to.x, to.y)) { return null; }
  const key = (x: number, y: number) => y * grid.cols + x;
  const h = (x: number, y: number) => Math.hypot(to.x - x, to.y - y);
  const open = new Heap();
  const best = new Map<number, number>();
  const parent = new Map<number, number>();
  const start = key(from.x, from.y);
  best.set(start, 0);
  open.push({ x: from.x, y: from.y, g: 0, f: h(from.x, from.y) });
  let expanded = 0;
  while (open.size > 0) {
    const cur = open.pop();
    const ck = key(cur.x, cur.y);
    if (cur.g > (best.get(ck) ?? Infinity)) { continue; }
    if (cur.x === to.x && cur.y === to.y) {
      const cells: Point[] = [];
      let k: number | undefined = ck;
      while (k !== undefined) {
        cells.push({ x: k % grid.cols, y: Math.floor(k / grid.cols) });
        k = parent.get(k);
      }
      cells.reverse();
      return smooth(grid, cells.map((c) => ({ x: c.x + 0.5, y: c.y + 0.5 })));
    }
    if (++expanded > SEARCH_LIMIT) { return null; }
    for (const [dx, dy, cost] of STEPS) {
      const nx = cur.x + dx;
      const ny = cur.y + dy;
      if (!grid.walkable(nx, ny)) { continue; }
      if (dx !== 0 && dy !== 0 && !(grid.walkable(cur.x + dx, cur.y) && grid.walkable(cur.x, cur.y + dy))) { continue; }
      const nk = key(nx, ny);
      const g = cur.g + cost;
      if (g < (best.get(nk) ?? Infinity)) {
        best.set(nk, g);
        parent.set(nk, ck);
        open.push({ x: nx, y: ny, g, f: g + h(nx, ny) });
      }
    }
  }
  return null;
}

/** 두 점 사이를 반지름 0.22칸의 몸이 벽에 닿지 않고 곧게 갈 수 있는가. */
export function clearLine(grid: Grid, a: Point, b: Point): boolean {
  const dist = Math.hypot(b.x - a.x, b.y - a.y);
  const steps = Math.max(1, Math.ceil(dist / 0.1));
  const r = ACTOR_RADIUS;
  for (let i = 0; i <= steps; i++) {
    const t = i / steps;
    const x = a.x + (b.x - a.x) * t;
    const y = a.y + (b.y - a.y) * t;
    for (const [ox, oy] of [[-r, -r], [r, -r], [-r, r], [r, r]]) {
      if (!grid.walkable(Math.floor(x + ox), Math.floor(y + oy))) { return false; }
    }
  }
  return true;
}

/** 곧게 갈 수 있는 가장 먼 점으로 건너뛰어 꺾이는 점만 남긴다. */
export function smooth(grid: Grid, points: Point[]): Point[] {
  if (points.length <= 2) { return points; }
  const out = [points[0]];
  let i = 0;
  while (i < points.length - 1) {
    let j = points.length - 1;
    while (j > i + 1 && !clearLine(grid, points[i], points[j])) { j--; }
    out.push(points[j]);
    i = j;
  }
  return out;
}
