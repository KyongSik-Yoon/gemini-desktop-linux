# Upstream 변화 대응 및 동기화 절차서 (`UPSTREAM_SYNC.md`)

Google 서버(Robin / Punctual / Scotty)와 원본 Gemini 데스크톱·웹 UI는 예고 없이 프로토콜 필드, 클라이언트 버전 정책, 화면 구성을 바꿀 수 있습니다. 이 문서는 **동작하던 기능이 깨졌을 때 빠르게 원인을 특정하고, 새 UI/기능을 안전하게 따라가기 위한 표준 운영 절차**를 정의합니다.

> **보안 원칙**: 이 저장소는 공개 저장소입니다. 기기 UUID, 호스트명, OAuth 토큰·Client Secret, 내부 역공학 오프셋·비공개 심볼은 절대로 Git 문서·코드·테스트·로그에 커밋하지 않습니다.

---

## 1. 계층 격리 원칙 (충격 흡수 구조)

변경 파급을 최소화하기 위해 모든 수정은 아래 3계층 경계를 지킵니다.

1. **프로토콜 어댑터 계층 (`internal/proto`, `internal/robin`, `internal/punctual`)**
   - Google Protobuf 확장 번호, 필드 번호, 메타데이터(`app_version`, `client_type`) 변화는 **전부 Go 계층에서 흡수**합니다.
   - CLI 원격 제어 데몬(`gemini-remote`)은 Rust GUI(`gemini-ui`)와 완전히 독립된 바이너리이므로 UI 리팩터링이 폰–PC 원격 연동에 영향을 주지 않습니다.
2. **안정 IPC 계약 계층 (`internal/localapi` ↔ `ui/src/ipc.rs`)**
   - JSON 필드는 추가 시 `omitempty`(Go) + `#[serde(default)]`(Rust)로 하위 호환을 유지합니다.
   - 필수 계약은 `internal/localapi/contract_test.go`와 `ui/src/ipc.rs` 단위 테스트로 상호 검증합니다.
3. **모듈화된 GUI 계층 (`ui/src/`)**
   - 화면 영역별로 분리된 뷰 모듈(`sidebar.rs`, `chat_view.rs`, `modals.rs`, `popovers.rs`, `pages.rs`, `markdown_view.rs`)만 국소 수정해 회귀 반경을 줄입니다.

---

## 2. 장애 발생 시 1분 진단 플로우

동작하던 기능(채팅, 원격 제어, 대화 목록, 예약 등)이 갑자기 실패하면 코드부터 추측하지 않고 아래 순서로 원인을 분리합니다.

### Step 1. `gemini-remote doctor` 실행
```bash
gemini-remote doctor
# 기계 판독용 JSON 출력
gemini-remote doctor --json
```

진단 결과로 장애 계층을 즉시 분리합니다.

| 실패 항목 | 추정 원인 | 1차 조치 |
| :--- | :--- | :--- |
| `oauth_token` | 토큰 만료·철회 또는 자격증명 누락 | `gemini-remote login` 또는 `extract-keys` 재실행 |
| `robin_user_status` / `INVALID_ARGUMENT` | `app_version` 또는 `RequestMetadata` 필드 변경 | Upstream Diff 체크리스트 3.1 확인 |
| `robin_conversations` / `robin_devices` | 특정 RPC 응답 스키마 또는 필드 번호 변경 | Step 2 와이어 디버그로 미지 태그 확인 |
| `punctual_choose_server` | Signaler 엔드포인트/헤더/네임스페이스 변경 | `internal/punctual/watcher.go` 점검 |
| `local_daemon_socket` | `gemini-remote.service` 중지 또는 소켓 권한 오류 | `systemctl --user status gemini-remote.service` 확인 |

### Step 2. 미지 프로토콜 필드 포착 (`GEMINI_DEBUG_WIRE=1`)
서버가 새 확장 번호나 필드를 내리기 시작했는지 확인하려면 일시적으로 와이어 디버그를 켭니다.

```bash
GEMINI_DEBUG_WIRE=1 gemini-remote ask --temp "ping"
```

- `[proto][unknown] ... 미처리 필드 감지: #1042(bytes,24B)` 형태로 **민감 본문 없이 번호·타입·크기만** 표준 오류로 출력됩니다.
- 디버그 출력은 반드시 `GEMINI_DEBUG_WIRE=1` 가드 뒤에만 유지하며, 상시 로그에 원시 hex를 남기지 않습니다(`scripts/perf-guard.sh` `GO-04` 가드).

---

## 3. 신버전 앱/UI 추적 체크리스트

새 버전 Gemini 앱이 출시되었거나 새 기능을 이식할 때 아래 4단계를 순서대로 수행합니다.

1. **메타데이터·인증 변화 확인**
   - `app_version` 문자열, OAuth 요청 스코프, 필수 `RequestMetadata` 변경 여부 확인.
2. **프로토콜 스키마 Diff 및 골든 테스트 추가**
   - 신규/변경 RPC 메서드, Proto2 확장 번호, 기능 모드 코드(`FEATURE_MODE_*`), 도구 선언 스키마 변경을 `internal/proto`에 반영.
   - 민감 정보를 제거한 합성/익명 와이어 픽스처를 `internal/proto/*_test.go`에 추가해 파서 회귀를 고정.
3. **IPC 계약 동기화**
   - 새 데이터가 UI에 필요하면 `internal/localapi` JSON 응답과 `ui/src/ipc.rs` 구조체에 `omitempty` + `#[serde(default)]`로 추가.
   - `internal/localapi/contract_test.go` 및 `ui/src/ipc.rs` 테스트에 새 필드 검증 추가.
4. **UI 모듈 국소 반영**
   - 변경 대상 화면 파일(`sidebar.rs`, `chat_view.rs`, `modals.rs`, `popovers.rs`, `pages.rs`)만 수정하고 `AGENTS.md`의 유휴 리드로우 0·프레임 할당 억제 원칙을 유지.

---

## 4. 배포 전 필수 게이트

모든 프로토콜·UI 동기화 작업은 아래 검증 통과 후에만 로컬 설치 및 커밋을 진행합니다.

```bash
# 1) 정적 성능 회귀 가드(10개) + Go vet/test -race + Rust test/clippy
./scripts/perf-guard.sh --full

# 2) 로컬 릴리스 빌드·설치 및 데몬 재시작
./install.sh

# 3) 실서버 연결 자가 진단
gemini-remote doctor
```
