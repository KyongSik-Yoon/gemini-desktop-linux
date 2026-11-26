use crate::i18n::{pick, t};
use crate::icons::Icon;
use crate::theme::{font_medium, font_regular};
use crate::{
    ActiveSection, App, CONTENT_MAX_WIDTH, ComposerMode, ConvFilter, FEATURE_MODE_CANVAS,
    FEATURE_MODE_DEEP_RESEARCH, FEATURE_MODE_GUIDED_LEARNING, FEATURE_MODE_IMAGE,
    FEATURE_MODE_MUSIC, FEATURE_MODE_VIDEO, Message, THINKING_HIGH, THINKING_LOW, THINKING_MEDIUM,
    WELCOME_MAX_WIDTH, ghost_btn_style, truncate_single_line,
};
use iced::widget::{button, column, container, mouse_area, row, space, stack, text};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding, Shadow, Vector};

fn popover_card<'a>(
    content: impl Into<Element<'a, Message>>,
    width: f32,
    bg: Color,
    border_subtle: Color,
    is_dark: bool,
) -> container::Container<'a, Message> {
    let shadow_alpha = if is_dark { 0.45 } else { 0.14 };
    container(content)
        .width(Length::Fixed(width))
        .padding(8)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                radius: 16.0.into(),
                width: 1.0,
                color: border_subtle,
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, shadow_alpha),
                offset: Vector::new(0.0, 8.0),
                blur_radius: 26.0,
            },
            ..container::Style::default()
        })
}

fn popover_divider<'a>(color: Color) -> Element<'a, Message> {
    container(space::horizontal())
        .width(Fill)
        .height(Length::Fixed(1.0))
        .padding([4, 0])
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            ..container::Style::default()
        })
        .into()
}

impl App {
    /// 입력창 `+` 버튼 클릭 시 표시되는 도구/첨부 플로팅 팝오버 (채팅/태스크 모드 분기).
    pub(crate) fn view_tools_popover_overlay<'a>(
        &'a self,
        is_welcome_view: bool,
    ) -> Element<'a, Message> {
        let pal = &self.palette;
        let loc = self.locale;
        let bg_popover = pal.bg_popover;
        let bg_hover = pal.bg_hover;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let accent = pal.accent;

        let menu_item = |icon: Icon,
                         label: &'static str,
                         active: bool,
                         trailing: Option<Element<'a, Message>>,
                         msg: Message|
         -> button::Button<'a, Message> {
            let mut r = row![
                icon.view(16.0, if active { accent } else { text_primary }),
                text(label)
                    .size(13.5)
                    .font(if active { font_medium() } else { font_regular() })
                    .color(if active { accent } else { text_primary }),
                space::horizontal(),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            if let Some(tr) = trailing {
                r = r.push(tr);
            }
            button(r)
                .on_press(msg)
                .width(Fill)
                .padding([8, 12])
                .style(move |_, status| {
                    let hovered = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: if active {
                            Some(Background::Color(accent_soft))
                        } else if hovered {
                            Some(Background::Color(bg_hover))
                        } else {
                            None
                        },
                        text_color: text_primary,
                        border: Border {
                            radius: 10.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }
                })
        };

        let mut list = column![
            menu_item(
                Icon::AttachFile,
                t(loc, "파일 업로드"),
                false,
                None,
                Message::OpenFilePicker,
            ),
            menu_item(
                Icon::Drive,
                t(loc, "Drive에서 파일 추가"),
                false,
                None,
                Message::OpenExternalUrl("https://drive.google.com".into()),
            ),
            menu_item(
                Icon::ScreenShare,
                t(loc, "화면 캡처 첨부"),
                false,
                Some(Icon::ChevronRight.view(14.0, text_secondary).into()),
                Message::StartScreenCapture,
            ),
        ]
        .spacing(2);

        if self.composer_mode == ComposerMode::Task {
            let dev_name = self
                .selected_target_device()
                .map(|d| truncate_single_line(&d.device_name, 14))
                .unwrap_or_else(|| t(loc, "기기 선택").into());
            list = list.push(popover_divider(border_subtle));
            list = list.push(menu_item(
                Icon::Laptop,
                t(loc, "데스크톱에서 실행"),
                false,
                Some(
                    row![
                        text(dev_name).size(11.5).color(text_secondary),
                        Icon::ChevronRight.view(14.0, text_secondary),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center)
                    .into(),
                ),
                Message::ToggleDeviceMenu,
            ));
        } else {
            let fm = self.selected_feature_mode;
            let check_if = |m: i32| -> Option<Element<'_, Message>> {
                if fm == m {
                    Some(Icon::Check.view(15.0, accent).into())
                } else {
                    None
                }
            };
            list = list.push(popover_divider(border_subtle));
            list = list
                .push(menu_item(
                    Icon::Image,
                    t(loc, "이미지 만들기"),
                    fm == FEATURE_MODE_IMAGE,
                    check_if(FEATURE_MODE_IMAGE),
                    Message::SelectFeatureMode(FEATURE_MODE_IMAGE),
                ))
                .push(menu_item(
                    Icon::Video,
                    t(loc, "동영상 만들기"),
                    fm == FEATURE_MODE_VIDEO,
                    check_if(FEATURE_MODE_VIDEO),
                    Message::SelectFeatureMode(FEATURE_MODE_VIDEO),
                ))
                .push(menu_item(
                    Icon::Music,
                    t(loc, "음악 만들기"),
                    fm == FEATURE_MODE_MUSIC,
                    check_if(FEATURE_MODE_MUSIC),
                    Message::SelectFeatureMode(FEATURE_MODE_MUSIC),
                ))
                .push(menu_item(
                    Icon::Canvas,
                    "Canvas",
                    fm == FEATURE_MODE_CANVAS,
                    check_if(FEATURE_MODE_CANVAS),
                    Message::SelectFeatureMode(FEATURE_MODE_CANVAS),
                ))
                .push(menu_item(
                    Icon::Research,
                    "Deep Research",
                    fm == FEATURE_MODE_DEEP_RESEARCH,
                    check_if(FEATURE_MODE_DEEP_RESEARCH),
                    Message::SelectFeatureMode(FEATURE_MODE_DEEP_RESEARCH),
                ))
                .push(menu_item(
                    Icon::Book,
                    t(loc, "가이드 학습"),
                    fm == FEATURE_MODE_GUIDED_LEARNING,
                    check_if(FEATURE_MODE_GUIDED_LEARNING),
                    Message::SelectFeatureMode(FEATURE_MODE_GUIDED_LEARNING),
                ));

            list = list.push(popover_divider(border_subtle));
            let pi_on = self.personal_intelligence;
            let pi_badge = container(
                text(if pi_on { t(loc, "켜짐") } else { "Labs" })
                    .size(11)
                    .font(font_medium())
                    .color(if pi_on { accent } else { text_secondary }),
            )
            .padding([2, 7])
            .style(move |_| container::Style {
                background: Some(Background::Color(if pi_on { accent_soft } else { pal.bg_chip })),
                border: Border {
                    radius: 8.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            });
            list = list.push(menu_item(
                Icon::Sparkle,
                t(loc, "개인 인텔리전스"),
                pi_on,
                Some(pi_badge.into()),
                Message::TogglePersonalIntelligence,
            ));
        }

        let card = popover_card(list, 260.0, bg_popover, border_subtle, pal.is_dark);
        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::CloseToolsMenu);

        let max_w = if is_welcome_view {
            WELCOME_MAX_WIDTH
        } else {
            CONTENT_MAX_WIDTH
        };
        let aligned_row = container(
            container(row![card, space::horizontal()].width(Fill))
                .max_width(max_w)
                .width(Fill),
        )
        .padding(Padding {
            top: 0.0,
            right: 24.0,
            bottom: 0.0,
            left: 28.0,
        })
        .center_x(Fill)
        .width(Fill);

        let positioned: Element<'_, Message> = if is_welcome_view {
            column![
                space::vertical().height(Length::FillPortion(54)),
                container(aligned_row)
                    .width(Fill)
                    .height(Length::FillPortion(46)),
            ]
            .width(Fill)
            .height(Fill)
            .into()
        } else {
            column![
                space::vertical().height(Fill),
                aligned_row,
                space::vertical().height(Length::Fixed(94.0)),
            ]
            .width(Fill)
            .height(Fill)
            .into()
        };

        let _ = text_strong;
        stack![backdrop, positioned].width(Fill).height(Fill).into()
    }

    /// 원본 Gemini 앱과 동일한 모델 선택기 팝오버 + 하단 `사고 수준 ▸` 서브메뉴.
    pub(crate) fn view_model_popover_overlay(&self, is_welcome_view: bool) -> Element<'_, Message> {
        let pal = &self.palette;
        let loc = self.locale;
        let bg_popover = pal.bg_popover;
        let bg_hover = pal.bg_hover;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let accent = pal.accent;

        let active_model = self.selected_model();
        let active_id = active_model.map(|m| m.id.as_str());
        let supports_thinking = active_model.is_some_and(|m| m.supports_thinking);

        let mut list = column![].spacing(2);
        let models = self.picker_models();
        if models.is_empty() {
            list = list.push(
                container(
                    text(t(loc, "사용 가능한 모델을 불러오는 중..."))
                        .size(13)
                        .color(text_secondary),
                )
                .padding([10, 12]),
            );
        } else {
            for m in models {
                let is_selected = active_id == Some(m.id.as_str());
                let subtitle = m.picker_subtitle();

                let mut label_col = column![
                    text(m.picker_heading())
                        .size(13.5)
                        .font(font_medium())
                        .color(if is_selected { text_strong } else { text_primary }),
                ]
                .spacing(2);

                if !subtitle.is_empty() {
                    label_col = label_col.push(
                        text(subtitle)
                            .size(12)
                            .color(text_secondary)
                            .wrapping(text::Wrapping::WordOrGlyph),
                    );
                }

                let mut item_row = row![label_col, space::horizontal()]
                    .spacing(12)
                    .align_y(Alignment::Center);
                if is_selected {
                    item_row = item_row.push(Icon::Check.view(16.0, accent));
                }

                let id_clone = m.id.clone();
                let item_btn = button(item_row)
                    .on_press(Message::SelectModel(id_clone))
                    .width(Fill)
                    .padding([8, 12])
                    .style(move |_, status| {
                        let hovered = matches!(status, button::Status::Hovered);
                        let bg = if is_selected {
                            Some(Background::Color(accent_soft))
                        } else if hovered {
                            Some(Background::Color(bg_hover))
                        } else {
                            None
                        };
                        button::Style {
                            background: bg,
                            text_color: text_primary,
                            border: Border {
                                radius: 12.0.into(),
                                ..Border::default()
                            },
                            ..button::Style::default()
                        }
                    });
                list = list.push(item_btn);
            }
        }

        // 사고 수준(Thinking Level) 섹션 — 지원하는 모델일 때만 하단 표시
        if supports_thinking {
            list = list.push(popover_divider(border_subtle));
            let cur_level_label = match self.thinking_level {
                THINKING_LOW => t(loc, "낮음"),
                THINKING_HIGH => t(loc, "높음"),
                _ => t(loc, "보통"),
            };
            let sub_open = self.thinking_submenu_open;
            let thinking_row = button(
                row![
                    text(t(loc, "사고 수준"))
                        .size(13.5)
                        .font(font_medium())
                        .color(text_primary),
                    space::horizontal(),
                    text(cur_level_label).size(12).color(text_secondary),
                    if sub_open {
                        Icon::ChevronUp.view(14.0, text_secondary)
                    } else {
                        Icon::ChevronRight.view(14.0, text_secondary)
                    },
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::ToggleThinkingSubmenu)
            .width(Fill)
            .padding([8, 12])
            .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 10.0));
            list = list.push(thinking_row);

            if sub_open {
                let levels = [
                    (THINKING_LOW, t(loc, "낮음"), t(loc, "속도 및 효율성")),
                    (THINKING_MEDIUM, t(loc, "보통"), t(loc, "균형 잡힌 깊이")),
                    (THINKING_HIGH, t(loc, "높음"), t(loc, "매우 철저함")),
                ];
                let mut sub_col = column![].spacing(2).padding(Padding {
                    top: 2.0,
                    right: 0.0,
                    bottom: 2.0,
                    left: 8.0,
                });
                for (code, title, desc) in levels {
                    let selected = self.thinking_level == code
                        || (self.thinking_level == 0 && code == THINKING_MEDIUM);
                    let mut r = row![
                        column![
                            text(title)
                                .size(13)
                                .font(font_medium())
                                .color(if selected { text_strong } else { text_primary }),
                            text(desc).size(11.5).color(text_secondary),
                        ]
                        .spacing(1),
                        space::horizontal(),
                    ]
                    .align_y(Alignment::Center);
                    if selected {
                        r = r.push(Icon::Check.view(15.0, accent));
                    }
                    sub_col = sub_col.push(
                        button(r)
                            .on_press(Message::SelectThinkingLevel(code))
                            .width(Fill)
                            .padding([7, 10])
                            .style(move |_, status| {
                                let hovered = matches!(status, button::Status::Hovered);
                                button::Style {
                                    background: if selected {
                                        Some(Background::Color(accent_soft))
                                    } else if hovered {
                                        Some(Background::Color(bg_hover))
                                    } else {
                                        None
                                    },
                                    text_color: text_primary,
                                    border: Border {
                                        radius: 10.0.into(),
                                        ..Border::default()
                                    },
                                    ..button::Style::default()
                                }
                            }),
                    );
                }
                list = list.push(sub_col);
            }
        }

        let card = popover_card(list, 310.0, bg_popover, border_subtle, pal.is_dark);
        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::CloseModelMenu);

        let max_w = if is_welcome_view {
            WELCOME_MAX_WIDTH
        } else {
            CONTENT_MAX_WIDTH
        };
        let aligned_row = container(
            container(row![space::horizontal(), card].width(Fill))
                .max_width(max_w)
                .width(Fill),
        )
        .padding(Padding {
            top: 0.0,
            right: 44.0,
            bottom: 0.0,
            left: 24.0,
        })
        .center_x(Fill)
        .width(Fill);

        let positioned: Element<'_, Message> = if is_welcome_view {
            column![
                space::vertical().height(Length::FillPortion(54)),
                container(aligned_row)
                    .width(Fill)
                    .height(Length::FillPortion(46)),
            ]
            .width(Fill)
            .height(Fill)
            .into()
        } else {
            column![
                space::vertical().height(Fill),
                aligned_row,
                space::vertical().height(Length::Fixed(94.0)),
            ]
            .width(Fill)
            .height(Fill)
            .into()
        };

        stack![backdrop, positioned].width(Fill).height(Fill).into()
    }

    /// 입력창에서 `@` 입력 시 표시되는 커넥터/스킬/MCP 멘션 팝오버.
    pub(crate) fn view_mention_popover_overlay(&self, is_welcome_view: bool) -> Element<'_, Message> {
        let Some(query) = &self.mention_query else {
            return container(space::horizontal()).into();
        };
        let pal = &self.palette;
        let loc = self.locale;
        let bg_popover = pal.bg_popover;
        let bg_hover = pal.bg_hover;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_secondary = pal.text_secondary;
        let accent = pal.accent;

        let q_lower = query.to_ascii_lowercase();
        let mut candidates: Vec<(String, String, String)> = vec![
            (
                "Google Drive".into(),
                t(loc, "드라이브 파일 검색·요약").into(),
                t(loc, "커넥터").into(),
            ),
            (
                "Gmail".into(),
                t(loc, "이메일 검색 및 초안 작성").into(),
                t(loc, "커넥터").into(),
            ),
            (
                "Google Calendar".into(),
                t(loc, "일정 조회 및 생성").into(),
                t(loc, "커넥터").into(),
            ),
            (
                "YouTube".into(),
                t(loc, "동영상 검색 및 내용 요약").into(),
                t(loc, "커넥터").into(),
            ),
            (
                "user-instructions".into(),
                t(loc, "내 맞춤 지침 적용").into(),
                t(loc, "스킬").into(),
            ),
        ];
        for srv in &self.remote_status.mcp_servers {
            let name = if !srv.display_name.is_empty() {
                srv.display_name.clone()
            } else {
                srv.server_id.clone()
            };
            if !name.is_empty() {
                candidates.push((name, t(loc, "로컬 연동 MCP 서버").into(), "MCP".into()));
            }
        }
        let filtered: Vec<(String, String, String)> = candidates
            .into_iter()
            .filter(|(n, d, _)| {
                q_lower.is_empty()
                    || n.to_ascii_lowercase().contains(&q_lower)
                    || d.to_ascii_lowercase().contains(&q_lower)
            })
            .take(7)
            .collect();

        let mut list = column![
            container(
                text(t(loc, "@ 멘션 — 커넥터 및 MCP"))
                    .size(11.5)
                    .font(font_medium())
                    .color(text_secondary),
            )
            .padding([5, 10]),
        ]
        .spacing(2);

        if filtered.is_empty() {
            list = list.push(
                container(
                    text(t(loc, "일치하는 커넥터나 MCP 서버가 없습니다"))
                        .size(12.5)
                        .color(text_secondary),
                )
                .padding([8, 10]),
            );
        } else {
            for (name, desc, badge) in filtered {
                let insert_val = name.clone();
                let r = row![
                    Icon::Sparkle.view(14.0, accent),
                    column![
                        text(format!("@{name}"))
                            .size(13)
                            .font(font_medium())
                            .color(text_primary),
                        text(desc).size(11.5).color(text_secondary),
                    ]
                    .spacing(1),
                    space::horizontal(),
                    text(badge).size(11).color(text_secondary),
                ]
                .spacing(8)
                .align_y(Alignment::Center);
                list = list.push(
                    button(r)
                        .on_press(Message::InsertMention(insert_val))
                        .width(Fill)
                        .padding([7, 10])
                        .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 10.0)),
                );
            }
        }

        let card = popover_card(list, 300.0, bg_popover, border_subtle, pal.is_dark);
        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::CloseMentionMenu);

        let max_w = if is_welcome_view {
            WELCOME_MAX_WIDTH
        } else {
            CONTENT_MAX_WIDTH
        };
        let aligned_row = container(
            container(row![card, space::horizontal()].width(Fill))
                .max_width(max_w)
                .width(Fill),
        )
        .padding(Padding {
            top: 0.0,
            right: 24.0,
            bottom: 0.0,
            left: 28.0,
        })
        .center_x(Fill)
        .width(Fill);

        let positioned: Element<'_, Message> = if is_welcome_view {
            column![
                space::vertical().height(Length::FillPortion(54)),
                container(aligned_row)
                    .width(Fill)
                    .height(Length::FillPortion(46)),
            ]
            .width(Fill)
            .height(Fill)
            .into()
        } else {
            column![
                space::vertical().height(Fill),
                aligned_row,
                space::vertical().height(Length::Fixed(104.0)),
            ]
            .width(Fill)
            .height(Fill)
            .into()
        };

        stack![backdrop, positioned].width(Fill).height(Fill).into()
    }

    /// 사이드바 `더보기` 클릭 시 우측에 뜨는 플로팅 팝오버 (`맞춤설정`, `동영상`, `Gems`).
    pub(crate) fn view_more_popover_overlay(&self) -> Element<'_, Message> {
        let pal = &self.palette;
        let loc = self.locale;
        let bg_popover = pal.bg_popover;
        let bg_hover = pal.bg_hover;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let accent = pal.accent;

        let item = |icon: Icon, label: &'static str, active: bool, sec: ActiveSection| {
            button(
                row![
                    icon.view(16.0, if active { accent } else { text_primary }),
                    text(label)
                        .size(13.5)
                        .font(if active { font_medium() } else { font_regular() })
                        .color(if active { text_strong } else { text_primary }),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .on_press(Message::SwitchSection(sec))
            .width(Fill)
            .padding([8, 12])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: if active {
                        Some(Background::Color(accent_soft))
                    } else if hovered {
                        Some(Background::Color(bg_hover))
                    } else {
                        None
                    },
                    text_color: text_primary,
                    border: Border {
                        radius: 10.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }
            })
        };

        let list = column![
            item(
                Icon::Tune,
                t(loc, "맞춤설정"),
                self.active_section == ActiveSection::Personalize,
                ActiveSection::Personalize,
            ),
            item(
                Icon::Video,
                t(loc, "동영상"),
                self.active_section == ActiveSection::Videos,
                ActiveSection::Videos,
            ),
            item(
                Icon::Gem,
                "Gems",
                self.active_section == ActiveSection::GemsHub,
                ActiveSection::GemsHub,
            ),
        ]
        .spacing(2);

        let card = popover_card(list, 200.0, bg_popover, border_subtle, pal.is_dark);
        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::CloseMoreMenu);

        let positioned = container(card)
            .padding(Padding {
                top: 235.0,
                right: 0.0,
                bottom: 0.0,
                left: (self.sidebar_width - 36.0).max(16.0),
            })
            .width(Fill)
            .height(Fill);

        stack![backdrop, positioned].width(Fill).height(Fill).into()
    }

    /// 사이드바 대화 필터(`Y`) 클릭 시 뜨는 플로팅 필터 메뉴 (`전체`, `준비됨`, `확인 필요`, `진행 중`, `고정됨`).
    pub(crate) fn view_filter_popover_overlay(&self) -> Element<'_, Message> {
        let pal = &self.palette;
        let loc = self.locale;
        let bg_popover = pal.bg_popover;
        let bg_hover = pal.bg_hover;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let accent = pal.accent;

        let options: [(ConvFilter, Option<Icon>); 5] = [
            (ConvFilter::All, None),
            (ConvFilter::Ready, Some(Icon::Unread)),
            (ConvFilter::NeedsAttention, Some(Icon::Flag)),
            (ConvFilter::InProgress, Some(Icon::Progress)),
            (ConvFilter::PinnedOnly, Some(Icon::Pin)),
        ];

        let mut list = column![].spacing(2);
        for (filter, icon_opt) in options {
            let selected = self.conv_filter == filter;
            let mut r = row![].spacing(9).align_y(Alignment::Center);
            if let Some(ic) = icon_opt {
                let ic_col = match filter {
                    ConvFilter::Ready => accent,
                    ConvFilter::NeedsAttention => pal.warning,
                    _ => text_primary,
                };
                r = r.push(ic.view(15.0, ic_col));
            } else {
                r = r.push(space::horizontal().width(Length::Fixed(15.0)));
            }
            r = r
                .push(
                    text(t(loc, filter.label()))
                        .size(13.5)
                        .font(if selected { font_medium() } else { font_regular() })
                        .color(if selected { text_strong } else { text_primary }),
                )
                .push(space::horizontal());
            if selected {
                r = r.push(Icon::Check.view(15.0, accent));
            }

            list = list.push(
                button(r)
                    .on_press(Message::SetConvFilter(filter))
                    .width(Fill)
                    .padding([8, 11])
                    .style(move |_, status| {
                        let hovered = matches!(status, button::Status::Hovered);
                        button::Style {
                            background: if selected {
                                Some(Background::Color(accent_soft))
                            } else if hovered {
                                Some(Background::Color(bg_hover))
                            } else {
                                None
                            },
                            text_color: text_primary,
                            border: Border {
                                radius: 10.0.into(),
                                ..Border::default()
                            },
                            ..button::Style::default()
                        }
                    }),
            );
        }

        let card = popover_card(list, 190.0, bg_popover, border_subtle, pal.is_dark);
        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::CloseFilterMenu);

        let positioned = container(card)
            .padding(Padding {
                top: 420.0,
                right: 0.0,
                bottom: 0.0,
                left: (self.sidebar_width - 88.0).max(16.0),
            })
            .width(Fill)
            .height(Fill);

        stack![backdrop, positioned].width(Fill).height(Fill).into()
    }

    /// 사이드바 대화 항목 `⋮` 클릭 시 표시되는 컨텍스트 메뉴 (공유·고정·이름 변경·읽지 않음·노트북 추가·삭제).
    pub(crate) fn view_conv_menu_popover_overlay(&self) -> Element<'_, Message> {
        let Some(target_id) = &self.conv_menu_target else {
            return container(space::horizontal()).into();
        };
        let Some(conv) = self
            .conversations
            .iter()
            .find(|c| &c.conversation_id == target_id)
        else {
            return container(space::horizontal()).into();
        };

        let pal = &self.palette;
        let loc = self.locale;
        let bg_popover = pal.bg_popover;
        let bg_hover = pal.bg_hover;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_secondary = pal.text_secondary;
        let is_dark = pal.is_dark;

        let cid = conv.conversation_id.clone();
        let title = conv.title.clone();
        let pinned = conv.pinned;
        let has_unread = conv.has_unread;

        let row_btn = |icon: Icon, label: String, color: Color, msg: Message| {
            button(
                row![
                    icon.view(15.5, color),
                    text(label).size(13.5).color(color),
                    space::horizontal(),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .on_press(msg)
            .width(Fill)
            .padding([8, 12])
            .style(move |_, status| ghost_btn_style(status, bg_hover, color, 10.0))
        };

        let mut nb_row = row![
            Icon::Notebook.view(15.5, text_primary),
            text(t(loc, "노트북에 추가")).size(13.5).color(text_primary),
            space::horizontal(),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if self.conv_menu_notebook_sub {
            nb_row = nb_row.push(Icon::ChevronUp.view(14.0, text_secondary));
        }

        // 원본 앱과 동일하게 상단 제목·구분선·위험색 강조 없이 6개 항목을 일관된 톤으로 배치한다.
        let mut list = column![
            row_btn(
                Icon::Share,
                t(loc, "대화 공유").into(),
                text_primary,
                Message::ShareConversation(cid.clone(), title.clone()),
            ),
            row_btn(
                if pinned { Icon::PinFilled } else { Icon::Pin },
                t(loc, if pinned { "고정 해제" } else { "고정" }).into(),
                text_primary,
                Message::TogglePinConversation(cid.clone(), !pinned),
            ),
            row_btn(
                Icon::Edit,
                t(loc, "이름 변경").into(),
                text_primary,
                Message::StartRename(cid.clone(), title),
            ),
            row_btn(
                Icon::Unread,
                t(
                    loc,
                    if has_unread {
                        "읽음으로 표시"
                    } else {
                        "읽지 않음으로 표시"
                    },
                )
                .into(),
                text_primary,
                Message::ToggleUnreadConversation(cid.clone(), !has_unread),
            ),
            button(nb_row)
                .on_press(Message::ToggleConvMenuNotebookSub)
                .width(Fill)
                .padding([8, 12])
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 10.0)),
        ]
        .spacing(2);

        if self.conv_menu_notebook_sub {
            let mut nb_sub = column![].spacing(1).padding(Padding {
                top: 2.0,
                right: 0.0,
                bottom: 2.0,
                left: 14.0,
            });
            for nb in self.notebooks.iter().take(6) {
                nb_sub = nb_sub.push(
                    button(
                        row![
                            Icon::Notebook.view(13.0, text_secondary),
                            text(truncate_single_line(t(loc, &nb.title), 20))
                                .size(12.5)
                                .color(text_primary),
                        ]
                        .spacing(7)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::AddConvToNotebook(cid.clone(), nb.id.clone()))
                    .width(Fill)
                    .padding([6, 10])
                    .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 8.0)),
                );
            }
            if self.notebooks.is_empty() {
                nb_sub = nb_sub.push(
                    container(text(t(loc, "노트북 없음")).size(12).color(text_secondary))
                        .padding([5, 10]),
                );
            }
            list = list.push(nb_sub);
        }

        list = list.push(row_btn(
            Icon::Trash,
            t(loc, "삭제").into(),
            text_primary,
            Message::DeleteConversation(cid),
        ));

        let card_width = if self.conv_menu_notebook_sub {
            216.0
        } else {
            194.0
        };
        let shadow_alpha = if is_dark { 0.44 } else { 0.12 };
        let card = container(list)
            .width(Length::Fixed(card_width))
            .padding(6)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_popover)),
                border: Border {
                    radius: 14.0.into(),
                    width: if is_dark { 1.0 } else { 0.0 },
                    color: border_subtle,
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, shadow_alpha),
                    offset: Vector::new(0.0, 6.0),
                    blur_radius: 22.0,
                },
                ..container::Style::default()
            });

        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::CloseConvMenu);

        // 클릭한 대화 행의 세로 위치를 사이드바 스크롤 오프셋과 창 높이에 맞춰 계산한다.
        let filtered = self.filtered_conversations();
        let row_idx = filtered
            .iter()
            .position(|c| c.conversation_id == *target_id)
            .unwrap_or(0);
        let nb_visible = self.notebooks.len().min(3) as f32;
        let conv_list_start_y = 50.0 + 214.0 + (100.0 + nb_visible * 33.0) + 42.0 + 6.0;
        let row_y = conv_list_start_y + (row_idx as f32) * 34.0 - self.sidebar_scroll_y;
        let est_menu_h = if self.conv_menu_notebook_sub {
            232.0 + (self.notebooks.len().clamp(1, 6) as f32) * 30.0
        } else {
            232.0
        };
        let max_top = (self.window_height - est_menu_h - 20.0).max(56.0);
        let top = (row_y + 16.0).clamp(56.0, max_top);

        let positioned = container(card)
            .padding(Padding {
                top,
                right: 0.0,
                bottom: 0.0,
                left: (self.sidebar_width - 10.0).max(16.0),
            })
            .width(Fill)
            .height(Fill);

        stack![backdrop, positioned].width(Fill).height(Fill).into()
    }

    /// 태스크 실행 대상 원격 PC 선택 플로팅 팝오버.
    pub(crate) fn view_device_popover_overlay(&self, is_welcome_view: bool) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_popover = pal.bg_popover;
        let bg_hover = pal.bg_hover;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;
        let accent = pal.accent;
        let success = pal.success;

        let active_id = self
            .selected_target_device()
            .map(|d| d.device_id);
        let devices = self.available_remote_devices();

        let mut list = column![
            container(
                text(t(loc, "원격 실행 대상 PC"))
                    .size(12)
                    .font(font_medium())
                    .color(text_secondary),
            )
            .padding([6, 12]),
        ]
        .spacing(2);

        if devices.is_empty() {
            list = list.push(
                container(
                    text(t(loc, "등록된 원격 기기가 없습니다."))
                        .size(13)
                        .color(text_secondary),
                )
                .padding([10, 12]),
            );
        } else {
            for d in devices {
                let is_selected = active_id
                    .as_ref()
                    .is_some_and(|id| id.eq_ignore_ascii_case(&d.device_id));
                let is_local = !self.remote_status.device_id.is_empty()
                    && d.device_id
                        .eq_ignore_ascii_case(&self.remote_status.device_id);
                let is_online = if is_local {
                    self.remote_status.remote_enabled && self.remote_status.punctual_connected
                } else {
                    d.remote_enabled
                };
                let dot_c = if is_online { success } else { text_muted };
                let local_prefix = if is_local {
                    pick(loc, "이 PC · ", "This PC · ")
                } else {
                    ""
                };
                let subtitle = if is_online {
                    if loc.is_en() {
                        format!("{local_prefix}Remote connected · {} tools", d.tool_count)
                    } else {
                        format!("{local_prefix}원격 연결됨 · 도구 {}개", d.tool_count)
                    }
                } else if d.remote_enabled {
                    if loc.is_en() {
                        format!("{local_prefix}Remote idle")
                    } else {
                        format!("{local_prefix}원격 대기 중")
                    }
                } else if loc.is_en() {
                    format!("{local_prefix}Remote off")
                } else {
                    format!("{local_prefix}원격 제어 꺼짐")
                };

                let label_col = column![
                    row![
                        container(space::horizontal())
                            .width(Length::Fixed(7.0))
                            .height(Length::Fixed(7.0))
                            .style(move |_| container::Style {
                                background: Some(Background::Color(dot_c)),
                                border: Border {
                                    radius: 4.0.into(),
                                    ..Border::default()
                                },
                                ..container::Style::default()
                            }),
                        Icon::Laptop.view(14.0, text_primary),
                        text(d.device_name.clone())
                            .size(13.5)
                            .font(font_medium())
                            .color(if is_selected { text_strong } else { text_primary }),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                    text(subtitle).size(11.5).color(text_secondary),
                ]
                .spacing(2);

                let mut item_row = row![label_col, space::horizontal()]
                    .spacing(12)
                    .align_y(Alignment::Center);
                if is_selected {
                    item_row = item_row.push(Icon::Check.view(15.5, accent));
                }

                let id_clone = d.device_id.clone();
                let item_btn = button(item_row)
                    .on_press(Message::SelectDevice(id_clone))
                    .width(Fill)
                    .padding([9, 12])
                    .style(move |_, status| {
                        let hovered = matches!(status, button::Status::Hovered);
                        let bg = if is_selected {
                            Some(Background::Color(accent_soft))
                        } else if hovered {
                            Some(Background::Color(bg_hover))
                        } else {
                            None
                        };
                        button::Style {
                            background: bg,
                            text_color: text_primary,
                            border: Border {
                                radius: 12.0.into(),
                                ..Border::default()
                            },
                            ..button::Style::default()
                        }
                    });
                list = list.push(item_btn);
            }
        }

        // 하단 원격 설정 바로가기
        list = list.push(
            container(space::horizontal())
                .width(Fill)
                .height(Length::Fixed(1.0))
                .style(move |_| container::Style {
                    background: Some(Background::Color(border_subtle)),
                    ..container::Style::default()
                }),
        );
        list = list.push(
            button(
                row![
                    Icon::Settings.view(14.0, text_secondary),
                    text(t(loc, "원격 PC 및 허용 폴더 설정"))
                        .size(12.5)
                        .color(text_secondary),
                ]
                .spacing(7)
                .align_y(Alignment::Center),
            )
            .on_press(Message::OpenSettingsModal)
            .width(Fill)
            .padding([8, 12])
            .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 10.0)),
        );

        let shadow_alpha = if pal.is_dark { 0.45 } else { 0.14 };
        let card = container(list)
            .width(Length::Fixed(310.0))
            .padding(8)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_popover)),
                border: Border {
                    radius: 18.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, shadow_alpha),
                    offset: Vector::new(0.0, 8.0),
                    blur_radius: 26.0,
                },
                ..container::Style::default()
            });

        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::CloseDeviceMenu);

        let max_w = if is_welcome_view {
            WELCOME_MAX_WIDTH
        } else {
            CONTENT_MAX_WIDTH
        };
        let aligned_row = container(
            container(row![card, space::horizontal()].width(Fill))
                .max_width(max_w)
                .width(Fill),
        )
        .padding(Padding {
            top: 0.0,
            right: 24.0,
            bottom: 0.0,
            left: 36.0,
        })
        .center_x(Fill)
        .width(Fill);

        let positioned: Element<'_, Message> = if is_welcome_view {
            column![
                space::vertical().height(Length::FillPortion(6)),
                aligned_row,
                space::vertical().height(Length::FillPortion(3)),
            ]
            .width(Fill)
            .height(Fill)
            .into()
        } else {
            column![
                space::vertical().height(Fill),
                aligned_row,
                space::vertical().height(Length::Fixed(94.0)),
            ]
            .width(Fill)
            .height(Fill)
            .into()
        };

        stack![backdrop, positioned].width(Fill).height(Fill).into()
    }
}
