use crate::i18n::{LanguagePref, pick, t};
use crate::icons::Icon;
use crate::ipc::{ConversationSummary, RegisteredDevice};
use crate::markdown_view;
use crate::theme::{AppTheme, ThemeMode, font_medium, font_regular, font_semibold};
use crate::{
    App, CanvasPreviewState, Message, SEARCH_INPUT_ID, format_reset_time, ghost_btn_style,
    is_html_artifact, truncate_single_line,
};
use iced::widget::{
    button, column, container, mouse_area, row, scrollable, space, stack, text, text_input,
};
use iced::{
    Alignment, Background, Border, Color, Element, Fill, Length, Padding, Shadow, Vector,
};

impl App {
    /// 검색 모달 (`Ctrl+K` / 사이드바 돋보기): 로컬 실시간 필터 + 서버 전문 검색(`SearchConversationHistory`).
    pub(crate) fn view_search_modal(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_popover = pal.bg_popover;
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

        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.28))),
                    ..container::Style::default()
                }),
        )
        .on_press(Message::CloseSearchModal);

        let search_in = text_input(
            t(loc, "대화 제목이나 본문 키워드 입력 후 Enter..."),
            &self.search_query,
        )
            .id(SEARCH_INPUT_ID)
            .on_input(Message::SearchQueryChanged)
            .on_submit(Message::SubmitServerSearch)
            .size(14.5)
            .padding([9, 10])
            .style(move |_, _| text_input::Style {
                background: Background::Color(Color::TRANSPARENT),
                border: Border::default(),
                icon: text_secondary,
                placeholder: text_muted,
                value: text_primary,
                selection: bg_active,
            });

        let top_bar = container(
            row![
                Icon::Search.view(17.0, text_secondary),
                search_in,
                button(
                    text(if self.search_loading {
                        t(loc, "검색 중...")
                    } else {
                        t(loc, "본문 검색")
                    })
                    .size(12.5)
                    .font(font_medium())
                    .color(accent),
                )
                .on_press(Message::SubmitServerSearch)
                .padding([6, 11])
                .style(move |_, status| ghost_btn_style(status, accent_soft, accent, 12.0)),
                button(container(Icon::Close.view(15.0, text_secondary)).center(Fill))
                    .on_press(Message::CloseSearchModal)
                    .padding(0)
                    .width(Length::Fixed(28.0))
                    .height(Length::Fixed(28.0))
                    .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 14.0)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([8, 12])
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_chip)),
            border: Border {
                radius: 14.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

        let mut results = column![].spacing(4);

        if !self.search_hits.is_empty() {
            results = results.push(
                container(
                    text(t(loc, "서버 전체 본문 검색 결과"))
                        .size(11.5)
                        .font(font_semibold())
                        .color(accent),
                )
                .padding([6, 8]),
            );
            for hit in self.search_hits.iter().take(15) {
                let mut col = column![
                    text(if hit.title.is_empty() {
                        hit.conversation_id.as_str()
                    } else {
                        hit.title.as_str()
                    })
                    .size(13.5)
                    .font(font_medium())
                    .color(text_strong),
                ]
                .spacing(2);
                if !hit.snippet.is_empty() {
                    col = col.push(
                        text(truncate_single_line(&hit.snippet, 72))
                            .size(12)
                            .color(text_secondary),
                    );
                }
                results = results.push(
                    button(col)
                        .on_press(Message::SelectSearchHit(
                            hit.conversation_id.clone(),
                            hit.title.clone(),
                            hit.request_id.clone(),
                        ))
                        .width(Fill)
                        .padding([9, 12])
                        .style(move |_, status| {
                            ghost_btn_style(status, bg_hover, text_primary, 12.0)
                        }),
                );
            }
        }

        let q_lower = self.search_query.trim().to_lowercase();
        let local_matches: Vec<&ConversationSummary> = self
            .conversations
            .iter()
            .filter(|c| {
                q_lower.is_empty()
                    || c.title.to_lowercase().contains(&q_lower)
                    || c.conversation_id.to_lowercase().contains(&q_lower)
            })
            .take(15)
            .collect();

        results = results.push(
            container(
                text(if q_lower.is_empty() {
                    t(loc, "최근 대화")
                } else {
                    t(loc, "로컬 목록 일치 항목")
                })
                .size(11.5)
                .font(font_semibold())
                .color(text_muted),
            )
            .padding([6, 8]),
        );

        for c in local_matches {
            results = results.push(
                button(
                    row![
                        text(truncate_single_line(&c.title, 54))
                            .size(13.5)
                            .color(text_primary),
                        space::horizontal(),
                        if c.pinned {
                            Element::from(Icon::Pin.view(12.0, text_muted))
                        } else if c.is_agent_chat {
                            Element::from(text(t(loc, "태스크")).size(11).color(accent))
                        } else {
                            Element::from(space::horizontal().width(Length::Fixed(1.0)))
                        },
                    ]
                    .align_y(Alignment::Center),
                )
                .on_press(Message::SelectConversation(
                    c.conversation_id.clone(),
                    c.title.clone(),
                ))
                .width(Fill)
                .padding([8, 12])
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 12.0)),
            );
        }

        let scroll = scrollable(results)
            .height(Length::Fixed(380.0))
            .width(Fill)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::default().width(4).scroller_width(4),
            ))
            .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(text_primary, status));

        let card = container(column![top_bar, scroll].spacing(10))
            .width(Length::Fixed(560.0))
            .padding(14)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_popover)),
                border: Border {
                    radius: 20.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.24),
                    offset: Vector::new(0.0, 12.0),
                    blur_radius: 36.0,
                },
                ..container::Style::default()
            });

        stack![backdrop, container(card).center(Fill)]
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// Canvas 아티팩트 프리뷰 모달 (마크다운 문서는 리치 렌더링, 코드는 고정폭 프리뷰).
    pub(crate) fn view_canvas_preview_modal<'a>(
        &'a self,
        preview: &'a CanvasPreviewState,
    ) -> Element<'a, Message> {
        let msg_idx = preview.msg_idx;
        let art_idx = preview.art_idx;
        let art = &preview.artifact;
        let is_md_doc = preview.parsed_markdown.is_some();
        let loc = self.locale;
        let pal = &self.palette;
        let bg_popover = pal.bg_popover;
        let bg_canvas = pal.bg_canvas;
        let bg_code = pal.bg_code;
        let bg_chip = pal.bg_chip;
        let bg_hover = pal.bg_hover;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let accent = pal.accent;
        let accent_soft = pal.accent_soft;

        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.35))),
                    ..container::Style::default()
                }),
        )
        .on_press(Message::CloseCanvasPreview);

        let title_str = if art.title.trim().is_empty() {
            t(loc, "Canvas 문서")
        } else {
            art.title.trim()
        };

        let mut header = row![
            Icon::Canvas.view(18.0, accent),
            text(title_str)
                .size(16)
                .font(font_semibold())
                .color(text_strong),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let badge_label = if !art.language.is_empty() {
            art.language.to_uppercase()
        } else if is_md_doc {
            "MARKDOWN".to_string()
        } else {
            String::new()
        };
        if !badge_label.is_empty() {
            header = header.push(
                container(
                    text(badge_label)
                        .size(11)
                        .font(font_medium())
                        .color(text_secondary),
                )
                .padding([2, 8])
                .style(move |_| container::Style {
                    background: Some(Background::Color(bg_chip)),
                    border: Border {
                        radius: 8.0.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                }),
            );
        }

        header = header.push(space::horizontal());

        if is_md_doc {
            let toggle_label = if preview.show_raw {
                t(loc, "문서 보기")
            } else {
                t(loc, "원문 보기")
            };
            header = header.push(
                button(text(toggle_label).size(12).color(text_secondary))
                    .on_press(Message::ToggleCanvasRaw)
                    .padding([5, 10])
                    .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 8.0)),
            );
        }

        if !art.content.is_empty() {
            let copy_label = if is_md_doc {
                t(loc, "내용 복사")
            } else {
                t(loc, "코드 복사")
            };
            header = header.push(
                button(
                    row![
                        Icon::Copy.view(13.0, text_secondary),
                        text(copy_label).size(12).color(text_secondary),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::CopyCanvasPreview)
                .padding([5, 10])
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 8.0)),
            );
        }

        if is_html_artifact(art) && !art.content.is_empty() {
            header = header.push(
                button(
                    row![
                        Icon::Play.view(13.0, accent),
                        text(t(loc, "브라우저에서 실행"))
                            .size(12)
                            .font(font_medium())
                            .color(accent),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::RunCanvasHtml(msg_idx, art_idx))
                .padding([5, 10])
                .style(move |_, status| {
                    let hovered = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: Some(Background::Color(if hovered {
                            bg_hover
                        } else {
                            accent_soft
                        })),
                        text_color: accent,
                        border: Border {
                            radius: 8.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }
                }),
            );
        } else if !art.url.is_empty() {
            header = header.push(
                button(
                    row![
                        Icon::Share.view(13.0, accent),
                        text(t(loc, "웹에서 열기"))
                            .size(12)
                            .font(font_medium())
                            .color(accent),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::OpenExternalUrl(art.url.clone()))
                .padding([5, 10])
                .style(move |_, status| ghost_btn_style(status, bg_hover, accent, 8.0)),
            );
        }

        header = header.push(
            button(container(Icon::Close.view(15.0, text_secondary)).center(Fill))
                .on_press(Message::CloseCanvasPreview)
                .padding(0)
                .width(Length::Fixed(30.0))
                .height(Length::Fixed(30.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 15.0)),
        );

        let body: Element<'a, Message> = if let (false, Some(parsed)) =
            (preview.show_raw, preview.parsed_markdown.as_ref())
        {
            let rendered = markdown_view::view(
                parsed,
                self.palette.markdown_settings(),
                &self.palette,
                loc,
                markdown_view::RenderContext {
                    message_idx: msg_idx,
                    copy_target: markdown_view::CopyTarget::CanvasPreview,
                    generated_images: &[],
                    artifacts: &[],
                    image_cache: &self.library_image_cache,
                    failed_images: &self.failed_image_keys,
                },
            );
            let scroll_fg = self.palette.text_primary;
            container(
                scrollable(container(rendered).padding([18, 22]).width(Fill))
                    .width(Fill)
                    .height(Fill)
                    .direction(scrollable::Direction::Vertical(
                        scrollable::Scrollbar::default().width(6).scroller_width(6),
                    ))
                    .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(scroll_fg, status)),
            )
            .width(Fill)
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_canvas)),
                border: Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                ..container::Style::default()
            })
            .into()
        } else {
            let text_code = self.palette.text_code;
            container(
                scrollable(
                    container(
                        text(&art.content)
                            .size(12.5)
                            .font(iced::Font::MONOSPACE)
                            .line_height(text::LineHeight::Relative(1.52))
                            .color(text_code),
                    )
                    .padding([14, 16]),
                )
                .width(Fill)
                .height(Fill)
                .direction(scrollable::Direction::Both {
                    vertical: scrollable::Scrollbar::default().width(6).scroller_width(6),
                    horizontal: scrollable::Scrollbar::default().width(6).scroller_width(6),
                })
                .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(text_code, status)),
            )
            .width(Fill)
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_code)),
                border: Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                ..container::Style::default()
            })
            .into()
        };

        let modal_card = container(column![header, body].spacing(12).width(Fill).height(Fill))
            .padding(18)
            .max_width(880.0)
            .width(Fill)
            .height(Length::Fixed(640.0))
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_popover)),
                border: Border {
                    radius: 18.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
                    offset: Vector::new(0.0, 12.0),
                    blur_radius: 32.0,
                },
                ..container::Style::default()
            });

        let centered = container(modal_card)
            .padding(24)
            .center_x(Fill)
            .center_y(Fill)
            .width(Fill)
            .height(Fill);

        stack![backdrop, centered].width(Fill).height(Fill).into()
    }

    /// 설정 모달 (`⚙`): 계정 프로필 · 이 PC 원격 제어(ON/OFF·허용 폴더·MCP·실행 로그) · 등록 기기 · 단축키.
    pub(crate) fn view_settings_modal(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_popover = pal.bg_popover;
        let bg_composer = pal.bg_composer;
        let bg_chip = pal.bg_chip;
        let bg_hover = pal.bg_hover;
        let bg_thought = pal.bg_thought;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;
        let accent = pal.accent;
        let accent_soft = pal.accent_soft;
        let accent_fg = pal.accent_fg;
        let success = pal.success;
        let danger = pal.danger;

        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.28))),
                    ..container::Style::default()
                }),
        )
        .on_press(Message::CloseSettingsModal);

        let header = row![
            text(t(loc, "설정"))
                .size(18)
                .font(font_semibold())
                .color(text_strong),
            space::horizontal(),
            button(
                row![
                    Icon::Refresh.view(13.5, text_secondary),
                    text(t(loc, "동기화")).size(12).color(text_secondary),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .on_press(Message::RefreshAll)
            .padding([5, 10])
            .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 10.0)),
            button(container(Icon::Close.view(15.0, text_secondary)).center(Fill))
                .on_press(Message::CloseSettingsModal)
                .padding(0)
                .width(Length::Fixed(30.0))
                .height(Length::Fixed(30.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 15.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let profile_card = container(
            row![
                column![
                    row![
                        text(if self.profile.name.is_empty() {
                            t(loc, "Google 계정")
                        } else {
                            self.profile.name.as_str()
                        })
                        .size(15)
                        .font(font_semibold())
                        .color(text_strong),
                        if !self.profile.tier.is_empty() {
                            Element::from(
                                container(
                                    text(&self.profile.tier)
                                        .size(11)
                                        .font(font_medium())
                                        .color(accent),
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
                            )
                        } else {
                            Element::from(space::horizontal().width(Length::Fixed(1.0)))
                        },
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text(&self.profile.email).size(12.5).color(text_secondary),
                    text(format!(
                        "{}: {} ({})",
                        pick(loc, "테마", "Theme"),
                        self.palette.display_name(loc),
                        self.theme_mode.label_for(loc)
                    ))
                    .size(12)
                    .color(text_muted),
                ]
                .spacing(4),
            ]
            .align_y(Alignment::Center),
        )
        .padding(14)
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_chip)),
            border: Border {
                radius: 14.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

        // 환경설정 카드 (테마 수동/자동 전환 + 언어 + 개인 인텔리전스)
        let theme_seg = |m: ThemeMode| {
            let active = self.theme_mode == m;
            button(
                text(m.label_for(loc))
                    .size(12)
                    .font(if active { font_medium() } else { font_regular() })
                    .color(if active { accent } else { text_primary }),
            )
            .on_press(Message::SetThemeMode(m))
            .padding([5, 11])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if active {
                        accent_soft
                    } else if hovered {
                        bg_hover
                    } else {
                        bg_composer
                    })),
                    text_color: if active { accent } else { text_primary },
                    border: Border {
                        radius: 10.0.into(),
                        width: 1.0,
                        color: if active { accent } else { border_subtle },
                    },
                    ..button::Style::default()
                }
            })
        };

        let lang_seg = |pref: LanguagePref| {
            let active = self.language_pref == pref;
            button(
                text(pref.label(loc))
                    .size(12)
                    .font(if active { font_medium() } else { font_regular() })
                    .color(if active { accent } else { text_primary }),
            )
            .on_press(Message::SetLanguagePref(pref))
            .padding([5, 11])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if active {
                        accent_soft
                    } else if hovered {
                        bg_hover
                    } else {
                        bg_composer
                    })),
                    text_color: if active { accent } else { text_primary },
                    border: Border {
                        radius: 10.0.into(),
                        width: 1.0,
                        color: if active { accent } else { border_subtle },
                    },
                    ..button::Style::default()
                }
            })
        };

        let pi_on = self.personal_intelligence;
        let pi_btn = button(
            text(if pi_on { t(loc, "켜짐") } else { t(loc, "꺼짐") })
                .size(12)
                .font(font_medium())
                .color(if pi_on { accent } else { text_secondary }),
        )
        .on_press(Message::TogglePersonalIntelligence)
        .padding([4, 12])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if pi_on {
                    accent_soft
                } else if hovered {
                    bg_hover
                } else {
                    bg_composer
                })),
                text_color: if pi_on { accent } else { text_secondary },
                border: Border {
                    radius: 10.0.into(),
                    width: 1.0,
                    color: if pi_on { accent } else { border_subtle },
                },
                ..button::Style::default()
            }
        });

        let tray_on = self.close_to_tray;
        let tray_btn = button(
            text(if tray_on { t(loc, "켜짐") } else { t(loc, "꺼짐") })
                .size(12)
                .font(font_medium())
                .color(if tray_on { accent } else { text_secondary }),
        )
        .on_press(Message::ToggleCloseToTray)
        .padding([4, 12])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if tray_on {
                    accent_soft
                } else if hovered {
                    bg_hover
                } else {
                    bg_composer
                })),
                text_color: if tray_on { accent } else { text_secondary },
                border: Border {
                    radius: 10.0.into(),
                    width: 1.0,
                    color: if tray_on { accent } else { border_subtle },
                },
                ..button::Style::default()
            }
        });

        let prefs_card = container(
            column![
                row![
                    text(t(loc, "화면 테마"))
                        .size(13)
                        .font(font_medium())
                        .color(text_strong),
                    space::horizontal(),
                    row![
                        theme_seg(ThemeMode::System),
                        theme_seg(ThemeMode::Light),
                        theme_seg(ThemeMode::Dark),
                    ]
                    .spacing(5),
                ]
                .align_y(Alignment::Center),
                row![
                    text(t(loc, "언어"))
                        .size(13)
                        .font(font_medium())
                        .color(text_strong),
                    space::horizontal(),
                    row![
                        lang_seg(LanguagePref::System),
                        lang_seg(LanguagePref::Ko),
                        lang_seg(LanguagePref::En),
                    ]
                    .spacing(5),
                ]
                .align_y(Alignment::Center),
                row![
                    column![
                        text(t(loc, "창 닫기 시 트레이로 최소화"))
                            .size(13)
                            .font(font_medium())
                            .color(text_strong),
                        text(t(
                            loc,
                            "창을 닫아도 시스템 트레이에서 백그라운드로 유지합니다.",
                        ))
                            .size(11.5)
                            .color(text_muted),
                    ]
                    .spacing(2),
                    space::horizontal(),
                    tray_btn,
                ]
                .align_y(Alignment::Center),
                row![
                    column![
                        text(t(loc, "개인 인텔리전스 (Labs)"))
                            .size(13)
                            .font(font_medium())
                            .color(text_strong),
                        text(t(
                            loc,
                            "내 활동과 워크스페이스 컨텍스트를 답변에 자동 반영합니다.",
                        ))
                            .size(11.5)
                            .color(text_muted),
                    ]
                    .spacing(2),
                    space::horizontal(),
                    pi_btn,
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(10),
        )
        .padding(14)
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_chip)),
            border: Border {
                radius: 14.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

        // 사용량 한도 카드 (원본 설정 화면 핵심 섹션)
        let mut usage_col = column![
            row![
                text(t(loc, "사용량 한도"))
                    .size(13)
                    .font(font_semibold())
                    .color(text_strong),
                space::horizontal(),
                text(if self.profile.tier.is_empty() {
                    "Gemini"
                } else {
                    self.profile.tier.as_str()
                })
                .size(11.5)
                .color(accent),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(8);

        if let Some(u) = &self.usage_info {
            if u.buckets.is_empty() {
                usage_col = usage_col.push(
                    text(t(loc, "현재 표시할 사용량 한도 버킷이 없습니다."))
                        .size(12)
                        .color(text_muted),
                );
            } else {
                for b in &u.buckets {
                    let ratio = b.used_ratio.clamp(0.0, 1.0);
                    let pct = (ratio * 100.0).round() as i32;
                    let fill_portion = ((ratio * 100.0).round() as u16).max(if ratio > 0.0 { 2 } else { 0 });
                    let empty_portion = 100u16.saturating_sub(fill_portion);
                    let reset_label = format_reset_time(b.reset_at_unix, loc);
                    let bar_color = if ratio >= 0.85 { danger } else { accent };
                    let mut bar_row = row![].width(Fill).height(Length::Fixed(6.0));
                    if fill_portion > 0 {
                        bar_row = bar_row.push(
                            container(space::horizontal())
                                .width(Length::FillPortion(fill_portion))
                                .height(Fill)
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(bar_color)),
                                    border: Border {
                                        radius: 3.0.into(),
                                        ..Border::default()
                                    },
                                    ..container::Style::default()
                                }),
                        );
                    }
                    if empty_portion > 0 {
                        bar_row = bar_row.push(
                            container(space::horizontal())
                                .width(Length::FillPortion(empty_portion))
                                .height(Fill),
                        );
                    }
                    let bar_track = container(bar_row)
                        .width(Fill)
                        .height(Length::Fixed(6.0))
                        .style(move |_| container::Style {
                            background: Some(Background::Color(bg_composer)),
                            border: Border {
                                radius: 3.0.into(),
                                ..Border::default()
                            },
                            ..container::Style::default()
                        });
                    usage_col = usage_col.push(
                        column![
                            row![
                                text(&b.label)
                                    .size(12.5)
                                    .font(font_medium())
                                    .color(text_primary),
                                space::horizontal(),
                                text(if loc.is_en() {
                                    format!("{pct}% used")
                                } else {
                                    format!("{pct}% 사용")
                                })
                                    .size(11.5)
                                    .color(text_secondary),
                            ]
                            .align_y(Alignment::Center),
                            bar_track,
                            text(reset_label).size(11).color(text_muted),
                        ]
                        .spacing(4),
                    );
                }
            }
            if !u.notice_text.is_empty() {
                usage_col = usage_col.push(
                    text(&u.notice_text)
                        .size(11.5)
                        .color(text_muted)
                        .wrapping(text::Wrapping::WordOrGlyph),
                );
            }
        } else {
            usage_col = usage_col.push(
                text(t(loc, "사용량 한도 정보를 불러오는 중..."))
                    .size(12)
                    .color(text_muted),
            );
        }

        let usage_card = container(usage_col)
            .padding(14)
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_chip)),
                border: Border {
                    radius: 14.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            });

        // 이 PC 원격 제어 카드
        let rs = &self.remote_status;
        let local_name = if !rs.device_name.is_empty() {
            rs.device_name.as_str()
        } else {
            self.daemon_label.as_str()
        };
        let (badge_dot, badge_text) = if !rs.current_tool.is_empty() {
            (
                accent,
                if loc.is_en() {
                    format!("Running: {}", rs.current_tool)
                } else {
                    format!("실행 중: {}", rs.current_tool)
                },
            )
        } else if rs.remote_enabled && rs.punctual_connected {
            (success, t(loc, "원격 연결됨").to_string())
        } else if rs.remote_enabled {
            (text_muted, t(loc, "원격 대기").to_string())
        } else {
            (text_muted, t(loc, "원격 꺼짐").to_string())
        };

        let status_badge = container(
            row![
                container(space::horizontal())
                    .width(Length::Fixed(7.0))
                    .height(Length::Fixed(7.0))
                    .style(move |_| container::Style {
                        background: Some(Background::Color(badge_dot)),
                        border: Border {
                            radius: 4.0.into(),
                            ..Border::default()
                        },
                        ..container::Style::default()
                    }),
                text(badge_text)
                    .size(11.5)
                    .font(font_medium())
                    .color(badge_dot),
            ]
            .spacing(5)
            .align_y(Alignment::Center),
        )
        .padding([3, 9])
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_composer)),
            border: Border {
                radius: 10.0.into(),
                width: 1.0,
                color: border_subtle,
            },
            ..container::Style::default()
        });

        let toggle_label = if self.remote_busy {
            t(loc, "처리 중...")
        } else if rs.remote_enabled {
            t(loc, "원격 제어 끄기")
        } else {
            t(loc, "원격 제어 켜기")
        };
        let is_enabled = rs.remote_enabled;
        let toggle_btn = button(
            text(toggle_label)
                .size(12.5)
                .font(font_medium())
                .color(if is_enabled { text_primary } else { accent_fg }),
        )
        .on_press_maybe((!self.remote_busy).then_some(Message::ToggleRemoteEnabled))
        .padding([6, 12])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if is_enabled {
                    if hovered { bg_hover } else { bg_composer }
                } else if hovered {
                    Color::from_rgba(accent.r, accent.g, accent.b, 0.88)
                } else {
                    accent
                })),
                text_color: if is_enabled { text_primary } else { accent_fg },
                border: Border {
                    radius: 10.0.into(),
                    width: if is_enabled { 1.0 } else { 0.0 },
                    color: border_subtle,
                },
                ..button::Style::default()
            }
        });

        let sync_btn = button(
            row![
                Icon::Refresh.view(13.0, text_primary),
                text(t(loc, "지금 재등록")).size(12.5).color(text_primary),
            ]
            .spacing(5)
            .align_y(Alignment::Center),
        )
        .on_press_maybe((!self.remote_busy).then_some(Message::SyncRemoteRegistration))
        .padding([6, 12])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if hovered {
                    bg_hover
                } else {
                    bg_composer
                })),
                text_color: text_primary,
                border: Border {
                    radius: 10.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                ..button::Style::default()
            }
        });

        let remote_summary = if loc.is_en() {
            format!(
                "{} tools · {} active sessions · Punctual {}",
                rs.tool_count,
                rs.active_sessions.len(),
                if rs.punctual_connected {
                    "connected"
                } else {
                    "disconnected"
                }
            )
        } else {
            format!(
                "등록 도구 {}개 · 활성 세션 {}개 · Punctual {}",
                rs.tool_count,
                rs.active_sessions.len(),
                if rs.punctual_connected {
                    "연결됨"
                } else {
                    "미연결"
                }
            )
        };

        let mut local_col = column![
            row![
                Icon::Laptop.view(16.0, text_strong),
                text(local_name)
                    .size(14.5)
                    .font(font_semibold())
                    .color(text_strong),
                space::horizontal(),
                status_badge,
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            text(remote_summary).size(12).color(text_secondary),
            row![toggle_btn, sync_btn]
                .spacing(8)
                .align_y(Alignment::Center),
        ]
        .spacing(8);

        // 허용 폴더 관리
        let mut folders_col = column![
            row![
                text(t(loc, "허용 폴더"))
                    .size(12.5)
                    .font(font_semibold())
                    .color(text_strong),
                space::horizontal(),
                button(
                    row![
                        Icon::Plus.view(13.0, accent),
                        text(t(loc, "폴더 선택"))
                            .size(12)
                            .font(font_medium())
                            .color(accent),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .on_press_maybe((!self.remote_busy).then_some(Message::PickRemoteFolder))
                .padding([4, 9])
                .style(move |_, status| {
                    let hovered = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: Some(Background::Color(if hovered {
                            bg_hover
                        } else {
                            accent_soft
                        })),
                        text_color: accent,
                        border: Border {
                            radius: 9.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }
                }),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(6);

        if rs.connected_folders.is_empty() {
            folders_col = folders_col.push(
                text(t(
                    loc,
                    "등록된 허용 폴더가 없습니다. (도구는 기본 작업 경로 기준으로 동작)",
                ))
                    .size(11.5)
                    .color(text_muted),
            );
        } else {
            for folder in &rs.connected_folders {
                let folder_clone = folder.clone();
                folders_col = folders_col.push(
                    container(
                        row![
                            text(folder)
                                .size(12)
                                .color(text_primary)
                                .width(Fill)
                                .wrapping(text::Wrapping::WordOrGlyph),
                            button(container(Icon::Close.view(12.5, text_primary)).center(Fill))
                                .on_press_maybe(
                                    (!self.remote_busy)
                                        .then_some(Message::RemoveRemoteFolder(folder_clone)),
                                )
                                .padding(0)
                                .width(Length::Fixed(24.0))
                                .height(Length::Fixed(24.0))
                                .style(move |_, status| {
                                    let hovered = matches!(status, button::Status::Hovered);
                                    button::Style {
                                        background: Some(Background::Color(if hovered {
                                            Color::from_rgba(danger.r, danger.g, danger.b, 0.16)
                                        } else {
                                            bg_chip
                                        })),
                                        text_color: if hovered { danger } else { text_primary },
                                        border: Border {
                                            radius: 12.0.into(),
                                            width: 1.0,
                                            color: if hovered { danger } else { border_subtle },
                                        },
                                        ..button::Style::default()
                                    }
                                }),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .padding([6, 10])
                    .width(Fill)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_composer)),
                        border: Border {
                            radius: 9.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..container::Style::default()
                    }),
                );
            }
        }

        let folder_input = text_input(
            t(loc, "폴더 경로 직접 입력 (예: ~/dev)"),
            &self.folder_input_draft,
        )
            .on_input(Message::FolderDraftChanged)
            .on_submit(Message::AddRemoteFolderManual)
            .size(12.5)
            .padding([6, 10])
            .style(move |_, _| text_input::Style {
                background: Background::Color(bg_composer),
                border: Border {
                    radius: 9.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                icon: text_muted,
                placeholder: text_muted,
                value: text_primary,
                selection: accent_soft,
            });
        let add_folder_btn = button(
            text(t(loc, "추가"))
                .size(12)
                .font(font_medium())
                .color(text_primary),
        )
            .on_press_maybe(
                (!self.remote_busy && !self.folder_input_draft.trim().is_empty())
                    .then_some(Message::AddRemoteFolderManual),
            )
            .padding([6, 12])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if hovered {
                        bg_hover
                    } else {
                        bg_composer
                    })),
                    text_color: text_primary,
                    border: Border {
                        radius: 9.0.into(),
                        width: 1.0,
                        color: border_subtle,
                    },
                    ..button::Style::default()
                }
            });
        folders_col = folders_col.push(
            row![folder_input, add_folder_btn]
                .spacing(6)
                .align_y(Alignment::Center),
        );
        local_col = local_col.push(folders_col);

        // MCP 서버 칩
        if !rs.mcp_servers.is_empty() {
            let mcp_chips: Vec<Element<'_, Message>> = rs
                .mcp_servers
                .iter()
                .map(|srv| {
                    let name = if !srv.display_name.is_empty() {
                        srv.display_name.as_str()
                    } else {
                        srv.server_id.as_str()
                    };
                    let dot_c = if srv.enabled { success } else { text_muted };
                    container(
                        row![
                            container(space::horizontal())
                                .width(Length::Fixed(6.0))
                                .height(Length::Fixed(6.0))
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(dot_c)),
                                    border: Border {
                                        radius: 3.0.into(),
                                        ..Border::default()
                                    },
                                    ..container::Style::default()
                                }),
                            text(name).size(11.5).color(text_primary),
                        ]
                        .spacing(5)
                        .align_y(Alignment::Center),
                    )
                    .padding([3, 9])
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_composer)),
                        border: Border {
                            radius: 10.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..container::Style::default()
                    })
                    .into()
                })
                .collect();
            local_col = local_col.push(
                column![
                    text(t(loc, "연동 MCP 서버"))
                        .size(12.5)
                        .font(font_semibold())
                        .color(text_strong),
                    row(mcp_chips).spacing(6),
                ]
                .spacing(5),
            );
        }

        // 최근 원격 도구 실행 로그 (내부 상태 변경 이벤트 제외, 최신 6건)
        let tool_events: Vec<_> = rs
            .recent_events
            .iter()
            .rev()
            .filter(|ev| ev.tool_name != "remote_session" && ev.phase != "status_changed")
            .take(6)
            .collect();
        if !tool_events.is_empty() {
            let mut log_col = column![
                text(t(loc, "최근 원격 도구 실행 기록"))
                    .size(12.5)
                    .font(font_semibold())
                    .color(text_strong),
            ]
            .spacing(4);
            for ev in tool_events {
                let ev_dot = if !ev.error.is_empty() {
                    danger
                } else if ev.phase == "started" {
                    accent
                } else {
                    success
                };
                let mut detail = String::new();
                if !ev.args_brief.is_empty() {
                    detail.push_str(&truncate_single_line(&ev.args_brief, 34));
                }
                if !ev.error.is_empty() {
                    if !detail.is_empty() {
                        detail.push_str(" · ");
                    }
                    detail.push_str(&truncate_single_line(&ev.error, 28));
                } else if ev.duration_ms > 0 {
                    if !detail.is_empty() {
                        detail.push_str(" · ");
                    }
                    detail.push_str(&format!("{}ms", ev.duration_ms));
                }
                log_col = log_col.push(
                    container(
                        row![
                            container(space::horizontal())
                                .width(Length::Fixed(6.0))
                                .height(Length::Fixed(6.0))
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(ev_dot)),
                                    border: Border {
                                        radius: 3.0.into(),
                                        ..Border::default()
                                    },
                                    ..container::Style::default()
                                }),
                            text(&ev.tool_name)
                                .size(12)
                                .font(font_medium())
                                .color(text_strong),
                            text(detail).size(11.5).color(text_secondary),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([5, 9])
                    .width(Fill)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_thought)),
                        border: Border {
                            radius: 8.0.into(),
                            ..Border::default()
                        },
                        ..container::Style::default()
                    }),
                );
            }
            local_col = local_col.push(log_col);
        }

        let local_remote_card = container(local_col)
            .padding(14)
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_chip)),
                border: Border {
                    radius: 14.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            });

        let mut dev_col = column![
            text(t(loc, "이 PC 원격 제어"))
                .size(13)
                .font(font_semibold())
                .color(text_strong),
            local_remote_card,
        ]
        .spacing(8);

        let other_devices: Vec<&RegisteredDevice> = self
            .devices
            .iter()
            .filter(|d| {
                rs.device_id.is_empty() || !d.device_id.eq_ignore_ascii_case(&rs.device_id)
            })
            .collect();
        if !other_devices.is_empty() {
            dev_col = dev_col.push(
                text(t(loc, "다른 등록 기기"))
                    .size(13)
                    .font(font_semibold())
                    .color(text_strong),
            );
            for d in other_devices {
                let status_col = if d.remote_enabled { success } else { text_muted };
                let folders = d
                    .folders
                    .as_ref()
                    .map(|f| f.join(", "))
                    .unwrap_or_default();
                let mut info = column![
                    row![
                        Icon::Laptop.view(14.5, text_secondary),
                        text(&d.device_name)
                            .size(13)
                            .font(font_medium())
                            .color(text_strong),
                        text(if d.remote_enabled {
                            if loc.is_en() {
                                format!("Remote active · {} tools", d.tool_count)
                            } else {
                                format!("원격 활성 · 도구 {}개", d.tool_count)
                            }
                        } else {
                            t(loc, "대기").into()
                        })
                        .size(11.5)
                        .color(status_col),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(2);
                if !folders.is_empty() {
                    info = info.push(
                        text(format!("{}: {folders}", t(loc, "허용 폴더")))
                            .size(11.5)
                            .color(text_muted),
                    );
                }
                dev_col = dev_col.push(
                    container(info)
                        .padding([10, 12])
                        .width(Fill)
                        .style(move |_| container::Style {
                            background: Some(Background::Color(bg_chip)),
                            border: Border {
                                radius: 12.0.into(),
                                ..Border::default()
                            },
                            ..container::Style::default()
                        }),
                );
            }
        }

        let shortcuts = container(
            column![
                text(t(loc, "단축키"))
                    .size(13)
                    .font(font_semibold())
                    .color(text_strong),
                text(t(
                    loc,
                    "Ctrl+N 새 채팅   ·   Ctrl+K 대화 검색   ·   Ctrl+B 사이드바   ·   Esc 닫기/중단",
                ))
                    .size(12)
                    .color(text_secondary),
            ]
            .spacing(5),
        )
        .padding([10, 12])
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_chip)),
            border: Border {
                radius: 12.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

        let modal_scroll = scrollable(
            column![profile_card, usage_card, prefs_card, dev_col, shortcuts]
                .spacing(14)
                .padding(Padding {
                    top: 0.0,
                    right: 6.0,
                    bottom: 0.0,
                    left: 0.0,
                }),
        )
        .height(Length::Fixed(740.0))
        .width(Fill)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::default().width(4).scroller_width(4),
        ))
        .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(text_primary, status));

        let card = container(column![header, modal_scroll].spacing(14))
            .width(Length::Fixed(580.0))
            .padding(18)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_popover)),
                border: Border {
                    radius: 20.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.24),
                    offset: Vector::new(0.0, 12.0),
                    blur_radius: 36.0,
                },
                ..container::Style::default()
            });

        stack![backdrop, container(card).center(Fill)]
            .width(Fill)
            .height(Fill)
            .into()
    }
}
