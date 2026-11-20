# gemini-remote-linux

[English](./README.md)

**리눅스 PC 및 워크스테이션을 Gemini 원격 실행 환경으로 연결**해 주는 비공식 Linux 데스크톱 데몬(Go)·CLI 및 네이티브 데스크톱 GUI(`gemini-ui`, Rust)입니다.

Ubuntu, Arch, Fedora, Wayland/Hyprland, X11 환경에서 스마트폰·웹 브라우저·리눅스 네이티브 데스크톱 앱을 통해 로컬 폴더 접근, 터미널 명령 실행, 파일 편집, MCP 서버 연동, GUI 화면 제어(Computer Use)까지 통합으로 사용할 수 있습니다.

> **안내**: 비공식 커뮤니티 클라이언트입니다. 저장소에는 어떠한 OAuth 시크릿·토큰·독점 에셋도 포함되지 않으며, 사용자가 직접 OAuth 자격증명을 설정하고 Google 계정으로 로그인해야 합니다.

## 주요 기능

- **네이티브 Linux 데스크톱 GUI (`ui/`, Rust + `iced 0.14`)**
  - **채팅 & 태스크**: 마크다운·사고 과정 스트리밍, 채팅/태스크 전환 및 원격 PC 지정 실행, 임시 채팅, 모델 선택기 및 사고 수준(`낮음`/`보통`/`높음`), `+` 도구 팝오버, `@` 커넥터·MCP 멘션
  - **워크스페이스 페이지**: 라이브러리(Canvas 문서·생성 이미지 썸네일), 이미지 스튜디오, 예약된 작업(서버·로컬·추천 예약), 학생 학습 도우미, 노트북, Gems 허브, 동영상 스튜디오, 맞춤설정
  - **테마 & 다국어**: 시스템 라이트/다크 테마 자동 추종 및 수동 전환, **한국어 / English 다국어 지원** (시스템 로케일 자동 감지 + 설정 모달에서 즉시 전환)
  - **이 PC 원격 제어**: 원격 제어 켜기/끄기, 허용 폴더 추가·삭제, 연동 MCP 서버 확인, 실시간 원격 도구 실행 모니터링
- **원격 데스크톱 데몬 & CLI (`gemini-remote`, Go)**
  - 기기 등록, 온라인 상태 보고, 양방향 스트리밍 원격 도구 실행 및 멀티턴 세션 자동 갱신, 로컬 Unix 소켓 IPC(`0600` 권한)
  - **셸 / 코드**: `run_command`, `command_status`, `send_command_input`, `run_python`
  - **파일 시스템 / 탐색**: `read_file`, `view_file`, `write_file`, `replace_file_content`, `multi_replace_file_content`, `list_directory`, `find_by_name`, `grep_search`, `find_folders`, `connect_folders`
  - **스킬 / MCP**: `list_skills`, `ReadSkill`, `list_mcp`, `execute_mcp` (내장 `git`, `obsidian` 및 외부 stdio MCP 서버 브리지)
  - **GUI / Computer Use**: `ui_control` (**Wayland / Hyprland** `hyprctl`, `grim`, `wtype`, `ydotool` 및 **X11** `xdotool`, `import` 지원)

## 요구 사항

- **데몬**: Go 1.22+
- **데스크톱 GUI (`ui/`)**: Rust 1.85+ (`edition = "2024"`)
- **선택적 런타임 도구** (GUI 부가 기능용):
  - 네이티브 파일/폴더 선택기: `xdg-desktop-portal`
  - 화면 캡처: Wayland `slurp` + `grim`, X11 `import` 또는 `scrot`
  - 음성 녹음: `pw-record` (PipeWire) 또는 `arecord` (ALSA)
  - 한글 렌더링: `Noto Sans CJK KR` (또는 시스템 CJK 폰트)

## 빌드 및 실행

```bash
# 1. Go 데몬 빌드
go build -o gemini-remote ./cmd/gemini-remote

# 2. OAuth 클라이언트 자격증명 설정 (기존 Gemini 데스크톱 앱 경로 지정 또는 ~/.config/gemini-remote/config.json 설정)
./gemini-remote extract-keys /path/to/Gemini.app

# 3. Google 계정 OAuth 2.0 로그인
./gemini-remote login

# 4. 허용할 로컬 작업 폴더 추가
./gemini-remote folders add ~/dev/git-repo
./gemini-remote folders add ~/Downloads

# 5. 기기 등록 및 데몬 실행
./gemini-remote register
./gemini-remote service install   # 또는 포그라운드 실행: ./gemini-remote serve

# 6. 네이티브 Rust GUI 빌드 및 실행
cargo build --release --manifest-path ui/Cargo.toml
./ui/target/release/gemini-ui
```

## 라이선스

[MIT](LICENSE)
