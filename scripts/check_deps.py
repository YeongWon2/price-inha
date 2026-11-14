#!/usr/bin/env python3
"""의존 방향 검사. docs/dev/pipeline.md 6장의 표와 같아야 한다."""
import json
import subprocess
import sys

ALLOWED = {
    "pi-store": set(),
    "pi-secrets": set(),
    "pi-ai": {"pi-secrets"},
    "pi-market": {"pi-secrets"},
    "pi-gate": {"pi-store"},
    "price-inha": {"pi-store", "pi-secrets", "pi-ai", "pi-market", "pi-gate"},
}

meta = json.loads(
    subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"])
)
names = {p["name"] for p in meta["packages"]}
bad = []
for p in meta["packages"]:
    if p["name"] not in ALLOWED:
        bad.append(f'{p["name"]}: 표에 없는 모듈')
        continue
    for d in p["dependencies"]:
        # 테스트용 의존은 방향 검사에서 뺀다 (실행 코드의 방향만 본다).
        if d["name"] in names and d.get("kind") != "dev" and d["name"] not in ALLOWED[p["name"]]:
            bad.append(f'{p["name"]} → {d["name"]}: 허용되지 않은 방향')
for b in bad:
    print(b)
sys.exit(1 if bad else 0)
