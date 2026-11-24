// GLB 불러오기(server 20차 1 · 2번): 같은 출처 경로의 원본 DeskRPG 자산을 three GLTFLoader로 부른다.
// - 원본처럼 주소마다 한 번만 부르고(약속을 나눠 쓴다) 쓸 때마다 복제한다(뼈대가 있으면 SkeletonUtils.clone).
// - GLB 안의 그림은 `blob:` 주소가 된다. 원본 GLTFLoader는 ImageBitmapLoader로 `fetch(blob:)`를 하는데 이것은
//   화면 정책의 connect-src를 넓혀야 한다. 대신 그림 요소(TextureLoader)로 풀어 `img-src blob:`만으로 되게 한다.

import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { clone as cloneSkinned } from "three/examples/jsm/utils/SkeletonUtils.js";
import type { LoadedLook } from "./actor";

/** three의 값(형 정의 없이 묶는다 — three.d.ts). */
type Obj = any;

/** 불러온 GLB(장면 · 클립). */
export interface Gltf { scene: Obj; animations: Obj[] }

const cache = new Map<string, Promise<Gltf>>();
let loader: Obj = null;

function gltfLoader(): Obj {
  if (loader) { return loader; }
  loader = new GLTFLoader();
  // 그림은 그림 요소로 푼다(위 설명). 플러그인은 해석기가 만들어진 바로 뒤에 불린다.
  loader.register((parser: Obj) => {
    parser.textureLoader = new THREE.TextureLoader(parser.options.manager);
    return { name: "price-inha-image-element" };
  });
  return loader;
}

/** 주소의 GLB를 부른다(한 번만). 실패하면 다음에 다시 부를 수 있게 기억을 지운다. */
export function loadGltf(url: string): Promise<Gltf> {
  const found = cache.get(url);
  if (found) { return found; }
  const pending = (gltfLoader().loadAsync(url) as Promise<Gltf>).catch((error: unknown) => {
    cache.delete(url);
    throw error;
  });
  cache.set(url, pending);
  return pending;
}

/** 외형 불러오기(actor.ts의 `LookLoader`): 클립 이름을 함께 돌려준다. */
export async function loadLook(url: string): Promise<LoadedLook> {
  const gltf = await loadGltf(url);
  return {
    clips: gltf.animations.map((clip: Obj) => String(clip.name).toLowerCase()),
    scene: gltf.scene,
    animations: gltf.animations,
  };
}

/** 한 번 쓸 복제본. 뼈대가 있는 몸은 SkeletonUtils로, 가구는 그냥 복제한다(형상 · 재질은 나눠 쓴다). */
export function instance(source: Obj, skinned: boolean): Obj {
  const copy = skinned ? cloneSkinned(source) : source.clone(true);
  copy.traverse((node: Obj) => {
    if (node.isMesh) {
      node.castShadow = true;
      node.receiveShadow = true;
    }
  });
  return copy;
}

/** 불러온 모든 GLB의 형상 · 재질 · 그림을 치운다(장면을 닫을 때). */
export function disposeLoaded(): void {
  for (const pending of cache.values()) {
    pending.then((gltf) => {
      gltf.scene.traverse((node: Obj) => {
        if (!node.isMesh) { return; }
        node.geometry?.dispose();
        const mats = Array.isArray(node.material) ? node.material : [node.material];
        for (const mat of mats) {
          for (const value of Object.values(mat ?? {})) { if ((value as Obj)?.isTexture) { (value as Obj).dispose(); } }
          mat?.dispose();
        }
      });
    }, () => {});
  }
  cache.clear();
}
