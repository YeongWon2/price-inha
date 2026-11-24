// three는 npm 판(package.json에 고정)을 묶는다. 형 정의(@types/three)는 의존을 더하지 않으려고 쓰지 않는다
// (server 11차 1번: 그 밖의 의존은 더하지 않는다) — 장면 코드에서 three의 값은 형 검사 밖(any)이다.
declare module "three";
declare module "three/examples/jsm/loaders/GLTFLoader.js";
declare module "three/examples/jsm/utils/SkeletonUtils.js";
