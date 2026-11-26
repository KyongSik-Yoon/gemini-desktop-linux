mod capture;
mod chat_view;
mod filepicker;
mod glow;
mod i18n;
mod icons;
mod ipc;
mod markdown_view;
mod modals;
mod pages;
mod popovers;
mod sidebar;
mod theme;
mod tray;

use capture::VoiceRecorder;
use i18n::{LanguagePref, Locale, t};
use iced::widget::{
    button, canvas, container, image, markdown, mouse_area, operation, row, space, stack,
    text_editor,
};
use iced::{
    Background, Border, Color, Element, Event, Fill, Length, Size, Subscription, Task, Theme,
    clipboard, event, keyboard, mouse, system, task, time, window,
};
use ipc::{
    BardModeOption, BotSummary, BotsResponse, ChatStreamChunk, ChatStreamRequest,
    ConversationSummary, ConversationTurn, DevicesResponse, ElicitationsGroup, FollowUpPrompt,
    GeneratedImage, HealthResponse, LibraryEntry, LibraryResponse, ListConversationsResponse,
    ListTurnsResponse, ModelsResponse, Notebook, NotebooksResponse, RecommendedSchedule,
    RegisteredDevice, RemoteEvent, RemoteStatusSnapshot, RemoteToolActivity, ScheduleRunResult,
    ScheduledTask, SchedulesResponse, SearchHit, SearchResponse, ShareResponse, StreamEvent,
    TurnArtifact, TurnAttachment, UIPreferences, UsageInfo, UserProfile,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use theme::{AppTheme, ThemeMode, font_regular};

pub(crate) const CHAT_SCROLL_ID: &str = "chat_scroll";
pub(crate) const SEARCH_INPUT_ID: &str = "search_modal_input";
pub(crate) const WELCOME_MAX_WIDTH: f32 = 660.0;
pub(crate) const CONTENT_MAX_WIDTH: f32 = 760.0;
pub(crate) const SIDEBAR_DEFAULT_WIDTH: f32 = 284.0;
pub(crate) const SIDEBAR_MIN_WIDTH: f32 = 220.0;
pub(crate) const SIDEBAR_MAX_WIDTH: f32 = 480.0;
const SIDEBAR_MAIN_MIN_RESERVE: f32 = 360.0;

// 기능 모드 코드 (서버 채팅 옵션 연동)
pub(crate) const FEATURE_MODE_DEEP_RESEARCH: i32 = 1;
pub(crate) const FEATURE_MODE_CANVAS: i32 = 2;
pub(crate) const FEATURE_MODE_FLASHCARDS: i32 = 9;
pub(crate) const FEATURE_MODE_QUIZ: i32 = 10;
pub(crate) const FEATURE_MODE_VIDEO: i32 = 11;
pub(crate) const FEATURE_MODE_IMAGE: i32 = 14;
pub(crate) const FEATURE_MODE_MUSIC: i32 = 21;
pub(crate) const FEATURE_MODE_GUIDED_LEARNING: i32 = 24;

// 사고 수준 코드
pub(crate) const THINKING_LOW: i32 = 1;
pub(crate) const THINKING_HIGH: i32 = 2;
pub(crate) const THINKING_MEDIUM: i32 = 4;

pub(crate) fn feature_mode_label(mode: i32) -> Option<&'static str> {
    match mode {
        FEATURE_MODE_DEEP_RESEARCH => Some("Deep Research"),
        FEATURE_MODE_CANVAS => Some("Canvas"),
        FEATURE_MODE_FLASHCARDS => Some("플래시카드"),
        FEATURE_MODE_QUIZ => Some("퀴즈"),
        FEATURE_MODE_VIDEO => Some("동영상 만들기"),
        FEATURE_MODE_IMAGE => Some("이미지 생성"),
        FEATURE_MODE_MUSIC => Some("음악 만들기"),
        FEATURE_MODE_GUIDED_LEARNING => Some("가이드 학습"),
        _ => None,
    }
}

fn main() -> iced::Result {
    let launch_mode = match tray::LaunchMode::from_args(std::env::args()) {
        Ok(mode) => mode,
        Err(help) => {
            print!("{help}");
            return Ok(());
        }
    };

    if tray::try_activate_existing_instance(launch_mode) {
        return Ok(());
    }

    let start_hidden = launch_mode == tray::LaunchMode::StartInTray;
    iced::daemon(move || App::new(start_hidden), App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(App::theme)
        .default_font(font_regular())
        .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    User,
    Model,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ActiveSection {
    Chat,
    Library,
    Images,
    Schedules,
    Student,
    Notebooks,
    GemsHub,
    Videos,
    Personalize,
    Gem(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComposerMode {
    Chat,
    Task,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConvFilter {
    All,
    Ready,
    NeedsAttention,
    InProgress,
    PinnedOnly,
}

impl ConvFilter {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "전체",
            Self::Ready => "준비됨",
            Self::NeedsAttention => "확인 필요",
            Self::InProgress => "진행 중",
            Self::PinnedOnly => "고정됨",
        }
    }
}

pub(crate) struct ChatMessage {
    pub(crate) role: Role,
    pub(crate) raw_text: String,
    pub(crate) markdown: markdown_view::ParsedMarkdown,
    pub(crate) thoughts: String,
    pub(crate) thoughts_expanded: bool,
    pub(crate) model_name: String,
    pub(crate) request_id: String,
    pub(crate) response_id: String,
    pub(crate) status_text: String,
    pub(crate) tool_activities: Vec<RemoteToolActivity>,
    pub(crate) tools_expanded: bool,
    pub(crate) target_device_name: String,
    pub(crate) attachments: Vec<String>,
    pub(crate) attachment_items: Vec<TurnAttachment>,
    pub(crate) generated_images: Vec<GeneratedImage>,
    pub(crate) artifacts: Vec<TurnArtifact>,
    pub(crate) follow_up: Option<FollowUpPrompt>,
    pub(crate) elicitations: Option<ElicitationsGroup>,
    pub(crate) is_scheduled_run: bool,
    pub(crate) scheduled_title: String,
    pub(crate) aborted: bool,
    pub(crate) timestamp_unix: i64,
}

impl ChatMessage {
    fn user(text: String, attachments: Vec<String>) -> Self {
        Self {
            role: Role::User,
            raw_text: text,
            markdown: markdown_view::ParsedMarkdown::empty(),
            thoughts: String::new(),
            thoughts_expanded: false,
            model_name: String::new(),
            request_id: String::new(),
            response_id: String::new(),
            status_text: String::new(),
            tool_activities: Vec::new(),
            tools_expanded: false,
            target_device_name: String::new(),
            attachments,
            attachment_items: Vec::new(),
            generated_images: Vec::new(),
            artifacts: Vec::new(),
            follow_up: None,
            elicitations: None,
            is_scheduled_run: false,
            scheduled_title: String::new(),
            aborted: false,
            timestamp_unix: 0,
        }
    }

    fn model(text: String, thoughts: String, model_name: String) -> Self {
        let md = markdown_view::ParsedMarkdown::parse(&text);
        Self {
            role: Role::Model,
            raw_text: text,
            markdown: md,
            thoughts,
            thoughts_expanded: false,
            model_name,
            request_id: String::new(),
            response_id: String::new(),
            status_text: String::new(),
            tool_activities: Vec::new(),
            tools_expanded: false,
            target_device_name: String::new(),
            attachments: Vec::new(),
            attachment_items: Vec::new(),
            generated_images: Vec::new(),
            artifacts: Vec::new(),
            follow_up: None,
            elicitations: None,
            is_scheduled_run: false,
            scheduled_title: String::new(),
            aborted: false,
            timestamp_unix: 0,
        }
    }

    fn push_delta(&mut self, delta: &str, full_text: &str, defer_reparse: bool) -> bool {
        if !delta.is_empty() && (full_text.is_empty() || full_text.starts_with(&self.raw_text)) {
            let prev_ended_star = self.raw_text.ends_with('*');
            self.raw_text.push_str(delta);
            self.markdown
                .push_delta(prev_ended_star, delta, &self.raw_text, defer_reparse)
        } else if !full_text.is_empty() && full_text != self.raw_text {
            self.raw_text = full_text.to_string();
            if defer_reparse {
                true
            } else {
                self.markdown = markdown_view::ParsedMarkdown::parse(&self.raw_text);
                false
            }
        } else {
            false
        }
    }
}

pub(crate) struct CanvasPreviewState {
    pub(crate) msg_idx: usize,
    pub(crate) art_idx: usize,
    pub(crate) artifact: TurnArtifact,
    pub(crate) parsed_markdown: Option<markdown_view::ParsedMarkdown>,
    pub(crate) show_raw: bool,
}

pub(crate) struct App {
    pub(crate) palette: AppTheme,
    cached_iced_theme: Theme,
    pub(crate) theme_mode: ThemeMode,
    pub(crate) language_pref: LanguagePref,
    pub(crate) locale: Locale,
    system_dark_fallback: bool,

    // 데몬 · 계정 프로필 · 등록 기기 · 원격 제어 상태
    pub(crate) daemon_connected: bool,
    pub(crate) daemon_label: String,
    pub(crate) profile: UserProfile,
    pub(crate) avatar_handle: Option<image::Handle>,
    pub(crate) devices: Vec<RegisteredDevice>,
    pub(crate) remote_status: RemoteStatusSnapshot,
    pub(crate) selected_device_id: Option<String>,
    pub(crate) device_menu_open: bool,
    pub(crate) remote_busy: bool,
    pub(crate) folder_input_draft: String,
    pub(crate) usage_info: Option<UsageInfo>,

    // 내비게이션 · 사이드바 · 팝오버 · 모달
    pub(crate) sidebar_open: bool,
    pub(crate) sidebar_width: f32,
    sidebar_resizing: bool,
    sidebar_resize_hovered: bool,
    sidebar_resize_start_width: Option<f32>,
    window_width: f32,
    pub(crate) window_height: f32,
    pub(crate) sidebar_scroll_y: f32,
    pub(crate) hovered_conv_id: Option<String>,
    pub(crate) active_section: ActiveSection,
    pub(crate) more_menu_open: bool,
    pub(crate) filter_menu_open: bool,
    pub(crate) conv_filter: ConvFilter,
    pub(crate) conv_menu_target: Option<String>,
    pub(crate) conv_menu_notebook_sub: bool,
    search_modal_open: bool,
    pub(crate) search_query: String,
    pub(crate) search_hits: Vec<SearchHit>,
    pub(crate) search_loading: bool,
    settings_modal_open: bool,
    canvas_preview: Option<CanvasPreviewState>,
    pub(crate) local_tz_offset_secs: i64,

    // 모델 선택기 · 사고 수준 · 도구/기능 모드 · 맞춤설정
    pub(crate) mode_picker_title: String,
    pub(crate) models: Vec<BardModeOption>,
    pub(crate) selected_model_id: Option<String>,
    pub(crate) model_menu_open: bool,
    pub(crate) thinking_level: i32,
    pub(crate) thinking_submenu_open: bool,
    pub(crate) tools_menu_open: bool,
    pub(crate) selected_feature_mode: i32,
    pub(crate) personal_intelligence: bool,
    pub(crate) custom_instructions_draft: String,
    pub(crate) mention_query: Option<String>,

    // Gems / 노트북 / 예약 / 라이브러리
    pub(crate) bots: Vec<BotSummary>,
    pub(crate) active_bot_id: Option<String>,
    pub(crate) notebooks: Vec<Notebook>,
    pub(crate) editing_notebook: Notebook,
    pub(crate) notebook_notes_editor: text_editor::Content,
    pub(crate) schedules: Vec<ScheduledTask>,
    pub(crate) recommended_schedules: Vec<RecommendedSchedule>,
    pub(crate) sched_filter: String,
    pub(crate) sched_title_draft: String,
    pub(crate) sched_prompt_draft: String,
    pub(crate) sched_type_draft: String,
    pub(crate) sched_time_draft: String,
    pub(crate) running_schedule_id: Option<String>,
    pub(crate) library_items: Vec<LibraryEntry>,
    pub(crate) library_filter: String,
    pub(crate) library_image_cache: HashMap<String, image::Handle>,
    library_image_order: VecDeque<String>,
    pub(crate) failed_image_keys: HashSet<String>,
    pub(crate) inflight_image_keys: HashSet<String>,

    // 이미지 · 동영상 스튜디오 상태
    pub(crate) image_prompt_draft: String,
    pub(crate) image_aspect: String,
    pub(crate) image_style: String,
    pub(crate) video_prompt_draft: String,
    pub(crate) video_aspect: String,
    pub(crate) video_duration: String,

    // 대화 상태
    pub(crate) conversations: Vec<ConversationSummary>,
    pub(crate) conversations_next_page_token: Option<String>,
    pub(crate) loading_more_conversations: bool,
    pub(crate) active_conv_id: Option<String>,
    pub(crate) active_conv_is_agent: bool,
    pub(crate) active_project_id: Option<String>,
    pub(crate) active_title: String,
    parent_request_id: String,
    parent_response_id: String,
    pending_scroll_request_id: Option<String>,
    pub(crate) messages: Vec<ChatMessage>,
    pub(crate) message_feedback: HashMap<usize, bool>,

    // 인라인 이름 변경
    pub(crate) renaming_conv_id: Option<String>,
    pub(crate) rename_draft: String,

    // 입력창 · 모드 · 첨부 · 화면 공유 · 마이크 녹음
    pub(crate) composer: text_editor::Content,
    pub(crate) composer_has_text: bool,
    pub(crate) composer_mode: ComposerMode,
    pub(crate) attached_files: Vec<PathBuf>,
    pub(crate) file_hovering: bool,
    pub(crate) temporary_chat: bool,
    pub(crate) screen_capturing: bool,
    pub(crate) voice_recorder: Option<VoiceRecorder>,
    pub(crate) voice_stopping: bool,
    pub(crate) recording_seconds: u32,

    pub(crate) loading_history: bool,
    conv_load_seq: u64,
    pub(crate) is_streaming: bool,
    pub(crate) streaming_req_id: String,
    stream_abort_handle: Option<task::Handle>,
    last_stream_reparse: Option<Instant>,
    last_stream_scroll: Option<Instant>,
    stream_markdown_dirty: bool,
    stream_had_attachments: bool,

    // 캔버스 지오메트리 캐시 (테마/모드 변경 시 무효화)
    pub(crate) welcome_glow_cache: canvas::Cache,
    pub(crate) avatar_mask_cache: canvas::Cache,

    // 창 생명주기 · 시스템 트레이
    main_window: Option<window::Id>,
    pub(crate) close_to_tray: bool,
    pub(crate) tray_available: bool,
    tray_tx: Option<tray::TrayUpdateSender>,

    pub(crate) error_banner: Option<String>,
    pub(crate) toast: Option<(String, u8)>,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Tick,
    OmarchyThemeChanged(String),
    SystemThemeChanged(iced::theme::Mode),
    RuntimeEvent(Event),
    WindowOpened(window::Id),
    WindowCloseRequested(window::Id),
    WindowClosed(window::Id),
    TrayEventReceived(tray::TrayEvent),
    ToggleCloseToTray,

    // 초기 및 동기화 응답
    HealthLoaded(Result<HealthResponse, String>),
    ProfileLoaded(Result<UserProfile, String>),
    AvatarLoaded(Result<Vec<u8>, String>),
    ModelsLoaded(Result<ModelsResponse, String>),
    BotsLoaded(Result<BotsResponse, String>),
    DevicesLoaded(Result<DevicesResponse, String>),
    RemoteStatusLoaded(Result<RemoteStatusSnapshot, String>),
    RemoteEventReceived(RemoteEvent),
    ConversationsLoaded(Result<ListConversationsResponse, String>),
    LoadMoreConversations,
    MoreConversationsLoaded(Result<ListConversationsResponse, String>),
    NotebooksLoaded(Result<NotebooksResponse, String>),
    SchedulesLoaded(Result<SchedulesResponse, String>),
    LibraryLoaded(Result<LibraryResponse, String>),
    LibraryImageLoaded(String, Result<Vec<u8>, String>),
    PreferencesLoaded(Result<UIPreferences, String>),
    PreferencesSaved(Result<UIPreferences, String>),
    UsageLoaded(Result<UsageInfo, String>),
    RefreshAll,

    // 원격 PC 선택 · 제어
    ToggleDeviceMenu,
    CloseDeviceMenu,
    SelectDevice(String),
    ToggleRemoteEnabled,
    SyncRemoteRegistration,
    PickRemoteFolder,
    RemoteFolderPicked(Result<Option<PathBuf>, String>),
    FolderDraftChanged(String),
    AddRemoteFolderManual,
    RemoveRemoteFolder(String),
    RemoteActionDone(String, Result<RemoteStatusSnapshot, String>),

    // 사이드바 · 내비게이션 · 필터 · 팝오버
    ToggleSidebar,
    StartSidebarResize,
    SidebarResizeHover(bool),
    ResetSidebarWidth,
    SidebarResizeEvent(Event),
    SwitchSection(ActiveSection),
    ToggleMoreMenu,
    CloseMoreMenu,
    ToggleFilterMenu,
    CloseFilterMenu,
    SetConvFilter(ConvFilter),
    OpenConvMenu(String),
    CloseConvMenu,
    ToggleConvMenuNotebookSub,
    HoverConv(String),
    LeaveConv(String),
    SidebarScrolled(f32),

    // 검색 모달
    OpenSearchModal,
    CloseSearchModal,
    SearchQueryChanged(String),
    SubmitServerSearch,
    ServerSearchLoaded(Result<SearchResponse, String>),

    // 설정 모달 · 환경설정 · 맞춤설정
    OpenSettingsModal,
    CloseSettingsModal,
    SetThemeMode(ThemeMode),
    SetLanguagePref(LanguagePref),
    TogglePersonalIntelligence,
    CustomInstructionsChanged(String),
    SaveCustomInstructions,

    // 모델 선택기 · 사고 수준 · 도구 팝오버 · @멘션
    ToggleModelMenu,
    CloseModelMenu,
    SelectModel(String),
    ToggleThinkingSubmenu,
    SelectThinkingLevel(i32),
    ToggleToolsMenu,
    CloseToolsMenu,
    SelectFeatureMode(i32),
    ClearFeatureMode,
    InsertMention(String),
    CloseMentionMenu,

    // 대화 선택 · 관리 · 공유 · 읽지않음 · 피드백
    SelectConversation(String, String),
    SelectSearchHit(String, String, String),
    TurnsLoaded(u64, String, Result<ListTurnsResponse, String>),
    NewChat,
    StartRename(String, String),
    RenameDraftChanged(String),
    SubmitRename,
    CancelRename,
    ConversationRenamed(Result<(String, String), String>),
    TogglePinConversation(String, bool),
    ConversationPinned(Result<(String, bool), String>),
    ToggleUnreadConversation(String, bool),
    ConversationUnreadUpdated(Result<(String, bool), String>),
    ConversationUnreadSynced(String, Result<(String, bool), String>),
    ShareConversation(String, String),
    ConversationShared(Result<ShareResponse, String>),
    AddConvToNotebook(String, String),
    ConvAddedToNotebook(Result<(), String>),
    DeleteConversation(String),
    ConversationDeleted(Result<String, String>),
    SendFeedback(usize, bool),
    FeedbackSent(usize, Option<bool>, Result<(), String>),
    SendFollowUp(usize),
    SendElicitation(usize, usize),

    // 노트북
    NewNotebook,
    OpenNotebook(String),
    NotebookTitleChanged(String),
    NotebookDescChanged(String),
    NotebookNotesAction(text_editor::Action),
    AddNotebookSource,
    NotebookSourcesPicked(Result<Vec<PathBuf>, String>),
    RemoveNotebookSource(usize),
    SaveNotebook,
    NotebookSaved(Result<Notebook, String>),
    DeleteNotebook(String),
    NotebookDeleted(Result<String, String>),
    ChatWithNotebook,

    // 예약 작업
    SetSchedFilter(String),
    SchedTitleChanged(String),
    SchedPromptChanged(String),
    SchedTypeChanged(String),
    SchedTimeChanged(String),
    CreateSchedule,
    QuickCreateSchedule,
    AddRecommendedSchedule(String, String, String, String),
    RecommendedScheduleAdded(Result<String, String>),
    ScheduleCreated(Result<ScheduledTask, String>),
    ToggleScheduleEnabled(String, bool),
    SchedulePatched(Result<ScheduledTask, String>),
    DeleteSchedule(String),
    ScheduleDeleted(Result<String, String>),
    RunScheduleNow(String),
    ScheduleRunFinished(String, Result<ScheduleRunResult, String>),

    // 라이브러리 · 이미지 · 동영상 · 학생/Gem
    SetLibraryFilter(String),
    AttachLibraryPath(String),
    ImagePromptChanged(String),
    SetImageAspect(String),
    SetImageStyle(String),
    GenerateImagePrompt,
    VideoPromptChanged(String),
    SetVideoAspect(String),
    SetVideoDuration(String),
    GenerateVideoPrompt,
    StartBotPrompt(String, String),
    StartBotChat(String),
    StartFeaturePrompt(i32, String),

    // 입력창 · 화면 공유 · 마이크 · 스트리밍
    SetComposerMode(ComposerMode),
    ToggleTemporary,
    ToggleMessageThoughts(usize),
    ToggleMessageTools(usize),
    OpenCanvasPreview(usize, usize),
    ToggleCanvasRaw,
    CloseCanvasPreview,
    RunCanvasHtml(usize, usize),
    CanvasHtmlOpened(Result<PathBuf, String>),
    RetryImageLoad(String, String),
    OpenFilePicker,
    FilesPicked(Result<Vec<PathBuf>, String>),
    StartScreenCapture,
    ScreenCaptured(Result<PathBuf, String>),
    ToggleVoiceRecording,
    VoiceRecordStopped(Result<PathBuf, String>),
    EditorAction(text_editor::Action),
    RemoveAttachment(usize),
    SendPrompt,
    RegenerateLast,
    StreamReceived(StreamEvent),
    AbortStream,
    AbortFinished(Result<(), String>),

    CopyText(String),
    CopyChatItem(usize),
    CopyCanvasPreview,
    CopyMarkdownTable(markdown_view::CopyTarget, usize),
    CopyMarkdownCode(markdown_view::CopyTarget, usize, usize),
    LinkClicked(markdown::Uri),
    OpenExternalUrl(String),
    DismissError,
}

impl App {
    fn main_window_settings() -> window::Settings {
        window::Settings {
            size: Size::new(1240.0, 860.0),
            min_size: Some(Size::new(780.0, 560.0)),
            exit_on_close_request: false,
            platform_specific: window::settings::PlatformSpecific {
                application_id: "gemini-ui".into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn new(start_hidden: bool) -> (Self, Task<Message>) {
        let palette = AppTheme::load_with_mode(ThemeMode::System, false);
        let cached_iced_theme = palette.to_iced_theme();
        let language_pref = LanguagePref::System;
        let locale = language_pref.resolve();
        let (main_window, open_window_task) = if start_hidden {
            (None, Task::none())
        } else {
            let (id, task) = window::open(Self::main_window_settings());
            (Some(id), task.map(Message::WindowOpened))
        };
        let app = Self {
            palette,
            cached_iced_theme,
            theme_mode: ThemeMode::System,
            language_pref,
            locale,
            system_dark_fallback: false,
            daemon_connected: false,
            daemon_label: t(locale, "데몬 연결 중...").into(),
            profile: UserProfile::default(),
            avatar_handle: None,
            devices: Vec::new(),
            remote_status: RemoteStatusSnapshot::default(),
            selected_device_id: None,
            device_menu_open: false,
            remote_busy: false,
            folder_input_draft: String::new(),
            usage_info: None,
            sidebar_open: true,
            sidebar_width: SIDEBAR_DEFAULT_WIDTH,
            sidebar_resizing: false,
            sidebar_resize_hovered: false,
            sidebar_resize_start_width: None,
            window_width: 1240.0,
            window_height: 820.0,
            sidebar_scroll_y: 0.0,
            hovered_conv_id: None,
            active_section: ActiveSection::Chat,
            more_menu_open: false,
            filter_menu_open: false,
            conv_filter: ConvFilter::All,
            conv_menu_target: None,
            conv_menu_notebook_sub: false,
            search_modal_open: false,
            search_query: String::new(),
            search_hits: Vec::new(),
            search_loading: false,
            settings_modal_open: false,
            canvas_preview: None,
            local_tz_offset_secs: detect_local_tz_offset_secs(),
            mode_picker_title: String::new(),
            models: Vec::new(),
            selected_model_id: None,
            model_menu_open: false,
            thinking_level: THINKING_MEDIUM,
            thinking_submenu_open: false,
            tools_menu_open: false,
            selected_feature_mode: 0,
            personal_intelligence: false,
            custom_instructions_draft: String::new(),
            mention_query: None,
            bots: Vec::new(),
            active_bot_id: None,
            notebooks: Vec::new(),
            editing_notebook: Notebook::default(),
            notebook_notes_editor: text_editor::Content::new(),
            schedules: Vec::new(),
            recommended_schedules: Vec::new(),
            sched_filter: "all".into(),
            sched_title_draft: String::new(),
            sched_prompt_draft: String::new(),
            sched_type_draft: "daily".into(),
            sched_time_draft: "09:00".into(),
            running_schedule_id: None,
            library_items: Vec::new(),
            library_filter: "all".into(),
            library_image_cache: HashMap::new(),
            library_image_order: VecDeque::new(),
            failed_image_keys: HashSet::new(),
            inflight_image_keys: HashSet::new(),
            image_prompt_draft: String::new(),
            image_aspect: "1:1".into(),
            image_style: "도시락".into(),
            video_prompt_draft: String::new(),
            video_aspect: "16:9".into(),
            video_duration: "5초".into(),
            conversations: Vec::new(),
            conversations_next_page_token: None,
            loading_more_conversations: false,
            active_conv_id: None,
            active_conv_is_agent: false,
            active_project_id: None,
            active_title: String::new(),
            parent_request_id: String::new(),
            parent_response_id: String::new(),
            pending_scroll_request_id: None,
            messages: Vec::new(),
            message_feedback: HashMap::new(),
            renaming_conv_id: None,
            rename_draft: String::new(),
            composer: text_editor::Content::new(),
            composer_has_text: false,
            composer_mode: ComposerMode::Chat,
            attached_files: Vec::new(),
            file_hovering: false,
            temporary_chat: false,
            screen_capturing: false,
            voice_recorder: None,
            voice_stopping: false,
            recording_seconds: 0,
            loading_history: false,
            conv_load_seq: 0,
            is_streaming: false,
            streaming_req_id: String::new(),
            stream_abort_handle: None,
            last_stream_reparse: None,
            last_stream_scroll: None,
            stream_markdown_dirty: false,
            stream_had_attachments: false,
            welcome_glow_cache: canvas::Cache::new(),
            avatar_mask_cache: canvas::Cache::new(),
            main_window,
            close_to_tray: true,
            tray_available: false,
            tray_tx: None,
            error_banner: None,
            toast: None,
        };

        let init = Task::batch([
            open_window_task,
            system::theme().map(Message::SystemThemeChanged),
            Task::perform(ipc::health(), Message::HealthLoaded),
            Task::perform(ipc::get_profile(), Message::ProfileLoaded),
            Task::perform(ipc::get_avatar_bytes(), Message::AvatarLoaded),
            Task::perform(ipc::get_preferences(), Message::PreferencesLoaded),
            Task::perform(ipc::get_usage(), Message::UsageLoaded),
            Task::perform(ipc::list_models(), Message::ModelsLoaded),
            Task::perform(ipc::list_bots(), Message::BotsLoaded),
            Task::perform(ipc::list_devices(), Message::DevicesLoaded),
            Task::perform(ipc::get_remote_status(), Message::RemoteStatusLoaded),
            Task::perform(ipc::list_conversations(60), Message::ConversationsLoaded),
            Task::perform(ipc::list_notebooks(), Message::NotebooksLoaded),
            Task::perform(ipc::list_schedules(), Message::SchedulesLoaded),
        ]);
        (app, init)
    }

    fn title(&self, _window: window::Id) -> String {
        if self.active_title.is_empty() {
            "Gemini".into()
        } else {
            format!("{} — Gemini", self.active_title)
        }
    }

    fn theme(&self, _window: window::Id) -> Theme {
        self.cached_iced_theme.clone()
    }

    fn sync_tray_state(&self) {
        if let Some(tx) = &self.tray_tx {
            tx.sync(
                self.main_window.is_some(),
                self.locale,
                self.daemon_connected,
                self.remote_status.remote_enabled,
                self.remote_status.punctual_connected,
                &self.remote_status.current_tool,
                self.is_streaming,
            );
        }
    }

    fn open_or_focus_window(&mut self) -> Task<Message> {
        if let Some(id) = self.main_window {
            return window::gain_focus(id);
        }
        let (id, open_task) = window::open(Self::main_window_settings());
        self.main_window = Some(id);
        open_task.map(Message::WindowOpened)
    }

    fn hide_window_to_tray(&mut self) -> Task<Message> {
        if let Some(id) = self.main_window.take() {
            self.release_hidden_window_caches();
            window::close(id)
        } else {
            Task::none()
        }
    }

    /// 창을 트레이로 숨길 때 재생성 가능한 이미지·캔버스 캐시와 토스트 타이머를 비워 백그라운드 RAM·깨어남을 줄인다.
    fn release_hidden_window_caches(&mut self) {
        self.library_image_cache.clear();
        self.library_image_order.clear();
        self.failed_image_keys.clear();
        self.clear_canvas_caches();
        self.toast = None;
    }

    fn sync_system_theme_if_needed(&mut self) {
        if self.theme_mode == ThemeMode::System {
            let cur = AppTheme::current_omarchy_generation().unwrap_or_default();
            if cur != self.palette.generation {
                let next = AppTheme::load_with_mode(self.theme_mode, self.system_dark_fallback);
                self.set_palette(next);
            }
        }
    }

    fn preload_active_chat_images(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut preload_count = 0usize;
        for msg in &self.messages {
            for att in &msg.attachment_items {
                if preload_count < 24 && is_attachment_image(att) && !att.url.is_empty() {
                    let key = markdown_view::turn_attachment_cache_key(&att.url);
                    if !self.library_image_cache.contains_key(&key)
                        && !self.failed_image_keys.contains(&key)
                        && self.inflight_image_keys.insert(key.clone())
                    {
                        let url = att.url.clone();
                        tasks.push(Task::perform(ipc::fetch_library_image(url), move |r| {
                            Message::LibraryImageLoaded(key, r)
                        }));
                        preload_count += 1;
                    }
                }
            }
            for img in &msg.generated_images {
                if preload_count < 24 && !img.url.is_empty() {
                    let key = markdown_view::generated_image_cache_key(&img.url);
                    if !self.library_image_cache.contains_key(&key)
                        && !self.failed_image_keys.contains(&key)
                        && self.inflight_image_keys.insert(key.clone())
                    {
                        let url = img.url.clone();
                        tasks.push(Task::perform(ipc::fetch_library_image(url), move |r| {
                            Message::LibraryImageLoaded(key, r)
                        }));
                        preload_count += 1;
                    }
                }
            }
            if preload_count >= 24 {
                break;
            }
        }
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    fn preload_visible_images(&mut self) -> Task<Message> {
        match self.active_section {
            ActiveSection::Library | ActiveSection::Images => self.preload_library_images(),
            ActiveSection::Chat => self.preload_active_chat_images(),
            _ => Task::none(),
        }
    }

    fn quit_app(&mut self) -> Task<Message> {
        self.cancel_active_stream();
        if let Some(rec) = self.voice_recorder.take() {
            drop(rec);
        }
        iced::exit()
    }

    fn set_palette(&mut self, next: AppTheme) {
        self.cached_iced_theme = next.to_iced_theme();
        self.palette = next;
        self.clear_canvas_caches();
    }

    fn reset_chat_state(&mut self, clear_bot: bool) {
        self.cancel_active_stream();
        self.active_section = ActiveSection::Chat;
        if clear_bot {
            self.active_bot_id = None;
        }
        self.active_conv_id = None;
        self.active_conv_is_agent = false;
        self.active_project_id = None;
        self.active_title.clear();
        self.parent_request_id.clear();
        self.parent_response_id.clear();
        self.pending_scroll_request_id = None;
        self.messages.clear();
        self.message_feedback.clear();
        self.renaming_conv_id = None;
        self.canvas_preview = None;
        self.loading_history = false;
        self.error_banner = None;
        self.conv_load_seq = self.conv_load_seq.wrapping_add(1);
    }

    fn clear_canvas_caches(&mut self) {
        self.welcome_glow_cache.clear();
        self.avatar_mask_cache.clear();
    }

    fn preload_library_images(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        for item in &self.library_items {
            if item.kind == "image"
                && !item.url.is_empty()
                && !self.library_image_cache.contains_key(&item.id)
                && !self.failed_image_keys.contains(&item.id)
                && !self.inflight_image_keys.contains(&item.id)
            {
                let id = item.id.clone();
                let url = item.url.clone();
                self.inflight_image_keys.insert(id.clone());
                tasks.push(Task::perform(ipc::fetch_library_image(url), move |r| {
                    Message::LibraryImageLoaded(id, r)
                }));
                if tasks.len() >= 24 {
                    break;
                }
            }
        }
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            system::theme_changes().map(Message::SystemThemeChanged),
            window::close_requests().map(Message::WindowCloseRequested),
            window::close_events().map(Message::WindowClosed),
            Subscription::run(tray::run_tray_service).map(Message::TrayEventReceived),
            event::listen_with(|ev, _status, _id| {
                let interested = match &ev {
                    Event::Window(
                        window::Event::FileHovered(_)
                        | window::Event::FilesHoveredLeft
                        | window::Event::FileDropped(_)
                        | window::Event::Resized(_),
                    ) => true,
                    Event::Keyboard(keyboard::Event::KeyPressed {
                        key, modifiers, ..
                    }) => {
                        if modifiers.control() {
                            matches!(
                                key,
                                keyboard::Key::Character(ch)
                                    if matches!(ch.as_str(), "n" | "N" | "b" | "B" | "k" | "K" | "q" | "Q" | ",")
                            )
                        } else {
                            matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape))
                        }
                    }
                    _ => false,
                };
                if interested {
                    Some(Message::RuntimeEvent(ev))
                } else {
                    None
                }
            }),
            Subscription::run(ipc::stream_remote_events).map(Message::RemoteEventReceived),
        ];

        // 사이드바 구분선 드래그 중일 때만 커서 이동/해제 이벤트를 구독해 평소 유휴 리드로우를 0으로 유지한다.
        if self.sidebar_resizing {
            subs.push(event::listen_with(|ev, _status, _id| {
                let interested = matches!(
                    ev,
                    Event::Mouse(
                        mouse::Event::CursorMoved { .. }
                            | mouse::Event::ButtonReleased(mouse::Button::Left)
                    ) | Event::Window(window::Event::Unfocused)
                );
                if interested {
                    Some(Message::SidebarResizeEvent(ev))
                } else {
                    None
                }
            }));
        }

        // 녹음 중이거나 토스트 표시 중일 때만 1초 타이머를 구동해 유휴 리드로우를 0으로 줄인다.
        if self.voice_recorder.is_some() || self.toast.is_some() {
            subs.push(time::every(Duration::from_secs(1)).map(|_| Message::Tick));
        }

        // 창이 표시 중이고 System 테마 모드일 때만 3초 간격 Omarchy 테마 변경을 감시한다.
        // 트레이 숨김 중에는 주기 깨어남을 0으로 유지하고, 창을 다시 열 때 즉시 1회 동기화한다.
        if self.main_window.is_some() && self.theme_mode == ThemeMode::System {
            subs.push(Subscription::run(watch_omarchy_theme).map(Message::OmarchyThemeChanged));
        }

        Subscription::batch(subs)
    }

    fn scroll_to_bottom() -> Task<Message> {
        operation::snap_to(CHAT_SCROLL_ID, operation::RelativeOffset::END)
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), 3));
    }

    pub(crate) fn picker_models(&self) -> Vec<&BardModeOption> {
        let in_picker: Vec<&BardModeOption> =
            self.models.iter().filter(|m| m.in_picker).collect();
        if !in_picker.is_empty() {
            in_picker
        } else {
            self.models.iter().collect()
        }
    }

    pub(crate) fn selected_model(&self) -> Option<&BardModeOption> {
        if let Some(id) = &self.selected_model_id
            && let Some(found) = self.models.iter().find(|m| &m.id == id)
        {
            return Some(found);
        }
        self.picker_models().into_iter().next()
    }

    pub(crate) fn find_bot(&self, id: &str) -> Option<&BotSummary> {
        self.bots.iter().find(|b| b.id == id)
    }

    /// 서버에 등록된 기기와 이 PC 데몬 상태를 병합한 선택 가능 원격 기기 목록을 반환한다.
    pub(crate) fn available_remote_devices(&self) -> Vec<RegisteredDevice> {
        let mut out: Vec<RegisteredDevice> = self.devices.clone();
        if !self.remote_status.device_id.is_empty() {
            if let Some(local) = out
                .iter_mut()
                .find(|d| d.device_id.eq_ignore_ascii_case(&self.remote_status.device_id))
            {
                local.remote_enabled = self.remote_status.remote_enabled;
                if self.remote_status.tool_count > 0 {
                    local.tool_count = self.remote_status.tool_count;
                }
                local.folders = Some(self.remote_status.connected_folders.clone());
            } else {
                out.insert(
                    0,
                    RegisteredDevice {
                        device_id: self.remote_status.device_id.clone(),
                        device_name: self.remote_status.device_name.clone(),
                        remote_enabled: self.remote_status.remote_enabled,
                        tool_count: self.remote_status.tool_count,
                        folders: Some(self.remote_status.connected_folders.clone()),
                    },
                );
            }
        }
        out
    }

    /// 현재 태스크 모드에서 대상으로 선택된 원격 기기를 반환한다.
    pub(crate) fn selected_target_device(&self) -> Option<RegisteredDevice> {
        let all = self.available_remote_devices();
        if let Some(id) = &self.selected_device_id
            && let Some(found) = all.iter().find(|d| d.device_id.eq_ignore_ascii_case(id))
        {
            return Some(found.clone());
        }
        all.iter()
            .find(|d| d.remote_enabled)
            .or_else(|| all.first())
            .cloned()
    }

    fn ensure_selected_device(&mut self) {
        let all = self.available_remote_devices();
        let valid = self
            .selected_device_id
            .as_ref()
            .is_some_and(|id| all.iter().any(|d| d.device_id.eq_ignore_ascii_case(id)));
        if !valid {
            self.selected_device_id = all
                .iter()
                .find(|d| d.remote_enabled)
                .or_else(|| all.first())
                .map(|d| d.device_id.clone());
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.update_inner(message);
        self.sync_tray_state();
        task
    }

    fn update_inner(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowOpened(id) => {
                self.main_window = Some(id);
                self.sync_system_theme_if_needed();
                let mut tasks = Vec::new();
                if self.theme_mode == ThemeMode::System {
                    tasks.push(system::theme().map(Message::SystemThemeChanged));
                }
                tasks.push(self.preload_visible_images());
                if !self.messages.is_empty() {
                    tasks.push(Self::scroll_to_bottom());
                }
                Task::batch(tasks)
            }
            Message::WindowCloseRequested(id) => {
                if self.close_to_tray && self.tray_available {
                    if self.main_window == Some(id) {
                        self.main_window = None;
                        self.release_hidden_window_caches();
                    }
                    window::close(id)
                } else {
                    self.quit_app()
                }
            }
            Message::WindowClosed(id) => {
                if self.main_window == Some(id) {
                    self.main_window = None;
                    self.release_hidden_window_caches();
                }
                Task::none()
            }
            Message::TrayEventReceived(ev) => match ev {
                tray::TrayEvent::Ready { available, updater } => {
                    self.tray_available = available;
                    self.tray_tx = updater;
                    Task::none()
                }
                tray::TrayEvent::WatcherAvailabilityChanged(available) => {
                    self.tray_available = available;
                    Task::none()
                }
                tray::TrayEvent::Action(action) => match action {
                    tray::TrayAction::ShowWindow => self.open_or_focus_window(),
                    tray::TrayAction::ToggleWindow => {
                        if self.main_window.is_some() {
                            self.hide_window_to_tray()
                        } else {
                            self.open_or_focus_window()
                        }
                    }
                    tray::TrayAction::NewChat => {
                        let open = self.open_or_focus_window();
                        let chat = self.update_inner(Message::NewChat);
                        Task::batch([open, chat])
                    }
                    tray::TrayAction::ToggleRemote => {
                        self.update_inner(Message::ToggleRemoteEnabled)
                    }
                    tray::TrayAction::OpenSettings => {
                        let open = self.open_or_focus_window();
                        let settings = self.update_inner(Message::OpenSettingsModal);
                        Task::batch([open, settings])
                    }
                    tray::TrayAction::Quit => self.quit_app(),
                },
            },
            Message::ToggleCloseToTray => {
                self.close_to_tray = !self.close_to_tray;
                let enabled = self.close_to_tray;
                self.show_toast(t(
                    self.locale,
                    if enabled {
                        "창 닫기 시 트레이 최소화를 켰습니다"
                    } else {
                        "창 닫기 시 앱을 바로 종료합니다"
                    },
                ));
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "close_to_tray": enabled })),
                    Message::PreferencesSaved,
                )
            }
            Message::Tick => {
                if self.voice_recorder.is_some() {
                    self.recording_seconds = self.recording_seconds.saturating_add(1);
                }
                if let Some((_, ttl)) = &mut self.toast {
                    if *ttl <= 1 {
                        self.toast = None;
                    } else {
                        *ttl -= 1;
                    }
                }
                Task::none()
            }
            Message::OmarchyThemeChanged(generation) => {
                if self.theme_mode == ThemeMode::System && generation != self.palette.generation {
                    let next =
                        AppTheme::load_with_mode(self.theme_mode, self.system_dark_fallback);
                    self.set_palette(next);
                }
                Task::none()
            }
            Message::SystemThemeChanged(mode) => {
                self.system_dark_fallback = matches!(mode, iced::theme::Mode::Dark);
                if self.theme_mode == ThemeMode::System {
                    let next =
                        AppTheme::load_with_mode(self.theme_mode, self.system_dark_fallback);
                    self.set_palette(next);
                }
                Task::none()
            }
            Message::RuntimeEvent(ev) => match ev {
                Event::Window(window::Event::Resized(size)) => {
                    self.window_width = size.width;
                    self.window_height = size.height;
                    self.sidebar_width = clamp_sidebar_width(self.sidebar_width, self.window_width);
                    Task::none()
                }
                Event::Window(window::Event::FileHovered(_)) => {
                    self.file_hovering = true;
                    Task::none()
                }
                Event::Window(window::Event::FilesHoveredLeft) => {
                    self.file_hovering = false;
                    Task::none()
                }
                Event::Window(window::Event::FileDropped(path)) => {
                    self.file_hovering = false;
                    if path.is_file() && !self.attached_files.contains(&path) {
                        let name = path
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.display().to_string());
                        self.attached_files.push(path);
                        self.show_toast(if self.locale.is_en() {
                            format!("Attached file: {name}")
                        } else {
                            format!("파일 첨부됨: {name}")
                        });
                    }
                    Task::none()
                }
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key, modifiers, ..
                }) => {
                    if modifiers.control() {
                        if let keyboard::Key::Character(ref ch) = key {
                            match ch.as_str() {
                                "n" | "N" => return self.update_inner(Message::NewChat),
                                "b" | "B" => return self.update_inner(Message::ToggleSidebar),
                                "k" | "K" => return self.update_inner(Message::OpenSearchModal),
                                "q" | "Q" => return self.quit_app(),
                                "," => return self.update_inner(Message::OpenSettingsModal),
                                _ => {}
                            }
                        }
                    } else if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        if self.canvas_preview.is_some() {
                            self.canvas_preview = None;
                            return Task::none();
                        }
                        if self.search_modal_open {
                            self.search_modal_open = false;
                            return Task::none();
                        }
                        if self.settings_modal_open {
                            self.settings_modal_open = false;
                            return Task::none();
                        }
                        if self.tools_menu_open {
                            self.tools_menu_open = false;
                            return Task::none();
                        }
                        if self.model_menu_open {
                            self.model_menu_open = false;
                            self.thinking_submenu_open = false;
                            return Task::none();
                        }
                        if self.device_menu_open {
                            self.device_menu_open = false;
                            return Task::none();
                        }
                        if self.more_menu_open {
                            self.more_menu_open = false;
                            return Task::none();
                        }
                        if self.filter_menu_open {
                            self.filter_menu_open = false;
                            return Task::none();
                        }
                        if self.conv_menu_target.is_some() {
                            self.conv_menu_target = None;
                            self.conv_menu_notebook_sub = false;
                            return Task::none();
                        }
                        if self.mention_query.is_some() {
                            self.mention_query = None;
                            return Task::none();
                        }
                        if self.renaming_conv_id.is_some() {
                            return self.update(Message::CancelRename);
                        }
                        if self.is_streaming {
                            return self.update(Message::AbortStream);
                        }
                    }
                    Task::none()
                }
                _ => Task::none(),
            },
            Message::HealthLoaded(Ok(h)) => {
                self.daemon_connected = h.ok;
                self.daemon_label = if h.ok {
                    h.device_name
                } else {
                    t(self.locale, "데몬 비정상").into()
                };
                Task::none()
            }
            Message::HealthLoaded(Err(e)) => {
                self.daemon_connected = false;
                self.daemon_label = t(self.locale, "데몬 미연결").into();
                self.error_banner = Some(if self.locale.is_en() {
                    format!("{e}\nStart the daemon with `systemctl --user start gemini-remote.service`.")
                } else {
                    format!("{e}\n`systemctl --user start gemini-remote.service`로 데몬을 시작하세요.")
                });
                Task::none()
            }
            Message::ProfileLoaded(Ok(p)) => {
                self.profile = p;
                Task::none()
            }
            Message::ProfileLoaded(Err(_)) => Task::none(),
            Message::AvatarLoaded(Ok(bytes)) => {
                if !bytes.is_empty() {
                    self.avatar_handle = Some(image::Handle::from_bytes(bytes));
                }
                Task::none()
            }
            Message::AvatarLoaded(Err(_)) => Task::none(),
            Message::ModelsLoaded(Ok(res)) => {
                self.mode_picker_title = res.mode_picker_title;
                if self.profile.tier.is_empty() && !res.tier.is_empty() {
                    self.profile.tier = res.tier;
                }
                self.models = res.models;
                let keep_valid = self
                    .selected_model_id
                    .as_ref()
                    .is_some_and(|id| self.models.iter().any(|m| &m.id == id));
                if !keep_valid {
                    self.selected_model_id = self
                        .picker_models()
                        .first()
                        .map(|m| m.id.clone());
                }
                Task::none()
            }
            Message::ModelsLoaded(Err(_)) => Task::none(),
            Message::BotsLoaded(Ok(res)) => {
                self.bots = res.bots;
                Task::none()
            }
            Message::BotsLoaded(Err(_)) => Task::none(),
            Message::DevicesLoaded(Ok(res)) => {
                self.devices = res.devices;
                self.ensure_selected_device();
                Task::none()
            }
            Message::DevicesLoaded(Err(_)) => Task::none(),
            Message::RemoteStatusLoaded(Ok(snap)) => {
                self.remote_status = snap;
                self.ensure_selected_device();
                Task::none()
            }
            Message::RemoteStatusLoaded(Err(_)) => Task::none(),
            Message::RemoteEventReceived(ev) => match ev {
                RemoteEvent::Status(snap) => {
                    self.remote_status = snap;
                    self.ensure_selected_device();
                    Task::none()
                }
                RemoteEvent::Tool(tool_ev) => {
                    if tool_ev.phase == "status_changed" {
                        if tool_ev.tool_name == "conversations_updated" {
                            return Task::perform(
                                ipc::list_conversations(60),
                                Message::ConversationsLoaded,
                            );
                        }
                        if let Some(task_name) = tool_ev.tool_name.strip_prefix("scheduled_task:") {
                            self.show_toast(if self.locale.is_en() {
                                format!("Scheduled task finished: {task_name}")
                            } else {
                                format!("예약 작업 완료: {task_name}")
                            });
                            return Task::batch([
                                Task::perform(
                                    ipc::list_conversations(60),
                                    Message::ConversationsLoaded,
                                ),
                                Task::perform(ipc::list_schedules(), Message::SchedulesLoaded),
                            ]);
                        }
                        return Task::none();
                    }
                    match tool_ev.phase.as_str() {
                        "started" | "start" => {
                            self.remote_status.current_tool = tool_ev.tool_name.clone();
                        }
                        "finished" | "done" | "error" | "session_ended" => {
                            self.remote_status.current_tool.clear();
                        }
                        _ => {}
                    }
                    self.remote_status.recent_events.insert(0, tool_ev);
                    self.remote_status.recent_events.truncate(20);
                    Task::none()
                }
            },
            Message::ToggleDeviceMenu => {
                self.device_menu_open = !self.device_menu_open;
                if self.device_menu_open {
                    self.model_menu_open = false;
                }
                Task::none()
            }
            Message::CloseDeviceMenu => {
                self.device_menu_open = false;
                Task::none()
            }
            Message::SelectDevice(id) => {
                self.selected_device_id = Some(id);
                self.device_menu_open = false;
                Task::none()
            }
            Message::ToggleRemoteEnabled => {
                if self.remote_busy {
                    return Task::none();
                }
                self.remote_busy = true;
                let next = !self.remote_status.remote_enabled;
                let label = t(
                    self.locale,
                    if next {
                        "원격 제어를 켰습니다"
                    } else {
                        "원격 제어를 껐습니다"
                    },
                )
                .to_string();
                Task::perform(ipc::toggle_remote(next), move |res| {
                    Message::RemoteActionDone(label, res)
                })
            }
            Message::SyncRemoteRegistration => {
                if self.remote_busy {
                    return Task::none();
                }
                self.remote_busy = true;
                let msg = t(self.locale, "기기 등록 상태를 동기화했습니다").to_string();
                Task::perform(ipc::register_remote(), move |res| {
                    Message::RemoteActionDone(msg, res)
                })
            }
            Message::PickRemoteFolder => {
                if self.remote_busy {
                    return Task::none();
                }
                Task::perform(filepicker::pick_folder(), Message::RemoteFolderPicked)
            }
            Message::RemoteFolderPicked(Ok(Some(path))) => {
                self.remote_busy = true;
                let path_str = path.to_string_lossy().to_string();
                let msg = t(self.locale, "허용 폴더를 추가했습니다").to_string();
                Task::perform(ipc::add_remote_folder(path_str), move |res| {
                    Message::RemoteActionDone(msg, res)
                })
            }
            Message::RemoteFolderPicked(Ok(None)) => Task::none(),
            Message::RemoteFolderPicked(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to select folder: {e}")
                } else {
                    format!("폴더 선택 실패: {e}")
                });
                Task::none()
            }
            Message::FolderDraftChanged(val) => {
                self.folder_input_draft = val;
                Task::none()
            }
            Message::AddRemoteFolderManual => {
                let raw = self.folder_input_draft.trim().to_string();
                if raw.is_empty() || self.remote_busy {
                    return Task::none();
                }
                self.remote_busy = true;
                self.folder_input_draft.clear();
                let msg = t(self.locale, "허용 폴더를 추가했습니다").to_string();
                Task::perform(ipc::add_remote_folder(raw), move |res| {
                    Message::RemoteActionDone(msg, res)
                })
            }
            Message::RemoveRemoteFolder(path) => {
                if self.remote_busy {
                    return Task::none();
                }
                self.remote_busy = true;
                let msg = t(self.locale, "허용 폴더를 제거했습니다").to_string();
                Task::perform(ipc::remove_remote_folder(path), move |res| {
                    Message::RemoteActionDone(msg, res)
                })
            }
            Message::RemoteActionDone(toast_msg, Ok(snap)) => {
                self.remote_busy = false;
                self.remote_status = snap;
                self.ensure_selected_device();
                self.show_toast(toast_msg);
                Task::perform(ipc::list_devices(), Message::DevicesLoaded)
            }
            Message::RemoteActionDone(_, Err(e)) => {
                self.remote_busy = false;
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to update remote control: {e}")
                } else {
                    format!("원격 제어 변경 실패: {e}")
                });
                Task::none()
            }
            Message::NotebooksLoaded(Ok(res)) => {
                self.notebooks = res.notebooks;
                Task::none()
            }
            Message::NotebooksLoaded(Err(_)) => Task::none(),
            Message::SchedulesLoaded(Ok(res)) => {
                self.schedules = res.schedules;
                self.recommended_schedules = res.recommended;
                Task::none()
            }
            Message::SchedulesLoaded(Err(_)) => Task::none(),
            Message::LibraryLoaded(Ok(res)) => {
                self.library_items = res.items;
                if self.main_window.is_some()
                    && matches!(
                        self.active_section,
                        ActiveSection::Library | ActiveSection::Images
                    )
                {
                    self.preload_library_images()
                } else {
                    Task::none()
                }
            }
            Message::LibraryLoaded(Err(_)) => Task::none(),
            Message::LibraryImageLoaded(id, Ok(bytes)) => {
                self.inflight_image_keys.remove(&id);
                if self.main_window.is_none() {
                    return Task::none();
                }
                if bytes.is_empty() {
                    self.failed_image_keys.insert(id);
                } else {
                    self.failed_image_keys.remove(&id);
                    self.cache_library_image(id, image::Handle::from_bytes(bytes));
                }
                Task::none()
            }
            Message::LibraryImageLoaded(id, Err(_)) => {
                self.inflight_image_keys.remove(&id);
                self.failed_image_keys.insert(id);
                Task::none()
            }
            Message::RetryImageLoad(key, url) => {
                self.failed_image_keys.remove(&key);
                if url.trim().is_empty() || !self.inflight_image_keys.insert(key.clone()) {
                    return Task::none();
                }
                Task::perform(ipc::fetch_library_image(url), move |r| {
                    Message::LibraryImageLoaded(key, r)
                })
            }
            Message::PreferencesLoaded(Ok(prefs)) => {
                if !prefs.selected_model_id.is_empty() {
                    self.selected_model_id = Some(prefs.selected_model_id);
                }
                if prefs.thinking_level > 0 {
                    self.thinking_level = prefs.thinking_level;
                }
                let next_mode = ThemeMode::from_str(&prefs.theme_mode);
                if next_mode != self.theme_mode {
                    self.theme_mode = next_mode;
                    let next =
                        AppTheme::load_with_mode(self.theme_mode, self.system_dark_fallback);
                    self.set_palette(next);
                }
                let next_lang = LanguagePref::from_str(&prefs.language);
                self.language_pref = next_lang;
                self.locale = next_lang.resolve();
                self.personal_intelligence = prefs.personal_intelligence;
                self.custom_instructions_draft = prefs.custom_instructions;
                self.close_to_tray = prefs.close_to_tray;
                if prefs.sidebar_width > 0 {
                    self.sidebar_width =
                        clamp_sidebar_width(prefs.sidebar_width as f32, self.window_width);
                }
                Task::none()
            }
            Message::PreferencesLoaded(Err(_)) => Task::none(),
            Message::PreferencesSaved(Ok(_)) => Task::none(),
            Message::PreferencesSaved(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to save preferences: {e}")
                } else {
                    format!("환경설정 저장 실패: {e}")
                });
                Task::none()
            }
            Message::UsageLoaded(Ok(u)) => {
                self.usage_info = Some(u);
                Task::none()
            }
            Message::UsageLoaded(Err(_)) => Task::none(),
            Message::RefreshAll => {
                self.error_banner = None;
                self.model_menu_open = false;
                self.thinking_submenu_open = false;
                self.device_menu_open = false;
                self.tools_menu_open = false;
                self.more_menu_open = false;
                self.filter_menu_open = false;
                self.conv_menu_target = None;
                Task::batch([
                    Task::perform(ipc::health(), Message::HealthLoaded),
                    Task::perform(ipc::get_profile(), Message::ProfileLoaded),
                    Task::perform(ipc::get_usage(), Message::UsageLoaded),
                    Task::perform(ipc::list_models(), Message::ModelsLoaded),
                    Task::perform(ipc::list_bots(), Message::BotsLoaded),
                    Task::perform(ipc::list_devices(), Message::DevicesLoaded),
                    Task::perform(ipc::get_remote_status(), Message::RemoteStatusLoaded),
                    Task::perform(ipc::list_conversations(60), Message::ConversationsLoaded),
                    Task::perform(ipc::list_notebooks(), Message::NotebooksLoaded),
                    Task::perform(ipc::list_schedules(), Message::SchedulesLoaded),
                    Task::perform(ipc::list_library(), Message::LibraryLoaded),
                ])
            }
            Message::ToggleSidebar => {
                self.sidebar_open = !self.sidebar_open;
                if !self.sidebar_open {
                    self.sidebar_resizing = false;
                    self.sidebar_resize_hovered = false;
                    self.sidebar_resize_start_width = None;
                }
                Task::none()
            }
            Message::StartSidebarResize => {
                if self.sidebar_open {
                    self.sidebar_resizing = true;
                    self.sidebar_resize_start_width = Some(self.sidebar_width);
                }
                Task::none()
            }
            Message::SidebarResizeHover(hovered) => {
                self.sidebar_resize_hovered = hovered;
                Task::none()
            }
            Message::ResetSidebarWidth => {
                self.sidebar_resizing = false;
                self.sidebar_resize_start_width = None;
                let next = clamp_sidebar_width(SIDEBAR_DEFAULT_WIDTH, self.window_width);
                if (self.sidebar_width - next).abs() < 0.5 {
                    return Task::none();
                }
                self.sidebar_width = next;
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "sidebar_width": next.round() as i32 })),
                    Message::PreferencesSaved,
                )
            }
            Message::SidebarResizeEvent(ev) => match ev {
                Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    if self.sidebar_resizing {
                        let next = clamp_sidebar_width(position.x, self.window_width);
                        if (self.sidebar_width - next).abs() >= 1.0 {
                            self.sidebar_width = next;
                        }
                    }
                    Task::none()
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                | Event::Window(window::Event::Unfocused) => {
                    if !self.sidebar_resizing {
                        return Task::none();
                    }
                    self.sidebar_resizing = false;
                    let start = self.sidebar_resize_start_width.take();
                    if start.is_some_and(|w| (self.sidebar_width - w).abs() < 1.0) {
                        return Task::none();
                    }
                    let saved_w = self.sidebar_width.round() as i32;
                    Task::perform(
                        ipc::save_preferences(serde_json::json!({ "sidebar_width": saved_w })),
                        Message::PreferencesSaved,
                    )
                }
                _ => Task::none(),
            },
            Message::SwitchSection(sec) => {
                self.model_menu_open = false;
                self.thinking_submenu_open = false;
                self.tools_menu_open = false;
                self.more_menu_open = false;
                self.filter_menu_open = false;
                self.conv_menu_target = None;
                self.active_section = sec.clone();
                match sec {
                    ActiveSection::Library | ActiveSection::Images => Task::batch([
                        Task::perform(ipc::list_library(), Message::LibraryLoaded),
                        self.preload_library_images(),
                    ]),
                    ActiveSection::Schedules => {
                        Task::perform(ipc::list_schedules(), Message::SchedulesLoaded)
                    }
                    ActiveSection::Notebooks => {
                        Task::perform(ipc::list_notebooks(), Message::NotebooksLoaded)
                    }
                    ActiveSection::GemsHub => Task::perform(ipc::list_bots(), Message::BotsLoaded),
                    _ => Task::none(),
                }
            }
            Message::ToggleMoreMenu => {
                self.more_menu_open = !self.more_menu_open;
                if self.more_menu_open {
                    self.filter_menu_open = false;
                    self.conv_menu_target = None;
                }
                Task::none()
            }
            Message::CloseMoreMenu => {
                self.more_menu_open = false;
                Task::none()
            }
            Message::ToggleFilterMenu => {
                self.filter_menu_open = !self.filter_menu_open;
                if self.filter_menu_open {
                    self.more_menu_open = false;
                    self.conv_menu_target = None;
                }
                Task::none()
            }
            Message::CloseFilterMenu => {
                self.filter_menu_open = false;
                Task::none()
            }
            Message::SetConvFilter(f) => {
                self.conv_filter = f;
                self.filter_menu_open = false;
                Task::none()
            }
            Message::OpenConvMenu(conv_id) => {
                let same = self.conv_menu_target.as_deref() == Some(conv_id.as_str());
                self.conv_menu_target = if same { None } else { Some(conv_id) };
                self.conv_menu_notebook_sub = false;
                self.more_menu_open = false;
                self.filter_menu_open = false;
                Task::none()
            }
            Message::CloseConvMenu => {
                self.conv_menu_target = None;
                self.conv_menu_notebook_sub = false;
                Task::none()
            }
            Message::ToggleConvMenuNotebookSub => {
                self.conv_menu_notebook_sub = !self.conv_menu_notebook_sub;
                Task::none()
            }
            Message::HoverConv(conv_id) => {
                self.hovered_conv_id = Some(conv_id);
                Task::none()
            }
            Message::LeaveConv(conv_id) => {
                if self.hovered_conv_id.as_deref() == Some(conv_id.as_str()) {
                    self.hovered_conv_id = None;
                }
                Task::none()
            }
            Message::SidebarScrolled(y) => {
                self.sidebar_scroll_y = y;
                Task::none()
            }
            Message::OpenSearchModal => {
                self.search_modal_open = true;
                self.model_menu_open = false;
                self.thinking_submenu_open = false;
                self.tools_menu_open = false;
                self.device_menu_open = false;
                self.more_menu_open = false;
                self.filter_menu_open = false;
                self.conv_menu_target = None;
                self.conv_menu_notebook_sub = false;
                self.settings_modal_open = false;
                operation::focus(SEARCH_INPUT_ID)
            }
            Message::CloseSearchModal => {
                self.search_modal_open = false;
                Task::none()
            }
            Message::SearchQueryChanged(q) => {
                self.search_query = q;
                if self.search_query.trim().is_empty() {
                    self.search_hits.clear();
                }
                Task::none()
            }
            Message::SubmitServerSearch => {
                let q = self.search_query.trim().to_string();
                if q.is_empty() {
                    self.search_hits.clear();
                    return Task::none();
                }
                self.search_loading = true;
                Task::perform(ipc::search_conversations(q), Message::ServerSearchLoaded)
            }
            Message::ServerSearchLoaded(Ok(res)) => {
                self.search_loading = false;
                self.search_hits = res.hits;
                Task::none()
            }
            Message::ServerSearchLoaded(Err(e)) => {
                self.search_loading = false;
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Conversation search failed: {e}")
                } else {
                    format!("대화 검색 실패: {e}")
                });
                Task::none()
            }
            Message::OpenSettingsModal => {
                self.settings_modal_open = true;
                self.search_modal_open = false;
                self.model_menu_open = false;
                self.thinking_submenu_open = false;
                self.tools_menu_open = false;
                self.device_menu_open = false;
                self.more_menu_open = false;
                self.filter_menu_open = false;
                self.conv_menu_target = None;
                Task::batch([
                    Task::perform(ipc::list_devices(), Message::DevicesLoaded),
                    Task::perform(ipc::get_remote_status(), Message::RemoteStatusLoaded),
                    Task::perform(ipc::get_usage(), Message::UsageLoaded),
                ])
            }
            Message::CloseSettingsModal => {
                self.settings_modal_open = false;
                Task::none()
            }
            Message::SetThemeMode(mode) => {
                self.theme_mode = mode;
                let next = AppTheme::load_with_mode(mode, self.system_dark_fallback);
                self.set_palette(next);
                let lbl = mode.label_for(self.locale);
                self.show_toast(if self.locale.is_en() {
                    format!("Theme: {lbl}")
                } else {
                    format!("테마: {lbl}")
                });
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "theme_mode": mode.as_str() })),
                    Message::PreferencesSaved,
                )
            }
            Message::SetLanguagePref(pref) => {
                self.language_pref = pref;
                self.locale = pref.resolve();
                let lbl = pref.label(self.locale);
                self.show_toast(if self.locale.is_en() {
                    format!("Language: {lbl}")
                } else {
                    format!("언어: {lbl}")
                });
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "language": pref.as_str() })),
                    Message::PreferencesSaved,
                )
            }
            Message::TogglePersonalIntelligence => {
                self.personal_intelligence = !self.personal_intelligence;
                let enabled = self.personal_intelligence;
                self.show_toast(t(
                    self.locale,
                    if enabled {
                        "개인 인텔리전스를 켰습니다"
                    } else {
                        "개인 인텔리전스를 껐습니다"
                    },
                ));
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "personal_intelligence": enabled })),
                    Message::PreferencesSaved,
                )
            }
            Message::CustomInstructionsChanged(val) => {
                self.custom_instructions_draft = val;
                Task::none()
            }
            Message::SaveCustomInstructions => {
                let text = self.custom_instructions_draft.trim().to_string();
                self.show_toast(t(self.locale, "맞춤 지침을 저장했습니다"));
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "custom_instructions": text })),
                    Message::PreferencesSaved,
                )
            }
            Message::ToggleModelMenu => {
                self.model_menu_open = !self.model_menu_open;
                self.thinking_submenu_open = false;
                if self.model_menu_open {
                    self.device_menu_open = false;
                    self.tools_menu_open = false;
                }
                Task::none()
            }
            Message::CloseModelMenu => {
                self.model_menu_open = false;
                self.thinking_submenu_open = false;
                Task::none()
            }
            Message::SelectModel(id) => {
                self.selected_model_id = Some(id.clone());
                self.model_menu_open = false;
                self.thinking_submenu_open = false;
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "selected_model_id": id })),
                    Message::PreferencesSaved,
                )
            }
            Message::ToggleThinkingSubmenu => {
                self.thinking_submenu_open = !self.thinking_submenu_open;
                Task::none()
            }
            Message::SelectThinkingLevel(lv) => {
                self.thinking_level = lv;
                self.thinking_submenu_open = false;
                self.model_menu_open = false;
                let label = t(
                    self.locale,
                    match lv {
                        THINKING_LOW => "낮음",
                        THINKING_HIGH => "높음",
                        _ => "보통",
                    },
                );
                self.show_toast(if self.locale.is_en() {
                    format!("Thinking level: {label}")
                } else {
                    format!("사고 수준: {label}")
                });
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "thinking_level": lv })),
                    Message::PreferencesSaved,
                )
            }
            Message::ToggleToolsMenu => {
                self.tools_menu_open = !self.tools_menu_open;
                if self.tools_menu_open {
                    self.model_menu_open = false;
                    self.thinking_submenu_open = false;
                    self.device_menu_open = false;
                }
                Task::none()
            }
            Message::CloseToolsMenu => {
                self.tools_menu_open = false;
                Task::none()
            }
            Message::SelectFeatureMode(mode) => {
                self.selected_feature_mode = if self.selected_feature_mode == mode {
                    0
                } else {
                    mode
                };
                self.tools_menu_open = false;
                if let Some(lbl) = feature_mode_label(self.selected_feature_mode) {
                    let loc_lbl = t(self.locale, lbl);
                    self.show_toast(if self.locale.is_en() {
                        format!("Tool enabled: {loc_lbl}")
                    } else {
                        format!("도구 활성화: {loc_lbl}")
                    });
                }
                Task::none()
            }
            Message::ClearFeatureMode => {
                self.selected_feature_mode = 0;
                Task::none()
            }
            Message::InsertMention(name) => {
                let current = self.composer.text();
                let replaced = if let Some(at_idx) = current.rfind('@') {
                    format!("{}@{} ", &current[..at_idx], name)
                } else {
                    format!("{current}@{name} ")
                };
                self.composer = text_editor::Content::with_text(&replaced);
                self.composer_has_text = !replaced.trim().is_empty();
                self.mention_query = None;
                Task::none()
            }
            Message::CloseMentionMenu => {
                self.mention_query = None;
                Task::none()
            }
            Message::ConversationsLoaded(Ok(res)) => {
                self.conversations = res.conversations;
                self.conversations_next_page_token =
                    (!res.next_page_token.is_empty()).then_some(res.next_page_token);
                self.loading_more_conversations = false;
                Task::none()
            }
            Message::ConversationsLoaded(Err(e)) => {
                self.loading_more_conversations = false;
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to load conversations: {e}")
                } else {
                    format!("대화 목록 조회 실패: {e}")
                });
                Task::none()
            }
            Message::LoadMoreConversations => {
                if self.loading_more_conversations {
                    return Task::none();
                }
                let Some(token) = self.conversations_next_page_token.clone() else {
                    return Task::none();
                };
                self.loading_more_conversations = true;
                Task::perform(
                    ipc::list_conversations_page(60, token),
                    Message::MoreConversationsLoaded,
                )
            }
            Message::MoreConversationsLoaded(Ok(res)) => {
                self.loading_more_conversations = false;
                let existing: HashSet<String> = self
                    .conversations
                    .iter()
                    .map(|c| c.conversation_id.clone())
                    .collect();
                for c in res.conversations {
                    if !existing.contains(&c.conversation_id) {
                        self.conversations.push(c);
                    }
                }
                self.conversations_next_page_token =
                    (!res.next_page_token.is_empty()).then_some(res.next_page_token);
                Task::none()
            }
            Message::MoreConversationsLoaded(Err(e)) => {
                self.loading_more_conversations = false;
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to load older chats: {e}")
                } else {
                    format!("이전 대화 추가 조회 실패: {e}")
                });
                Task::none()
            }
            Message::NewChat => {
                self.model_menu_open = false;
                self.reset_chat_state(true);
                Task::none()
            }
            Message::SelectSearchHit(conv_id, title, request_id) => {
                let task = self.update(Message::SelectConversation(conv_id, title));
                self.pending_scroll_request_id = (!request_id.is_empty()).then_some(request_id);
                task
            }
            Message::SelectConversation(conv_id, title) => {
                self.model_menu_open = false;
                self.search_modal_open = false;
                self.temporary_chat = false;
                self.reset_chat_state(true);
                self.active_conv_id = Some(conv_id.clone());
                self.active_title = title;
                self.loading_history = true;

                let mut is_agent = false;
                let mut was_unread = false;
                for c in &mut self.conversations {
                    if c.conversation_id == conv_id {
                        if c.has_unread {
                            was_unread = true;
                        }
                        c.has_unread = false;
                        is_agent = c.is_agent_chat;
                    }
                }
                // 원앱과 동일하게 기존 대화를 열 때 입력창에 강제 Task 칩을 띄우지 않고,
                // 에이전트 스레드 여부만 내부 플래그로 유지한다.
                self.active_conv_is_agent = is_agent;
                self.composer_mode = ComposerMode::Chat;

                let seq = self.conv_load_seq;
                let id_for_cb = conv_id.clone();
                let load_task = Task::perform(ipc::list_turns(conv_id.clone(), 50), move |res| {
                    Message::TurnsLoaded(seq, id_for_cb, res)
                });
                if was_unread {
                    let sync_cid = conv_id.clone();
                    Task::batch([
                        load_task,
                        Task::perform(
                            ipc::mark_conversation_unread(conv_id, false),
                            move |res| Message::ConversationUnreadSynced(sync_cid, res),
                        ),
                    ])
                } else {
                    load_task
                }
            }
            Message::TurnsLoaded(seq, conv_id, Ok(res)) => {
                if seq != self.conv_load_seq
                    || self.is_streaming
                    || self.active_conv_id.as_deref() != Some(conv_id.as_str())
                {
                    return Task::none();
                }
                self.loading_history = false;
                let mut turns: Vec<ConversationTurn> = res.turns;
                if turns.len() > 1
                    && turns[0].timestamp_unix > turns[turns.len() - 1].timestamp_unix
                {
                    turns.reverse();
                }
                self.messages.clear();
                self.message_feedback.clear();
                let mut tasks: Vec<Task<Message>> = vec![Self::scroll_to_bottom()];
                let mut preload_count = 0usize;

                for t in &turns {
                    if !t.user_text.is_empty() || !t.user_attachments.is_empty() {
                        let names: Vec<String> = t
                            .user_attachments
                            .iter()
                            .filter(|a| !a.name.is_empty())
                            .map(|a| a.name.clone())
                            .collect();
                        let mut um = ChatMessage::user(t.user_text.clone(), names);
                        um.timestamp_unix = t.timestamp_unix;
                        um.attachment_items = t.user_attachments.clone();
                        for att in &t.user_attachments {
                            if preload_count < 24
                                && is_attachment_image(att)
                                && !att.url.is_empty()
                            {
                                let key = markdown_view::turn_attachment_cache_key(&att.url);
                                if !self.library_image_cache.contains_key(&key)
                                    && !self.failed_image_keys.contains(&key)
                                    && self.inflight_image_keys.insert(key.clone())
                                {
                                    let url = att.url.clone();
                                    tasks.push(Task::perform(
                                        ipc::fetch_library_image(url),
                                        move |r| Message::LibraryImageLoaded(key, r),
                                    ));
                                    preload_count += 1;
                                }
                            }
                        }
                        self.messages.push(um);
                    }
                    if !t.model_text.is_empty()
                        || !t.thoughts.is_empty()
                        || !t.tool_activities.is_empty()
                        || !t.generated_images.is_empty()
                        || !t.artifacts.is_empty()
                        || t.follow_up.is_some()
                        || t.elicitations.is_some()
                        || t.aborted
                    {
                        let mut m = ChatMessage::model(
                            t.model_text.clone(),
                            t.thoughts.clone(),
                            t.model_attribution.clone(),
                        );
                        m.request_id = t.request_id.clone();
                        m.response_id = t.response_id.clone();
                        m.status_text = t.status_text.clone();
                        m.tool_activities = t.tool_activities.clone();
                        m.generated_images = t.generated_images.clone();
                        m.artifacts = t.artifacts.clone();
                        m.follow_up = t.follow_up.clone();
                        m.elicitations = t.elicitations.clone();
                        m.is_scheduled_run = t.is_scheduled_run;
                        m.scheduled_title = t.scheduled_title.clone();
                        m.aborted = t.aborted;
                        m.timestamp_unix = t.timestamp_unix;
                        for img in &t.generated_images {
                            if preload_count < 24 && !img.url.is_empty() {
                                let key = markdown_view::generated_image_cache_key(&img.url);
                                if !self.library_image_cache.contains_key(&key)
                                    && !self.failed_image_keys.contains(&key)
                                    && self.inflight_image_keys.insert(key.clone())
                                {
                                    let url = img.url.clone();
                                    tasks.push(Task::perform(
                                        ipc::fetch_library_image(url),
                                        move |r| Message::LibraryImageLoaded(key, r),
                                    ));
                                    preload_count += 1;
                                }
                            }
                        }
                        self.messages.push(m);
                    }
                }
                if let Some(last) = turns.last() {
                    self.parent_request_id = last.request_id.clone();
                    self.parent_response_id = last.response_id.clone();
                }
                if let Some(target_req) = self.pending_scroll_request_id.take()
                    && let Some(pos) = self
                        .messages
                        .iter()
                        .position(|m| m.request_id == target_req)
                    && !self.messages.is_empty()
                {
                    let denom = self.messages.len().saturating_sub(1).max(1) as f32;
                    let ratio = (pos as f32 / denom).clamp(0.0, 1.0);
                    tasks[0] = operation::snap_to(
                        CHAT_SCROLL_ID,
                        operation::RelativeOffset { x: 0.0, y: ratio },
                    );
                }
                Task::batch(tasks)
            }
            Message::TurnsLoaded(seq, conv_id, Err(e)) => {
                if seq == self.conv_load_seq
                    && self.active_conv_id.as_deref() == Some(conv_id.as_str())
                {
                    self.loading_history = false;
                    self.error_banner = Some(if self.locale.is_en() {
                        format!("Failed to load conversation history: {e}")
                    } else {
                        format!("대화 이력 조회 실패: {e}")
                    });
                }
                Task::none()
            }
            Message::StartRename(conv_id, current_title) => {
                self.conv_menu_target = None;
                self.conv_menu_notebook_sub = false;
                self.renaming_conv_id = Some(conv_id);
                self.rename_draft = current_title;
                Task::none()
            }
            Message::RenameDraftChanged(val) => {
                self.rename_draft = val;
                Task::none()
            }
            Message::CancelRename => {
                self.renaming_conv_id = None;
                self.rename_draft.clear();
                Task::none()
            }
            Message::SubmitRename => {
                let Some(conv_id) = self.renaming_conv_id.take() else {
                    return Task::none();
                };
                let title = self.rename_draft.trim().to_string();
                self.rename_draft.clear();
                if title.is_empty() {
                    return Task::none();
                }
                Task::perform(
                    ipc::rename_conversation(conv_id, title),
                    Message::ConversationRenamed,
                )
            }
            Message::ConversationRenamed(Ok((conv_id, new_title))) => {
                for c in &mut self.conversations {
                    if c.conversation_id == conv_id {
                        c.title = new_title.clone();
                    }
                }
                if self.active_conv_id.as_deref() == Some(conv_id.as_str()) {
                    self.active_title = new_title;
                }
                self.show_toast(t(self.locale, "대화 제목이 변경되었습니다"));
                Task::none()
            }
            Message::ConversationRenamed(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to rename chat: {e}")
                } else {
                    format!("제목 변경 실패: {e}")
                });
                Task::none()
            }
            Message::TogglePinConversation(conv_id, next_pinned) => {
                self.conv_menu_target = None;
                Task::perform(
                    ipc::pin_conversation(conv_id, next_pinned),
                    Message::ConversationPinned,
                )
            }
            Message::ConversationPinned(Ok((conv_id, pinned))) => {
                for c in &mut self.conversations {
                    if c.conversation_id == conv_id {
                        c.pinned = pinned;
                    }
                }
                self.conversations.sort_by(|a, b| {
                    b.pinned
                        .cmp(&a.pinned)
                        .then_with(|| b.timestamp_unix.cmp(&a.timestamp_unix))
                });
                self.show_toast(t(
                    self.locale,
                    if pinned {
                        "대화를 상단에 고정했습니다"
                    } else {
                        "대화 고정을 해제했습니다"
                    },
                ));
                Task::none()
            }
            Message::ConversationPinned(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to update pin status: {e}")
                } else {
                    format!("고정 상태 변경 실패: {e}")
                });
                Task::none()
            }
            Message::ToggleUnreadConversation(conv_id, unread) => {
                self.conv_menu_target = None;
                Task::perform(
                    ipc::mark_conversation_unread(conv_id, unread),
                    Message::ConversationUnreadUpdated,
                )
            }
            Message::ConversationUnreadUpdated(Ok((conv_id, unread))) => {
                for c in &mut self.conversations {
                    if c.conversation_id == conv_id {
                        c.has_unread = unread;
                    }
                }
                self.show_toast(t(
                    self.locale,
                    if unread {
                        "읽지 않음으로 표시했습니다"
                    } else {
                        "읽음으로 표시했습니다"
                    },
                ));
                Task::none()
            }
            Message::ConversationUnreadUpdated(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to update read status: {e}")
                } else {
                    format!("읽음 상태 변경 실패: {e}")
                });
                Task::none()
            }
            Message::ConversationUnreadSynced(cid, res) => {
                match res {
                    Ok((updated_id, unread)) => {
                        if let Some(c) = self
                            .conversations
                            .iter_mut()
                            .find(|c| c.conversation_id == updated_id)
                        {
                            c.has_unread = unread;
                        }
                    }
                    Err(_) => {
                        if let Some(c) = self
                            .conversations
                            .iter_mut()
                            .find(|c| c.conversation_id == cid)
                        {
                            c.has_unread = true;
                        }
                    }
                }
                Task::none()
            }
            Message::ShareConversation(conv_id, title) => {
                self.conv_menu_target = None;
                Task::perform(
                    ipc::share_conversation(conv_id, title),
                    Message::ConversationShared,
                )
            }
            Message::ConversationShared(Ok(res)) => {
                let url = res.url;
                self.show_toast(if self.locale.is_en() {
                    format!("Share link copied: {url}")
                } else {
                    format!("공유 링크 복사됨: {url}")
                });
                clipboard::write(url)
            }
            Message::ConversationShared(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to share chat: {e}")
                } else {
                    format!("대화 공유 실패: {e}")
                });
                Task::none()
            }
            Message::AddConvToNotebook(conv_id, nb_id) => {
                self.conv_menu_target = None;
                self.conv_menu_notebook_sub = false;
                Task::perform(
                    ipc::add_conversation_to_notebook(nb_id, conv_id),
                    Message::ConvAddedToNotebook,
                )
            }
            Message::ConvAddedToNotebook(Ok(())) => {
                self.show_toast(t(self.locale, "대화를 노트북에 추가했습니다"));
                Task::perform(ipc::list_notebooks(), Message::NotebooksLoaded)
            }
            Message::ConvAddedToNotebook(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to add to notebook: {e}")
                } else {
                    format!("노트북 추가 실패: {e}")
                });
                Task::none()
            }
            Message::DeleteConversation(conv_id) => {
                self.conv_menu_target = None;
                Task::perform(
                    ipc::delete_conversation(conv_id),
                    Message::ConversationDeleted,
                )
            }
            Message::ConversationDeleted(Ok(deleted_id)) => {
                self.conversations
                    .retain(|c| c.conversation_id != deleted_id);
                if self.active_conv_id.as_deref() == Some(deleted_id.as_str()) {
                    return self.update(Message::NewChat);
                }
                self.show_toast(t(self.locale, "대화가 삭제되었습니다"));
                Task::none()
            }
            Message::ConversationDeleted(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to delete chat: {e}")
                } else {
                    format!("대화 삭제 실패: {e}")
                });
                Task::none()
            }
            Message::SendFeedback(idx, positive) => {
                let prev = self.message_feedback.insert(idx, positive);
                let conv_id = self.active_conv_id.clone().unwrap_or_default();
                let (req_id, resp_id) = self
                    .messages
                    .get(idx)
                    .map(|m| (m.request_id.clone(), m.response_id.clone()))
                    .unwrap_or_default();
                self.show_toast(t(
                    self.locale,
                    if positive {
                        "좋은 응답으로 피드백을 보냈습니다"
                    } else {
                        "개선 필요 피드백을 보냈습니다"
                    },
                ));
                Task::perform(
                    ipc::send_feedback(conv_id, req_id, resp_id, positive),
                    move |res| Message::FeedbackSent(idx, prev, res),
                )
            }
            Message::FeedbackSent(_, _, Ok(())) => Task::none(),
            Message::FeedbackSent(idx, prev, Err(e)) => {
                match prev {
                    Some(v) => {
                        self.message_feedback.insert(idx, v);
                    }
                    None => {
                        self.message_feedback.remove(&idx);
                    }
                }
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to send feedback: {e}")
                } else {
                    format!("피드백 전송 실패: {e}")
                });
                Task::none()
            }

            // --- 노트북 ---
            Message::NewNotebook => {
                self.active_section = ActiveSection::Notebooks;
                self.editing_notebook = Notebook {
                    title: t(self.locale, "새 노트북").into(),
                    ..Notebook::default()
                };
                self.notebook_notes_editor = text_editor::Content::new();
                Task::none()
            }
            Message::OpenNotebook(id) => {
                self.active_section = ActiveSection::Notebooks;
                if let Some(nb) = self.notebooks.iter().find(|n| n.id == id).cloned() {
                    self.notebook_notes_editor = text_editor::Content::with_text(&nb.notes);
                    self.editing_notebook = nb;
                }
                Task::none()
            }
            Message::NotebookTitleChanged(v) => {
                self.editing_notebook.title = v;
                Task::none()
            }
            Message::NotebookDescChanged(v) => {
                self.editing_notebook.description = v;
                Task::none()
            }
            Message::NotebookNotesAction(action) => {
                let is_edit = action.is_edit();
                self.notebook_notes_editor.perform(action);
                if is_edit {
                    self.editing_notebook.notes = self.notebook_notes_editor.text();
                }
                Task::none()
            }
            Message::AddNotebookSource => {
                Task::perform(filepicker::pick_files(), Message::NotebookSourcesPicked)
            }
            Message::NotebookSourcesPicked(Ok(paths)) => {
                for p in paths {
                    let s = p.to_string_lossy().to_string();
                    if !self.editing_notebook.sources.contains(&s) {
                        self.editing_notebook.sources.push(s);
                    }
                }
                Task::none()
            }
            Message::NotebookSourcesPicked(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to pick source files: {e}")
                } else {
                    format!("소스 파일 선택 오류: {e}")
                });
                Task::none()
            }
            Message::RemoveNotebookSource(idx) => {
                if idx < self.editing_notebook.sources.len() {
                    self.editing_notebook.sources.remove(idx);
                }
                Task::none()
            }
            Message::SaveNotebook => {
                let mut nb = self.editing_notebook.clone();
                nb.notes = self.notebook_notes_editor.text();
                Task::perform(ipc::upsert_notebook(nb), Message::NotebookSaved)
            }
            Message::NotebookSaved(Ok(saved)) => {
                self.editing_notebook = saved.clone();
                self.notebooks.retain(|n| n.id != saved.id);
                self.notebooks.insert(0, saved);
                self.show_toast(t(self.locale, "노트북이 저장되었습니다"));
                Task::none()
            }
            Message::NotebookSaved(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to save notebook: {e}")
                } else {
                    format!("노트북 저장 실패: {e}")
                });
                Task::none()
            }
            Message::DeleteNotebook(id) => {
                Task::perform(ipc::delete_notebook(id), Message::NotebookDeleted)
            }
            Message::NotebookDeleted(Ok(id)) => {
                self.notebooks.retain(|n| n.id != id);
                if self.editing_notebook.id == id {
                    self.editing_notebook = Notebook::default();
                    self.notebook_notes_editor = text_editor::Content::new();
                }
                self.show_toast(t(self.locale, "노트북이 삭제되었습니다"));
                Task::none()
            }
            Message::NotebookDeleted(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to delete notebook: {e}")
                } else {
                    format!("노트북 삭제 실패: {e}")
                });
                Task::none()
            }
            Message::ChatWithNotebook => {
                let nb = self.editing_notebook.clone();
                let notes = self.notebook_notes_editor.text().trim().to_string();
                let mut files = Vec::new();
                for src in &nb.sources {
                    let p = PathBuf::from(src);
                    if p.is_file() {
                        files.push(p);
                    }
                }
                self.reset_chat_state(true);
                if nb.is_remote && !nb.id.is_empty() {
                    self.active_project_id = Some(nb.id.clone());
                }
                self.attached_files = files;
                let nb_title = t(self.locale, &nb.title);
                let prompt = if notes.is_empty() {
                    if self.locale.is_en() {
                        format!(
                            "[Notebook: {}] Analyze the attached source materials and summarize the key points.",
                            nb_title
                        )
                    } else {
                        format!(
                            "[노트북: {}] 첨부된 소스 자료를 분석하고 핵심 내용을 요약해 주세요.",
                            nb_title
                        )
                    }
                } else if self.locale.is_en() {
                    format!(
                        "[Notebook: {}]\nNotes context:\n{}\n\nSummarize the key points from these notes and attached sources and get ready for questions.",
                        nb_title, notes
                    )
                } else {
                    format!(
                        "[노트북: {}]\n메모 컨텍스트:\n{}\n\n위 메모와 첨부 소스를 바탕으로 핵심을 정리하고 질문할 준비를 해 주세요.",
                        nb_title, notes
                    )
                };
                self.composer = text_editor::Content::with_text(&prompt);
                self.composer_has_text = !prompt.trim().is_empty();
                self.show_toast(if self.locale.is_en() {
                    format!("Connected sources from notebook '{}' to chat", nb_title)
                } else {
                    format!("노트북 '{}' 소스가 채팅에 연결되었습니다", nb_title)
                });
                Task::none()
            }

            // --- 예약 작업 ---
            Message::SetSchedFilter(f) => {
                self.sched_filter = f;
                Task::none()
            }
            Message::SchedTitleChanged(v) => {
                self.sched_title_draft = v;
                Task::none()
            }
            Message::SchedPromptChanged(v) => {
                self.sched_prompt_draft = v;
                Task::none()
            }
            Message::SchedTypeChanged(v) => {
                self.sched_type_draft = v;
                Task::none()
            }
            Message::SchedTimeChanged(v) => {
                self.sched_time_draft = v;
                Task::none()
            }
            Message::QuickCreateSchedule => {
                if !self.sched_prompt_draft.trim().is_empty() {
                    return self.update(Message::CreateSchedule);
                }
                self.sched_title_draft = t(self.locale, "매일 아침 브리핑").into();
                self.sched_prompt_draft = t(
                    self.locale,
                    "오늘 주요 기술 뉴스와 주목할 AI 업데이트 5가지를 한국어로 요약해 주세요.",
                )
                .into();
                self.sched_type_draft = "daily".into();
                self.sched_time_draft = "09:00".into();
                self.show_toast(t(self.locale, "예약 작업 입력란에 기본 템플릿을 채웠습니다"));
                Task::none()
            }
            Message::AddRecommendedSchedule(title, prompt, sched_type, time_of_day) => {
                Task::perform(
                    ipc::add_recommended_schedule(title, prompt, sched_type, time_of_day),
                    Message::RecommendedScheduleAdded,
                )
            }
            Message::RecommendedScheduleAdded(Ok(title)) => {
                self.show_toast(if self.locale.is_en() {
                    format!("Added recommended schedule: {title}")
                } else {
                    format!("추천 예약 추가됨: {title}")
                });
                Task::perform(ipc::list_schedules(), Message::SchedulesLoaded)
            }
            Message::RecommendedScheduleAdded(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to add recommended schedule: {e}")
                } else {
                    format!("추천 예약 추가 실패: {e}")
                });
                Task::none()
            }
            Message::CreateSchedule => {
                if self.sched_prompt_draft.trim().is_empty() {
                    self.show_toast(t(self.locale, "예약 실행할 프롬프트를 입력해 주세요"));
                    return Task::none();
                }
                let task = ScheduledTask {
                    title: if self.sched_title_draft.trim().is_empty() {
                        t(self.locale, "예약 브리핑").into()
                    } else {
                        self.sched_title_draft.trim().to_string()
                    },
                    prompt: self.sched_prompt_draft.trim().to_string(),
                    schedule_type: self.sched_type_draft.clone(),
                    time_of_day: if self.sched_time_draft.trim().is_empty() {
                        "09:00".into()
                    } else {
                        self.sched_time_draft.trim().to_string()
                    },
                    enabled: true,
                    ..ScheduledTask::default()
                };
                Task::perform(ipc::create_schedule(task), Message::ScheduleCreated)
            }
            Message::ScheduleCreated(Ok(created)) => {
                self.sched_title_draft.clear();
                self.sched_prompt_draft.clear();
                self.schedules.insert(0, created);
                self.show_toast(t(self.locale, "예약 작업이 등록되었습니다"));
                Task::none()
            }
            Message::ScheduleCreated(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to create schedule: {e}")
                } else {
                    format!("예약 생성 실패: {e}")
                });
                Task::none()
            }
            Message::ToggleScheduleEnabled(id, enabled) => {
                Task::perform(ipc::patch_schedule(id, enabled), Message::SchedulePatched)
            }
            Message::SchedulePatched(Ok(updated)) => {
                for s in &mut self.schedules {
                    if s.id == updated.id {
                        *s = updated.clone();
                    }
                }
                Task::none()
            }
            Message::SchedulePatched(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to update schedule: {e}")
                } else {
                    format!("예약 변경 실패: {e}")
                });
                Task::none()
            }
            Message::DeleteSchedule(id) => {
                Task::perform(ipc::delete_schedule(id), Message::ScheduleDeleted)
            }
            Message::ScheduleDeleted(Ok(id)) => {
                self.schedules.retain(|s| s.id != id);
                self.show_toast(t(self.locale, "예약 작업이 삭제되었습니다"));
                Task::none()
            }
            Message::ScheduleDeleted(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to delete schedule: {e}")
                } else {
                    format!("예약 삭제 실패: {e}")
                });
                Task::none()
            }
            Message::RunScheduleNow(id) => {
                self.running_schedule_id = Some(id.clone());
                let id_cb = id.clone();
                Task::perform(ipc::run_schedule_now(id), move |res| {
                    Message::ScheduleRunFinished(id_cb, res)
                })
            }
            Message::ScheduleRunFinished(_id, Ok(res)) => {
                self.running_schedule_id = None;
                let title_str = if res.title.is_empty() {
                    t(self.locale, "대화 저장됨")
                } else {
                    res.title.as_str()
                };
                self.show_toast(if self.locale.is_en() {
                    format!("Schedule run complete: {title_str}")
                } else {
                    format!("예약 실행 완료: {title_str}")
                });
                Task::batch([
                    Task::perform(ipc::list_schedules(), Message::SchedulesLoaded),
                    Task::perform(ipc::list_conversations(60), Message::ConversationsLoaded),
                ])
            }
            Message::ScheduleRunFinished(_id, Err(e)) => {
                self.running_schedule_id = None;
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to run schedule: {e}")
                } else {
                    format!("예약 즉시 실행 실패: {e}")
                });
                Task::none()
            }

            // --- 라이브러리 · 이미지 · 학생/Gem ---
            Message::SetLibraryFilter(f) => {
                self.library_filter = f;
                Task::none()
            }
            Message::AttachLibraryPath(path_str) => {
                let p = PathBuf::from(&path_str);
                if p.is_file() {
                    if !self.attached_files.contains(&p) {
                        self.attached_files.push(p);
                    }
                    self.active_section = ActiveSection::Chat;
                    self.show_toast(t(self.locale, "라이브러리 파일이 입력창에 첨부되었습니다"));
                } else {
                    self.show_toast(t(self.locale, "원본 로컬 파일을 찾을 수 없습니다"));
                }
                Task::none()
            }
            Message::ImagePromptChanged(v) => {
                self.image_prompt_draft = v;
                Task::none()
            }
            Message::SetImageAspect(v) => {
                self.image_aspect = v;
                Task::none()
            }
            Message::SetImageStyle(v) => {
                self.image_style = v;
                Task::none()
            }
            Message::GenerateImagePrompt => {
                let raw = self.image_prompt_draft.trim().to_string();
                if raw.is_empty() {
                    self.show_toast(t(self.locale, "생성할 이미지 설명을 입력해 주세요"));
                    return Task::none();
                }
                self.image_prompt_draft.clear();
                self.reset_chat_state(true);
                self.selected_feature_mode = FEATURE_MODE_IMAGE;
                let style_label = t(self.locale, &self.image_style);
                let full_prompt = if self.locale.is_en() {
                    format!(
                        "Generate a high-quality image based on the following description.\n- Style: {}\n- Aspect ratio: {}\n- Description: {}",
                        style_label, self.image_aspect, raw
                    )
                } else {
                    format!(
                        "다음 설명으로 고품질 이미지를 생성해 주세요.\n- 스타일: {}\n- 화면 비율: {}\n- 설명: {}",
                        style_label, self.image_aspect, raw
                    )
                };
                self.dispatch_prompt(full_prompt, Vec::new())
            }
            Message::VideoPromptChanged(v) => {
                self.video_prompt_draft = v;
                Task::none()
            }
            Message::SetVideoAspect(v) => {
                self.video_aspect = v;
                Task::none()
            }
            Message::SetVideoDuration(v) => {
                self.video_duration = v;
                Task::none()
            }
            Message::GenerateVideoPrompt => {
                let raw = self.video_prompt_draft.trim().to_string();
                if raw.is_empty() {
                    self.show_toast(t(self.locale, "생성할 동영상 장면을 입력해 주세요"));
                    return Task::none();
                }
                self.video_prompt_draft.clear();
                self.reset_chat_state(true);
                self.selected_feature_mode = FEATURE_MODE_VIDEO;
                let dur_label = t(self.locale, &self.video_duration);
                let full_prompt = if self.locale.is_en() {
                    format!(
                        "Generate a video for the following scene.\n- Aspect ratio: {}\n- Duration: {}\n- Scene: {}",
                        self.video_aspect, dur_label, raw
                    )
                } else {
                    format!(
                        "다음 장면으로 동영상을 생성해 주세요.\n- 화면 비율: {}\n- 길이: {}\n- 장면 설명: {}",
                        self.video_aspect, dur_label, raw
                    )
                };
                self.dispatch_prompt(full_prompt, Vec::new())
            }
            Message::StartBotPrompt(bot_id, prompt) => {
                self.reset_chat_state(false);
                self.active_bot_id = Some(bot_id);
                self.dispatch_prompt(prompt, Vec::new())
            }
            Message::StartBotChat(bot_id) => {
                self.reset_chat_state(false);
                self.active_bot_id = Some(bot_id);
                Task::none()
            }
            Message::StartFeaturePrompt(fmode, prompt) => {
                self.reset_chat_state(true);
                self.selected_feature_mode = fmode;
                self.dispatch_prompt(prompt, Vec::new())
            }

            // --- 입력창 · 화면 공유 · 마이크 · 스트리밍 ---
            Message::SetComposerMode(mode) => {
                if self.composer_mode != mode {
                    self.composer_mode = mode;
                    self.welcome_glow_cache.clear();
                }
                Task::none()
            }
            Message::ToggleTemporary => {
                let next = !self.temporary_chat;
                self.model_menu_open = false;
                if next && self.active_conv_id.is_some() {
                    self.reset_chat_state(true);
                }
                self.active_section = ActiveSection::Chat;
                self.temporary_chat = next;
                Task::none()
            }
            Message::ToggleMessageThoughts(idx) => {
                if let Some(msg) = self.messages.get_mut(idx) {
                    msg.thoughts_expanded = !msg.thoughts_expanded;
                }
                Task::none()
            }
            Message::ToggleMessageTools(idx) => {
                if let Some(msg) = self.messages.get_mut(idx) {
                    msg.tools_expanded = !msg.tools_expanded;
                }
                Task::none()
            }
            Message::OpenCanvasPreview(msg_idx, art_idx) => {
                if let Some(art) = self
                    .messages
                    .get(msg_idx)
                    .and_then(|m| m.artifacts.get(art_idx))
                {
                    let parsed_markdown = if markdown_view::is_markdown_artifact(art) {
                        Some(markdown_view::ParsedMarkdown::parse(&art.content))
                    } else {
                        None
                    };
                    self.canvas_preview = Some(CanvasPreviewState {
                        msg_idx,
                        art_idx,
                        artifact: art.clone(),
                        parsed_markdown,
                        show_raw: false,
                    });
                }
                Task::none()
            }
            Message::ToggleCanvasRaw => {
                if let Some(preview) = &mut self.canvas_preview {
                    preview.show_raw = !preview.show_raw;
                }
                Task::none()
            }
            Message::CloseCanvasPreview => {
                self.canvas_preview = None;
                Task::none()
            }
            Message::RunCanvasHtml(msg_idx, art_idx) => {
                if let Some(art) = self
                    .messages
                    .get(msg_idx)
                    .and_then(|m| m.artifacts.get(art_idx))
                {
                    let title = art.title.clone();
                    let content = art.content.clone();
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                write_and_open_canvas_html(&title, &content)
                            })
                            .await
                            .map_err(|e| format!("백그라운드 작업 오류: {e}"))?
                        },
                        Message::CanvasHtmlOpened,
                    );
                }
                Task::none()
            }
            Message::CanvasHtmlOpened(Ok(_)) => {
                self.show_toast(if self.locale.is_en() {
                    "Opened Canvas HTML in browser"
                } else {
                    "브라우저에서 Canvas HTML을 열었습니다"
                });
                Task::none()
            }
            Message::CanvasHtmlOpened(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to launch Canvas HTML: {e}")
                } else {
                    format!("Canvas HTML 실행 실패: {e}")
                });
                Task::none()
            }
            Message::OpenFilePicker => {
                self.model_menu_open = false;
                Task::perform(filepicker::pick_files(), Message::FilesPicked)
            }
            Message::FilesPicked(Ok(paths)) => {
                let mut added = 0usize;
                for p in paths {
                    if !self.attached_files.contains(&p) {
                        self.attached_files.push(p);
                        added += 1;
                    }
                }
                if added == 1 {
                    if let Some(last) = self.attached_files.last() {
                        let name = last
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| last.display().to_string());
                        self.show_toast(if self.locale.is_en() {
                            format!("Attached file: {name}")
                        } else {
                            format!("파일 첨부됨: {name}")
                        });
                    }
                } else if added > 1 {
                    self.show_toast(if self.locale.is_en() {
                        format!("Attached {added} files")
                    } else {
                        format!("파일 {added}개 첨부됨")
                    });
                }
                Task::none()
            }
            Message::FilesPicked(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("File picker error: {e}")
                } else {
                    format!("파일 선택기 오류: {e}")
                });
                Task::none()
            }
            Message::StartScreenCapture => {
                if self.screen_capturing {
                    return Task::none();
                }
                self.screen_capturing = true;
                self.model_menu_open = false;
                Task::perform(capture::capture_screen(), Message::ScreenCaptured)
            }
            Message::ScreenCaptured(Ok(path)) => {
                self.screen_capturing = false;
                if !self.attached_files.contains(&path) {
                    self.attached_files.push(path);
                }
                self.show_toast(t(self.locale, "화면 캡처가 첨부되었습니다"));
                Task::none()
            }
            Message::ScreenCaptured(Err(e)) => {
                self.screen_capturing = false;
                if !e.contains("취소") && !e.to_ascii_lowercase().contains("cancel") {
                    self.error_banner = Some(e);
                }
                Task::none()
            }
            Message::ToggleVoiceRecording => {
                if self.voice_stopping {
                    return Task::none();
                }
                if let Some(recorder) = self.voice_recorder.take() {
                    self.recording_seconds = 0;
                    self.voice_stopping = true;
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || recorder.stop())
                                .await
                                .map_err(|e| format!("음성 녹음 종료 작업 오류: {e}"))?
                        },
                        Message::VoiceRecordStopped,
                    );
                } else {
                    match VoiceRecorder::start() {
                        Ok(rec) => {
                            self.voice_recorder = Some(rec);
                            self.recording_seconds = 0;
                            self.show_toast(t(self.locale, "음성 녹음을 시작했습니다 (다시 누르면 종료)"));
                        }
                        Err(e) => {
                            self.error_banner = Some(e);
                        }
                    }
                }
                Task::none()
            }
            Message::VoiceRecordStopped(Ok(wav_path)) => {
                self.voice_stopping = false;
                self.attached_files.push(wav_path);
                self.show_toast(t(self.locale, "음성 녹음이 첨부되었습니다 (전송 시 분석)"));
                Task::none()
            }
            Message::VoiceRecordStopped(Err(e)) => {
                self.voice_stopping = false;
                self.show_toast(e);
                Task::none()
            }
            Message::EditorAction(action) => {
                let is_edit = action.is_edit();
                self.composer.perform(action);
                if is_edit {
                    let current = self.composer.text();
                    self.composer_has_text = !current.trim().is_empty();
                    self.mention_query = extract_mention_query(&current);
                }
                Task::none()
            }
            Message::RemoveAttachment(idx) => {
                if idx < self.attached_files.len() {
                    self.attached_files.remove(idx);
                }
                Task::none()
            }
            Message::SendPrompt => {
                if self.is_streaming {
                    return Task::none();
                }
                let prompt = self.composer.text().trim().to_string();
                if prompt.is_empty() && self.attached_files.is_empty() {
                    return Task::none();
                }
                self.composer = text_editor::Content::new();
                self.composer_has_text = false;
                let files = std::mem::take(&mut self.attached_files);
                self.active_section = ActiveSection::Chat;
                self.dispatch_prompt(prompt, files)
            }
            Message::RegenerateLast => {
                if self.is_streaming {
                    return Task::none();
                }
                if self.messages.last().is_some_and(|m| m.role == Role::Model) {
                    let removed_idx = self.messages.len() - 1;
                    self.messages.pop();
                    self.message_feedback.remove(&removed_idx);
                }
                let prompt = if self.messages.last().is_some_and(|m| m.role == Role::User) {
                    self.messages.pop().map(|m| m.raw_text)
                } else {
                    self.messages
                        .iter()
                        .rev()
                        .find(|m| m.role == Role::User)
                        .map(|m| m.raw_text.clone())
                };
                if let Some(prev_model) = self.messages.iter().rev().find(|m| m.role == Role::Model) {
                    self.parent_request_id = prev_model.request_id.clone();
                    self.parent_response_id = prev_model.response_id.clone();
                } else {
                    self.parent_request_id.clear();
                    self.parent_response_id.clear();
                }
                if let Some(prompt) = prompt {
                    return self.dispatch_prompt(prompt, Vec::new());
                }
                Task::none()
            }
            Message::StreamReceived(ev) => match ev {
                StreamEvent::Chunk(ch) => {
                    self.apply_chunk(&ch, false);
                    const STREAM_SCROLL_INTERVAL: Duration = Duration::from_millis(60);
                    let now = Instant::now();
                    if self
                        .last_stream_scroll
                        .is_none_or(|prev| now.duration_since(prev) >= STREAM_SCROLL_INTERVAL)
                    {
                        self.last_stream_scroll = Some(now);
                        Self::scroll_to_bottom()
                    } else {
                        Task::none()
                    }
                }
                StreamEvent::Done(ch) => {
                    self.apply_chunk(&ch, true);
                    self.is_streaming = false;
                    self.stream_abort_handle = None;
                    let had_attachments = std::mem::take(&mut self.stream_had_attachments);
                    if !self.temporary_chat {
                        let mut tasks = vec![
                            Task::perform(
                                ipc::list_conversations(60),
                                Message::ConversationsLoaded,
                            ),
                            Self::scroll_to_bottom(),
                        ];
                        // 방금 완료된 응답에 생성 이미지 플레이스홀더나 미보강 Canvas 칩이 있으면
                        // 최신 턴을 재조회해 실제 생성 이미지 URL·아티팩트를 즉시 렌더링한다.
                        let (needs_turn_refresh, has_generated_or_artifact) = self
                            .messages
                            .last()
                            .filter(|m| m.role == Role::Model)
                            .map(|m| {
                                let has_img_ref = m
                                    .raw_text
                                    .contains("googleusercontent.com/image_generation_content/");
                                let has_canvas_ref = m
                                    .raw_text
                                    .contains("googleusercontent.com/immersive_entry_chip/");
                                let needs_refresh = (has_img_ref && m.generated_images.is_empty())
                                    || (has_canvas_ref && m.artifacts.is_empty());
                                let has_media = has_img_ref
                                    || has_canvas_ref
                                    || !m.generated_images.is_empty()
                                    || !m.artifacts.is_empty();
                                (needs_refresh, has_media)
                            })
                            .unwrap_or((false, false));

                        if needs_turn_refresh
                            || had_attachments
                            || has_generated_or_artifact
                            || matches!(
                                self.active_section,
                                ActiveSection::Library | ActiveSection::Images
                            )
                        {
                            tasks.push(Task::perform(ipc::list_library(), Message::LibraryLoaded));
                        }
                        if needs_turn_refresh
                            && let Some(conv_id) = self.active_conv_id.clone()
                        {
                            self.conv_load_seq = self.conv_load_seq.wrapping_add(1);
                            let seq = self.conv_load_seq;
                            let id_cb = conv_id.clone();
                            tasks.push(Task::perform(
                                ipc::list_turns(conv_id, 50),
                                move |res| Message::TurnsLoaded(seq, id_cb, res),
                            ));
                        }
                        Task::batch(tasks)
                    } else {
                        Self::scroll_to_bottom()
                    }
                }
                StreamEvent::Error(err) => {
                    self.flush_dirty_stream_markdown();
                    self.is_streaming = false;
                    self.stream_abort_handle = None;
                    self.stream_had_attachments = false;
                    self.error_banner = Some(if self.locale.is_en() {
                        format!("Streaming error: {err}")
                    } else {
                        format!("스트리밍 오류: {err}")
                    });
                    Task::none()
                }
            },
            Message::AbortStream => {
                self.cancel_active_stream();
                self.flush_dirty_stream_markdown();
                if let Some(last) = self.messages.last_mut()
                    && last.role == Role::Model
                {
                    last.aborted = true;
                }
                if let Some(conv_id) = self.active_conv_id.clone()
                    && !self.streaming_req_id.is_empty()
                {
                    let req_id = std::mem::take(&mut self.streaming_req_id);
                    return Task::perform(
                        ipc::abort_generation(conv_id, req_id),
                        Message::AbortFinished,
                    );
                }
                Task::none()
            }
            Message::AbortFinished(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Stop request warning: {e}")
                } else {
                    format!("중단 요청 경고: {e}")
                });
                Task::none()
            }
            Message::AbortFinished(Ok(())) => {
                self.show_toast(t(self.locale, "응답 생성을 중단했습니다"));
                Task::none()
            }
            Message::CopyText(content) => {
                self.show_toast(t(self.locale, "클립보드에 복사되었습니다"));
                clipboard::write(content)
            }
            Message::CopyChatItem(idx) => {
                if let Some(msg) = self.messages.get(idx) {
                    let content = msg.raw_text.clone();
                    self.show_toast(t(self.locale, "클립보드에 복사되었습니다"));
                    clipboard::write(content)
                } else {
                    Task::none()
                }
            }
            Message::CopyCanvasPreview => {
                if let Some(preview) = &self.canvas_preview {
                    let content = preview.artifact.content.clone();
                    self.show_toast(t(self.locale, "클립보드에 복사되었습니다"));
                    clipboard::write(content)
                } else {
                    Task::none()
                }
            }
            Message::CopyMarkdownTable(target, block_idx) => {
                let parsed = match target {
                    markdown_view::CopyTarget::Message(msg_idx) => {
                        self.messages.get(msg_idx).map(|m| &m.markdown)
                    }
                    markdown_view::CopyTarget::CanvasPreview => self
                        .canvas_preview
                        .as_ref()
                        .and_then(|p| p.parsed_markdown.as_ref()),
                };
                if let Some(raw) = parsed.and_then(|p| p.table_raw_markdown(block_idx)) {
                    let content = raw.to_string();
                    self.show_toast(t(self.locale, "클립보드에 복사되었습니다"));
                    clipboard::write(content)
                } else {
                    Task::none()
                }
            }
            Message::CopyMarkdownCode(target, block_idx, code_idx) => {
                let parsed = match target {
                    markdown_view::CopyTarget::Message(msg_idx) => {
                        self.messages.get(msg_idx).map(|m| &m.markdown)
                    }
                    markdown_view::CopyTarget::CanvasPreview => self
                        .canvas_preview
                        .as_ref()
                        .and_then(|p| p.parsed_markdown.as_ref()),
                };
                if let Some(code) = parsed.and_then(|p| p.code_block_text(block_idx, code_idx)) {
                    let content = code.to_string();
                    self.show_toast(t(self.locale, "클립보드에 복사되었습니다"));
                    clipboard::write(content)
                } else {
                    Task::none()
                }
            }
            Message::LinkClicked(url) => {
                if !open_safe_external_url(&url) {
                    self.show_toast(t(self.locale, "안전하지 않은 링크 형식은 열 수 없습니다"));
                }
                Task::none()
            }
            Message::OpenExternalUrl(url) => {
                if !open_safe_external_url(&url) {
                    self.show_toast(t(self.locale, "안전하지 않은 링크 형식은 열 수 없습니다"));
                }
                Task::none()
            }
            Message::SendFollowUp(idx) => {
                if self.is_streaming {
                    return Task::none();
                }
                let query = self
                    .messages
                    .get(idx)
                    .and_then(|m| m.follow_up.as_ref())
                    .map(|fu| {
                        if fu.query.trim().is_empty() {
                            fu.label.clone()
                        } else {
                            fu.query.clone()
                        }
                    })
                    .unwrap_or_default();
                if query.trim().is_empty() {
                    return Task::none();
                }
                self.dispatch_prompt(query, Vec::new())
            }
            Message::SendElicitation(msg_idx, item_idx) => {
                if self.is_streaming {
                    return Task::none();
                }
                let query = self
                    .messages
                    .get(msg_idx)
                    .and_then(|m| m.elicitations.as_ref())
                    .and_then(|g| g.items.get(item_idx))
                    .map(|it| {
                        if it.query.trim().is_empty() {
                            it.label.clone()
                        } else {
                            it.query.clone()
                        }
                    })
                    .unwrap_or_default();
                if query.trim().is_empty() {
                    return Task::none();
                }
                self.dispatch_prompt(query, Vec::new())
            }
            Message::DismissError => {
                self.error_banner = None;
                Task::none()
            }
        }
    }

    fn dispatch_prompt(&mut self, prompt: String, files: Vec<PathBuf>) -> Task<Message> {
        self.error_banner = None;
        self.model_menu_open = false;
        self.thinking_submenu_open = false;
        self.tools_menu_open = false;
        self.device_menu_open = false;
        self.mention_query = None;
        self.is_streaming = true;
        self.streaming_req_id.clear();
        self.last_stream_reparse = None;
        self.last_stream_scroll = None;
        self.stream_markdown_dirty = false;
        self.stream_had_attachments = !files.is_empty();

        let file_names: Vec<String> = files
            .iter()
            .map(|p| {
                p.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.display().to_string())
            })
            .collect();
        let file_paths: Vec<String> = files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        let display_text = if prompt.is_empty() && !file_names.is_empty() {
            let is_audio = file_names.iter().any(|n| n.ends_with(".wav"));
            t(
                self.locale,
                if is_audio {
                    "첨부한 음성 녹음을 듣고 답변해 주세요."
                } else {
                    "첨부한 화면/파일을 확인하고 설명해 주세요."
                },
            )
            .to_string()
        } else {
            prompt
        };

        let is_task = self.composer_mode == ComposerMode::Task;
        let target_dev = if is_task {
            self.selected_target_device()
        } else {
            None
        };
        let target_device_id = target_dev
            .as_ref()
            .map(|d| d.device_id.clone())
            .unwrap_or_default();
        let target_device_name = target_dev
            .as_ref()
            .map(|d| d.device_name.clone())
            .unwrap_or_default();

        self.messages
            .push(ChatMessage::user(display_text.clone(), file_names));
        let mut model_msg = ChatMessage::model(String::new(), String::new(), String::new());
        if is_task && !target_device_name.is_empty() {
            model_msg.target_device_name = target_device_name.clone();
            model_msg.status_text = if self.locale.is_en() {
                format!("Sending remote task to {target_device_name}...")
            } else {
                format!("{target_device_name}에 원격 태스크를 전달하는 중...")
            };
        }
        self.messages.push(model_msg);

        let (mode_category, mode_variant_id, supports_thinking) = match self.selected_model() {
            Some(m) => (m.mode_category, m.id.clone(), m.supports_thinking),
            None => (0, String::new(), false),
        };
        let feature_mode = std::mem::take(&mut self.selected_feature_mode);
        let thinking_level = if supports_thinking {
            self.thinking_level
        } else {
            0
        };

        let req = ChatStreamRequest {
            text: display_text,
            conversation_id: if self.temporary_chat {
                String::new()
            } else {
                self.active_conv_id.clone().unwrap_or_default()
            },
            parent_request_id: if self.temporary_chat {
                String::new()
            } else {
                self.parent_request_id.clone()
            },
            parent_response_id: if self.temporary_chat {
                String::new()
            } else {
                self.parent_response_id.clone()
            },
            temporary: self.temporary_chat,
            bot_id: self.active_bot_id.clone().unwrap_or_default(),
            project_id: self.active_project_id.clone().unwrap_or_default(),
            mode_category,
            mode_variant_id,
            skip_thinking: false,
            thinking_level,
            feature_mode,
            personal_intelligence: self.personal_intelligence,
            agent_task: is_task || self.active_conv_is_agent,
            target_device_id,
            target_device_name,
            file_paths,
        };

        let (stream_task, handle) =
            Task::run(ipc::stream_chat(req), Message::StreamReceived).abortable();
        self.stream_abort_handle = Some(handle);

        Task::batch([stream_task, Self::scroll_to_bottom()])
    }

    fn cancel_active_stream(&mut self) {
        if let Some(handle) = self.stream_abort_handle.take() {
            handle.abort();
        }
        self.is_streaming = false;
    }

    /// 이미지 캐시 크기를 최대 96개로 제한해 장시간 사용 시 메모리 누적을 방지한다.
    fn cache_library_image(&mut self, key: String, handle: image::Handle) {
        const MAX_IMAGE_CACHE_ENTRIES: usize = 96;
        if let Some(pos) = self.library_image_order.iter().position(|k| k == &key) {
            self.library_image_order.remove(pos);
        }
        self.library_image_cache.insert(key.clone(), handle);
        self.library_image_order.push_back(key);
        while self.library_image_order.len() > MAX_IMAGE_CACHE_ENTRIES {
            if let Some(oldest) = self.library_image_order.pop_front() {
                self.library_image_cache.remove(&oldest);
            }
        }
    }

    fn flush_dirty_stream_markdown(&mut self) {
        if self.stream_markdown_dirty {
            if let Some(last) = self.messages.last_mut()
                && last.role == Role::Model
            {
                last.markdown = markdown_view::ParsedMarkdown::parse(&last.raw_text);
            }
            self.stream_markdown_dirty = false;
        }
    }

    fn apply_chunk(&mut self, ch: &ChatStreamChunk, is_done: bool) {
        if !ch.conversation_id.is_empty() && !self.temporary_chat {
            self.active_conv_id = Some(ch.conversation_id.clone());
        }
        if !ch.request_id.is_empty() {
            self.streaming_req_id = ch.request_id.clone();
            if is_done {
                self.parent_request_id = ch.request_id.clone();
            }
        }
        if !ch.response_id.is_empty() && is_done {
            self.parent_response_id = ch.response_id.clone();
        }
        if !ch.title.is_empty() {
            self.active_title = ch.title.clone();
        }

        if let Some(last) = self.messages.last_mut()
            && last.role == Role::Model
        {
            const STREAM_REPARSE_INTERVAL: Duration = Duration::from_millis(80);
            let now = Instant::now();
            let can_reparse = is_done
                || self
                    .last_stream_reparse
                    .is_none_or(|prev| now.duration_since(prev) >= STREAM_REPARSE_INTERVAL);

            if self.stream_markdown_dirty && can_reparse {
                if !ch.delta.is_empty()
                    && (ch.text.is_empty() || ch.text.starts_with(&last.raw_text))
                {
                    last.raw_text.push_str(&ch.delta);
                } else if !ch.text.is_empty() {
                    last.raw_text = ch.text.clone();
                }
                last.markdown = markdown_view::ParsedMarkdown::parse(&last.raw_text);
                self.stream_markdown_dirty = false;
                self.last_stream_reparse = Some(now);
            } else {
                let deferred = last.push_delta(&ch.delta, &ch.text, !can_reparse);
                if deferred {
                    self.stream_markdown_dirty = true;
                } else if can_reparse && (!ch.delta.is_empty() || !ch.text.is_empty()) {
                    self.last_stream_reparse = Some(now);
                }
            }
            if !ch.thoughts.is_empty() {
                last.thoughts = ch.thoughts.clone();
            } else if !ch.thoughts_delta.is_empty() {
                last.thoughts.push_str(&ch.thoughts_delta);
            }
            if !ch.model.is_empty() {
                last.model_name = ch.model.clone();
            }
            if !ch.request_id.is_empty() {
                last.request_id = ch.request_id.clone();
            }
            if !ch.response_id.is_empty() {
                last.response_id = ch.response_id.clone();
            }
            if !ch.target_device_name.is_empty() {
                last.target_device_name = ch.target_device_name.clone();
            }
            if !ch.status_text.is_empty() {
                last.status_text = ch.status_text.clone();
            }
            if !ch.artifacts.is_empty() {
                last.artifacts = ch.artifacts.clone();
            }
            if ch.follow_up.is_some() {
                last.follow_up = ch.follow_up.clone();
            }
            if ch.elicitations.is_some() {
                last.elicitations = ch.elicitations.clone();
            }
            for act in &ch.tool_activities {
                let exists = last.tool_activities.iter_mut().find(|existing| {
                    (!act.id.is_empty() && existing.id == act.id)
                        || (act.id.is_empty()
                            && existing.name == act.name
                            && existing.explanation == act.explanation)
                });
                if let Some(existing) = exists {
                    *existing = act.clone();
                } else {
                    last.tool_activities.push(act.clone());
                }
            }
        }
    }

    fn view(&self, _window: window::Id) -> Element<'_, Message> {
        let mut root_row = row![].width(Fill).height(Fill);
        if self.sidebar_open {
            let border_subtle = self.palette.border_subtle;
            let accent = self.palette.accent;
            let bg_sidebar = self.palette.bg_sidebar;
            let bg_canvas = self.palette.bg_canvas;
            let active_line = self.sidebar_resizing || self.sidebar_resize_hovered;
            let line_color = if active_line { accent } else { border_subtle };
            let line_width = if active_line { 2.0 } else { 1.0 };
            let right_pad = if active_line { 2.0 } else { 3.0 };

            // 평소에는 1px 구분선으로 보이되 6px 히트 영역을 확보해 드래그로 사이드바 폭을 조절할 수 있게 한다.
            let handle_visual = row![
                container(space::vertical().height(Fill))
                    .width(Length::Fixed(2.0))
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_sidebar)),
                        ..container::Style::default()
                    }),
                container(space::vertical().height(Fill))
                    .width(Length::Fixed(line_width))
                    .style(move |_| container::Style {
                        background: Some(Background::Color(line_color)),
                        ..container::Style::default()
                    }),
                container(space::vertical().height(Fill))
                    .width(Length::Fixed(right_pad))
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_canvas)),
                        ..container::Style::default()
                    }),
            ]
            .width(Length::Fixed(6.0))
            .height(Fill);

            let resize_handle = mouse_area(handle_visual)
                .on_press(Message::StartSidebarResize)
                .on_double_click(Message::ResetSidebarWidth)
                .on_enter(Message::SidebarResizeHover(true))
                .on_exit(Message::SidebarResizeHover(false))
                .interaction(mouse::Interaction::ResizingHorizontally);

            root_row = root_row.push(self.view_sidebar()).push(resize_handle);
        }
        root_row = root_row.push(self.view_main());

        let bg_canvas = self.palette.bg_canvas;
        let base: Element<'_, Message> = container(root_row)
            .width(Fill)
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_canvas)),
                ..container::Style::default()
            })
            .into();

        if let Some(preview) = &self.canvas_preview {
            stack![base, self.view_canvas_preview_modal(preview)]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.search_modal_open {
            stack![base, self.view_search_modal()]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.settings_modal_open {
            stack![base, self.view_settings_modal()]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.more_menu_open {
            stack![base, self.view_more_popover_overlay()]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.filter_menu_open {
            stack![base, self.view_filter_popover_overlay()]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.conv_menu_target.is_some() {
            stack![base, self.view_conv_menu_popover_overlay()]
                .width(Fill)
                .height(Fill)
                .into()
        } else {
            base
        }
    }
}

pub(crate) fn card_style(bg: Color, border_col: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: 16.0.into(),
            width: 1.0,
            color: border_subtle_or(border_col),
        },
        ..container::Style::default()
    }
}

pub(crate) fn border_subtle_or(c: Color) -> Color {
    c
}

/// 사이드바 폭을 최소·최대 한계 및 현재 창 폭의 본문 확보 여백에 맞춰 클램프한다.
fn clamp_sidebar_width(desired: f32, window_width: f32) -> f32 {
    let max_w = if window_width > 0.0 {
        (window_width - SIDEBAR_MAIN_MIN_RESERVE).clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
    } else {
        SIDEBAR_MAX_WIDTH
    };
    desired.round().clamp(SIDEBAR_MIN_WIDTH, max_w)
}

/// 현재 사이드바 폭에 비례해 단일 줄 말줄임 허용 단위(ASCII=1, 비ASCII=2)를 계산한다.
pub(crate) fn sidebar_text_units(sidebar_width: f32, base_units: usize) -> usize {
    let delta_px = sidebar_width - SIDEBAR_DEFAULT_WIDTH;
    ((base_units as f32) + delta_px / 7.5)
        .round()
        .clamp(12.0, 96.0) as usize
}

/// 사이드바 대화 제목을 단일 줄 표시 폭에 맞게 말줄임(`…`) 처리한다.
pub(crate) fn truncate_single_line(s: &str, max_units: usize) -> String {
    let single = s.replace(['\n', '\r'], " ");
    let mut units = 0usize;
    let mut out = String::new();
    for ch in single.chars() {
        let w = if ch.is_ascii() { 1 } else { 2 };
        if units + w > max_units {
            out.push('…');
            return out;
        }
        out.push(ch);
        units += w;
    }
    out
}

pub(crate) fn ghost_btn_style(
    status: button::Status,
    hover_bg: Color,
    fg: Color,
    radius: f32,
) -> button::Style {
    button::Style {
        background: if matches!(status, button::Status::Hovered) {
            Some(Background::Color(hover_bg))
        } else {
            None
        },
        text_color: fg,
        border: Border {
            radius: radius.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

/// 입력창 끝부분에 `@검색어`가 입력 중이면 해당 검색어를 추출한다.
fn extract_mention_query(text: &str) -> Option<String> {
    let trimmed = text.trim_end_matches('\n');
    let last_token = trimmed.rsplit([' ', '\n', '\t']).next()?;
    let rest = last_token.strip_prefix('@')?;
    if rest.chars().count() <= 24 {
        Some(rest.to_string())
    } else {
        None
    }
}

/// 허용된 외부 링크 스킴(`http://`, `https://`)인지 검증한다.
fn is_safe_external_url(url: &str) -> bool {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && !trimmed.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// 마크다운·버튼에서 호출하는 외부 URL을 스킴 검증 후 `xdg-open`으로 연다.
fn open_safe_external_url(url: &str) -> bool {
    let trimmed = url.trim();
    if !is_safe_external_url(trimmed) {
        return false;
    }
    std::process::Command::new("xdg-open")
        .arg(trimmed)
        .spawn()
        .is_ok()
}

/// 유닉스 초 단위 초기화 시각을 현재 UI 로케일에 맞게 사람이 읽기 쉬운 문구로 변환한다.
pub(crate) fn format_reset_time(reset_unix: i64, loc: Locale) -> String {
    if reset_unix <= 0 {
        return t(loc, "주기적 자동 초기화").to_string();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let diff = reset_unix - now;
    if diff <= 0 {
        return t(loc, "곧 초기화됨").to_string();
    }
    let hours = diff / 3600;
    let mins = (diff % 3600) / 60;
    if loc.is_en() {
        if hours >= 24 {
            let days = hours / 24;
            let rem_h = hours % 24;
            format!("Resets in about {days}d {rem_h}h")
        } else if hours > 0 {
            format!("Resets in about {hours}h {mins}m")
        } else {
            format!("Resets in about {}m", mins.max(1))
        }
    } else if hours >= 24 {
        let days = hours / 24;
        let rem_h = hours % 24;
        format!("약 {days}일 {rem_h}시간 후 초기화")
    } else if hours > 0 {
        format!("약 {hours}시간 {mins}분 후 초기화")
    } else {
        format!("약 {}분 후 초기화", mins.max(1))
    }
}

/// 첨부 메타데이터가 이미지 형식인지 판별한다.
pub(crate) fn is_attachment_image(att: &TurnAttachment) -> bool {
    if att.mime_type.to_ascii_lowercase().starts_with("image/") {
        return true;
    }
    if att.width > 0 && att.height > 0 {
        return true;
    }
    let lower_name = att.name.to_ascii_lowercase();
    lower_name.ends_with(".png")
        || lower_name.ends_with(".jpg")
        || lower_name.ends_with(".jpeg")
        || lower_name.ends_with(".webp")
        || lower_name.ends_with(".gif")
        || att.url.contains("lh3.googleusercontent.com/")
}

/// Canvas 아티팩트가 브라우저에서 바로 실행 가능한 HTML 문서인지 판별한다.
pub(crate) fn is_html_artifact(art: &TurnArtifact) -> bool {
    let lang = art.language.trim();
    if lang.eq_ignore_ascii_case("html") || lang.eq_ignore_ascii_case("htm") {
        return true;
    }
    let trimmed = art.content.trim_start().as_bytes();
    trimmed
        .get(..14)
        .is_some_and(|h| h.eq_ignore_ascii_case(b"<!doctype html"))
        || trimmed
            .get(..5)
            .is_some_and(|h| h.eq_ignore_ascii_case(b"<html"))
}

/// UI 요소 벡터를 지정한 최대 개수의 행 묶음으로 분할한다.
pub(crate) fn chunk_elements<'a>(items: Vec<Element<'a, Message>>, per_row: usize) -> Vec<Vec<Element<'a, Message>>> {
    let per_row = per_row.max(1);
    let mut rows = Vec::new();
    let mut cur = Vec::with_capacity(per_row);
    for item in items {
        cur.push(item);
        if cur.len() == per_row {
            rows.push(std::mem::replace(&mut cur, Vec::with_capacity(per_row)));
        }
    }
    if !cur.is_empty() {
        rows.push(cur);
    }
    rows
}

/// Canvas HTML 아티팩트를 사용자 전용 임시 디렉터리에 저장하고 기본 브라우저로 연다.
fn write_and_open_canvas_html(title: &str, content: &str) -> Result<PathBuf, String> {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let base_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CACHE_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(std::env::temp_dir);
    let dir = base_dir.join("gemini-ui-canvas");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));

    // 24시간 이상 지난 이전 임시 HTML 파일 자동 정리
    if let Ok(entries) = fs::read_dir(&dir) {
        let now = std::time::SystemTime::now();
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("html")
                && let Ok(meta) = entry.metadata()
                && let Ok(modified) = meta.modified()
                && now.duration_since(modified).map(|d| d.as_secs() > 86_400).unwrap_or(false)
            {
                let _ = fs::remove_file(p);
            }
        }
    }

    let slug: String = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(36)
        .collect();
    let safe_slug = if slug.trim_matches('_').is_empty() {
        "canvas".to_string()
    } else {
        slug
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!("{safe_slug}_{}_{nanos}.html", std::process::id()));
    fs::write(&path, content).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));

    std::process::Command::new("xdg-open")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(path)
}

/// 시스템 로컬 타임존 UTC 오프셋(초)을 부팅 시 1회 조회한다.
fn detect_local_tz_offset_secs() -> i64 {
    std::process::Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|s| parse_tz_offset_hhmm(s.trim()))
        .unwrap_or(0)
}

fn parse_tz_offset_hhmm(s: &str) -> Option<i64> {
    if !s.is_ascii() || s.len() != 5 {
        return None;
    }
    let sign = match s.as_bytes()[0] {
        b'+' => 1i64,
        b'-' => -1i64,
        _ => return None,
    };
    let hours: i64 = s[1..3].parse().ok()?;
    let mins: i64 = s[3..5].parse().ok()?;
    if hours > 14 || mins >= 60 {
        return None;
    }
    Some(sign * (hours * 3600 + mins * 60))
}

pub(crate) fn local_day_number(unix_secs: i64, tz_offset_secs: i64) -> i64 {
    (unix_secs + tz_offset_secs).div_euclid(86_400)
}

/// 유닉스 일수(1970-01-01 = 0)를 그레고리력 `(년, 월, 일)`로 변환한다 (Howard Hinnant 알고리즘).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = y + i64::from(m <= 2);
    (year, m, d)
}

/// 대화 턴 타임스탬프를 현재 로케일·로컬 시간대 기준의 날짜/시각 라벨로 변환한다.
pub(crate) fn format_turn_timestamp(unix_secs: i64, tz_offset_secs: i64, loc: Locale) -> String {
    let local = unix_secs + tz_offset_secs;
    let day = local.div_euclid(86_400);
    let sec_of_day = local.rem_euclid(86_400);
    let hour24 = (sec_of_day / 3600) as u32;
    let minute = ((sec_of_day % 3600) / 60) as u32;
    let (year, month, dom) = civil_from_days(day);

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(unix_secs);
    let today_day = local_day_number(now_unix, tz_offset_secs);
    let (now_year, _, _) = civil_from_days(today_day);

    if loc.is_en() {
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let (ampm, h12) = if hour24 == 0 {
            ("AM", 12)
        } else if hour24 < 12 {
            ("AM", hour24)
        } else if hour24 == 12 {
            ("PM", 12)
        } else {
            ("PM", hour24 - 12)
        };
        let date_part = if day == today_day {
            "Today".to_string()
        } else if day + 1 == today_day {
            "Yesterday".to_string()
        } else {
            let mon = MONTHS[(month.saturating_sub(1) as usize).min(11)];
            if year == now_year {
                format!("{mon} {dom}")
            } else {
                format!("{mon} {dom}, {year}")
            }
        };
        format!("{date_part} · {h12}:{minute:02} {ampm}")
    } else {
        let (ampm, h12) = if hour24 == 0 {
            ("오전", 12)
        } else if hour24 < 12 {
            ("오전", hour24)
        } else if hour24 == 12 {
            ("오후", 12)
        } else {
            ("오후", hour24 - 12)
        };
        let date_part = if day == today_day {
            "오늘".to_string()
        } else if day + 1 == today_day {
            "어제".to_string()
        } else if year == now_year {
            format!("{month}월 {dom}일")
        } else {
            format!("{year}년 {month}월 {dom}일")
        };
        format!("{date_part} · {ampm} {h12}:{minute:02}")
    }
}

/// Omarchy 테마 변경 파일(generation)을 3초 간격으로 감시해 실제 변경 시에만 이벤트를 방출한다.
fn watch_omarchy_theme() -> impl iced::futures::Stream<Item = String> {
    iced::stream::channel(
        4,
        |mut tx: iced::futures::channel::mpsc::Sender<String>| async move {
            let mut last = AppTheme::current_omarchy_generation();
            loop {
                tokio::time::sleep(Duration::from_secs(3)).await;
                let cur = AppTheme::current_omarchy_generation();
                if cur != last {
                    last = cur.clone();
                    if let Some(generation) = cur
                        && let Err(err) = tx.try_send(generation)
                        && err.is_disconnected()
                    {
                        break;
                    }
                }
            }
        },
    )
}

#[cfg(test)]
mod url_safety_tests {
    use super::{
        ChatMessage, Locale, SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH,
        civil_from_days, clamp_sidebar_width, format_turn_timestamp, is_safe_external_url,
        parse_tz_offset_hhmm, sidebar_text_units,
    };

    #[test]
    fn 사이드바_폭_클램프와_동적_말줄임_계산() {
        assert_eq!(clamp_sidebar_width(120.0, 1240.0), SIDEBAR_MIN_WIDTH);
        assert_eq!(clamp_sidebar_width(600.0, 1240.0), SIDEBAR_MAX_WIDTH);
        assert_eq!(clamp_sidebar_width(340.4, 1240.0), 340.0);
        // 좁은 창(660px)에서는 본문 최소폭(360px)을 보장하기 위해 300px까지만 허용된다.
        assert_eq!(clamp_sidebar_width(420.0, 660.0), 300.0);

        assert_eq!(sidebar_text_units(SIDEBAR_DEFAULT_WIDTH, 28), 28);
        assert!(sidebar_text_units(420.0, 28) > 40);
        assert!(sidebar_text_units(SIDEBAR_MIN_WIDTH, 28) < 28);
    }

    #[test]
    fn 경량_sse_빈_full_text_증분_누적() {
        let mut msg = ChatMessage::model(String::new(), String::new(), String::new());
        assert!(!msg.push_delta("안녕", "", true));
        assert!(!msg.push_delta("하세요", "", true));
        assert_eq!(msg.raw_text, "안녕하세요");
        // 전체 교체 패킷이 오면 덮어쓴다
        assert!(!msg.push_delta("", "최종 문장", false));
        assert_eq!(msg.raw_text, "최종 문장");
    }

    #[test]
    fn allows_only_http_and_https_schemes() {
        assert!(is_safe_external_url("https://gemini.google.com/app"));
        assert!(is_safe_external_url("http://localhost:8080/test"));
        assert!(!is_safe_external_url("file:///etc/passwd"));
        assert!(!is_safe_external_url("javascript:alert(1)"));
        assert!(!is_safe_external_url("ssh://user@host"));
        assert!(!is_safe_external_url("https://example.com\nmalicious"));
        assert!(!is_safe_external_url(""));
    }

    #[test]
    fn 타임존_오프셋과_그레고리력_타임스탬프_포맷() {
        assert_eq!(parse_tz_offset_hhmm("+0900"), Some(32_400));
        assert_eq!(parse_tz_offset_hhmm("-0530"), Some(-19_800));
        assert_eq!(parse_tz_offset_hhmm("invalid"), None);
        // 5바이트 비ASCII 입력에서 UTF-8 슬라이싱 패닉이 발생하지 않아야 한다.
        assert_eq!(parse_tz_offset_hhmm("+가0"), None);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_555), (2026, 4, 12));

        // 2024-03-15 05:30:00 UTC == KST(+0900) 2024-03-15 14:30 (오후 2:30)
        let ts = 1_710_480_600i64;
        let ko = format_turn_timestamp(ts, 32_400, Locale::Ko);
        let en = format_turn_timestamp(ts, 32_400, Locale::En);
        assert!(ko.contains("2024년 3월 15일") && ko.contains("오후 2:30"), "ko={ko}");
        assert!(en.contains("Mar 15, 2024") && en.contains("2:30 PM"), "en={en}");
    }
}
