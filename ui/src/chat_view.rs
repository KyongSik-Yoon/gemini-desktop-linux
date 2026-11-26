use crate::glow::WelcomeGlow;
use crate::i18n::t;
use crate::icons::{Icon, sparkle_color};
use crate::markdown_view;
use crate::theme::{AppTheme, font_medium, font_regular, font_semibold, mix};
use crate::{
    ActiveSection, App, CHAT_SCROLL_ID, CONTENT_MAX_WIDTH, ChatMessage, ComposerMode, Message,
    Role, THINKING_HIGH, THINKING_LOW, WELCOME_MAX_WIDTH, chunk_elements, feature_mode_label,
    format_turn_timestamp, ghost_btn_style, is_attachment_image, local_day_number,
    truncate_single_line,
};
use iced::widget::{
    button, canvas, column, container, image, row, scrollable, space, stack, text, text_editor,
};
use iced::{
    Alignment, Background, Border, Color, ContentFit, Element, Fill, Length, Padding, Shadow,
    Vector, keyboard,
};

impl App {
    pub(crate) fn view_main(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_hover = pal.bg_hover;
        let bg_composer = pal.bg_composer;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let accent = pal.accent;
        let accent_soft = pal.accent_soft;
        let warning = pal.warning;
        let danger = pal.danger;

        // 원본과 동일한 극도로 미니멀한 메인 상단 바:
        // 사이드바가 열려 있을 때는 우측 상단에 임시 채팅 아이콘 버튼만 배치한다.
        let mut header_row = row![].spacing(8).align_y(Alignment::Center);

        if !self.sidebar_open {
            header_row = header_row
                .push(
                    button(container(Icon::SidebarToggle.view(17.0, text_secondary)).center(Fill))
                        .on_press(Message::ToggleSidebar)
                        .width(Length::Fixed(32.0))
                        .height(Length::Fixed(32.0))
                        .style(move |_, status| {
                            ghost_btn_style(status, bg_hover, text_secondary, 16.0)
                        }),
                )
                .push(sparkle_color(18.0))
                .push(
                    text("Gemini")
                        .size(17)
                        .font(font_semibold())
                        .color(text_strong),
                );
        }

        if self.active_section == ActiveSection::Chat
            && !self.active_title.is_empty()
            && !self.messages.is_empty()
        {
            header_row = header_row.push(
                text(truncate_single_line(&self.active_title, 42))
                    .size(14)
                    .font(font_medium())
                    .color(text_secondary),
            );
        }

        let temp_active = self.temporary_chat;
        let temp_fg = if temp_active { accent } else { text_secondary };
        let temp_btn: Element<'_, Message> = if temp_active {
            button(
                row![
                    Icon::TempChat.view(16.0, temp_fg),
                    text(t(loc, "임시 채팅")).size(12).font(font_medium()).color(temp_fg),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::ToggleTemporary)
            .padding([5, 11])
            .style(move |_, _| button::Style {
                background: Some(Background::Color(accent_soft)),
                text_color: temp_fg,
                border: Border {
                    radius: 16.0.into(),
                    width: 1.0,
                    color: accent,
                },
                ..button::Style::default()
            })
            .into()
        } else {
            button(container(Icon::TempChat.view(19.0, text_primary)).center(Fill))
                .on_press(Message::ToggleTemporary)
                .width(Length::Fixed(36.0))
                .height(Length::Fixed(36.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 18.0))
                .into()
        };

        header_row = header_row.push(space::horizontal()).push(temp_btn);
        let top_bar = container(header_row).padding([10, 18]).width(Fill);

        let is_welcome_view = self.active_section == ActiveSection::Chat
            && !self.loading_history
            && self.messages.is_empty();

        // 본문 영역 조립
        let mut body = column![top_bar].width(Fill).height(Fill);

        if self.file_hovering {
            body = body.push(
                container(
                    row![
                        Icon::AttachFile.view(15.0, accent),
                        text(t(loc, "여기에 파일을 놓으면 질문에 첨부됩니다 (이미지·PDF·문서·코드)"))
                            .size(13)
                            .font(font_medium())
                            .color(accent),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding([9, 18])
                .width(Fill)
                .center_x(Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(accent_soft)),
                    border: Border {
                        width: 1.0,
                        color: accent,
                        ..Border::default()
                    },
                    ..container::Style::default()
                }),
            );
        }

        if let Some(err) = &self.error_banner {
            let err_bg = Color::from_rgba(danger.r, danger.g, danger.b, 0.12);
            body = body.push(
                container(
                    row![
                        text(err).size(12.5).color(danger),
                        space::horizontal(),
                        button(text(t(loc, "닫기")).size(12))
                            .on_press(Message::DismissError)
                            .padding([4, 10])
                            .style(move |_, status| ghost_btn_style(status, bg_hover, danger, 8.0)),
                    ]
                    .align_y(Alignment::Center)
                    .spacing(12),
                )
                .padding([8, 18])
                .width(Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(err_bg)),
                    ..container::Style::default()
                }),
            );
        }

        if self.temporary_chat && !self.messages.is_empty() {
            let warn_bg = Color::from_rgba(warning.r, warning.g, warning.b, 0.10);
            body = body.push(
                container(
                    text(t(
                        loc,
                        "임시 채팅 · 이 대화는 최근 항목에 표시되지 않으며 저장되지 않습니다.",
                    ))
                        .size(12)
                        .color(text_secondary),
                )
                .padding([6, 18])
                .width(Fill)
                .center_x(Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(warn_bg)),
                    ..container::Style::default()
                }),
            );
        }

        let main_content: Element<'_, Message> = match &self.active_section {
            ActiveSection::Library => self.view_library(),
            ActiveSection::Images => self.view_images(),
            ActiveSection::Schedules => self.view_schedules(),
            ActiveSection::Student => self.view_student(),
            ActiveSection::Notebooks => self.view_notebooks(),
            ActiveSection::GemsHub => self.view_gems_hub(),
            ActiveSection::Videos => self.view_videos(),
            ActiveSection::Personalize => self.view_personalize(),
            ActiveSection::Gem(bot_id) => self.view_gem_landing(bot_id),
            ActiveSection::Chat => {
                if self.loading_history {
                    container(
                        text(t(loc, "대화 이력을 불러오는 중..."))
                            .size(14)
                            .color(text_secondary),
                    )
                    .center(Fill)
                    .into()
                } else if self.messages.is_empty() {
                    self.view_welcome()
                } else {
                    self.view_chat_messages()
                }
            }
        };

        body = body.push(main_content);

        if let Some((toast_msg, _)) = &self.toast {
            let toast_bar = container(
                container(text(toast_msg).size(12).color(text_primary))
                    .padding([7, 14])
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_composer)),
                        border: Border {
                            radius: 16.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        shadow: Shadow {
                            color: Color::from_rgba(0.0, 0.0, 0.0, 0.14),
                            offset: Vector::new(0.0, 2.0),
                            blur_radius: 10.0,
                        },
                        ..container::Style::default()
                    }),
            )
            .padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: 10.0,
                left: 0.0,
            })
            .center_x(Fill)
            .width(Fill);
            body = body.push(toast_bar);
        }

        // 환영 화면에서는 채팅(블루) / 태스크(라벤더) 모드별 부드러운 라디얼 글로우 적용
        let layered: Element<'_, Message> = if is_welcome_view {
            let glow_color = if self.composer_mode == ComposerMode::Task {
                pal.glow_task_color
            } else {
                pal.glow_color
            };
            let glow_bg = canvas(WelcomeGlow {
                cache: &self.welcome_glow_cache,
                bg: pal.bg_canvas,
                color: glow_color,
                is_dark: pal.is_dark,
            })
            .width(Fill)
            .height(Fill);
            stack![glow_bg, body].width(Fill).height(Fill).into()
        } else {
            body.into()
        };

        if self.tools_menu_open {
            stack![layered, self.view_tools_popover_overlay(is_welcome_view)]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.model_menu_open {
            stack![layered, self.view_model_popover_overlay(is_welcome_view)]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.device_menu_open {
            stack![layered, self.view_device_popover_overlay(is_welcome_view)]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.mention_query.is_some() {
            stack![layered, self.view_mention_popover_overlay(is_welcome_view)]
                .width(Fill)
                .height(Fill)
                .into()
        } else {
            layered
        }
    }

    /// 원본과 1:1 동일한 환영 화면: 중앙 제목 + 컴팩트 화이트 입력창 + [채팅 | 태스크] 토글.
    fn view_welcome(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let bg_segment = pal.bg_segment;
        let bg_composer = pal.bg_composer;
        let accent = pal.accent;

        let heading: Element<'_, Message> = if self.temporary_chat {
            column![
                row![
                    Icon::TempChat.view(24.0, accent),
                    text(t(loc, "임시 채팅"))
                        .size(26)
                        .font(font_semibold())
                        .color(text_strong),
                ]
                .spacing(9)
                .align_y(Alignment::Center),
                text(t(loc, "이 대화는 최근 항목에 표시되지 않으며 저장되지 않습니다."))
                    .size(13.5)
                    .color(text_secondary),
            ]
            .spacing(8)
            .align_x(Alignment::Center)
            .into()
        } else if let Some(bot_id) = &self.active_bot_id
            && let Some(bot) = self.find_bot(bot_id)
        {
            column![
                row![
                    Icon::Gem.view(22.0, accent),
                    text(&bot.name)
                        .size(26)
                        .font(font_semibold())
                        .color(text_strong),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text(&bot.description)
                    .size(13.5)
                    .color(text_secondary),
            ]
            .spacing(8)
            .align_x(Alignment::Center)
            .into()
        } else if self.composer_mode == ComposerMode::Task {
            text(t(loc, "목록에서 어떤 일을 처리해 드릴까요?"))
                .size(28)
                .font(font_regular())
                .color(text_strong)
                .into()
        } else {
            text(t(loc, "무엇을 도와드릴까요?"))
                .size(30)
                .font(font_regular())
                .color(text_strong)
                .into()
        };

        // 입력창 바로 아래 [ 채팅 | 태스크 ] 세그먼트 토글 (원본 앱 핵심 UI)
        let is_chat = self.composer_mode == ComposerMode::Chat;
        let seg_btn = |label: &'static str, active: bool, mode: ComposerMode| {
            button(
                text(label)
                    .size(13)
                    .font(if active { font_medium() } else { font_regular() })
                    .color(if active { text_strong } else { text_secondary }),
            )
            .on_press(Message::SetComposerMode(mode))
            .padding([6, 16])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: if active {
                        Some(Background::Color(bg_composer))
                    } else if hovered {
                        Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.28)))
                    } else {
                        None
                    },
                    text_color: if active { text_strong } else { text_secondary },
                    border: Border {
                        radius: 16.0.into(),
                        ..Border::default()
                    },
                    shadow: if active {
                        Shadow {
                            color: Color::from_rgba(0.0, 0.0, 0.0, 0.08),
                            offset: Vector::new(0.0, 1.0),
                            blur_radius: 4.0,
                        }
                    } else {
                        Shadow::default()
                    },
                    ..button::Style::default()
                }
            })
        };

        let mode_toggle = container(
            row![
                seg_btn(t(loc, "채팅"), is_chat, ComposerMode::Chat),
                seg_btn(t(loc, "태스크"), !is_chat, ComposerMode::Task),
            ]
            .spacing(2)
            .align_y(Alignment::Center),
        )
        .padding(3)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_segment)),
            border: Border {
                radius: 20.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

        container(
            column![
                space::vertical().height(Length::FillPortion(4)),
                column![
                    heading,
                    column![self.view_composer(true), mode_toggle]
                        .spacing(14)
                        .align_x(Alignment::Center)
                        .width(Fill),
                ]
                .spacing(24)
                .align_x(Alignment::Center)
                .max_width(WELCOME_MAX_WIDTH)
                .width(Fill),
                space::vertical().height(Length::FillPortion(5)),
            ]
            .padding([0, 28])
            .align_x(Alignment::Center)
            .width(Fill)
            .height(Fill),
        )
        .center_x(Fill)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn view_chat_messages(&self) -> Element<'_, Message> {
        let pal = &self.palette;
        let text_muted = pal.text_muted;
        let border_subtle = pal.border_subtle;
        let mut items: Vec<Element<'_, Message>> = Vec::with_capacity(self.messages.len() + 4);
        let mut last_header_ts: i64 = 0;

        for (idx, msg) in self.messages.iter().enumerate() {
            let ts = msg.timestamp_unix;
            if ts > 0 {
                let show_header = last_header_ts == 0
                    || (ts != last_header_ts
                        && (msg.is_scheduled_run
                            || (ts - last_header_ts).abs() >= 3600
                            || local_day_number(ts, self.local_tz_offset_secs)
                                != local_day_number(last_header_ts, self.local_tz_offset_secs)));
                if show_header {
                    let label = format_turn_timestamp(ts, self.local_tz_offset_secs, self.locale);
                    let line_left = container(space::horizontal())
                        .width(Fill)
                        .height(Length::Fixed(1.0))
                        .style(move |_| container::Style {
                            background: Some(Background::Color(border_subtle)),
                            ..container::Style::default()
                        });
                    let line_right = container(space::horizontal())
                        .width(Fill)
                        .height(Length::Fixed(1.0))
                        .style(move |_| container::Style {
                            background: Some(Background::Color(border_subtle)),
                            ..container::Style::default()
                        });
                    items.push(
                        row![
                            line_left,
                            text(label).size(11.5).color(text_muted),
                            line_right,
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center)
                        .width(Fill)
                        .into(),
                    );
                    last_header_ts = ts;
                }
            }
            let is_last = idx + 1 == self.messages.len();
            items.push(self.view_message(idx, msg, is_last));
        }

        items.push(space::vertical().height(Length::Fixed(16.0)).into());

        let scroll_fg = self.palette.text_primary;
        let chat_scroll = scrollable(
            container(
                column(items)
                    .spacing(22)
                    .padding(Padding {
                        top: 10.0,
                        right: 28.0,
                        bottom: 12.0,
                        left: 24.0,
                    })
                    .max_width(CONTENT_MAX_WIDTH)
                    .width(Fill),
            )
            .center_x(Fill)
            .width(Fill),
        )
        .id(CHAT_SCROLL_ID)
        .height(Fill)
        .width(Fill)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::default().width(5).scroller_width(5),
        ))
        .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(scroll_fg, status));

        let composer_dock = container(
            column![
                self.view_composer(false),
                text(t(
                    self.locale,
                    "Gemini는 실수를 할 수 있으므로 다시 한번 확인하세요.",
                ))
                    .size(11)
                    .color(pal.text_muted),
            ]
            .spacing(7)
            .align_x(Alignment::Center)
            .max_width(CONTENT_MAX_WIDTH)
            .width(Fill),
        )
        .padding(Padding {
            top: 8.0,
            right: 24.0,
            bottom: 12.0,
            left: 24.0,
        })
        .center_x(Fill)
        .width(Fill);

        column![chat_scroll, composer_dock]
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// 원본과 동일한 라운드 카드 입력창: `Gemini에게 물어보기`, 좌측 `+`·`화면 공유`, 우측 `모델 ⌄`·`마이크/전송`.
    fn view_composer(&self, is_welcome: bool) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let is_dark = pal.is_dark;
        let bg_composer = pal.bg_composer;
        let bg_chip = pal.bg_chip;
        let bg_hover = pal.bg_hover;
        let bg_active = pal.bg_active;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;
        let accent = pal.accent;
        let accent_soft = pal.accent_soft;
        let success = pal.success;
        let danger = pal.danger;

        let line_count = self.composer.line_count().clamp(1, 6) as f32;
        let min_h = if is_welcome { 52.0 } else { 42.0 };
        let editor_height = (line_count * 24.0 + 12.0).clamp(min_h, 160.0);

        let placeholder: String = if self.composer_mode == ComposerMode::Task {
            t(loc, "태스크를 설명하세요").to_string()
        } else {
            t(loc, "Gemini에게 물어보기").to_string()
        };

        let editor = text_editor(&self.composer)
            .placeholder(placeholder)
            .on_action(Message::EditorAction)
            .key_binding(|key_press| {
                if matches!(
                    key_press.key.as_ref(),
                    keyboard::Key::Named(keyboard::key::Named::Enter)
                ) && !key_press.modifiers.shift()
                {
                    Some(text_editor::Binding::Custom(Message::SendPrompt))
                } else {
                    text_editor::Binding::from_key_press(key_press)
                }
            })
            .size(14.5)
            .padding(Padding {
                top: 4.0,
                right: 6.0,
                bottom: 2.0,
                left: 6.0,
            })
            .height(Length::Fixed(editor_height))
            .style(move |_, _| text_editor::Style {
                background: Background::Color(Color::TRANSPARENT),
                border: Border::default(),
                placeholder: text_muted,
                value: text_primary,
                selection: bg_active,
            });

        let mut card_col = column![].spacing(6).width(Fill);

        // 첨부 파일 칩 표시
        if !self.attached_files.is_empty() {
            let chips: Vec<Element<'_, Message>> = self
                .attached_files
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let name = p
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| p.display().to_string());
                    container(
                        row![
                            Icon::AttachFile.view(12.5, text_secondary),
                            text(truncate_single_line(&name, 22))
                                .size(12)
                                .color(text_primary),
                            button(container(Icon::Close.view(11.5, text_secondary)).center(Fill))
                                .on_press(Message::RemoveAttachment(i))
                                .padding(0)
                                .width(Length::Fixed(18.0))
                                .height(Length::Fixed(18.0))
                                .style(move |_, status| {
                                    ghost_btn_style(status, bg_hover, text_secondary, 9.0)
                                }),
                        ]
                        .spacing(5)
                        .align_y(Alignment::Center),
                    )
                    .padding([4, 9])
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_chip)),
                        border: Border {
                            radius: 12.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..container::Style::default()
                    })
                    .into()
                })
                .collect();
            card_col = card_col.push(row(chips).spacing(6));
        }

        card_col = card_col.push(editor);

        // 좌측 하단: + 도구/첨부 팝오버 토글 + [화면 공유] 필 버튼
        let plus_icon = if self.tools_menu_open {
            Icon::Close.view(17.5, text_primary)
        } else {
            Icon::Plus.view(19.0, text_primary)
        };
        let attach_btn = button(container(plus_icon).center(Fill))
            .on_press(Message::ToggleToolsMenu)
            .padding(0)
            .width(Length::Fixed(34.0))
            .height(Length::Fixed(34.0))
            .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 17.0));

        let screen_label = if self.screen_capturing {
            t(loc, "캡처 중...")
        } else {
            t(loc, "화면 공유")
        };
        let screen_btn = button(
            row![
                Icon::ScreenShare.view(16.0, text_primary),
                text(screen_label)
                    .size(13)
                    .font(font_medium())
                    .color(text_primary),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .on_press(Message::StartScreenCapture)
        .padding([6, 13])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if hovered {
                    bg_hover
                } else {
                    bg_chip
                })),
                text_color: text_primary,
                border: Border {
                    radius: 16.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                ..button::Style::default()
            }
        });

        // 우측 하단: 모델·Effort 선택기 + 마이크 / 전송(입력 시 마이크 옆에 표시) / 중단(스트리밍 시)
        let selected_model = self.selected_model();
        let model_label = selected_model
            .map(|m| m.pill_label().to_string())
            .unwrap_or_else(|| t(loc, "자동 모드").into());
        let effort_label = selected_model
            .filter(|m| m.supports_thinking)
            .map(|_| match self.thinking_level {
                THINKING_LOW => t(loc, "낮음"),
                THINKING_HIGH => t(loc, "높음"),
                _ => t(loc, "보통"),
            });
        let chevron = if self.model_menu_open {
            Icon::ChevronUp.view(14.5, text_primary)
        } else {
            Icon::ChevronDown.view(14.5, text_primary)
        };
        let menu_open = self.model_menu_open;
        let mut model_row = row![
            text(model_label)
                .size(13)
                .font(font_medium())
                .color(text_primary),
        ]
        .spacing(5)
        .align_y(Alignment::Center);
        if let Some(effort) = effort_label {
            model_row = model_row.push(
                text(effort)
                    .size(13)
                    .font(font_regular())
                    .color(text_secondary),
            );
        }
        model_row = model_row.push(chevron);
        let model_btn = button(model_row)
            .on_press(Message::ToggleModelMenu)
            .padding([6, 10])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if menu_open || hovered {
                        bg_hover
                    } else {
                        Color::TRANSPARENT
                    })),
                    text_color: text_primary,
                    border: Border {
                        radius: 16.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }
            });

        let can_send = self.composer_has_text || !self.attached_files.is_empty();
        let is_recording = self.voice_recorder.is_some();

        let stop_bg = mix(bg_composer, text_primary, if is_dark { 0.12 } else { 0.08 });
        let stop_bg_hover = mix(bg_composer, text_primary, if is_dark { 0.20 } else { 0.14 });
        let send_bg = mix(bg_composer, accent, if is_dark { 0.28 } else { 0.20 });
        let send_bg_hover = mix(bg_composer, accent, if is_dark { 0.38 } else { 0.30 });

        let mut right_actions = row![].spacing(6).align_y(Alignment::Center);
        if self.is_streaming {
            // 원본 Gemini 앱과 동일하게 중립 표면 원형 + 진한 사각형 중지 버튼 사용
            let stop_btn = button(container(Icon::Stop.view(14.5, text_strong)).center(Fill))
                .on_press(Message::AbortStream)
                .padding(0)
                .width(Length::Fixed(34.0))
                .height(Length::Fixed(34.0))
                .style(move |_, status| {
                    let hovered = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: Some(Background::Color(if hovered {
                            stop_bg_hover
                        } else {
                            stop_bg
                        })),
                        text_color: text_strong,
                        border: Border {
                            radius: 17.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }
                });
            right_actions = right_actions.push(stop_btn);
        } else if is_recording {
            let rec_btn = button(
                row![
                    Icon::MicOff.view(13.0, Color::WHITE),
                    text(if loc.is_en() {
                        format!("{}s", self.recording_seconds)
                    } else {
                        format!("{}초", self.recording_seconds)
                    })
                    .size(12)
                    .font(font_medium())
                    .color(Color::WHITE),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .on_press(Message::ToggleVoiceRecording)
            .padding([6, 11])
            .style(move |_, _| button::Style {
                background: Some(Background::Color(danger)),
                text_color: Color::WHITE,
                border: Border {
                    radius: 16.0.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            });
            right_actions = right_actions.push(rec_btn);
        } else {
            // 입력 중에도 마이크 버튼을 유지하고, 전송 가능할 때 우측에 연한 파스텔 전송 버튼을 함께 배치
            let mic_btn = button(container(Icon::Mic.view(18.5, text_primary)).center(Fill))
                .on_press(Message::ToggleVoiceRecording)
                .padding(0)
                .width(Length::Fixed(34.0))
                .height(Length::Fixed(34.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 17.0));
            right_actions = right_actions.push(mic_btn);

            if can_send {
                let send_btn =
                    button(container(Icon::ArrowUp.view(16.5, text_strong)).center(Fill))
                        .on_press(Message::SendPrompt)
                        .padding(0)
                        .width(Length::Fixed(34.0))
                        .height(Length::Fixed(34.0))
                        .style(move |_, status| {
                            let hovered = matches!(status, button::Status::Hovered);
                            button::Style {
                                background: Some(Background::Color(if hovered {
                                    send_bg_hover
                                } else {
                                    send_bg
                                })),
                                text_color: text_strong,
                                border: Border {
                                    radius: 17.0.into(),
                                    ..Border::default()
                                },
                                ..button::Style::default()
                            }
                        });
                right_actions = right_actions.push(send_btn);
            }
        }

        let mut toolbar = row![attach_btn, screen_btn]
            .spacing(6)
            .align_y(Alignment::Center);

        if let Some(f_label) = feature_mode_label(self.selected_feature_mode) {
            let feature_chip = container(
                row![
                    Icon::Sparkle.view(13.0, accent),
                    text(t(loc, f_label))
                        .size(12.5)
                        .font(font_medium())
                        .color(accent),
                    button(container(Icon::Close.view(11.0, accent)).center(Fill))
                        .on_press(Message::ClearFeatureMode)
                        .padding(0)
                        .width(Length::Fixed(18.0))
                        .height(Length::Fixed(18.0))
                        .style(move |_, status| ghost_btn_style(status, bg_hover, accent, 9.0)),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .padding([4, 9])
            .style(move |_| container::Style {
                background: Some(Background::Color(accent_soft)),
                border: Border {
                    radius: 14.0.into(),
                    width: 1.0,
                    color: accent,
                },
                ..container::Style::default()
            });
            toolbar = toolbar.push(feature_chip);
        }

        if !is_welcome {
            let is_task = self.composer_mode == ComposerMode::Task;
            let next_mode = if is_task {
                ComposerMode::Chat
            } else {
                ComposerMode::Task
            };
            let task_toggle_btn = button(
                row![
                    Icon::Sparkle.view(14.0, if is_task { accent } else { text_secondary }),
                    text(t(loc, "태스크"))
                        .size(12.5)
                        .font(font_medium())
                        .color(if is_task { accent } else { text_primary }),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .on_press(Message::SetComposerMode(next_mode))
            .padding([6, 11])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if is_task {
                        accent_soft
                    } else if hovered {
                        bg_hover
                    } else {
                        bg_chip
                    })),
                    text_color: if is_task { accent } else { text_primary },
                    border: Border {
                        radius: 16.0.into(),
                        width: 1.0,
                        color: if is_task { accent } else { border_subtle },
                    },
                    ..button::Style::default()
                }
            });
            toolbar = toolbar.push(task_toggle_btn);
        }

        if self.composer_mode == ComposerMode::Task {
            let selected_dev = self.selected_target_device();
            let (dev_label, dev_online) = if let Some(dev) = &selected_dev {
                let is_local = !self.remote_status.device_id.is_empty()
                    && dev
                        .device_id
                        .eq_ignore_ascii_case(&self.remote_status.device_id);
                let online = if is_local {
                    self.remote_status.remote_enabled && self.remote_status.punctual_connected
                } else {
                    dev.remote_enabled
                };
                (truncate_single_line(&dev.device_name, 16), online)
            } else {
                (t(loc, "원격 PC 선택").to_string(), false)
            };
            let dot_color = if dev_online { success } else { text_muted };
            let dev_dot = container(space::horizontal())
                .width(Length::Fixed(7.0))
                .height(Length::Fixed(7.0))
                .style(move |_| container::Style {
                    background: Some(Background::Color(dot_color)),
                    border: Border {
                        radius: 4.0.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                });
            let dev_chevron = if self.device_menu_open {
                Icon::ChevronUp.view(13.5, text_primary)
            } else {
                Icon::ChevronDown.view(13.5, text_primary)
            };
            let dev_open = self.device_menu_open;
            let device_btn = button(
                row![
                    dev_dot,
                    Icon::Laptop.view(14.5, text_primary),
                    text(dev_label)
                        .size(12.5)
                        .font(font_medium())
                        .color(text_primary),
                    dev_chevron,
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::ToggleDeviceMenu)
            .padding([6, 11])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if dev_open || hovered {
                        bg_hover
                    } else {
                        bg_chip
                    })),
                    text_color: text_primary,
                    border: Border {
                        radius: 16.0.into(),
                        width: 1.0,
                        color: if dev_open { accent } else { border_subtle },
                    },
                    ..button::Style::default()
                }
            });
            toolbar = toolbar.push(device_btn);
        }

        toolbar = toolbar
            .push(space::horizontal())
            .push(model_btn)
            .push(right_actions);

        card_col = card_col.push(toolbar);

        let shadow_alpha = if pal.is_dark { 0.34 } else { 0.065 };
        container(card_col)
            .padding(Padding {
                top: 15.0,
                right: 16.0,
                bottom: 12.0,
                left: 18.0,
            })
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_composer)),
                border: Border {
                    radius: 26.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, shadow_alpha),
                    offset: Vector::new(0.0, 4.0),
                    blur_radius: 20.0,
                },
                ..container::Style::default()
            })
            .into()
    }

    pub(crate) fn wrap_section_page<'a>(
        &'a self,
        content: iced::widget::Column<'a, Message>,
    ) -> Element<'a, Message> {
        let scroll_fg = self.palette.text_primary;
        scrollable(
            container(
                content
                    .padding(Padding {
                        top: 8.0,
                        right: 28.0,
                        bottom: 28.0,
                        left: 28.0,
                    })
                    .max_width(CONTENT_MAX_WIDTH)
                    .width(Fill),
            )
            .center_x(Fill)
            .width(Fill),
        )
        .height(Fill)
        .width(Fill)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::default().width(5).scroller_width(5),
        ))
        .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(scroll_fg, status))
        .into()
    }

    fn view_message<'a>(
        &'a self,
        idx: usize,
        msg: &'a ChatMessage,
        is_last: bool,
    ) -> Element<'a, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_user = pal.bg_user_bubble;
        let bg_hover = pal.bg_hover;
        let bg_thought = pal.bg_thought;
        let bg_chip = pal.bg_chip;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;

        match msg.role {
            Role::User => {
                let mut col = column![].spacing(6).align_x(Alignment::End);

                if !msg.attachment_items.is_empty() {
                    let mut thumb_items: Vec<Element<'_, Message>> = Vec::new();
                    let mut chip_items: Vec<Element<'_, Message>> = Vec::new();
                    for att in &msg.attachment_items {
                        let key = markdown_view::turn_attachment_cache_key(&att.url);
                        if is_attachment_image(att)
                            && let Some(handle) = self.library_image_cache.get(&key)
                        {
                            let thumb_img = image(handle.clone())
                                .content_fit(ContentFit::Cover)
                                .width(Length::Fixed(88.0))
                                .height(Length::Fixed(88.0));
                            let thumb_card = container(thumb_img)
                                .clip(true)
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(bg_chip)),
                                    border: Border {
                                        radius: 12.0.into(),
                                        width: 1.0,
                                        color: border_subtle,
                                    },
                                    ..container::Style::default()
                                });
                            if !att.url.is_empty() {
                                thumb_items.push(
                                    button(thumb_card)
                                        .on_press(Message::OpenExternalUrl(att.url.clone()))
                                        .padding(0)
                                        .style(move |_, status| {
                                            ghost_btn_style(status, bg_hover, text_primary, 12.0)
                                        })
                                        .into(),
                                );
                            } else {
                                thumb_items.push(thumb_card.into());
                            }
                        } else {
                            let label = if !att.name.is_empty() {
                                att.name.as_str()
                            } else if is_attachment_image(att) {
                                t(loc, "이미지")
                            } else {
                                t(loc, "첨부 파일")
                            };
                            let icon = if is_attachment_image(att) {
                                Icon::Image.view(12.0, text_secondary)
                            } else {
                                Icon::AttachFile.view(12.0, text_secondary)
                            };
                            let chip = container(
                                row![icon, text(label).size(12).color(text_secondary)]
                                    .spacing(5)
                                    .align_y(Alignment::Center),
                            )
                            .padding([4, 10])
                            .style(move |_| container::Style {
                                background: Some(Background::Color(bg_chip)),
                                border: Border {
                                    radius: 12.0.into(),
                                    width: 1.0,
                                    color: border_subtle,
                                },
                                ..container::Style::default()
                            });
                            if !att.url.is_empty() {
                                chip_items.push(
                                    button(chip)
                                        .on_press(Message::OpenExternalUrl(att.url.clone()))
                                        .padding(0)
                                        .style(move |_, status| {
                                            ghost_btn_style(status, bg_hover, text_secondary, 12.0)
                                        })
                                        .into(),
                                );
                            } else {
                                chip_items.push(chip.into());
                            }
                        }
                    }
                    if !thumb_items.is_empty() {
                        for chunk in chunk_elements(thumb_items, 3) {
                            col = col.push(row(chunk).spacing(8));
                        }
                    }
                    if !chip_items.is_empty() {
                        for chunk in chunk_elements(chip_items, 3) {
                            col = col.push(row(chunk).spacing(6));
                        }
                    }
                } else if !msg.attachments.is_empty() {
                    let chips: Vec<Element<'_, Message>> = msg
                        .attachments
                        .iter()
                        .map(|name| {
                            container(
                                row![
                                    Icon::AttachFile.view(12.0, text_secondary),
                                    text(name).size(12).color(text_secondary),
                                ]
                                .spacing(5)
                                .align_y(Alignment::Center),
                            )
                            .padding([4, 10])
                            .style(move |_| container::Style {
                                background: Some(Background::Color(bg_chip)),
                                border: Border {
                                    radius: 12.0.into(),
                                    width: 1.0,
                                    color: border_subtle,
                                },
                                ..container::Style::default()
                            })
                            .into()
                        })
                        .collect();
                    col = col.push(row(chips).spacing(6));
                }

                if !msg.raw_text.is_empty() {
                    let bubble = container(
                        text(&msg.raw_text)
                            .size(15)
                            .line_height(text::LineHeight::Relative(1.58))
                            .color(text_primary)
                            .wrapping(text::Wrapping::WordOrGlyph),
                    )
                    .padding([12, 18])
                    .max_width(580.0)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_user)),
                        border: Border {
                            radius: iced::border::Radius {
                                top_left: 20.0,
                                top_right: 6.0,
                                bottom_right: 20.0,
                                bottom_left: 20.0,
                            },
                            ..Border::default()
                        },
                        ..container::Style::default()
                    });

                    let copy_btn = button(
                        row![
                            Icon::Copy.view(12.0, text_muted),
                            text(t(loc, "복사")).size(11).color(text_muted),
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::CopyChatItem(idx))
                    .padding([3, 8])
                    .style(move |_, status| ghost_btn_style(status, bg_hover, text_muted, 8.0));

                    col = col.push(bubble).push(copy_btn);
                }
                row![space::horizontal(), col].width(Fill).into()
            }
            Role::Model => {
                let is_streaming_here = self.is_streaming && is_last;
                let accent = pal.accent;
                let accent_soft = pal.accent_soft;
                let success = pal.success;
                let warning = pal.warning;
                let mut content = column![].spacing(10).width(Fill);

                if msg.is_scheduled_run {
                    let sched_text = if msg.scheduled_title.trim().is_empty() {
                        t(loc, "예약 실행됨").to_string()
                    } else {
                        format!(
                            "{} · {}",
                            t(loc, "예약 실행됨"),
                            msg.scheduled_title.trim()
                        )
                    };
                    let sched_banner = container(
                        row![
                            Icon::Clock.view(13.0, accent),
                            text(sched_text)
                                .size(12)
                                .font(font_medium())
                                .color(accent),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([5, 11])
                    .style(move |_| container::Style {
                        background: Some(Background::Color(accent_soft)),
                        border: Border {
                            radius: 12.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..container::Style::default()
                    });
                    content = content.push(sched_banner);
                }

                // 일반 완료 응답은 원본 앱처럼 군더더기 헤더 없이 본문부터 바로 시작하고,
                // 스트리밍 중·원격 기기 실행·사고 과정 토글이 있을 때만 상단 헤더를 표시한다.
                let show_header = is_streaming_here
                    || !msg.target_device_name.is_empty()
                    || !msg.thoughts.is_empty();
                if show_header {
                    let mut header = row![sparkle_color(17.0)]
                        .spacing(8)
                        .align_y(Alignment::Center);

                    if !msg.target_device_name.is_empty() {
                        header = header.push(
                            container(
                                row![
                                    Icon::Laptop.view(12.0, accent),
                                    text(&msg.target_device_name)
                                        .size(11.5)
                                        .font(font_medium())
                                        .color(accent),
                                ]
                                .spacing(4)
                                .align_y(Alignment::Center),
                            )
                            .padding([2, 8])
                            .style(move |_| container::Style {
                                background: Some(Background::Color(accent_soft)),
                                border: Border {
                                    radius: 10.0.into(),
                                    ..Border::default()
                                },
                                ..container::Style::default()
                            }),
                        );
                    }

                    if !msg.thoughts.is_empty() {
                        let toggle_label = if msg.thoughts_expanded {
                            t(loc, "사고 과정 숨기기")
                        } else if is_streaming_here && msg.raw_text.is_empty() {
                            t(loc, "생각하는 중...")
                        } else {
                            t(loc, "사고 과정 표시")
                        };
                        let toggle_icon = if msg.thoughts_expanded {
                            Icon::ChevronUp.view(14.0, text_secondary)
                        } else {
                            Icon::ChevronDown.view(14.0, text_secondary)
                        };
                        header = header.push(
                            button(
                                row![
                                    toggle_icon,
                                    text(toggle_label).size(12).color(text_secondary),
                                ]
                                .spacing(4)
                                .align_y(Alignment::Center),
                            )
                            .on_press(Message::ToggleMessageThoughts(idx))
                            .padding([4, 10])
                            .style(move |_, status| {
                                let hovered = matches!(status, button::Status::Hovered);
                                button::Style {
                                    background: Some(Background::Color(if hovered {
                                        bg_hover
                                    } else {
                                        bg_thought
                                    })),
                                    text_color: text_secondary,
                                    border: Border {
                                        radius: 12.0.into(),
                                        width: 1.0,
                                        color: border_subtle,
                                    },
                                    ..button::Style::default()
                                }
                            }),
                        );
                    }
                    content = content.push(header);
                }

                // 원격 PC·에이전트 도구 실행 및 진행 상태 카드
                // 완료된 일반 웹 검색(Google Search)은 원본 앱과 동일하게 도구 배지를 숨긴다.
                let live_tool_running =
                    is_streaming_here && !self.remote_status.current_tool.is_empty();
                let visible_tools: Vec<&crate::ipc::RemoteToolActivity> = if is_streaming_here {
                    msg.tool_activities.iter().collect()
                } else {
                    msg.tool_activities
                        .iter()
                        .filter(|act| !is_passive_web_search_activity(act))
                        .collect()
                };
                let show_status_text = !msg.status_text.is_empty()
                    && (is_streaming_here || !is_passive_web_search_status(&msg.status_text));
                let has_tools = live_tool_running || !visible_tools.is_empty() || show_status_text;
                if has_tools {
                    let show_tools_expanded = live_tool_running || msg.tools_expanded;
                    let summary_label: String = if !visible_tools.is_empty() {
                        if loc.is_en() {
                            let n = visible_tools.len();
                            if n == 1 {
                                "Used 1 tool".into()
                            } else {
                                format!("Used {n} tools")
                            }
                        } else {
                            format!("도구 {}건 사용", visible_tools.len())
                        }
                    } else if show_status_text {
                        truncate_single_line(msg.status_text.trim(), 52)
                    } else {
                        t(loc, "도구 활동 표시").to_string()
                    };

                    if !show_tools_expanded {
                        let toggle_row = button(
                            row![
                                Icon::Check.view(13.0, success),
                                text(summary_label)
                                    .size(12.0)
                                    .font(font_medium())
                                    .color(text_secondary),
                                Icon::ChevronDown.view(13.5, text_muted),
                            ]
                            .spacing(6)
                            .align_y(Alignment::Center),
                        )
                        .on_press(Message::ToggleMessageTools(idx))
                        .padding([5, 11])
                        .style(move |_, status| {
                            let hovered = matches!(status, button::Status::Hovered);
                            button::Style {
                                background: Some(Background::Color(if hovered {
                                    bg_hover
                                } else {
                                    bg_thought
                                })),
                                text_color: text_secondary,
                                border: Border {
                                    radius: 11.0.into(),
                                    width: 1.0,
                                    color: border_subtle,
                                },
                                ..button::Style::default()
                            }
                        });
                        content = content.push(toggle_row);
                    } else {
                        let mut activity_col = column![].spacing(6).width(Fill);
                        if !live_tool_running {
                            let collapse_btn = button(
                                row![
                                    Icon::Check.view(13.0, success),
                                    text(summary_label)
                                        .size(12.0)
                                        .font(font_medium())
                                        .color(text_secondary),
                                    space::horizontal(),
                                    text(t(loc, "도구 활동 숨기기")).size(11.5).color(text_muted),
                                    Icon::ChevronUp.view(13.5, text_muted),
                                ]
                                .spacing(6)
                                .align_y(Alignment::Center)
                                .width(Fill),
                            )
                            .on_press(Message::ToggleMessageTools(idx))
                            .padding([2, 4])
                            .width(Fill)
                            .style(move |_, status| {
                                ghost_btn_style(status, bg_hover, text_secondary, 8.0)
                            });
                            activity_col = activity_col.push(collapse_btn);
                        }
                        if live_tool_running {
                            let args_part = if !self.remote_status.current_tool_args.is_empty() {
                                format!(
                                    " ({})",
                                    truncate_single_line(&self.remote_status.current_tool_args, 46)
                                )
                            } else {
                                String::new()
                            };
                            activity_col = activity_col.push(
                                row![
                                    container(space::horizontal())
                                        .width(Length::Fixed(7.0))
                                        .height(Length::Fixed(7.0))
                                        .style(move |_| container::Style {
                                            background: Some(Background::Color(accent)),
                                            border: Border {
                                                radius: 4.0.into(),
                                                ..Border::default()
                                            },
                                            ..container::Style::default()
                                        }),
                                    text(if loc.is_en() {
                                        format!(
                                            "Running: {}{}",
                                            self.remote_status.current_tool, args_part
                                        )
                                    } else {
                                        format!(
                                            "실행 중: {}{}",
                                            self.remote_status.current_tool, args_part
                                        )
                                    })
                                    .size(12.5)
                                    .font(font_medium())
                                    .color(accent),
                                ]
                                .spacing(7)
                                .align_y(Alignment::Center),
                            );
                        }
                        for act in &visible_tools {
                            let title = if !act.name.is_empty() {
                                act.name.as_str()
                            } else {
                                t(loc, "원격 도구 실행")
                            };
                            let detail = if !act.explanation.is_empty() {
                                act.explanation.as_str()
                            } else if !act.summary.is_empty() {
                                act.summary.as_str()
                            } else {
                                act.action.as_str()
                            };
                            let mut act_row = row![
                                Icon::Check.view(13.0, success),
                                text(title)
                                    .size(12.5)
                                    .font(font_medium())
                                    .color(text_primary),
                            ]
                            .spacing(6)
                            .align_y(Alignment::Center);
                            if !detail.is_empty() {
                                act_row = act_row.push(
                                    text(truncate_single_line(detail, 58))
                                        .size(12.0)
                                        .color(text_secondary),
                                );
                            }
                            activity_col = activity_col.push(act_row);
                        }
                        if show_status_text {
                            activity_col = activity_col.push(
                                text(msg.status_text.trim())
                                    .size(12.5)
                                    .color(text_secondary)
                                    .wrapping(text::Wrapping::WordOrGlyph),
                            );
                        }
                        let activity_card = container(activity_col)
                            .padding([10, 14])
                            .width(Fill)
                            .style(move |_| container::Style {
                                background: Some(Background::Color(bg_thought)),
                                border: Border {
                                    radius: 12.0.into(),
                                    width: 1.0,
                                    color: border_subtle,
                                },
                                ..container::Style::default()
                            });
                        content = content.push(activity_card);
                    }
                }

                if msg.thoughts_expanded && !msg.thoughts.is_empty() {
                    let thought_box = container(
                        text(&msg.thoughts)
                            .size(13)
                            .line_height(text::LineHeight::Relative(1.58))
                            .color(text_secondary)
                            .wrapping(text::Wrapping::WordOrGlyph),
                    )
                    .padding([12, 16])
                    .width(Fill)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_thought)),
                        border: Border {
                            radius: 12.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..container::Style::default()
                    });
                    content = content.push(thought_box);
                }

                let has_body = !msg.raw_text.is_empty()
                    || !msg.generated_images.is_empty()
                    || !msg.artifacts.is_empty()
                    || msg.follow_up.is_some()
                    || msg.elicitations.is_some();

                if !has_body && is_streaming_here {
                    let waiting_msg = if live_tool_running || !visible_tools.is_empty() {
                        t(loc, "원격 PC 실행 결과를 정리하는 중...")
                    } else {
                        t(loc, "생각하는 중...")
                    };
                    content = content.push(
                        text(waiting_msg)
                            .size(14)
                            .color(text_secondary),
                    );
                } else if has_body {
                    if !msg.raw_text.is_empty()
                        || !msg.generated_images.is_empty()
                        || !msg.artifacts.is_empty()
                    {
                        let md_view = markdown_view::view(
                            &msg.markdown,
                            self.palette.markdown_settings(),
                            &self.palette,
                            loc,
                            markdown_view::RenderContext {
                                message_idx: idx,
                                copy_target: markdown_view::CopyTarget::Message(idx),
                                generated_images: &msg.generated_images,
                                artifacts: &msg.artifacts,
                                image_cache: &self.library_image_cache,
                                failed_images: &self.failed_image_keys,
                            },
                        );
                        content = content.push(md_view);
                    }

                    if let Some(fu) = &msg.follow_up {
                        content = content.push(
                            container(
                                text(&fu.label)
                                    .size(15.0)
                                    .line_height(text::LineHeight::Relative(1.62))
                                    .color(text_primary)
                                    .wrapping(text::Wrapping::WordOrGlyph),
                            )
                            .padding(Padding {
                                top: 6.0,
                                right: 0.0,
                                bottom: 0.0,
                                left: 0.0,
                            }),
                        );
                        if !is_streaming_here {
                            let yes_btn = button(
                                row![
                                    Icon::CornerDownRight.view(14.0, text_secondary),
                                    text(t(loc, "예"))
                                        .size(13.5)
                                        .font(font_medium())
                                        .color(text_primary),
                                ]
                                .spacing(6)
                                .align_y(Alignment::Center),
                            )
                            .on_press(Message::SendFollowUp(idx))
                            .padding([8, 16])
                            .style(move |_, status| {
                                let hovered = matches!(status, button::Status::Hovered);
                                button::Style {
                                    background: Some(Background::Color(if hovered {
                                        bg_hover
                                    } else {
                                        bg_chip
                                    })),
                                    text_color: text_primary,
                                    border: Border {
                                        radius: 18.0.into(),
                                        ..Border::default()
                                    },
                                    ..button::Style::default()
                                }
                            });
                            content = content.push(
                                row![space::horizontal(), yes_btn]
                                    .width(Fill)
                                    .align_y(Alignment::Center),
                            );
                        }
                    }

                    // 후속 탐색 제안 카드 그룹 (<ElicitationsGroup>) — 원본 앱과 동일하게 본문/FollowUp 바로 아래에 배치
                    if !is_streaming_here
                        && let Some(group) = &msg.elicitations
                        && !group.items.is_empty()
                    {
                        let mut el_col = column![].spacing(12).width(Fill);
                        if !group.message.is_empty() {
                            el_col = el_col.push(
                                text(&group.message)
                                    .size(15.0)
                                    .line_height(text::LineHeight::Relative(1.5))
                                    .color(text_primary)
                                    .wrapping(text::Wrapping::WordOrGlyph),
                            );
                        }
                        for (chunk_idx, chunk) in group.items.chunks(3).enumerate() {
                            let base_idx = chunk_idx * 3;
                            let mut card_row = row![].spacing(12).width(Fill);
                            for (offset, item) in chunk.iter().enumerate() {
                                let item_idx = base_idx + offset;
                                let card_btn = button(
                                    column![
                                        text(&item.label)
                                            .size(13.5)
                                            .line_height(text::LineHeight::Relative(1.45))
                                            .font(font_medium())
                                            .color(text_primary)
                                            .wrapping(text::Wrapping::WordOrGlyph),
                                        space::vertical(),
                                        Icon::CornerDownRight.view(14.5, text_secondary),
                                    ]
                                    .width(Fill)
                                    .height(Fill),
                                )
                                .on_press(Message::SendElicitation(idx, item_idx))
                                .padding([14, 15])
                                .width(Length::FillPortion(1))
                                .height(Length::Fixed(104.0))
                                .style(move |_, status| {
                                    let hovered = matches!(status, button::Status::Hovered);
                                    button::Style {
                                        background: Some(Background::Color(if hovered {
                                            bg_hover
                                        } else {
                                            bg_chip
                                        })),
                                        text_color: text_primary,
                                        border: Border {
                                            radius: 16.0.into(),
                                            ..Border::default()
                                        },
                                        ..button::Style::default()
                                    }
                                });
                                card_row = card_row.push(card_btn);
                            }
                            if chunk.len() < 3 {
                                for _ in chunk.len()..3 {
                                    card_row = card_row.push(
                                        space::horizontal().width(Length::FillPortion(1)),
                                    );
                                }
                            }
                            el_col = el_col.push(card_row);
                        }
                        content = content.push(
                            container(el_col).padding(Padding {
                                top: 12.0,
                                right: 0.0,
                                bottom: 0.0,
                                left: 0.0,
                            }),
                        );
                    }
                }

                if msg.aborted && !is_streaming_here {
                    let aborted_badge = container(
                        row![
                            Icon::Stop.view(12.5, warning),
                            text(t(loc, "응답 생성이 중단되었습니다"))
                                .size(12.0)
                                .font(font_medium())
                                .color(warning),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([5, 11])
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_thought)),
                        border: Border {
                            radius: 10.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..container::Style::default()
                    });
                    content = content.push(aborted_badge);
                }

                if !is_streaming_here && (has_body || msg.aborted) {
                    let fb = self.message_feedback.get(&idx).copied();
                    let up_active = fb == Some(true);
                    let down_active = fb == Some(false);
                    let mut actions = row![
                        button(
                            container(
                                Icon::ThumbsUp
                                    .view(14.5, if up_active { accent } else { text_secondary })
                            )
                            .center(Fill),
                        )
                        .on_press(Message::SendFeedback(idx, true))
                        .width(Length::Fixed(28.0))
                        .height(Length::Fixed(28.0))
                        .style(move |_, status| {
                            ghost_btn_style(
                                status,
                                bg_hover,
                                if up_active { accent } else { text_secondary },
                                14.0,
                            )
                        }),
                        button(
                            container(
                                Icon::ThumbsDown
                                    .view(14.5, if down_active { accent } else { text_secondary })
                            )
                            .center(Fill),
                        )
                        .on_press(Message::SendFeedback(idx, false))
                        .width(Length::Fixed(28.0))
                        .height(Length::Fixed(28.0))
                        .style(move |_, status| {
                            ghost_btn_style(
                                status,
                                bg_hover,
                                if down_active { accent } else { text_secondary },
                                14.0,
                            )
                        }),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center)
                    .width(Fill);

                    if !msg.raw_text.is_empty() {
                        actions = actions.push(
                            button(
                                row![
                                    Icon::Copy.view(13.0, text_secondary),
                                    text(t(loc, "복사")).size(12).color(text_secondary),
                                ]
                                .spacing(5)
                                .align_y(Alignment::Center),
                            )
                            .on_press(Message::CopyChatItem(idx))
                            .padding([4, 10])
                            .style(move |_, status| {
                                ghost_btn_style(status, bg_hover, text_secondary, 8.0)
                            }),
                        );
                    }

                    if let Some(cid) = &self.active_conv_id {
                        actions = actions.push(
                            button(
                                row![
                                    Icon::Share.view(13.0, text_secondary),
                                    text(t(loc, "공유")).size(12).color(text_secondary),
                                ]
                                .spacing(5)
                                .align_y(Alignment::Center),
                            )
                            .on_press(Message::ShareConversation(
                                cid.clone(),
                                self.active_title.clone(),
                            ))
                            .padding([4, 10])
                            .style(move |_, status| {
                                ghost_btn_style(status, bg_hover, text_secondary, 8.0)
                            }),
                        );
                    }

                    if is_last {
                        actions = actions.push(
                            button(
                                row![
                                    Icon::Refresh.view(13.0, text_secondary),
                                    text(t(loc, "다시 생성")).size(12).color(text_secondary),
                                ]
                                .spacing(5)
                                .align_y(Alignment::Center),
                            )
                            .on_press(Message::RegenerateLast)
                            .padding([4, 10])
                            .style(move |_, status| {
                                ghost_btn_style(status, bg_hover, text_secondary, 8.0)
                            }),
                        );
                    }

                    if !msg.model_name.is_empty() {
                        actions = actions.push(space::horizontal()).push(
                            text(&msg.model_name)
                                .size(11.5)
                                .color(text_muted),
                        );
                    }
                    content = content.push(actions);
                }

                container(content).padding(4).width(Fill).into()
            }
        }
    }
}

/// 일반 백그라운드 웹 검색(Google Search) 완료 활동 여부 판별.
fn is_passive_web_search_activity(act: &crate::ipc::RemoteToolActivity) -> bool {
    act.action.is_empty()
        && (act.id.eq_ignore_ascii_case("google")
            || act.name.eq_ignore_ascii_case("google search"))
}

/// 일반 백그라운드 웹 검색 상태 문구 여부 판별.
fn is_passive_web_search_status(status: &str) -> bool {
    let s = status.trim();
    s.eq_ignore_ascii_case("searching the web")
        || s.eq_ignore_ascii_case("google search")
        || s == "웹 검색 중"
}
