// 내 캐릭터의 3D 몸 · 이름표(server 32차 3번). 담당과 같은 방식으로 그린다: 원본 외형 GLB(actor.ts `makeActor` +
// gltf.ts), 부르는 동안 · 실패하면 코드 인형(doll.ts). 클립은 걸으면 walk, 아니면 idle(내 캐릭터는 앉지 않는다).
// 보는 쪽은 4방향(큰 축)이고 몸은 원본처럼 `1 - exp(-14·dt)`로 돈다. 이름표는 발 아래 녹색(`.tag.me` — player-tag.ts).

import * as THREE from "three";
import { CROSSFADE_SECONDS, clipName, makeActor } from "./actor";
import { makeDoll, poseDoll, removeDoll, type Doll } from "./doll";
import { facingYaw } from "./furniture";
import { instance, loadLook } from "./gltf";
import { TAG_FOOT_GAP } from "./layout";
import type { Point } from "./nav";
import type { Facing } from "./player";
import { makeNameTag } from "./player-tag";

type Obj = any;

/** 코드 인형 색을 고르는 id(담당 id가 아니면 회색). */
const DOLL_ID = "me";
/** 몸이 도는 빠르기(원본 `turnToward`). */
const TURN_RATE = 14;
/** 화면 가장자리 여백(px). */
const SCREEN_MARGIN = 8;

interface Body {
  model: Obj;
  mixer: Obj;
  actions: Map<string, Obj>;
  current: Obj;
}

/** 내 캐릭터의 몸. */
export interface PlayerBody {
  /** 이 프레임의 자리 · 보는 쪽 · 걷는 중을 그린다. */
  update(pos: Point, facing: Facing, walking: boolean, dt: number, t: number): void;
  /** 이름표를 발 아래에 놓는다(화면 밖이면 숨긴다). */
  placeTag(camera: Obj, width: number, height: number): void;
  setLook(look: string): void;
  setName(name: string): void;
  dispose(): void;
}

function angleDiff(a: number, b: number): number {
  let d = a - b;
  while (d > Math.PI) { d -= Math.PI * 2; }
  while (d < -Math.PI) { d += Math.PI * 2; }
  return d;
}

export function createPlayerBody(scene: Obj, labels: HTMLElement, name: string, look: string): PlayerBody {
  const root = new THREE.Group();
  root.userData.player = true;
  scene.add(root);
  let doll: Doll | null = makeDoll(root, DOLL_ID);
  let body: Body | null = null;
  let wanted = "";
  let disposed = false;
  let yaw: number | null = null;
  const tag = makeNameTag(document, name, true);
  labels.appendChild(tag);
  let tagSize: { width: number; height: number } | null = null;
  const projected = new THREE.Vector3();

  function dropBody(): void {
    if (!body) { return; }
    body.mixer.stopAllAction();
    body.mixer.uncacheRoot(body.model);
    body.model.parent?.remove(body.model);
    body = null;
  }

  function setLook(next: string): void {
    if (next === wanted) { return; }
    wanted = next;
    void makeActor(next, loadLook).then((made) => {
      if (disposed || wanted !== next) { return; }
      dropBody();
      if (made.kind === "doll") {
        if (!doll) { doll = makeDoll(root, DOLL_ID); }
        return;
      }
      const model = instance(made.model.scene, true);
      const mixer = new THREE.AnimationMixer(model);
      const actions = new Map<string, Obj>();
      for (const clip of (made.model.animations ?? []) as Obj[]) { actions.set(String(clip.name).toLowerCase(), mixer.clipAction(clip)); }
      root.add(model);
      if (doll) { removeDoll(doll); doll = null; }
      body = { model, mixer, actions, current: null };
    });
  }
  setLook(look);

  function animate(clip: string, dt: number): void {
    if (!body) { return; }
    const next = body.actions.get(clip) ?? body.actions.get("idle");
    if (next && next !== body.current) {
      next.reset().setEffectiveTimeScale(1).setEffectiveWeight(1).play();
      if (body.current) { next.crossFadeFrom(body.current, CROSSFADE_SECONDS, false); }
      body.current = next;
    }
    body.mixer.update(dt);
  }

  return {
    update(pos, facing, walking, dt, t) {
      const goal = facingYaw(facing);
      yaw = yaw === null ? goal : yaw + angleDiff(goal, yaw) * (1 - Math.exp(-TURN_RATE * Math.min(dt, 0.1)));
      root.position.set(pos.x, 0, pos.y);
      root.rotation.set(0, yaw, 0);
      const clip = clipName(walking, false);
      animate(clip, dt);
      if (doll) { poseDoll(doll, clip, "idle", t); }
    },
    placeTag(camera, width, height) {
      projected.set(root.position.x, 0, root.position.z).project(camera);
      const inside = projected.z < 1 && Math.abs(projected.x) <= 1 && Math.abs(projected.y) <= 1;
      if (!inside) { tag.style.display = "none"; return; }
      tag.style.display = "";
      if (!tagSize) { tagSize = { width: tag.offsetWidth, height: tag.offsetHeight }; }
      const footX = ((projected.x + 1) / 2) * width;
      const footY = ((1 - projected.y) / 2) * height;
      const maxX = Math.max(SCREEN_MARGIN, width - SCREEN_MARGIN - tagSize.width);
      const x = Math.min(maxX, Math.max(SCREEN_MARGIN, footX - tagSize.width / 2));
      tag.style.transform = `translate(${Math.round(x)}px, ${Math.round(footY + TAG_FOOT_GAP)}px)`;
    },
    setLook,
    setName(next) {
      if (tag.textContent === next) { return; }
      tag.textContent = next;
      tag.title = next;
      tagSize = null;
    },
    dispose() {
      disposed = true;
      dropBody();
      if (doll) { removeDoll(doll); doll = null; }
      root.parent?.remove(root);
      tag.remove();
    },
  };
}
