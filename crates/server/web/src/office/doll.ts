// 코드 인형(server 20차 2번): 외형 GLB를 부르는 동안 · 부르지 못했을 때 그리는 몸(원본도 대신 몸을 둔다).
// 서기 · 걷기 · 앉기만 있다(누운 자세는 없다 — 20차 고침).

import * as THREE from "three";

type Obj = any;

/** 인형의 마디. */
export interface Doll { body: Obj; head: Obj; armL: Obj; armR: Obj; legL: Obj; legR: Obj }

const COLORS: Record<string, number> = {
  news: 0xd9822b, strategy: 0x3f7cc4, risk: 0xc0453a,
  review: 0x7b5bb5, trader: 0x2f9a6b, leader: 0x37474f,
};

/** 인형을 만들어 `parent` 아래에 붙인다. */
export function makeDoll(parent: Obj, agentId: string): Doll {
  const body = new THREE.Group();
  parent.add(body);
  const cloth = new THREE.MeshStandardMaterial({ color: COLORS[agentId] ?? 0x888888, roughness: 0.7 });
  const skin = new THREE.MeshStandardMaterial({ color: 0xf1c9a5, roughness: 0.8 });
  const dark = new THREE.MeshStandardMaterial({ color: 0x2d2a26, roughness: 0.9 });
  const torso = new THREE.Mesh(new THREE.CapsuleGeometry(0.17, 0.3, 4, 10), cloth);
  torso.position.y = 0.72;
  body.add(torso);
  const head = new THREE.Group();
  head.position.y = 1.08;
  const skull = new THREE.Mesh(new THREE.SphereGeometry(0.15, 16, 12), skin);
  const hair = new THREE.Mesh(new THREE.SphereGeometry(0.155, 16, 8, 0, Math.PI * 2, 0, Math.PI / 2), dark);
  hair.position.y = 0.02;
  head.add(skull, hair);
  body.add(head);
  const limb = (w: number, h: number, mat: Obj, x: number, y: number): Obj => {
    const pivot = new THREE.Group();
    pivot.position.set(x, y, 0);
    const mesh = new THREE.Mesh(new THREE.BoxGeometry(w, h, w), mat);
    mesh.position.y = -h / 2;
    pivot.add(mesh);
    body.add(pivot);
    return pivot;
  };
  const doll = {
    body, head,
    armL: limb(0.08, 0.36, cloth, -0.23, 0.9),
    armR: limb(0.08, 0.36, cloth, 0.23, 0.9),
    legL: limb(0.1, 0.44, dark, -0.09, 0.46),
    legR: limb(0.1, 0.44, dark, 0.09, 0.46),
  };
  body.scale.setScalar(1.45); // 원본 외형(키 약 1.8m)과 비슷한 키로
  body.traverse((o: Obj) => { if (o.isMesh) { o.castShadow = true; } });
  return doll;
}

/** 인형을 치운다. */
export function removeDoll(doll: Doll): void {
  doll.body.parent?.remove(doll.body);
  doll.body.traverse((o: Obj) => {
    if (o.isMesh) { o.geometry.dispose(); o.material.dispose(); }
  });
}

/** 인형 자세: 클립 이름(idle · walk · sit)과 일(생각 중 · 실패)로. `k`는 담당마다 어긋난 시각. */
export function poseDoll(p: Doll, clip: string, work: string, k: number): void {
  p.body.position.set(0, 0, 0);
  p.head.rotation.set(0, 0, 0);
  for (const limb of [p.armL, p.armR, p.legL, p.legR]) { limb.rotation.set(0, 0, 0); }
  if (clip === "walk") {
    const s = Math.sin(k * 9);
    p.armL.rotation.x = s * 0.55;
    p.armR.rotation.x = -s * 0.55;
    p.legL.rotation.x = -s * 0.5;
    p.legR.rotation.x = s * 0.5;
  } else if (clip === "sit") {
    p.body.position.y = -0.06;
    p.legL.rotation.x = -Math.PI / 2;
    p.legR.rotation.x = -Math.PI / 2;
    p.armL.rotation.x = -0.5;
    p.armR.rotation.x = -0.5;
  }
  if (clip !== "walk") {
    if (work === "thinking") {
      p.armR.rotation.x = -2.5;
      p.head.rotation.x = 0.25;
    } else if (work === "failed") {
      p.head.rotation.x = 0.45;
    } else {
      p.head.rotation.y = Math.sin(k * (Math.PI * 2 / 13)) * 0.18;
      p.head.rotation.x = Math.sin(k * (Math.PI * 2 / 17)) * 0.05;
    }
  }
  p.body.scale.y = 1.45 * (1 + Math.sin(k * 2.3) * 0.012);
}
