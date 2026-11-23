use crate::glow::AvatarCircleMask;
use crate::i18n::t;
use crate::icons::{Icon, sparkle_color};
use crate::ipc::ConversationSummary;
use crate::theme::{AppTheme, font_medium, font_regular, font_semibold};
use crate::{
    ActiveSection, App, ConvFilter, Message, SIDEBAR_MIN_WIDTH, ghost_btn_style,
    sidebar_text_units, truncate_single_line,
};
use iced::widget::{
    button, canvas, column, container, image, row, scrollable, space, stack, text, text_input,
};
use iced::{Alignment, Background, Border, ContentFit, Element, Fill, Length, Padding};

impl App {
    /// 원본 Gemini 데스크톱 앱과 1:1 대응하는 좌측 사이드바.
    pub(crate) fn view_sidebar(&self) -> Element<'_, Message> {
        let pal = &self.palette;
        let bg_sidebar = pal.bg_sidebar;
        let bg_hover = pal.bg_hover;
        let bg_nav_active = pal.bg_nav_active;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;
        let accent = pal.accent;

        // 1. 최상단 헤더: 4색 스파클 + Gemini | 돋보기(검색) + 사이드바 토글
        let brand = button(
            row![
                sparkle_color(21.0),
                text("Gemini")
                    .size(17.5)
                    .font(font_semibold())
                    .color(text_strong),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .on_press(Message::NewChat)
        .padding([4, 6])
        .style(move |_, status| ghost_btn_style(status, bg_hover, text_strong, 10.0));

        let header = row![
            brand,
            space::horizontal(),
            button(container(Icon::Search.view(17.5, text_primary)).center(Fill))
                .on_press(Message::OpenSearchModal)
                .padding(0)
                .width(Length::Fixed(32.0))
                .height(Length::Fixed(32.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 16.0)),
            button(container(Icon::SidebarToggle.view(17.5, text_primary)).center(Fill))
                .on_press(Message::ToggleSidebar)
                .padding(0)
                .width(Length::Fixed(32.0))
                .height(Length::Fixed(32.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 16.0)),
        ]
        .spacing(2)
        .align_y(Alignment::Center);

        // 2. 메인 내비게이션 6개 항목 (새 채팅 / 라이브러리 / 이미지 / 예약 / 학생 / 더보기)
        let nav_row = |icon: Icon, label: &'static str, active: bool, msg: Message| {
            button(
                row![
                    icon.view(17.0, if active { text_strong } else { text_primary }),
                    text(label)
                        .size(13.5)
                        .font(if active { font_medium() } else { font_regular() })
                        .color(if active { text_strong } else { text_primary }),
                ]
                .spacing(11)
                .align_y(Alignment::Center),
            )
            .on_press(msg)
            .width(Fill)
            .padding([7, 11])
            .style(move |_, status| {
                let bg = if active {
                    Some(Background::Color(bg_nav_active))
                } else if matches!(status, button::Status::Hovered) {
                    Some(Background::Color(bg_hover))
                } else {
                    None
                };
                button::Style {
                    background: bg,
                    text_color: if active { text_strong } else { text_primary },
                    border: Border {
                        radius: 18.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }
            })
        };

        let loc = self.locale;
        let is_new_chat_active =
            self.active_section == ActiveSection::Chat && self.active_conv_id.is_none();

        let nav_col = column![
            nav_row(Icon::NewChat, t(loc, "새 채팅"), is_new_chat_active, Message::NewChat),
            nav_row(
                Icon::Library,
                t(loc, "라이브러리"),
                self.active_section == ActiveSection::Library,
                Message::SwitchSection(ActiveSection::Library),
            ),
            nav_row(
                Icon::Image,
                t(loc, "이미지"),
                self.active_section == ActiveSection::Images,
                Message::SwitchSection(ActiveSection::Images),
            ),
            nav_row(
                Icon::Clock,
                t(loc, "예약"),
                self.active_section == ActiveSection::Schedules,
                Message::SwitchSection(ActiveSection::Schedules),
            ),
            nav_row(
                Icon::School,
                t(loc, "학생"),
                self.active_section == ActiveSection::Student,
                Message::SwitchSection(ActiveSection::Student),
            ),
            nav_row(
                Icon::MoreHoriz,
                t(loc, "더보기"),
                self.more_menu_open
                    || matches!(
                        self.active_section,
                        ActiveSection::Personalize
                            | ActiveSection::Videos
                            | ActiveSection::GemsHub
                            | ActiveSection::Gem(_)
                    ),
                Message::ToggleMoreMenu,
            ),
        ]
        .spacing(2);

        // 3. 노트북 섹션
        let mut nb_col = column![
            container(
                text(t(loc, "노트북"))
                    .size(11.5)
                    .font(font_medium())
                    .color(text_muted),
            )
            .padding(Padding {
                top: 10.0,
                right: 10.0,
                bottom: 4.0,
                left: 10.0,
            }),
            nav_row(Icon::Plus, t(loc, "새 노트북"), false, Message::NewNotebook),
        ]
        .spacing(1);

        for nb in self.notebooks.iter().take(3) {
            let nb_id = nb.id.clone();
            let active = self.active_section == ActiveSection::Notebooks
                && self.editing_notebook.id == nb.id;
            let title = truncate_single_line(
                t(loc, &nb.title),
                sidebar_text_units(self.sidebar_width, 24),
            );
            nb_col = nb_col.push(
                button(
                    row![
                        Icon::Notebook.view(15.0, text_secondary),
                        text(title)
                            .size(13)
                            .color(if active { text_strong } else { text_primary })
                            .wrapping(text::Wrapping::None),
                    ]
                    .spacing(9)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::OpenNotebook(nb_id))
                .width(Fill)
                .padding([6, 11])
                .style(move |_, status| {
                    let bg = if active {
                        Some(Background::Color(bg_nav_active))
                    } else if matches!(status, button::Status::Hovered) {
                        Some(Background::Color(bg_hover))
                    } else {
                        None
                    };
                    button::Style {
                        background: bg,
                        text_color: text_primary,
                        border: Border {
                            radius: 16.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }
                }),
            );
        }

        nb_col = nb_col.push(nav_row(
            Icon::MoreHoriz,
            t(loc, "모든 노트북"),
            self.active_section == ActiveSection::Notebooks,
            Message::SwitchSection(ActiveSection::Notebooks),
        ));

        // 4. 전체 대화 목록 섹션 (헤더 + 필터 팝오버 + 고정 상단/일반·태스크 병합 + 안읽음 파란 점)
        let filter_active = self.conv_filter != ConvFilter::All;
        let conv_header = container(
            row![
                text(t(loc, self.conv_filter.label()))
                    .size(11.5)
                    .font(font_medium())
                    .color(if filter_active { accent } else { text_muted }),
                space::horizontal(),
                button(
                    container(
                        Icon::Filter.view(
                            16.0,
                            if filter_active || self.filter_menu_open {
                                accent
                            } else {
                                text_primary
                            },
                        )
                    )
                    .center(Fill),
                )
                .on_press(Message::ToggleFilterMenu)
                .padding(0)
                .width(Length::Fixed(26.0))
                .height(Length::Fixed(26.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 13.0)),
            ]
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 12.0,
            right: 6.0,
            bottom: 4.0,
            left: 10.0,
        });

        let filtered: Vec<&ConversationSummary> = self
            .conversations
            .iter()
            .filter(|c| match self.conv_filter {
                ConvFilter::All => true,
                ConvFilter::Ready => !c.has_unread,
                ConvFilter::NeedsAttention => c.has_unread,
                ConvFilter::InProgress => {
                    self.is_streaming
                        && self.active_conv_id.as_deref() == Some(c.conversation_id.as_str())
                }
                ConvFilter::PinnedOnly => c.pinned,
            })
            .collect();

        let mut conv_col = column![].spacing(1).padding(Padding {
            top: 0.0,
            right: 6.0,
            bottom: 12.0,
            left: 0.0,
        });
        if filtered.is_empty() {
            conv_col = conv_col.push(
                container(
                    text(t(loc, "표시할 대화가 없습니다"))
                        .size(12)
                        .color(text_muted),
                )
                .padding([12, 10]),
            );
        } else {
            for c in filtered {
                conv_col = conv_col.push(self.view_conv_item(c));
            }
            if self.conversations_next_page_token.is_some() {
                let load_lbl = if self.loading_more_conversations {
                    t(loc, "불러오는 중...")
                } else {
                    t(loc, "이전 대화 더 불러오기")
                };
                conv_col = conv_col.push(
                    container(
                        button(
                            text(load_lbl)
                                .size(12)
                                .font(font_medium())
                                .color(text_secondary),
                        )
                        .on_press(Message::LoadMoreConversations)
                        .width(Fill)
                        .padding([7, 10])
                        .style(move |_, status| {
                            ghost_btn_style(status, bg_hover, text_secondary, 12.0)
                        }),
                    )
                    .padding(Padding {
                        top: 6.0,
                        right: 0.0,
                        bottom: 4.0,
                        left: 0.0,
                    }),
                );
            }
        }

        // 내비게이션 + 노트북 + 전체 목록을 하나의 자연스러운 스크롤 컨테이너로 구성 (원본과 동일)
        let scroll_content = column![nav_col, nb_col, conv_header, conv_col]
            .spacing(2)
            .width(Fill);

        let body_scroll = scrollable(scroll_content)
            .height(Fill)
            .width(Fill)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::default().width(4).scroller_width(4),
            ))
            .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(text_primary, status));

        // 5. 하단 프로필 바 (원형 아바타 + 사용자 이름 + 플랜/상태 + 설정 톱니바퀴)
        let avatar_el: Element<'_, Message> = if let Some(handle) = &self.avatar_handle {
            stack![
                image(handle.clone())
                    .width(Length::Fixed(32.0))
                    .height(Length::Fixed(32.0))
                    .content_fit(ContentFit::Cover),
                canvas(AvatarCircleMask {
                    cache: &self.avatar_mask_cache,
                    bg: bg_sidebar,
                })
                    .width(Length::Fixed(32.0))
                    .height(Length::Fixed(32.0)),
            ]
            .width(Length::Fixed(32.0))
            .height(Length::Fixed(32.0))
            .into()
        } else {
            let initial = self
                .profile
                .name
                .chars()
                .next()
                .unwrap_or('G')
                .to_string();
            let accent_fg = pal.accent_fg;
            container(
                text(initial)
                    .size(13.5)
                    .font(font_semibold())
                    .color(accent_fg),
            )
            .width(Length::Fixed(32.0))
            .height(Length::Fixed(32.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_| container::Style {
                background: Some(Background::Color(accent)),
                border: Border {
                    radius: 16.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into()
        };

        let display_name = if !self.profile.name.is_empty() {
            self.profile.name.as_str()
        } else if self.daemon_connected {
            self.daemon_label.as_str()
        } else {
            t(loc, "Gemini 사용자")
        };
        let tier_label = if !self.profile.tier.is_empty() {
            self.profile.tier.as_str()
        } else if self.daemon_connected {
            t(loc, "연결됨")
        } else {
            t(loc, "오프라인")
        };
        let (remote_dot_color, remote_status_label) = if !self.remote_status.current_tool.is_empty()
        {
            let tool = truncate_single_line(&self.remote_status.current_tool, 10);
            (
                accent,
                if loc.is_en() {
                    format!("Running: {tool}")
                } else {
                    format!("실행 중: {tool}")
                },
            )
        } else if self.remote_status.remote_enabled && self.remote_status.punctual_connected {
            (pal.success, t(loc, "원격 연결됨").to_string())
        } else if self.remote_status.remote_enabled {
            (text_muted, t(loc, "원격 대기").to_string())
        } else if self.daemon_connected {
            (text_muted, t(loc, "원격 꺼짐").to_string())
        } else {
            (pal.danger, t(loc, "데몬 끊김").to_string())
        };
        let remote_dot = container(space::horizontal())
            .width(Length::Fixed(7.0))
            .height(Length::Fixed(7.0))
            .style(move |_| container::Style {
                background: Some(Background::Color(remote_dot_color)),
                border: Border {
                    radius: 4.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            });

        let profile_trigger = button(
            row![
                avatar_el,
                column![
                    text(truncate_single_line(
                        display_name,
                        sidebar_text_units(self.sidebar_width, 18),
                    ))
                        .size(13.5)
                        .font(font_medium())
                        .color(text_strong)
                        .wrapping(text::Wrapping::None),
                    row![
                        text(tier_label).size(11.0).color(text_secondary),
                        text("·").size(11.0).color(text_muted),
                        remote_dot,
                        text(remote_status_label)
                            .size(11.0)
                            .color(text_secondary)
                            .wrapping(text::Wrapping::None),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                ]
                .spacing(2),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .on_press(Message::OpenSettingsModal)
        .padding([4, 6])
        .width(Fill)
        .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 12.0));

        let profile_bar = container(
            row![
                profile_trigger,
                button(container(Icon::Settings.view(18.5, text_primary)).center(Fill))
                    .on_press(Message::OpenSettingsModal)
                    .padding(0)
                    .width(Length::Fixed(34.0))
                    .height(Length::Fixed(34.0))
                    .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 17.0)),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 8.0,
            right: 4.0,
            bottom: 4.0,
            left: 6.0,
        })
        .width(Fill);

        container(
            column![header, body_scroll, profile_bar]
                .spacing(8)
                .padding(Padding {
                    top: 10.0,
                    right: 10.0,
                    bottom: 8.0,
                    left: 10.0,
                }),
        )
        .width(Length::Fixed(
            (self.sidebar_width - 2.0).max(SIDEBAR_MIN_WIDTH - 2.0),
        ))
        .height(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_sidebar)),
            ..container::Style::default()
        })
        .into()
    }

    fn view_conv_item<'a>(&'a self, c: &'a ConversationSummary) -> Element<'a, Message> {
        let pal = &self.palette;
        let is_active = self.active_section == ActiveSection::Chat
            && self.active_conv_id.as_deref() == Some(c.conversation_id.as_str());
        let is_renaming = self.renaming_conv_id.as_deref() == Some(c.conversation_id.as_str());

        let bg_hover = pal.bg_hover;
        let bg_active = pal.bg_active;
        let bg_nav_active = pal.bg_nav_active;
        let bg_composer = pal.bg_composer;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;
        let accent = pal.accent;
        let danger = pal.danger;

        if is_renaming {
            let input = text_input(t(self.locale, "새 제목"), &self.rename_draft)
                .on_input(Message::RenameDraftChanged)
                .on_submit(Message::SubmitRename)
                .size(12.5)
                .padding([5, 8])
                .style(move |_, _| text_input::Style {
                    background: Background::Color(bg_composer),
                    border: Border {
                        radius: 10.0.into(),
                        width: 1.0,
                        color: accent,
                    },
                    icon: text_secondary,
                    placeholder: text_muted,
                    value: text_primary,
                    selection: bg_active,
                });

            let ok_btn = button(container(Icon::Check.view(13.0, accent)).center(Fill))
                .on_press(Message::SubmitRename)
                .padding(0)
                .width(Length::Fixed(24.0))
                .height(Length::Fixed(24.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, accent, 8.0));
            let cancel_btn = button(container(Icon::Close.view(12.0, text_secondary)).center(Fill))
                .on_press(Message::CancelRename)
                .padding(0)
                .width(Length::Fixed(24.0))
                .height(Length::Fixed(24.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_secondary, 8.0));

            return row![input, ok_btn, cancel_btn]
                .spacing(3)
                .align_y(Alignment::Center)
                .into();
        }

        let is_menu_target =
            self.conv_menu_target.as_deref() == Some(c.conversation_id.as_str());
        let raw_title = if c.title.is_empty() {
            c.conversation_id.as_str()
        } else {
            c.title.as_str()
        };
        let has_right_badge = c.pinned || c.has_unread;
        let base_units = if is_active || is_menu_target {
            24
        } else if has_right_badge {
            25
        } else {
            28
        };
        let display_title =
            truncate_single_line(raw_title, sidebar_text_units(self.sidebar_width, base_units));

        let mut btn_content = row![
            text(display_title)
                .size(13)
                .font(if is_active || c.has_unread {
                    font_medium()
                } else {
                    font_regular()
                })
                .color(if is_active || c.has_unread {
                    text_strong
                } else {
                    text_primary
                })
                .wrapping(text::Wrapping::None),
            space::horizontal(),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        if c.pinned {
            btn_content = btn_content.push(Icon::Pin.view(13.0, text_secondary));
        } else if c.has_unread {
            let unread_dot = container(space::horizontal())
                .width(Length::Fixed(6.5))
                .height(Length::Fixed(6.5))
                .style(move |_| container::Style {
                    background: Some(Background::Color(accent)),
                    border: Border {
                        radius: 3.25.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                });
            btn_content = btn_content.push(unread_dot);
        }

        let select_btn = button(btn_content)
            .on_press(Message::SelectConversation(
                c.conversation_id.clone(),
                c.title.clone(),
            ))
            .width(Fill)
            .padding([6, 10])
            .style(move |_, status| {
                let bg = if is_active || is_menu_target {
                    Some(Background::Color(bg_nav_active))
                } else if matches!(status, button::Status::Hovered) {
                    Some(Background::Color(bg_hover))
                } else {
                    None
                };
                button::Style {
                    background: bg,
                    text_color: if is_active { text_strong } else { text_primary },
                    border: Border {
                        radius: 16.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }
            });

        let more_icon_color = if is_active || is_menu_target {
            text_primary
        } else {
            text_muted
        };
        let more_btn = button(container(Icon::MoreVert.view(15.0, more_icon_color)).center(Fill))
            .on_press(Message::OpenConvMenu(c.conversation_id.clone()))
            .padding(0)
            .width(Length::Fixed(24.0))
            .height(Length::Fixed(24.0))
            .style(move |_, status| ghost_btn_style(status, bg_hover, more_icon_color, 12.0));

        let _ = danger;
        row![select_btn, more_btn]
            .spacing(1)
            .align_y(Alignment::Center)
            .into()
    }
}
