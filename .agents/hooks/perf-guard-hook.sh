#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PAYLOAD="$(cat)"

python3 - "$REPO_ROOT" "$PAYLOAD" <<'PY'
import json
import subprocess
import sys

repo_root = sys.argv[1]
raw_payload = sys.argv[2]

try:
    data = json.loads(raw_payload) if raw_payload.strip() else {}
except json.JSONDecodeError:
    print("{}")
    sys.exit(0)

# 정상 종료(NO_TOOL_CALL)이고 Stop 재진입 루프가 아닐 때만 고속 정적 성능 가드 실행
if data.get("terminationReason") != "NO_TOOL_CALL" or data.get("continuationActive"):
    print("{}")
    sys.exit(0)

res = subprocess.run(
    [f"{repo_root}/scripts/perf-guard.sh"],
    capture_output=True,
    text=True,
    check=False,
)
if res.returncode == 0:
    print("{}")
    sys.exit(0)

msg = (res.stderr or res.stdout).strip()
print(
    json.dumps(
        {
            "decision": "continue",
            "reason": (
                "프로젝트 성능 가드(scripts/perf-guard.sh) 위반이 감지되었습니다. "
                "AGENTS.md 불변조건에 맞게 수정한 뒤 다시 완료하세요:\n" + msg
            ),
        },
        ensure_ascii=False,
    )
)
PY
