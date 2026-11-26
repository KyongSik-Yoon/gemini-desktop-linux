use iced::widget::svg;
use iced::{Color, Length};
use std::sync::OnceLock;

/// 공식 Gemini 4색 멀티컬러 스파클(빨강·파랑·초록·노랑) 아이콘을 원본 색상 그대로 렌더링한다.
/// resvg 호환성을 위해 clipPath/중첩 radialGradient 대신 4분면 베지어 꽃잎 path로 선명하게 구성한다.
pub fn sparkle_color<'a>(size: f32) -> svg::Svg<'a> {
    const RAW: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
  <defs>
    <linearGradient id="p_top" x1="50%" y1="0%" x2="50%" y2="100%">
      <stop offset="0%" stop-color="#EA4335"/>
      <stop offset="100%" stop-color="#F15A3E"/>
    </linearGradient>
    <linearGradient id="p_right" x1="100%" y1="50%" x2="0%" y2="50%">
      <stop offset="0%" stop-color="#3186FF"/>
      <stop offset="100%" stop-color="#4285F4"/>
    </linearGradient>
    <linearGradient id="p_bottom" x1="50%" y1="100%" x2="50%" y2="0%">
      <stop offset="0%" stop-color="#00B95C"/>
      <stop offset="100%" stop-color="#34A853"/>
    </linearGradient>
    <linearGradient id="p_left" x1="0%" y1="50%" x2="100%" y2="50%">
      <stop offset="0%" stop-color="#FBBC04"/>
      <stop offset="100%" stop-color="#F9AB00"/>
    </linearGradient>
  </defs>
  <path d="M12 1.6C12.8 7.2 16.8 11.2 22.4 12C16.8 12.8 12.8 16.8 12 22.4C11.2 16.8 7.2 12.8 1.6 12C7.2 11.2 11.2 7.2 12 1.6Z" fill="#4285F4"/>
  <path d="M12 12L8.75 8.75C10.55 7.05 11.7 4.5 12 1.6C12.3 4.5 13.45 7.05 15.25 8.75Z" fill="url(#p_top)"/>
  <path d="M12 12L15.25 8.75C17.05 10.45 19.5 11.7 22.4 12C19.5 12.3 17.05 13.55 15.25 15.25Z" fill="url(#p_right)"/>
  <path d="M12 12L15.25 15.25C13.45 16.95 12.3 19.5 12 22.4C11.7 19.5 10.55 16.95 8.75 15.25Z" fill="url(#p_bottom)"/>
  <path d="M12 12L8.75 15.25C6.95 13.55 4.5 12.3 1.6 12C4.5 11.7 6.95 10.45 8.75 8.75Z" fill="url(#p_left)"/>
</svg>"##;

    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    svg(HANDLE.get_or_init(|| svg::Handle::from_memory(RAW)).clone())
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(|_, _| svg::Style { color: None })
}

/// 공식 Gemini 앱 UI용 벡터 SVG 아이콘 세트 (24x24 Material Symbols 스타일).
#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Sparkle,
    SidebarToggle,
    NewChat,
    Library,
    Image,
    Clock,
    School,
    MoreHoriz,
    Notebook,
    Filter,
    Settings,
    ScreenShare,
    Mic,
    MicOff,
    Gem,
    Play,
    Laptop,
    Refresh,
    Search,
    Plus,
    ArrowUp,
    Stop,
    Copy,
    Check,
    Close,
    Edit,
    Pin,
    PinFilled,
    Trash,
    ChevronDown,
    ChevronUp,
    ChevronRight,
    TempChat,
    AttachFile,
    ThumbsUp,
    ThumbsDown,
    Share,
    MoreVert,
    Video,
    Music,
    Canvas,
    Research,
    Book,
    Tune,
    Drive,
    Unread,
    Flag,
    Progress,
    CornerDownRight,
}

impl Icon {
    fn raw_svg(self) -> &'static [u8] {
        match self {
            Self::Sparkle => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C12.7 7.1 16.9 11.3 22 12C16.9 12.7 12.7 16.9 12 22C11.3 16.9 7.1 12.7 2 12C7.1 11.3 11.3 7.1 12 2Z"/></svg>"#,
            // 사이드바 패널 토글 아이콘
            Self::SidebarToggle => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="3"/><line x1="9" y1="4" x2="9" y2="20"/></svg>"#,
            // 새 채팅 (연필 + 노트)
            Self::NewChat => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 1 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>"#,
            // 라이브러리 (4칸 그리드)
            Self::Library => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/></svg>"#,
            // 이미지
            Self::Image => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2.5"/><circle cx="8.5" cy="9.5" r="1.5"/><path d="M21 16l-5-5-4.5 4.5-2.5-2.5L3 19"/></svg>"#,
            // 예약 (시계)
            Self::Clock => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><polyline points="12 7 12 12 15.5 14"/></svg>"#,
            // 학생 (학사모)
            Self::School => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M22 10l-10-5-10 5 10 5 10-5z"/><path d="M6 12v5c0 1.7 2.7 3 6 3s6-1.3 6-3v-5"/><path d="M22 10v6"/></svg>"#,
            // 더보기 (가로 점 3개)
            Self::MoreHoriz => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><circle cx="5.5" cy="12" r="1.7"/><circle cx="12" cy="12" r="1.7"/><circle cx="18.5" cy="12" r="1.7"/></svg>"#,
            // 노트북
            Self::Notebook => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M5 4h11a3 3 0 0 1 3 3v13H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z"/><line x1="8" y1="4" x2="8" y2="20"/><line x1="11.5" y1="9" x2="15.5" y2="9"/><line x1="11.5" y1="13" x2="15.5" y2="13"/></svg>"#,
            // 필터 깔때기
            Self::Filter => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round"><path d="M3 4h18l-7 8.5V19l-4 2v-8.5L3 4z"/></svg>"#,
            // 설정 톱니바퀴
            Self::Settings => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.08a1.65 1.65 0 0 0 .92-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.08a1.65 1.65 0 0 0 1.51.92H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>"#,
            // 화면 공유 (모니터 + 내부 창)
            Self::ScreenShare => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="4" width="19" height="13" rx="2"/><line x1="8" y1="20.5" x2="16" y2="20.5"/><line x1="12" y1="17" x2="12" y2="20.5"/><polyline points="9.5 11.5 12 9 14.5 11.5"/><line x1="12" y1="9" x2="12" y2="14"/></svg>"#,
            // 마이크
            Self::Mic => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="2.5" width="6" height="11" rx="3"/><path d="M5 10.5a7 7 0 0 0 14 0"/><line x1="12" y1="17.5" x2="12" y2="21.5"/><line x1="8.5" y1="21.5" x2="15.5" y2="21.5"/></svg>"#,
            // 마이크 중지
            Self::MicOff => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="3"/></svg>"#,
            // Gem 다이아몬드
            Self::Gem => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3h12l4 6-10 12L2 9l4-6z"/><line x1="2" y1="9" x2="22" y2="9"/></svg>"#,
            // 즉시 실행
            Self::Play => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M6 4l14 8-14 8V4z"/></svg>"#,
            // 기기 노트북
            Self::Laptop => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="12" rx="2"/><line x1="2" y1="20" x2="22" y2="20"/></svg>"#,
            // 새로고침
            Self::Refresh => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-2.64-6.36"/><polyline points="21 3 21 9 15 9"/></svg>"#,
            // 검색 돋보기
            Self::Search => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="7"/><line x1="20" y1="20" x2="16.2" y2="16.2"/></svg>"#,
            // +
            Self::Plus => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>"#,
            // 전송 화살표
            Self::ArrowUp => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="19" x2="12" y2="5"/><polyline points="6 11 12 5 18 11"/></svg>"#,
            // 중단
            Self::Stop => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2.5"/></svg>"#,
            // 복사
            Self::Copy => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>"#,
            // 체크
            Self::Check => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>"#,
            // 닫기
            Self::Close => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>"#,
            // 수정 연필
            Self::Edit => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M17 3a2.83 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z"/></svg>"#,
            // 핀 (윤곽)
            Self::Pin => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/></svg>"#,
            // 핀 (채움)
            Self::PinFilled => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M16 3a1 1 0 0 1 0 2v5.76c0 .4.22.77.58.95l1.78.9A3 3 0 0 1 20 15.24V16a2 2 0 0 1-2 2h-5v4a1 1 0 1 1-2 0v-4H6a2 2 0 0 1-2-2v-.76a3 3 0 0 1 1.64-2.63l1.78-.9c.36-.18.58-.55.58-.95V5a1 1 0 1 1 0-2h8z"/></svg>"#,
            // 휴지통
            Self::Trash => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/><path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/></svg>"#,
            // 아래 쉐브론
            Self::ChevronDown => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>"#,
            // 위 쉐브론
            Self::ChevronUp => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="18 15 12 9 6 15"/></svg>"#,
            // 우측 쉐브론
            Self::ChevronRight => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 6 15 12 9 18"/></svg>"#,
            // 임시 채팅 (원본 우측 상단 점선 말풍선 + 연필)
            Self::TempChat => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path stroke-dasharray="2.8 2.6" d="M20.5 11.5a8.5 8.5 0 0 1-8.5 8.5c-1.4 0-2.7-.3-3.9-.9L3 20.5l1.4-5.1A8.5 8.5 0 1 1 20.5 11.5z"/><path d="M9.5 14.5l4.8-4.8a1.2 1.2 0 0 1 1.7 1.7l-4.8 4.8-2.2.5.5-2.2z"/></svg>"#,
            // 첨부 파일
            Self::AttachFile => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>"#,
            // 좋아요
            Self::ThumbsUp => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M7 10v12"/><path d="M15 5.88L14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8A2 2 0 0 1 17.5 22H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h2.76a2 2 0 0 0 1.79-1.11L12 2a3.13 3.13 0 0 1 3 3.88z"/></svg>"#,
            // 싫어요
            Self::ThumbsDown => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M17 14V2"/><path d="M9 18.12L10 14H4.17a2 2 0 0 1-1.92-2.56l2.33-8A2 2 0 0 1 6.5 2H20a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-2.76a2 2 0 0 0-1.79 1.11L12 22a3.13 3.13 0 0 1-3-3.88z"/></svg>"#,
            // 공유
            Self::Share => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/><line x1="8.59" y1="13.51" x2="15.42" y2="17.49"/><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"/></svg>"#,
            // 세로 점 3개
            Self::MoreVert => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5.5" r="1.65"/><circle cx="12" cy="12" r="1.65"/><circle cx="12" cy="18.5" r="1.65"/></svg>"#,
            // 동영상
            Self::Video => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="5" width="14" height="14" rx="2.5"/><polygon points="21.5 8 16.5 11.5 16.5 12.5 21.5 16 21.5 8"/></svg>"#,
            // 음악
            Self::Music => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>"#,
            // Canvas 문서 편집
            Self::Canvas => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="3" width="16" height="18" rx="2.5"/><line x1="8" y1="8" x2="16" y2="8"/><line x1="8" y1="12" x2="16" y2="12"/><line x1="8" y1="16" x2="13" y2="16"/></svg>"#,
            // Deep Research
            Self::Research => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="10.5" cy="10.5" r="6.5"/><line x1="20" y1="20" x2="15.2" y2="15.2"/><path d="M10.5 7.5v6"/><path d="M7.5 10.5h6"/></svg>"#,
            // 가이드 학습 책
            Self::Book => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/></svg>"#,
            // 맞춤설정 슬라이더
            Self::Tune => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><line x1="4" y1="7" x2="20" y2="7"/><circle cx="9" cy="7" r="2"/><line x1="4" y1="17" x2="20" y2="17"/><circle cx="15" cy="17" r="2"/></svg>"#,
            // Google Drive 삼각형
            Self::Drive => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><polygon points="8.5 3.5 15.5 3.5 21.5 14 17.8 20.5 6.2 20.5 2.5 14 8.5 3.5"/><line x1="8.5" y1="3.5" x2="15" y2="14.5"/><line x1="2.5" y1="14" x2="21.5" y2="14"/></svg>"#,
            // 읽지 않음 표시 (우상단 알림 점이 있는 말풍선)
            Self::Unread => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M21 11.5V15a2.5 2.5 0 0 1-2.5 2.5H7.5L3.5 21V6.5A2.5 2.5 0 0 1 6 4h7.5"/><circle cx="19" cy="5.5" r="2.5" fill="currentColor" stroke="none"/></svg>"#,
            // 확인 필요 깃발
            Self::Flag => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M5 3v18"/><path d="M5 4h12l-2.5 4.5L17 13H5"/></svg>"#,
            // 진행 중 점선 원
            Self::Progress => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-dasharray="3.5 3.5"><circle cx="12" cy="12" r="8.5"/></svg>"#,
            // 후속 제안/빠른 답장 화살표 (↪)
            Self::CornerDownRight => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 10 20 15 15 20"/><path d="M4 4v7a4 4 0 0 0 4 4h12"/></svg>"#,
        }
    }

    const ALL: [Self; 49] = [
        Self::Sparkle,
        Self::SidebarToggle,
        Self::NewChat,
        Self::Library,
        Self::Image,
        Self::Clock,
        Self::School,
        Self::MoreHoriz,
        Self::Notebook,
        Self::Filter,
        Self::Settings,
        Self::ScreenShare,
        Self::Mic,
        Self::MicOff,
        Self::Gem,
        Self::Play,
        Self::Laptop,
        Self::Refresh,
        Self::Search,
        Self::Plus,
        Self::ArrowUp,
        Self::Stop,
        Self::Copy,
        Self::Check,
        Self::Close,
        Self::Edit,
        Self::Pin,
        Self::PinFilled,
        Self::Trash,
        Self::ChevronDown,
        Self::ChevronUp,
        Self::ChevronRight,
        Self::TempChat,
        Self::AttachFile,
        Self::ThumbsUp,
        Self::ThumbsDown,
        Self::Share,
        Self::MoreVert,
        Self::Video,
        Self::Music,
        Self::Canvas,
        Self::Research,
        Self::Book,
        Self::Tune,
        Self::Drive,
        Self::Unread,
        Self::Flag,
        Self::Progress,
        Self::CornerDownRight,
    ];

    fn cached_handle(self) -> svg::Handle {
        static HANDLES: OnceLock<[svg::Handle; 49]> = OnceLock::new();
        let table = HANDLES.get_or_init(|| Self::ALL.map(|icon| svg::Handle::from_memory(icon.raw_svg())));
        table[self as usize].clone()
    }

    pub fn view<'a>(self, size: f32, color: Color) -> svg::Svg<'a> {
        svg(self.cached_handle())
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .style(move |_, _| svg::Style { color: Some(color) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 아이콘_캐시_인덱스_일치() {
        for (idx, icon) in Icon::ALL.iter().enumerate() {
            assert_eq!(*icon as usize, idx);
        }
    }
}
