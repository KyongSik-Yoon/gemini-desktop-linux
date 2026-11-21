# gemini-desktop-linux 프로젝트 하네스

이 저장소는 Go 백엔드 데몬(`gemini-remote`, `cmd/`·`internal/`)과 Rust/iced 데스크톱 앱(`gemini-ui`, `ui/`)으로 구성된다. 새 기능을 추가하거나 리팩터링할 때 아래 성능·안정성 불변조건을 처음부터 기본 전제로 적용한다.

## 1. Go 백엔드 (`internal/localapi`, `internal/robin`, `internal/punctual`, `internal/scotty`)

1. **커넥션 풀·업로더 재사용**: 핸들러나 메서드마다 `&http.Client{}` 또는 `scotty.NewUploader()`를 새로 만들지 않는다. 반드시 `robin.Client`의 공용 풀(`getHTTPClient`, `getImageClient`, `getUploader`)을 사용한다.
2. **TTL 캐시 + singleflight 병합(취소 비전염)**: 부팅·새로고침·대화 전환 시 반복 호출되는 메타/프로필/기기/노트북/예약/라이브러리/아바타/원격 이미지 조회는 `ttlCallCache[T]`·`ttlCache[T]` 또는 `singleflightGroup[T]`(`internal/localapi/cache.go`, `internal/robin/client.go`)로 중복 RPC·원격 요청을 병합하되, 첫 호출자의 `ctx` 취소가 공용 실행과 다른 대기자에게 전염되지 않도록 분리된 실행 컨텍스트를 사용한다.
3. **SSE 증분 전송 (`O(N)`)**: `handleChatStream`의 스트리밍 청크는 순수 접두사 증가 구간에서 누적 본문 전체를 반복 전송하지 않고 `compactStreamChunk`로 델타만 전송한다(`done` 이벤트에서만 전체 최종본 전달).
4. **비블로킹 대화 전환**: `ListTurns` 등 인터랙티브 경로에서 무거운 미디어 인덱싱·원격 조회로 일반 대화를 동기 블로킹하지 않는다. stale-while-revalidate + 비동기 워밍업(`warmMediaCacheAsync`)을 유지한다.
5. **독립 I/O 병렬화**: 다중 파일 업로드 등 독립적인 네트워크 I/O는 제한된 동시성(예: 최대 4 워커)으로 병렬 실행하고 원본 순서를 보존한다.

## 2. Rust / iced 프론트엔드 (`ui/src/`)

1. **유휴 리드로우 0 원칙**: 상시 `time::every(...)` 구독을 금지한다. 타이머는 음성 녹음·토스트 등 실제 활성 상태에서만 구독하고, 파일/테마 감시는 실제 변경 시에만 이벤트를 방출하는 백그라운드 스트림(`watch_omarchy_theme`)으로 처리한다.
2. **전역 이벤트 필터링**: 무필터 `event::listen()` 사용을 금지한다. 반드시 `event::listen_with`로 필요한 이벤트(파일 드롭, 지정 단축키 등)만 통과시켜 마우스 이동·일반 타이핑 시 전체 `view()` 리빌드를 방지한다.
3. **`view()` 프레임 할당 억제**:
   - `AppTheme` 등 큰 구조체는 `&'a` 참조로 전달하고 프레임마다 `.clone()`하지 않는다.
   - 메시지·표·코드 블록·Canvas 복사 버튼에 원문 문자열(`raw_text`, `content`, `code`)을 직접 클론해 넣지 말고 인덱스 기반 메시지(`CopyChatItem`, `CopyCanvasPreview`, `CopyMarkdownTable`, `CopyMarkdownCode`)로 클릭 시점에 지연 조회한다.
   - `canvas::Program` 구현은 `canvas::Cache`를 받아 `self.cache.draw(...)`로 지오메트리를 캐시하고 테마/모드 변경 시에만 무효화한다.
   - 정적 SVG/아이콘 핸들은 `OnceLock`으로 1회 생성해 재사용한다.
4. **스트리밍 `O(delta)` + 쓰로틀링**:
   - `ParsedMarkdown::push_delta`의 끝 `Rich` 블록 증분 경로를 유지하고, 구조 변경 재파싱(80ms)과 자동 스크롤(60ms)을 쓰로틀링한다.
   - 스트림 완료(`Done`)·중단(`AbortStream`)·오류(`Error`) 시 지연된 `stream_markdown_dirty`를 즉시 최종 파싱(`flush_dirty_stream_markdown`)한다.
5. **지연 로드 및 IPC 위생**:
   - 라이브러리·이미지 탭 썸네일은 앱 부팅 직후 일괄 프리로드하지 않고 해당 섹션 진입 시에만 로드한다.
   - Unix 소켓 경로는 캐시(연결 실패 시 무효화)하고, SSE 스트림 파싱은 커서 기반 `LineBuffer`로 `drain` memmove를 피한다.

## 3. 보안 및 위생

- 기기 UUID, 호스트명, OAuth 토큰 등 민감 정보는 로그·문서·커밋 메시지에 노출하지 않는다.
- 와이어 hex 덤프(`hex=%x`) 등 원시 프로토콜 디버그 출력은 반드시 `GEMINI_DEBUG_WIRE=1` 환경변수 가드 뒤에만 둔다.
- 외부 URL 실행은 `http://`/`https://` 허용 목록 검증(`open_safe_external_url`)을 거친다.

## 4. 검증 및 배포 명령

- 정적 성능 회귀 가드(고속): `./scripts/perf-guard.sh`
- 전체 검증(가드 + Go vet/test -race + Rust test/clippy): `./scripts/perf-guard.sh --full`
- 로컬 빌드·설치 및 데몬 재시작: `./install.sh`
