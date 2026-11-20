#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RUN_FULL=0
if [[ "${1:-}" == "--full" ]]; then
  RUN_FULL=1
fi

violations=0

report_violation() {
  local rule="$1"
  local msg="$2"
  local detail="${3:-}"
  echo "[FAIL][$rule] $msg" >&2
  if [[ -n "$detail" ]]; then
    echo "$detail" | sed 's/^/  /' >&2
  fi
  violations=$((violations + 1))
}

# [UI-01] 무필터 event::listen() 금지
if hits="$(rg -n '\bevent::listen\s*\(' ui/src/ 2>/dev/null || true)" && [[ -n "$hits" ]]; then
  report_violation "UI-01" "무필터 event::listen() 사용 금지 — event::listen_with로 필요한 이벤트만 필터링하세요." "$hits"
fi

# [UI-02] view() 내 대형 본문 문자열 클론 복사 금지
if hits="$(rg -n 'Message::CopyText\([^)]*(\.raw_text|\.content)\.clone\(\)' ui/src/ 2>/dev/null || true)" && [[ -n "$hits" ]]; then
  report_violation "UI-02" "CopyText에 raw_text/content 직접 clone 금지 — 인덱스 기반 지연 복사 메시지를 사용하세요." "$hits"
fi

# [UI-03] ui/src/glow.rs의 모든 canvas::Program 구현이 self.cache.draw 사용
if [[ -f ui/src/glow.rs ]]; then
  prog_cnt="$(rg -c 'impl<.*>\s+canvas::Program' ui/src/glow.rs 2>/dev/null || echo 0)"
  cache_cnt="$(rg -c 'self\.cache\.draw\(' ui/src/glow.rs 2>/dev/null || echo 0)"
  if [[ "$prog_cnt" -gt "$cache_cnt" ]]; then
    report_violation "UI-03" "ui/src/glow.rs에 canvas::Cache 미적용 canvas::Program이 있습니다 (Program=${prog_cnt}, cache.draw=${cache_cnt})."
  fi
fi

# [UI-04] ui/src/icons.rs에서 OnceLock 캐시 외 svg::Handle::from_memory 직접 호출 증가 방지
if [[ -f ui/src/icons.rs ]]; then
  svg_mem_cnt="$(rg -c 'svg::Handle::from_memory\(' ui/src/icons.rs 2>/dev/null || echo 0)"
  if [[ "$svg_mem_cnt" -gt 2 ]]; then
    hits="$(rg -n 'svg::Handle::from_memory\(' ui/src/icons.rs 2>/dev/null || true)"
    report_violation "UI-04" "ui/src/icons.rs에 정적 캐시 외 svg::Handle::from_memory 호출이 늘었습니다 (허용 2건, 현재 ${svg_mem_cnt}건)." "$hits"
  fi
fi

# [UI-05] markdown_view::view 팔레트 참조 전달 유지
if [[ -f ui/src/markdown_view.rs ]]; then
  if ! rg -q 'palette:\s*&'\''a\s+AppTheme' ui/src/markdown_view.rs; then
    report_violation "UI-05" "ui/src/markdown_view.rs의 view()는 palette: &'a AppTheme 참조 전달을 유지해야 합니다."
  fi
fi

# [UI-06] view() 경로 내 self.palette.clone() 프레임 할당 금지
if hits="$(rg -n 'self\.palette\.clone\(\)' ui/src/ 2>/dev/null || true)" && [[ -n "$hits" ]]; then
  report_violation "UI-06" "self.palette.clone() 금지 — 필요한 Color 필드만 복사하거나 AppTheme::subtle_scrollable_style_with_fg를 사용하세요." "$hits"
fi

# [GO-01] internal/ 전역에서 공용 클라이언트 생성자 외 임시 &http.Client{} 생성 금지
if hits="$(rg -n '&http\.Client\{' internal/ --glob '!client.go' --glob '!watcher.go' --glob '!executor.go' --glob '!upload.go' --glob '!oauth.go' --glob '!*_test.go' 2>/dev/null || true)" && [[ -n "$hits" ]]; then
  report_violation "GO-01" "핸들러/메서드 내 임시 &http.Client{} 생성 금지 — 공용 http.Client 풀을 재사용하세요." "$hits"
fi

# [GO-02] internal/robin·internal/localapi에서 client.go 외 scotty.NewUploader() 직접 생성 금지
if hits="$(rg -n 'scotty\.NewUploader\(' internal/robin/ internal/localapi/ --glob '!client.go' --glob '!*_test.go' 2>/dev/null || true)" && [[ -n "$hits" ]]; then
  report_violation "GO-02" "임시 scotty.NewUploader() 생성 금지 — robin.Client.getUploader()를 재사용하세요." "$hits"
fi

# [GO-03] SSE 경량 증분 전송(compactStreamChunk) 유지
if [[ -f internal/localapi/server.go ]]; then
  if ! rg -q 'compactStreamChunk\(' internal/localapi/server.go; then
    report_violation "GO-03" "internal/localapi/server.go에 SSE 증분 전송(compactStreamChunk)이 누락되었습니다."
  fi
fi

# [GO-04] 와이어 hex 덤프 로그(hex=%x)는 GEMINI_DEBUG_WIRE 환경변수 가드 필수
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if ! rg -q 'GEMINI_DEBUG_WIRE|debugWire' "$f"; then
    report_violation "GO-04" "${f}에 GEMINI_DEBUG_WIRE 가드 없는 hex=%x 와이어 로그가 있습니다."
  fi
done < <(rg -l 'hex=%x' internal/ --glob '!*_test.go' 2>/dev/null || true)

if [[ "$violations" -gt 0 ]]; then
  echo "성능 가드 위반 ${violations}건이 감지되었습니다." >&2
  exit 1
fi

echo "[OK] 정적 성능 회귀 가드 10개 항목 통과"

if [[ "$RUN_FULL" -eq 1 ]]; then
  echo "[1/4] go vet ./..."
  go vet ./...
  echo "[2/4] go test -race ./..."
  go test -race ./...
  echo "[3/4] cargo test"
  cargo test --manifest-path ui/Cargo.toml
  echo "[4/4] cargo clippy --all-targets -- -D warnings"
  cargo clippy --manifest-path ui/Cargo.toml --all-targets -- -D warnings
  echo "[OK] 전체 검증 통과"
fi
