use std::env;

/// 사용자가 선택한 UI 언어 설정 (`system`, `ko`, `en`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LanguagePref {
    #[default]
    System,
    Ko,
    En,
}

/// 실제 렌더링에 적용되는 활성 로케일.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    Ko,
    En,
}

impl LanguagePref {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Ko => "ko",
            Self::En => "en",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "ko" | "ko-kr" | "ko_kr" => Self::Ko,
            "en" | "en-us" | "en_us" => Self::En,
            _ => Self::System,
        }
    }

    pub fn resolve(self) -> Locale {
        match self {
            Self::Ko => Locale::Ko,
            Self::En => Locale::En,
            Self::System => Locale::from_env(),
        }
    }

    pub fn label(self, locale: Locale) -> &'static str {
        match self {
            Self::System => t(locale, "시스템"),
            Self::Ko => "한국어",
            Self::En => "English",
        }
    }
}

impl Locale {
    /// 환경변수(`LC_ALL`, `LC_MESSAGES`, `LANG`)에서 시스템 로케일을 판별한다.
    pub fn from_env() -> Self {
        for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = env::var(key) {
                let trimmed = val.trim();
                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("C") || trimmed.eq_ignore_ascii_case("POSIX") {
                    continue;
                }
                return Self::from_lang_tag(trimmed);
            }
        }
        Self::En
    }

    pub fn from_lang_tag(tag: &str) -> Self {
        let lower = tag.trim().to_ascii_lowercase();
        if lower.starts_with("ko") {
            Self::Ko
        } else {
            Self::En
        }
    }

    pub fn is_en(self) -> bool {
        matches!(self, Self::En)
    }
}

/// 간단한 인라인 한·영 분기 헬퍼.
pub fn pick<'a>(locale: Locale, ko: &'a str, en: &'a str) -> &'a str {
    match locale {
        Locale::Ko => ko,
        Locale::En => en,
    }
}

/// 한국어 기준 UI 문자열을 현재 로케일에 맞게 번역한다.
/// `Locale::Ko`일 때는 추가 할당 없이 원문을 그대로 반환한다.
pub fn t(locale: Locale, ko: &str) -> &str {
    if !locale.is_en() {
        return ko;
    }
    match ko {
        // 공통 · 테마 · 언어 · 필터 · 기능 모드
        "시스템" => "System",
        "라이트" => "Light",
        "다크" => "Dark",
        "전체" => "All",
        "준비됨" => "Ready",
        "확인 필요" => "Needs attention",
        "진행 중" => "In progress",
        "고정됨" => "Pinned",
        "플래시카드" => "Flashcards",
        "퀴즈" => "Quiz",
        "동영상 만들기" => "Create video",
        "이미지 생성" => "Generate image",
        "음악 만들기" => "Create music",
        "가이드 학습" => "Guided Learning",
        "복사" => "Copy",
        "공유" => "Share",
        "다시 생성" => "Regenerate",
        "닫기" => "Close",
        "추가" => "Add",
        "저장" => "Save",
        "삭제" => "Delete",
        "새로고침" => "Refresh",
        "동기화" => "Sync",
        "켜짐" => "On",
        "꺼짐" => "Off",
        "취소" => "Cancel",
        "연결됨" => "Connected",
        "미연결" => "Disconnected",
        "오프라인" => "Offline",
        "대기" => "Idle",
        "서버" => "Cloud",
        "로컬" => "Local",

        // 사이드바 · 내비게이션
        "새 채팅" => "New chat",
        "라이브러리" => "Library",
        "이미지" => "Images",
        "예약" => "Schedules",
        "학생" => "Student",
        "더보기" => "More",
        "노트북" => "Notebooks",
        "새 노트북" => "New notebook",
        "모든 노트북" => "All notebooks",
        "표시할 대화가 없습니다" => "No conversations to show",
        "Gemini 사용자" => "Gemini User",
        "원격 연결됨" => "Remote connected",
        "원격 대기" => "Remote idle",
        "원격 꺼짐" => "Remote off",
        "데몬 끊김" => "Daemon offline",
        "데몬 연결 중..." => "Connecting to daemon...",
        "데몬 비정상" => "Daemon unhealthy",
        "데몬 미연결" => "Daemon disconnected",
        "새 제목" => "New title",

        // 환영 화면 · 컴포저 · 채팅
        "임시 채팅" => "Temporary chat",
        "여기에 파일을 놓으면 질문에 첨부됩니다 (이미지·PDF·문서·코드)" => {
            "Drop files here to attach to your prompt (images, PDFs, docs, code)"
        }
        "임시 채팅 · 이 대화는 최근 항목에 표시되지 않으며 저장되지 않습니다." => {
            "Temporary chat · This chat won't appear in recent history or be saved."
        }
        "이 대화는 최근 항목에 표시되지 않으며 저장되지 않습니다." => {
            "This chat won't appear in recent history or be saved."
        }
        "대화 이력을 불러오는 중..." => "Loading conversation history...",
        "목록에서 어떤 일을 처리해 드릴까요?" => "What can I help you get done?",
        "무엇을 도와드릴까요?" => "Where should we start?",
        "채팅" => "Chat",
        "태스크" => "Tasks",
        "Gemini는 실수를 할 수 있으므로 다시 한번 확인하세요." => {
            "Gemini can make mistakes, so double-check it."
        }
        "태스크를 설명하세요" => "Describe a task",
        "Gemini에게 물어보기" => "Ask Gemini",
        "캡처 중..." => "Capturing...",
        "화면 공유" => "Share screen",
        "자동 모드" => "Auto",
        "원격 PC 선택" => "Select remote PC",
        "사고 과정 숨기기" => "Hide thinking",
        "생각하는 중..." => "Thinking...",
        "사고 과정 표시" => "Show thinking",
        "원격 도구 실행" => "Remote tool execution",
        "원격 PC 실행 결과를 정리하는 중..." => "Summarizing remote PC results...",
        "첨부한 음성 녹음을 듣고 답변해 주세요." => "Please listen to the attached audio recording and respond.",
        "첨부한 화면/파일을 확인하고 설명해 주세요." => "Please review and explain the attached screen/files.",

        // 팝오버 (+ 도구 / 모델 / 사고 수준 / @ 멘션 / 더보기 / 대화 메뉴)
        "파일 업로드" => "Upload files",
        "Drive에서 파일 추가" => "Add from Drive",
        "화면 캡처 첨부" => "Attach screenshot",
        "기기 선택" => "Select device",
        "데스크톱에서 실행" => "Run on desktop",
        "이미지 만들기" => "Create image",
        "개인 인텔리전스" => "Personal Intelligence",
        "사용 가능한 모델을 불러오는 중..." => "Loading available models...",
        "낮음" => "Low",
        "보통" => "Medium",
        "높음" => "High",
        "사고 수준" => "Thinking level",
        "속도 및 효율성" => "Fast & efficient",
        "균형 잡힌 깊이" => "Balanced depth",
        "매우 철저함" => "Extra thorough",
        "드라이브 파일 검색·요약" => "Search & summarize Drive files",
        "이메일 검색 및 초안 작성" => "Search emails & draft replies",
        "일정 조회 및 생성" => "View & create calendar events",
        "동영상 검색 및 내용 요약" => "Search videos & summarize content",
        "내 맞춤 지침 적용" => "Apply my custom instructions",
        "커넥터" => "Connector",
        "스킬" => "Skill",
        "로컬 연동 MCP 서버" => "Local MCP server",
        "@ 멘션 — 커넥터 및 MCP" => "@ Mentions — Connectors & MCP",
        "일치하는 커넥터나 MCP 서버가 없습니다" => "No matching connectors or MCP servers",
        "맞춤설정" => "Personalize",
        "동영상" => "Videos",
        "대화 공유" => "Share chat",
        "고정 해제" => "Unpin",
        "고정" => "Pin",
        "이름 변경" => "Rename",
        "읽음으로 표시" => "Mark as read",
        "읽지 않음으로 표시" => "Mark as unread",
        "노트북에 추가" => "Add to notebook",
        "노트북 없음" => "No notebooks",

        // 검색 모달
        "대화 제목이나 본문 키워드 입력 후 Enter..." => {
            "Search conversation titles or messages and press Enter..."
        }
        "검색 중..." => "Searching...",
        "본문 검색" => "Search messages",
        "서버 전체 본문 검색 결과" => "Server full-text search results",
        "최근 대화" => "Recent chats",
        "로컬 목록 일치 항목" => "Matching local conversations",

        // 설정 모달 · 원격 PC 팝오버
        "설정" => "Settings",
        "Google 계정" => "Google Account",
        "화면 테마" => "Appearance",
        "언어" => "Language",
        "개인 인텔리전스 (Labs)" => "Personal Intelligence (Labs)",
        "내 활동과 워크스페이스 컨텍스트를 답변에 자동 반영합니다." => {
            "Automatically use your activity and workspace context in responses."
        }
        "사용량 한도" => "Usage limits",
        "현재 표시할 사용량 한도 버킷이 없습니다." => "No usage limit buckets to display.",
        "사용량 한도 정보를 불러오는 중..." => "Loading usage limits...",
        "처리 중..." => "Working...",
        "원격 제어 끄기" => "Disable remote control",
        "원격 제어 켜기" => "Enable remote control",
        "지금 재등록" => "Re-register now",
        "허용 폴더" => "Allowed folders",
        "폴더 선택" => "Choose folder",
        "등록된 허용 폴더가 없습니다. (도구는 기본 작업 경로 기준으로 동작)" => {
            "No allowed folders configured. (Tools run relative to default workspace)"
        }
        "폴더 경로 직접 입력 (예: ~/dev)" => "Enter folder path (e.g. ~/dev)",
        "연동 MCP 서버" => "Connected MCP servers",
        "최근 원격 도구 실행 기록" => "Recent remote tool activity",
        "이 PC 원격 제어" => "Remote control on this PC",
        "다른 등록 기기" => "Other registered devices",
        "단축키" => "Keyboard shortcuts",
        "Ctrl+N 새 채팅   ·   Ctrl+K 대화 검색   ·   Ctrl+B 사이드바   ·   Esc 닫기/중단" => {
            "Ctrl+N New chat   ·   Ctrl+K Search   ·   Ctrl+B Sidebar   ·   Esc Close/Stop"
        }
        "원격 실행 대상 PC" => "Target remote PC",
        "등록된 원격 기기가 없습니다." => "No registered remote devices.",
        "원격 PC 및 허용 폴더 설정" => "Manage remote PC & allowed folders",
        "주기적 자동 초기화" => "Resets periodically",
        "곧 초기화됨" => "Resets soon",

        // 페이지: 라이브러리
        "Gemini에서 만든 Canvas 문서·이미지 미디어와 첨부 파일을 모아봅니다." => {
            "Browse Canvas documents, generated images, and attachments created with Gemini."
        }
        "문서" => "Documents",
        "미디어" => "Media",
        "로컬 파일" => "Local files",
        "표시할 라이브러리 항목이 없습니다" => "No library items to display",
        "Canvas 문서나 이미지를 생성하거나 파일을 첨부하면 이곳에 표시됩니다." => {
            "Canvas documents, generated images, and attached files will appear here."
        }
        "Canvas 문서" => "Canvas doc",
        "오디오" => "Audio",
        "파일" => "File",
        "이미지 썸네일 불러오는 중..." => "Loading image thumbnail...",
        "대화 열기" => "Open chat",
        "링크 복사" => "Copy link",
        "채팅에 첨부" => "Attach to chat",

        // 페이지: 이미지 스튜디오
        "이미지 스튜디오" => "Image Studio",
        "스타일 템플릿을 고르거나 원하는 장면을 입력하면 바로 이미지를 생성합니다." => {
            "Pick a style template or describe any scene to generate an image right away."
        }
        "실사" => "Photorealistic",
        "도시락" => "Bento",
        "정갈한 일본식 도시락 일러스트 스타일" => "Neat Japanese bento box illustration style",
        "파스텔" => "Pastel",
        "부드러운 파스텔 톤 수채화 감성" => "Soft pastel watercolor aesthetic",
        "피어난 꽃" => "Blooming",
        "햇살 가득한 보태니컬 플로럴 아트워크" => "Sunlit botanical floral artwork",
        "페인트" => "Painted",
        "질감이 살아있는 아크릴 유화 붓터치" => "Textured acrylic and oil brushwork",
        "점토" => "Clay",
        "귀여운 3D 클레이 스톱모션 미니어처" => "Cute 3D clay stop-motion miniature",
        "만들고 싶은 이미지를 자세히 설명해 주세요 (예: 비 오는 서울 밤거리 네온사인)" => {
            "Describe the image you want to create (e.g. neon-lit rainy night street in Seoul)"
        }
        "비율:" => "Aspect:",
        "최근 생성한 이미지" => "Recently generated images",
        "아직 생성된 이미지가 없습니다. 위 입력창에서 첫 이미지를 만들어 보세요." => {
            "No generated images yet. Describe your first image above."
        }
        "이미지 미리보기" => "Image preview",

        // 페이지: 예약
        "예약된 작업" => "Scheduled actions",
        "지정한 일정에 따라 Gemini가 자동으로 브리핑과 작업을 실행합니다." => {
            "Gemini automatically runs briefings and actions on your chosen schedule."
        }
        "예약할 작업을 설명하세요 (예: 매일 오전 9시에 AI 뉴스 요약해 줘)" => {
            "Describe a scheduled task (e.g. Summarize AI news every day at 9:00 AM)"
        }
        "제목 (선택)" => "Title (optional)",
        "예약 추가" => "Add schedule",
        "매일" => "Daily",
        "매주" => "Weekly",
        "매시간" => "Hourly",
        "활성" => "Active",
        "일시중지" => "Paused",
        "표시할 예약 작업이 없습니다. 아래 추천 예약을 추가하거나 직접 만들어 보세요." => {
            "No scheduled actions yet. Add a recommended schedule below or create your own."
        }
        "서버 활성" => "Cloud active",
        "서버 중지" => "Cloud paused",
        "로컬 활성" => "Local active",
        "실행 중..." => "Running...",
        "지금 실행" => "Run now",
        "최근 결과 열기" => "Open last result",
        "다시 켜기" => "Resume",
        "추천 예약" => "Recommended",
        "서버 추천 예약 목록을 불러오는 중입니다." => "Loading recommended schedules...",
        "매일 아침 브리핑" => "Daily morning briefing",
        "오늘 주요 기술 뉴스와 주목할 AI 업데이트 5가지를 한국어로 요약해 주세요." => {
            "Summarize today's top 5 tech news stories and notable AI updates."
        }
        "예약 브리핑" => "Scheduled briefing",

        // 페이지: 학생
        "학습 도우미" => "Learning helper",
        "강의 계획서 정리, 몰입형 탐색, 확인 퀴즈, 플래시카드로 깊이 있게 학습하세요." => {
            "Study deeper with syllabus notebooks, immersive exploration, quizzes, and flashcards."
        }
        "강의 계획서로 학습 노트북 만들기" => "Build a study notebook from a syllabus",
        "수업 일정과 핵심 개념을 주차별 학습 계획으로 구조화합니다." => {
            "Organize course schedules and core concepts into a weekly study plan."
        }
        "강의 계획서를 바탕으로 주차별 학습 로드맵과 핵심 개념 요약표를 만들어 주세요." => {
            "Create a weekly study roadmap and key concept summary table based on my syllabus."
        }
        "몰입형 탐색" => "Immersive exploration",
        "단계별 질문과 시각적 비유로 어려운 개념을 끝까지 이해합니다." => {
            "Master complex concepts through step-by-step questions and visual analogies."
        }
        "새로운 주제를 단계별로 질문하며 함께 탐구하는 가이드 학습을 시작해 주세요." => {
            "Start a guided learning session exploring a new topic step by step with interactive questions."
        }
        "확인 퀴즈" => "Practice quiz",
        "주제별 객관식·서술형 문제를 풀고 즉각적인 해설을 받습니다." => {
            "Solve multiple-choice and short-answer questions with instant explanations."
        }
        "이해도를 점검할 수 있는 확인 퀴즈 5문항을 난이도 순으로 출제해 주세요." => {
            "Give me a 5-question practice quiz ordered by difficulty to check my understanding."
        }
        "플래시카드 만들기" => "Create flashcards",
        "핵심 용어와 개념을 암기용 앞뒤 플래시카드로 자동 정리합니다." => {
            "Turn key terms and concepts into front-and-back study flashcards."
        }
        "핵심 용어와 정의를 빠르게 암기할 수 있는 플래시카드 세트를 만들어 주세요." => {
            "Create a set of study flashcards with key terms and concise definitions."
        }
        "새 학습 노트북" => "New study notebook",
        "학습 노트북" => "Study notebooks",
        "모든 노트북 보기" => "View all notebooks",

        // 페이지: 노트북
        "서버 Gemini 노트북과 로컬 소스·메모 노트북을 한곳에서 관리합니다." => {
            "Manage cloud Gemini notebooks and local source/note notebooks in one place."
        }
        "노트북 제목" => "Notebook title",
        "노트북 설명 (선택)" => "Description (optional)",
        "로컬 소스 추가" => "Add local source",
        "노트북 메모나 참고 컨텍스트를 적어 두세요..." => {
            "Write notes or reference context for this notebook..."
        }
        "이 노트북으로 채팅" => "Chat with notebook",
        "노트북 상세 / 편집" => "Notebook details / edit",

        // 페이지: Gems
        "특정 목적에 맞춰 미리 구성된 맞춤형 Gemini 어시스턴트를 선택하세요." => {
            "Choose a custom Gemini assistant tailored for a specific task."
        }
        "사용 가능한 Gem 목록을 불러오는 중입니다..." => "Loading available Gems...",
        "내 Gem" => "My Gems",
        "Google 공식" => "By Google",
        "선택한 Gem을 찾을 수 없습니다." => "Selected Gem could not be found.",
        "Gems 목록" => "All Gems",
        "이 Gem으로 자유 대화 시작" => "Start chatting with this Gem",

        // 페이지: 동영상 (Veo)
        "장면·카메라 움직임·분위기를 설명하면 짧은 고화질 동영상을 생성합니다." => {
            "Describe a scene, camera motion, and mood to generate a short high-quality video."
        }
        "생성할 동영상 장면을 설명하세요 (예: 노을 지는 바닷가를 드론으로 천천히 비행하는 장면)" => {
            "Describe the video scene (e.g. a slow drone flight along a sunlit beach at sunset)"
        }
        "길이:" => "Duration:",
        "5초" => "5s",
        "8초" => "8s",
        "동영상 생성" => "Generate video",
        "새벽 안개가 피어오르는 한옥 마을을 천천히 걷는 1인칭 시점 시네마틱 영상" => {
            "Cinematic first-person walk through a misty historic village at dawn"
        }
        "우주 정거장 창밖으로 지구의 푸른 지평선과 오로라가 펼쳐지는 타임랩스" => {
            "Timelapse of Earth's glowing blue horizon and aurora from a space station window"
        }
        "아늑한 카페 테이블 위 따뜻한 라떼 아트가 완성되는 클로즈업 슬로우 모션" => {
            "Slow-motion close-up of warm latte art being poured on a cozy cafe table"
        }
        "추천 프롬프트" => "Suggested prompts",

        // 페이지: 맞춤설정
        "Gemini가 나를 이해하고 원하는 어조와 형식으로 응답하도록 설정합니다." => {
            "Help Gemini understand you and respond in your preferred tone and format."
        }
        "이전 대화 맥락과 선호 사항을 반영해 더 개인화된 답변을 제공합니다." => {
            "Use past conversation context and preferences for more personalized answers."
        }
        "Gemini가 기억해야 할 선호·응답 스타일을 입력하세요 (예: 항상 간결한 한국어 불릿으로 답변)" => {
            "Enter preferences and response style for Gemini to remember (e.g. always reply in concise bullets)"
        }
        "Gemini를 위한 저장된 지침" => "Saved info & instructions for Gemini",
        "입력한 지침은 로컬 설정에 영속 저장되며 `@user-instructions` 멘션으로도 활용할 수 있습니다." => {
            "Instructions are saved locally and can also be referenced with `@user-instructions`."
        }
        "지침 저장" => "Save instructions",

        // 토스트 · 알림 메시지
        "원격 제어를 켰습니다" => "Remote control enabled",
        "원격 제어를 껐습니다" => "Remote control disabled",
        "기기 등록 상태를 동기화했습니다" => "Device registration synced",
        "허용 폴더를 추가했습니다" => "Allowed folder added",
        "허용 폴더를 제거했습니다" => "Allowed folder removed",
        "개인 인텔리전스를 켰습니다" => "Personal Intelligence enabled",
        "개인 인텔리전스를 껐습니다" => "Personal Intelligence disabled",
        "맞춤 지침을 저장했습니다" => "Custom instructions saved",
        "대화 제목이 변경되었습니다" => "Conversation renamed",
        "대화를 상단에 고정했습니다" => "Conversation pinned",
        "대화 고정을 해제했습니다" => "Conversation unpinned",
        "읽지 않음으로 표시했습니다" => "Marked as unread",
        "읽음으로 표시했습니다" => "Marked as read",
        "대화를 노트북에 추가했습니다" => "Added conversation to notebook",
        "대화가 삭제되었습니다" => "Conversation deleted",
        "좋은 응답으로 피드백을 보냈습니다" => "Sent positive feedback",
        "개선 필요 피드백을 보냈습니다" => "Sent feedback for improvement",
        "노트북이 저장되었습니다" => "Notebook saved",
        "노트북이 삭제되었습니다" => "Notebook deleted",
        "예약 작업 입력란에 기본 템플릿을 채웠습니다" => "Filled schedule template",
        "예약 실행할 프롬프트를 입력해 주세요" => "Please enter a prompt for the schedule",
        "예약 작업이 등록되었습니다" => "Scheduled action created",
        "예약 작업이 삭제되었습니다" => "Scheduled action deleted",
        "대화 저장됨" => "Chat saved",
        "라이브러리 파일이 입력창에 첨부되었습니다" => "Attached library file to prompt",
        "원본 로컬 파일을 찾을 수 없습니다" => "Original local file not found",
        "생성할 이미지 설명을 입력해 주세요" => "Please describe the image to generate",
        "생성할 동영상 장면을 입력해 주세요" => "Please describe the video scene to generate",
        "화면 캡처가 첨부되었습니다" => "Screenshot attached",
        "음성 녹음이 첨부되었습니다 (전송 시 분석)" => "Voice recording attached (analyzed on send)",
        "음성 녹음을 시작했습니다 (다시 누르면 종료)" => "Recording voice (click again to stop)",
        "응답 생성을 중단했습니다" => "Generation stopped",
        "응답 생성이 중단되었습니다" => "Response generation was stopped",
        "클립보드에 복사되었습니다" => "Copied to clipboard",
        "안전하지 않은 링크 형식은 열 수 없습니다" => "Blocked unsupported link scheme",

        // 리치 렌더러 · 표 · 예약 실행 · 도구 접힘 · 생성 이미지 · Canvas
        "표 복사" => "Copy table",
        "예약 실행됨" => "Scheduled run",
        "도구 활동 숨기기" => "Hide tool activity",
        "도구 활동 표시" => "Show tool activity",
        "이미지 불러오는 중..." => "Loading image...",
        "이미지를 불러올 수 없습니다" => "Failed to load image",
        "다시 시도" => "Retry",
        "생성된 이미지" => "Generated image",
        "원본 열기" => "Open image",
        "웹 Gemini에서 보기" => "View in Gemini web",
        "코드 보기" => "View code",
        "문서 보기" => "View document",
        "원문 보기" => "View source",
        "브라우저에서 실행" => "Run in browser",
        "웹에서 열기" => "Open in web",
        "코드 복사" => "Copy code",
        "내용 복사" => "Copy content",
        "브라우저에서 Canvas HTML을 열었습니다" => "Opened Canvas HTML in browser",
        "Canvas HTML 임시 파일 생성에 실패했습니다" => "Failed to create temporary Canvas HTML file",
        "첨부 파일" => "Attachment",

        _ => ko,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_pref_roundtrip_and_locale_detection() {
        assert_eq!(LanguagePref::from_str("ko"), LanguagePref::Ko);
        assert_eq!(LanguagePref::from_str("en"), LanguagePref::En);
        assert_eq!(LanguagePref::from_str("system"), LanguagePref::System);
        assert_eq!(LanguagePref::from_str(""), LanguagePref::System);

        assert_eq!(Locale::from_lang_tag("ko_KR.UTF-8"), Locale::Ko);
        assert_eq!(Locale::from_lang_tag("ko-KR"), Locale::Ko);
        assert_eq!(Locale::from_lang_tag("en_US.UTF-8"), Locale::En);
        assert_eq!(Locale::from_lang_tag("ja_JP.UTF-8"), Locale::En);
    }

    #[test]
    fn key_translations_return_english_and_preserve_korean() {
        assert_eq!(t(Locale::Ko, "새 채팅"), "새 채팅");
        assert_eq!(t(Locale::En, "새 채팅"), "New chat");
        assert_eq!(t(Locale::En, "사고 수준"), "Thinking level");
        assert_eq!(t(Locale::En, "화면 테마"), "Appearance");
        assert_eq!(t(Locale::En, "언어"), "Language");
        assert_eq!(t(Locale::En, "허용 폴더"), "Allowed folders");
    }
}
