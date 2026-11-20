use crate::i18n::{Locale, pick, t};
use crate::icons::Icon;
use crate::theme::{font_medium, font_regular, font_semibold};
use crate::{
    ActiveSection, App, FEATURE_MODE_FLASHCARDS, FEATURE_MODE_GUIDED_LEARNING, FEATURE_MODE_QUIZ,
    FEATURE_MODE_VIDEO, Message, card_style, ghost_btn_style, truncate_single_line,
};
use iced::widget::{button, column, container, image, row, space, text, text_editor, text_input};
use iced::{Alignment, Background, Border, Color, ContentFit, Element, Fill, Length};

impl App {
    /// 라이브러리 뷰: 서버 Canvas 문서·생성 이미지 미디어 + 로컬 첨부 파일 통합 아카이브.
    pub(crate) fn view_library(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_composer = pal.bg_composer;
        let bg_hover = pal.bg_hover;
        let bg_chip = pal.bg_chip;
        let bg_thought = pal.bg_thought;
        let accent = pal.accent;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;

        let filter_pill = |code: &'static str, label: &'static str| {
            let active = self.library_filter == code;
            button(
                text(label)
                    .size(13)
                    .font(if active { font_medium() } else { font_regular() })
                    .color(if active { accent } else { text_primary }),
            )
            .on_press(Message::SetLibraryFilter(code.into()))
            .padding([6, 14])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if active {
                        accent_soft
                    } else if hovered {
                        bg_hover
                    } else {
                        bg_chip
                    })),
                    text_color: if active { accent } else { text_primary },
                    border: Border {
                        radius: 16.0.into(),
                        width: if active { 1.0 } else { 0.0 },
                        color: accent,
                    },
                    ..button::Style::default()
                }
            })
        };

        let header = row![
            column![
                text(t(loc, "라이브러리"))
                    .size(26)
                    .font(font_semibold())
                    .color(text_strong),
                text(t(
                    loc,
                    "Gemini에서 만든 Canvas 문서·이미지 미디어와 첨부 파일을 모아봅니다.",
                ))
                .size(13)
                .color(text_secondary),
            ]
            .spacing(4),
            space::horizontal(),
            button(
                row![
                    Icon::Refresh.view(14.0, text_secondary),
                    text(t(loc, "새로고침")).size(12.5).color(text_secondary),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::SwitchSection(ActiveSection::Library))
            .padding([6, 12])
            .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 12.0)),
        ]
        .align_y(Alignment::Center);

        let filters = row![
            filter_pill("all", t(loc, "전체")),
            filter_pill("document", t(loc, "문서")),
            filter_pill("image", t(loc, "미디어")),
            filter_pill("file", t(loc, "로컬 파일")),
        ]
        .spacing(8);

        let filtered: Vec<_> = self
            .library_items
            .iter()
            .filter(|it| match self.library_filter.as_str() {
                "all" => true,
                "document" => it.kind == "document",
                "image" => it.kind == "image" || it.kind == "video" || it.kind == "audio",
                "file" => !it.is_remote,
                _ => true,
            })
            .collect();

        let mut grid = column![].spacing(12);
        if filtered.is_empty() {
            grid = grid.push(
                container(
                    column![
                        Icon::Library.view(28.0, text_muted),
                        text(t(loc, "표시할 라이브러리 항목이 없습니다"))
                            .size(15)
                            .font(font_medium())
                            .color(text_primary),
                        text(t(
                            loc,
                            "Canvas 문서나 이미지를 생성하거나 파일을 첨부하면 이곳에 표시됩니다.",
                        ))
                        .size(13)
                        .color(text_secondary),
                    ]
                    .spacing(8)
                    .align_x(Alignment::Center),
                )
                .padding(36)
                .width(Fill)
                .center_x(Fill)
                .style(move |_| card_style(bg_composer, border_subtle)),
            );
        } else {
            for pair in filtered.chunks(2) {
                let mut r = row![].spacing(12).width(Fill);
                for it in pair {
                    let kind_label = match it.kind.as_str() {
                        "document" => t(loc, "Canvas 문서"),
                        "image" => pick(loc, "이미지", "Image"),
                        "audio" => t(loc, "오디오"),
                        _ => t(loc, "파일"),
                    };
                    let mut card_body = column![
                        row![
                            if it.kind == "document" {
                                Icon::Canvas.view(15.0, accent)
                            } else if it.kind == "image" {
                                Icon::Image.view(15.0, accent)
                            } else {
                                Icon::AttachFile.view(15.0, accent)
                            },
                            text(truncate_single_line(&it.title, 28))
                                .size(14)
                                .font(font_semibold())
                                .color(text_strong),
                            space::horizontal(),
                            container(text(kind_label).size(11).color(text_secondary))
                                .padding([2, 8])
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(bg_chip)),
                                    border: Border {
                                        radius: 8.0.into(),
                                        ..Border::default()
                                    },
                                    ..container::Style::default()
                                }),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    ]
                    .spacing(8);

                    if it.kind == "image" && !it.url.is_empty() {
                        if let Some(handle) = self.library_image_cache.get(&it.id) {
                            card_body = card_body.push(
                                container(
                                    image(handle.clone())
                                        .width(Fill)
                                        .height(Length::Fixed(140.0))
                                        .content_fit(ContentFit::Cover),
                                )
                                .width(Fill)
                                .height(Length::Fixed(140.0))
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(bg_thought)),
                                    border: Border {
                                        radius: 10.0.into(),
                                        ..Border::default()
                                    },
                                    ..container::Style::default()
                                }),
                            );
                        } else {
                            card_body = card_body.push(
                                container(
                                    row![
                                        Icon::Image.view(18.0, text_muted),
                                        text(t(loc, "이미지 썸네일 불러오는 중..."))
                                            .size(12)
                                            .color(text_muted),
                                    ]
                                    .spacing(8)
                                    .align_y(Alignment::Center),
                                )
                                .padding(14)
                                .width(Fill)
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(bg_thought)),
                                    border: Border {
                                        radius: 10.0.into(),
                                        ..Border::default()
                                    },
                                    ..container::Style::default()
                                }),
                            );
                        }
                    } else {
                        let snippet = if !it.content_preview.is_empty() {
                            it.content_preview.as_str()
                        } else {
                            it.preview.as_str()
                        };
                        if !snippet.is_empty() {
                            card_body = card_body.push(
                                container(
                                    text(truncate_single_line(snippet, 110))
                                        .size(12)
                                        .color(text_secondary)
                                        .wrapping(text::Wrapping::WordOrGlyph),
                                )
                                .padding([8, 10])
                                .width(Fill)
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(bg_thought)),
                                    border: Border {
                                        radius: 10.0.into(),
                                        ..Border::default()
                                    },
                                    ..container::Style::default()
                                }),
                            );
                        }
                    }

                    let mut actions = row![].spacing(8).align_y(Alignment::Center);
                    if !it.conversation_id.is_empty() {
                        actions = actions.push(
                            button(
                                text(t(loc, "대화 열기"))
                                    .size(12)
                                    .font(font_medium())
                                    .color(accent),
                            )
                            .on_press(Message::SelectConversation(
                                it.conversation_id.clone(),
                                it.title.clone(),
                            ))
                            .padding([5, 11])
                            .style(move |_, status| {
                                ghost_btn_style(status, accent_soft, accent, 10.0)
                            }),
                        );
                    }
                    if !it.url.is_empty() {
                        actions = actions.push(
                            button(text(t(loc, "링크 복사")).size(12).color(text_secondary))
                                .on_press(Message::CopyText(it.url.clone()))
                                .padding([5, 10])
                                .style(move |_, status| {
                                    ghost_btn_style(status, bg_hover, text_secondary, 10.0)
                                }),
                        );
                    } else if !it.path.is_empty() {
                        actions = actions.push(
                            button(text(t(loc, "채팅에 첨부")).size(12).color(accent))
                                .on_press(Message::AttachLibraryPath(it.path.clone()))
                                .padding([5, 10])
                                .style(move |_, status| {
                                    ghost_btn_style(status, accent_soft, accent, 10.0)
                                }),
                        );
                    }
                    card_body = card_body.push(actions);

                    r = r.push(
                        container(card_body)
                            .padding(14)
                            .width(Fill)
                            .style(move |_| card_style(bg_composer, border_subtle)),
                    );
                }
                if pair.len() == 1 {
                    r = r.push(space::horizontal().width(Fill));
                }
                grid = grid.push(r);
            }
        }

        self.wrap_section_page(column![header, filters, grid].spacing(18))
    }

    /// 이미지 스튜디오 뷰: 원본 `Nano Banana` 스타일 템플릿 5카드 + 프롬프트 생성기 + 최근 서버 이미지.
    pub(crate) fn view_images(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_composer = pal.bg_composer;
        let bg_hover = pal.bg_hover;
        let bg_chip = pal.bg_chip;
        let bg_thought = pal.bg_thought;
        let accent = pal.accent;
        let accent_fg = pal.accent_fg;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;

        let header = column![
            row![
                text("Nano Banana")
                    .size(26)
                    .font(font_semibold())
                    .color(text_strong),
                container(
                    text(t(loc, "이미지 스튜디오"))
                        .size(11.5)
                        .font(font_medium())
                        .color(accent),
                )
                .padding([3, 9])
                .style(move |_| container::Style {
                    background: Some(Background::Color(accent_soft)),
                    border: Border {
                        radius: 10.0.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                }),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            text(t(
                loc,
                "스타일 템플릿을 고르거나 원하는 장면을 입력하면 바로 이미지를 생성합니다.",
            ))
            .size(13)
            .color(text_secondary),
        ]
        .spacing(4);

        let templates = [
            ("도시락", "정갈한 일본식 도시락 일러스트 스타일", Color::from_rgb(0.96, 0.78, 0.42)),
            ("파스텔", "부드러운 파스텔 톤 수채화 감성", Color::from_rgb(0.86, 0.74, 0.96)),
            ("피어난 꽃", "햇살 가득한 보태니컬 플로럴 아트워크", Color::from_rgb(0.97, 0.72, 0.78)),
            ("페인트", "질감이 살아있는 아크릴 유화 붓터치", Color::from_rgb(0.56, 0.80, 0.96)),
            ("점토", "귀여운 3D 클레이 스톱모션 미니어처", Color::from_rgb(0.64, 0.88, 0.74)),
        ];

        let desc_max = if loc.is_en() { 28 } else { 16 };
        let mut tpl_row = row![].spacing(10).width(Fill);
        for (style_name, desc, swatch) in templates {
            let active = self.image_style == style_name;
            tpl_row = tpl_row.push(
                button(
                    column![
                        container(space::horizontal())
                            .width(Fill)
                            .height(Length::Fixed(54.0))
                            .style(move |_| container::Style {
                                background: Some(Background::Color(swatch)),
                                border: Border {
                                    radius: 10.0.into(),
                                    ..Border::default()
                                },
                                ..container::Style::default()
                            }),
                        text(t(loc, style_name))
                            .size(13)
                            .font(font_semibold())
                            .color(if active { accent } else { text_strong }),
                        text(truncate_single_line(t(loc, desc), desc_max))
                            .size(11)
                            .color(text_secondary),
                    ]
                    .spacing(6),
                )
                .on_press(Message::SetImageStyle(style_name.into()))
                .padding(10)
                .width(Fill)
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
                        text_color: text_primary,
                        border: Border {
                            radius: 14.0.into(),
                            width: if active { 1.5 } else { 1.0 },
                            color: if active { accent } else { border_subtle },
                        },
                        ..button::Style::default()
                    }
                }),
            );
        }

        let prompt_in = text_input(
            t(
                loc,
                "만들고 싶은 이미지를 자세히 설명해 주세요 (예: 비 오는 서울 밤거리 네온사인)",
            ),
            &self.image_prompt_draft,
        )
        .on_input(Message::ImagePromptChanged)
        .on_submit(Message::GenerateImagePrompt)
        .size(14)
        .padding([12, 14])
        .style(move |_, _| text_input::Style {
            background: Background::Color(bg_chip),
            border: Border {
                radius: 14.0.into(),
                width: 1.0,
                color: border_subtle,
            },
            icon: text_muted,
            placeholder: text_muted,
            value: text_primary,
            selection: accent_soft,
        });

        let chip = |label: &'static str, active: bool, msg: Message| {
            button(
                text(label)
                    .size(12)
                    .font(if active { font_medium() } else { font_regular() })
                    .color(if active { accent } else { text_primary }),
            )
            .on_press(msg)
            .padding([5, 11])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if active {
                        accent_soft
                    } else if hovered {
                        bg_hover
                    } else {
                        bg_chip
                    })),
                    text_color: text_primary,
                    border: Border {
                        radius: 14.0.into(),
                        width: if active { 1.0 } else { 0.0 },
                        color: accent,
                    },
                    ..button::Style::default()
                }
            })
        };

        let controls = row![
            container(
                row![
                    Icon::Image.view(13.5, accent),
                    text(pick(loc, "이미지", "Image"))
                        .size(12)
                        .font(font_medium())
                        .color(accent),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .padding([5, 10])
            .style(move |_| container::Style {
                background: Some(Background::Color(accent_soft)),
                border: Border {
                    radius: 14.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
            text(t(loc, "비율:")).size(12).color(text_secondary),
            chip("1:1", self.image_aspect == "1:1", Message::SetImageAspect("1:1".into())),
            chip("16:9", self.image_aspect == "16:9", Message::SetImageAspect("16:9".into())),
            chip("9:16", self.image_aspect == "9:16", Message::SetImageAspect("9:16".into())),
            chip("4:3", self.image_aspect == "4:3", Message::SetImageAspect("4:3".into())),
            space::horizontal(),
            button(
                row![
                    Icon::Sparkle.view(14.0, accent_fg),
                    text(t(loc, "이미지 생성"))
                        .size(13)
                        .font(font_medium())
                        .color(accent_fg),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::GenerateImagePrompt)
            .padding([8, 16])
            .style(move |_, _| button::Style {
                background: Some(Background::Color(accent)),
                text_color: accent_fg,
                border: Border {
                    radius: 16.0.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let studio_card = container(column![prompt_in, controls].spacing(12))
            .padding(16)
            .width(Fill)
            .style(move |_| card_style(bg_composer, border_subtle));

        // 최근 생성된 서버 이미지 갤러리
        let recent_imgs: Vec<_> = self
            .library_items
            .iter()
            .filter(|it| it.kind == "image")
            .take(6)
            .collect();
        let mut recent_col = column![
            text(t(loc, "최근 생성한 이미지"))
                .size(15)
                .font(font_semibold())
                .color(text_strong),
        ]
        .spacing(10);

        if recent_imgs.is_empty() {
            recent_col = recent_col.push(
                container(
                    text(t(
                        loc,
                        "아직 생성된 이미지가 없습니다. 위 입력창에서 첫 이미지를 만들어 보세요.",
                    ))
                    .size(13)
                    .color(text_secondary),
                )
                .padding(20)
                .width(Fill)
                .style(move |_| card_style(bg_composer, border_subtle)),
            );
        } else {
            for pair in recent_imgs.chunks(2) {
                let mut r = row![].spacing(12).width(Fill);
                for it in pair {
                    let preview_el: Element<'_, Message> = if let Some(h) =
                        self.library_image_cache.get(&it.id)
                    {
                        image(h.clone())
                            .width(Fill)
                            .height(Length::Fixed(130.0))
                            .content_fit(ContentFit::Cover)
                            .into()
                    } else {
                        container(text(t(loc, "이미지 미리보기")).size(12).color(text_muted))
                            .width(Fill)
                            .height(Length::Fixed(130.0))
                            .align_x(Alignment::Center)
                            .align_y(Alignment::Center)
                            .style(move |_| container::Style {
                                background: Some(Background::Color(bg_thought)),
                                border: Border {
                                    radius: 10.0.into(),
                                    ..Border::default()
                                },
                                ..container::Style::default()
                            })
                            .into()
                    };
                    let mut c = column![
                        preview_el,
                        text(truncate_single_line(&it.title, 26))
                            .size(13)
                            .font(font_medium())
                            .color(text_strong),
                    ]
                    .spacing(6);
                    if !it.conversation_id.is_empty() {
                        c = c.push(
                            button(text(t(loc, "대화 열기")).size(12).color(accent))
                                .on_press(Message::SelectConversation(
                                    it.conversation_id.clone(),
                                    it.title.clone(),
                                ))
                                .padding([4, 8])
                                .style(move |_, status| {
                                    ghost_btn_style(status, accent_soft, accent, 8.0)
                                }),
                        );
                    }
                    r = r.push(
                        container(c)
                            .padding(12)
                            .width(Fill)
                            .style(move |_| card_style(bg_composer, border_subtle)),
                    );
                }
                if pair.len() == 1 {
                    r = r.push(space::horizontal().width(Fill));
                }
                recent_col = recent_col.push(r);
            }
        }

        self.wrap_section_page(column![header, tpl_row, studio_card, recent_col].spacing(18))
    }

    /// 예약 작업 뷰: 빠른 예약 입력 + 필터 + 서버 활성 예약 + 추천 예약 5종 + 로컬 예약 폼.
    pub(crate) fn view_schedules(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_composer = pal.bg_composer;
        let bg_hover = pal.bg_hover;
        let bg_chip = pal.bg_chip;
        let accent = pal.accent;
        let accent_fg = pal.accent_fg;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;
        let success = pal.success;
        let danger = pal.danger;

        let header = row![
            column![
                text(t(loc, "예약된 작업"))
                    .size(26)
                    .font(font_semibold())
                    .color(text_strong),
                text(t(
                    loc,
                    "지정한 일정에 따라 Gemini가 자동으로 브리핑과 작업을 실행합니다.",
                ))
                .size(13)
                .color(text_secondary),
            ]
            .spacing(4),
            space::horizontal(),
            button(
                row![
                    Icon::Refresh.view(14.0, text_secondary),
                    text(t(loc, "새로고침")).size(12.5).color(text_secondary),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .on_press(Message::SwitchSection(ActiveSection::Schedules))
            .padding([6, 12])
            .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 12.0)),
        ]
        .align_y(Alignment::Center);

        // 상단 빠른 예약 바 + 로컬 상세 입력
        let quick_in = text_input(
            t(
                loc,
                "예약할 작업을 설명하세요 (예: 매일 오전 9시에 AI 뉴스 요약해 줘)",
            ),
            &self.sched_prompt_draft,
        )
        .on_input(Message::SchedPromptChanged)
        .on_submit(Message::QuickCreateSchedule)
        .size(13.5)
        .padding([10, 14])
        .style(move |_, _| text_input::Style {
            background: Background::Color(bg_chip),
            border: Border {
                radius: 14.0.into(),
                width: 1.0,
                color: border_subtle,
            },
            icon: text_muted,
            placeholder: text_muted,
            value: text_primary,
            selection: accent_soft,
        });

        let title_in = text_input(t(loc, "제목 (선택)"), &self.sched_title_draft)
            .on_input(Message::SchedTitleChanged)
            .size(12.5)
            .padding([7, 10])
            .width(Length::FillPortion(2))
            .style(move |_, _| text_input::Style {
                background: Background::Color(bg_chip),
                border: Border {
                    radius: 10.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                icon: text_muted,
                placeholder: text_muted,
                value: text_primary,
                selection: accent_soft,
            });
        let time_in = text_input("HH:MM", &self.sched_time_draft)
            .on_input(Message::SchedTimeChanged)
            .size(12.5)
            .padding([7, 10])
            .width(Length::Fixed(86.0))
            .style(move |_, _| text_input::Style {
                background: Background::Color(bg_chip),
                border: Border {
                    radius: 10.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                icon: text_muted,
                placeholder: text_muted,
                value: text_primary,
                selection: accent_soft,
            });

        let freq_btn = |code: &'static str, label: &'static str| {
            let active = self.sched_type_draft == code;
            button(
                text(label)
                    .size(12)
                    .color(if active { accent } else { text_primary }),
            )
            .on_press(Message::SchedTypeChanged(code.into()))
            .padding([6, 10])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if active {
                        accent_soft
                    } else if hovered {
                        bg_hover
                    } else {
                        bg_chip
                    })),
                    text_color: text_primary,
                    border: Border {
                        radius: 10.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }
            })
        };

        let create_card = container(
            column![
                row![
                    quick_in,
                    button(
                        row![
                            Icon::Plus.view(14.0, accent_fg),
                            text(t(loc, "예약 추가"))
                                .size(13)
                                .font(font_medium())
                                .color(accent_fg),
                        ]
                        .spacing(5)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::CreateSchedule)
                    .padding([10, 16])
                    .style(move |_, _| button::Style {
                        background: Some(Background::Color(accent)),
                        text_color: accent_fg,
                        border: Border {
                            radius: 14.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    title_in,
                    freq_btn("daily", t(loc, "매일")),
                    freq_btn("weekly", t(loc, "매주")),
                    freq_btn("hourly", t(loc, "매시간")),
                    time_in,
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ]
            .spacing(10),
        )
        .padding(14)
        .width(Fill)
        .style(move |_| card_style(bg_composer, border_subtle));

        // 필터 탭 (전체 / 활성 / 일시중지)
        let f_pill = |code: &'static str, label: &'static str| {
            let active = self.sched_filter == code;
            button(
                text(label)
                    .size(12.5)
                    .font(if active { font_medium() } else { font_regular() })
                    .color(if active { accent } else { text_primary }),
            )
            .on_press(Message::SetSchedFilter(code.into()))
            .padding([5, 13])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if active {
                        accent_soft
                    } else if hovered {
                        bg_hover
                    } else {
                        bg_chip
                    })),
                    text_color: text_primary,
                    border: Border {
                        radius: 14.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }
            })
        };

        let filter_row = row![
            f_pill("all", t(loc, "전체")),
            f_pill("active", t(loc, "활성")),
            f_pill("paused", t(loc, "일시중지")),
        ]
        .spacing(8);

        let visible_schedules: Vec<_> = self
            .schedules
            .iter()
            .filter(|s| match self.sched_filter.as_str() {
                "active" => s.enabled,
                "paused" => !s.enabled,
                _ => true,
            })
            .collect();

        let mut list_col = column![].spacing(10);
        if visible_schedules.is_empty() {
            list_col = list_col.push(
                container(
                    text(t(
                        loc,
                        "표시할 예약 작업이 없습니다. 아래 추천 예약을 추가하거나 직접 만들어 보세요.",
                    ))
                    .size(13)
                    .color(text_secondary),
                )
                .padding(20)
                .width(Fill)
                .style(move |_| card_style(bg_composer, border_subtle)),
            );
        } else {
            for s in visible_schedules {
                let schedule_label = if !s.schedule_desc.is_empty() {
                    s.schedule_desc.clone()
                } else {
                    let freq = match s.schedule_type.as_str() {
                        "weekly" => t(loc, "매주"),
                        "hourly" => t(loc, "매시간"),
                        _ => t(loc, "매일"),
                    };
                    format!("{freq} {}", s.time_of_day)
                };
                let is_running = self.running_schedule_id.as_deref() == Some(s.id.as_str());
                let badge_col = if s.enabled { success } else { text_muted };
                let badge_txt = if s.is_remote {
                    if s.enabled {
                        t(loc, "서버 활성")
                    } else {
                        t(loc, "서버 중지")
                    }
                } else if s.enabled {
                    t(loc, "로컬 활성")
                } else {
                    t(loc, "일시중지")
                };

                let mut action_row = row![
                    button(
                        row![
                            Icon::Play.view(12.0, accent),
                            text(if is_running {
                                t(loc, "실행 중...")
                            } else {
                                t(loc, "지금 실행")
                            })
                            .size(12)
                            .font(font_medium())
                            .color(accent),
                        ]
                        .spacing(5)
                        .align_y(Alignment::Center),
                    )
                    .on_press_maybe((!is_running).then_some(Message::RunScheduleNow(s.id.clone())))
                    .padding([5, 11])
                    .style(move |_, status| ghost_btn_style(status, accent_soft, accent, 10.0)),
                ]
                .spacing(6)
                .align_y(Alignment::Center);

                if !s.last_conversation_id.is_empty() {
                    action_row = action_row.push(
                        button(text(t(loc, "최근 결과 열기")).size(12).color(text_primary))
                            .on_press(Message::SelectConversation(
                                s.last_conversation_id.clone(),
                                s.title.clone(),
                            ))
                            .padding([5, 10])
                            .style(move |_, status| {
                                ghost_btn_style(status, bg_hover, text_primary, 10.0)
                            }),
                    );
                }
                if !s.is_remote {
                    action_row = action_row.push(
                        button(
                            text(if s.enabled {
                                t(loc, "일시중지")
                            } else {
                                t(loc, "다시 켜기")
                            })
                            .size(12)
                            .color(text_secondary),
                        )
                        .on_press(Message::ToggleScheduleEnabled(s.id.clone(), !s.enabled))
                        .padding([5, 10])
                        .style(move |_, status| {
                            ghost_btn_style(status, bg_hover, text_secondary, 10.0)
                        }),
                    );
                }
                action_row = action_row.push(space::horizontal()).push(
                    button(container(Icon::Trash.view(13.5, text_secondary)).center(Fill))
                        .on_press(Message::DeleteSchedule(s.id.clone()))
                        .padding(0)
                        .width(Length::Fixed(28.0))
                        .height(Length::Fixed(28.0))
                        .style(move |_, status| {
                            let h = matches!(status, button::Status::Hovered);
                            ghost_btn_style(status, bg_hover, if h { danger } else { text_secondary }, 8.0)
                        }),
                );

                list_col = list_col.push(
                    container(
                        column![
                            row![
                                Icon::Clock.view(16.0, accent),
                                text(&s.title)
                                    .size(14.5)
                                    .font(font_semibold())
                                    .color(text_strong),
                                space::horizontal(),
                                text(schedule_label).size(12).color(text_secondary),
                                container(text(badge_txt).size(11).font(font_medium()).color(badge_col))
                                    .padding([2, 8])
                                    .style(move |_| container::Style {
                                        background: Some(Background::Color(bg_chip)),
                                        border: Border {
                                            radius: 8.0.into(),
                                            ..Border::default()
                                        },
                                        ..container::Style::default()
                                    }),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                            text(truncate_single_line(&s.prompt, 120))
                                .size(12.5)
                                .color(text_secondary)
                                .wrapping(text::Wrapping::WordOrGlyph),
                            action_row,
                        ]
                        .spacing(8),
                    )
                    .padding(14)
                    .width(Fill)
                    .style(move |_| card_style(bg_composer, border_subtle)),
                );
            }
        }

        // 서버 추천 예약 섹션
        let mut rec_col = column![
            text(t(loc, "추천 예약"))
                .size(15.5)
                .font(font_semibold())
                .color(text_strong),
        ]
        .spacing(8);

        if self.recommended_schedules.is_empty() {
            rec_col = rec_col.push(
                text(t(loc, "서버 추천 예약 목록을 불러오는 중입니다."))
                    .size(12.5)
                    .color(text_muted),
            );
        } else {
            for rec in &self.recommended_schedules {
                let raw_desc = if !rec.description.is_empty() {
                    rec.description.as_str()
                } else {
                    rec.prompt.as_str()
                };
                let (rec_title, rec_desc) =
                    localize_recommended_schedule(loc, &rec.title, raw_desc);
                rec_col = rec_col.push(
                    container(
                        row![
                            column![
                                text(rec_title)
                                    .size(13.5)
                                    .font(font_semibold())
                                    .color(text_strong),
                                text(truncate_single_line(rec_desc, 96))
                                    .size(12)
                                    .color(text_secondary)
                                    .wrapping(text::Wrapping::WordOrGlyph),
                            ]
                            .spacing(3)
                            .width(Fill),
                            button(
                                row![
                                    Icon::Plus.view(13.0, accent),
                                    text(t(loc, "추가")).size(12).font(font_medium()).color(accent),
                                ]
                                .spacing(4)
                                .align_y(Alignment::Center),
                            )
                            .on_press(Message::AddRecommendedSchedule(
                                rec.id.clone(),
                                rec.title.clone(),
                                rec.description.clone(),
                                rec.prompt.clone(),
                            ))
                            .padding([6, 12])
                            .style(move |_, status| {
                                let h = matches!(status, button::Status::Hovered);
                                button::Style {
                                    background: Some(Background::Color(if h {
                                        bg_hover
                                    } else {
                                        accent_soft
                                    })),
                                    text_color: accent,
                                    border: Border {
                                        radius: 12.0.into(),
                                        ..Border::default()
                                    },
                                    ..button::Style::default()
                                }
                            }),
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    )
                    .padding([12, 14])
                    .width(Fill)
                    .style(move |_| card_style(bg_composer, border_subtle)),
                );
            }
        }

        self.wrap_section_page(
            column![header, create_card, filter_row, list_col, rec_col].spacing(16),
        )
    }

    /// 학생 허브 뷰: 학습 도구 카드 4종(FeatureMode 연동) + 학습 노트북 섹션.
    pub(crate) fn view_student(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_composer = pal.bg_composer;
        let bg_hover = pal.bg_hover;
        let accent = pal.accent;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;

        let header = column![
            row![
                Icon::School.view(24.0, accent),
                text(t(loc, "학습 도우미"))
                    .size(26)
                    .font(font_semibold())
                    .color(text_strong),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            text(t(
                loc,
                "강의 계획서 정리, 몰입형 탐색, 확인 퀴즈, 플래시카드로 깊이 있게 학습하세요.",
            ))
            .size(13)
            .color(text_secondary),
        ]
        .spacing(4);

        let cards: [(Icon, &'static str, &'static str, i32, &'static str); 4] = [
            (
                Icon::Notebook,
                "강의 계획서로 학습 노트북 만들기",
                "수업 일정과 핵심 개념을 주차별 학습 계획으로 구조화합니다.",
                FEATURE_MODE_GUIDED_LEARNING,
                "강의 계획서를 바탕으로 주차별 학습 로드맵과 핵심 개념 요약표를 만들어 주세요.",
            ),
            (
                Icon::Book,
                "몰입형 탐색",
                "단계별 질문과 시각적 비유로 어려운 개념을 끝까지 이해합니다.",
                FEATURE_MODE_GUIDED_LEARNING,
                "새로운 주제를 단계별로 질문하며 함께 탐구하는 가이드 학습을 시작해 주세요.",
            ),
            (
                Icon::Check,
                "확인 퀴즈",
                "주제별 객관식·서술형 문제를 풀고 즉각적인 해설을 받습니다.",
                FEATURE_MODE_QUIZ,
                "이해도를 점검할 수 있는 확인 퀴즈 5문항을 난이도 순으로 출제해 주세요.",
            ),
            (
                Icon::Canvas,
                "플래시카드 만들기",
                "핵심 용어와 개념을 암기용 앞뒤 플래시카드로 자동 정리합니다.",
                FEATURE_MODE_FLASHCARDS,
                "핵심 용어와 정의를 빠르게 암기할 수 있는 플래시카드 세트를 만들어 주세요.",
            ),
        ];

        let mut grid = column![].spacing(12);
        for pair in cards.chunks(2) {
            let mut r = row![].spacing(12).width(Fill);
            for (icon, title, desc, fmode, prompt) in pair {
                r = r.push(
                    button(
                        column![
                            container(icon.view(18.0, accent))
                                .width(Length::Fixed(36.0))
                                .height(Length::Fixed(36.0))
                                .align_x(Alignment::Center)
                                .align_y(Alignment::Center)
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(accent_soft)),
                                    border: Border {
                                        radius: 10.0.into(),
                                        ..Border::default()
                                    },
                                    ..container::Style::default()
                                }),
                            text(t(loc, title))
                                .size(14.5)
                                .font(font_semibold())
                                .color(text_strong),
                            text(t(loc, desc))
                                .size(12.5)
                                .color(text_secondary)
                                .wrapping(text::Wrapping::WordOrGlyph),
                        ]
                        .spacing(8),
                    )
                    .on_press(Message::StartFeaturePrompt(*fmode, t(loc, prompt).into()))
                    .padding(16)
                    .width(Fill)
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
                                radius: 16.0.into(),
                                width: 1.0,
                                color: border_subtle,
                            },
                            ..button::Style::default()
                        }
                    }),
                );
            }
            grid = grid.push(r);
        }

        // 학습 노트북 섹션
        let mut nb_row = row![
            button(
                column![
                    Icon::Plus.view(18.0, accent),
                    text(t(loc, "새 학습 노트북"))
                        .size(13)
                        .font(font_medium())
                        .color(accent),
                ]
                .spacing(6)
                .align_x(Alignment::Center),
            )
            .on_press(Message::NewNotebook)
            .padding(16)
            .width(Length::Fixed(150.0))
            .style(move |_, status| {
                let h = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if h { bg_hover } else { accent_soft })),
                    text_color: accent,
                    border: Border {
                        radius: 14.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }
            }),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        for nb in self.notebooks.iter().take(3) {
            let cnt = nb.source_count.max(nb.sources.len());
            let src_label = if loc.is_en() {
                if cnt == 1 {
                    format!("{cnt} source")
                } else {
                    format!("{cnt} sources")
                }
            } else {
                format!("출처 {cnt}개")
            };
            nb_row = nb_row.push(
                button(
                    column![
                        Icon::Notebook.view(16.0, text_secondary),
                        text(truncate_single_line(&nb.title, 18))
                            .size(13)
                            .font(font_medium())
                            .color(text_strong),
                        text(src_label).size(11.5).color(text_secondary),
                    ]
                    .spacing(4),
                )
                .on_press(Message::OpenNotebook(nb.id.clone()))
                .padding(14)
                .width(Fill)
                .style(move |_, status| {
                    let h = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: Some(Background::Color(if h { bg_hover } else { bg_composer })),
                        text_color: text_primary,
                        border: Border {
                            radius: 14.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..button::Style::default()
                    }
                }),
            );
        }

        let nb_section = column![
            row![
                text(t(loc, "학습 노트북"))
                    .size(15.5)
                    .font(font_semibold())
                    .color(text_strong),
                space::horizontal(),
                button(text(t(loc, "모든 노트북 보기")).size(12.5).color(accent))
                    .on_press(Message::SwitchSection(ActiveSection::Notebooks))
                    .padding([4, 10])
                    .style(move |_, status| ghost_btn_style(status, accent_soft, accent, 10.0)),
            ]
            .align_y(Alignment::Center),
            nb_row,
        ]
        .spacing(10);

        self.wrap_section_page(column![header, grid, nb_section].spacing(20))
    }

    /// 노트북 뷰: 서버+로컬 3열 카드 그리드 + 선택 노트북 상세 편집기.
    pub(crate) fn view_notebooks(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_composer = pal.bg_composer;
        let bg_hover = pal.bg_hover;
        let bg_chip = pal.bg_chip;
        let accent = pal.accent;
        let accent_fg = pal.accent_fg;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;
        let danger = pal.danger;

        let header = row![
            column![
                text(format!("{} ({})", t(loc, "노트북"), self.notebooks.len()))
                    .size(26)
                    .font(font_semibold())
                    .color(text_strong),
                text(t(
                    loc,
                    "서버 Gemini 노트북과 로컬 소스·메모 노트북을 한곳에서 관리합니다.",
                ))
                .size(13)
                .color(text_secondary),
            ]
            .spacing(4),
            space::horizontal(),
            button(
                row![
                    Icon::Plus.view(14.0, accent_fg),
                    text(t(loc, "새 노트북"))
                        .size(13)
                        .font(font_medium())
                        .color(accent_fg),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::NewNotebook)
            .padding([8, 15])
            .style(move |_, _| button::Style {
                background: Some(Background::Color(accent)),
                text_color: accent_fg,
                border: Border {
                    radius: 16.0.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
        ]
        .align_y(Alignment::Center);

        // 3열 카드 그리드
        let mut grid = column![].spacing(10);
        for chunk in self.notebooks.chunks(3) {
            let mut r = row![].spacing(10).width(Fill);
            for nb in chunk {
                let selected = !self.editing_notebook.id.is_empty()
                    && self.editing_notebook.id == nb.id;
                let cnt = nb.source_count.max(nb.sources.len());
                let badge = if nb.is_remote {
                    t(loc, "서버")
                } else {
                    t(loc, "로컬")
                };
                let meta_label = if loc.is_en() {
                    let c_cnt = nb.conversation_ids.len();
                    let s_word = if cnt == 1 { "source" } else { "sources" };
                    let c_word = if c_cnt == 1 { "chat" } else { "chats" };
                    format!("{cnt} {s_word} · {c_cnt} {c_word}")
                } else {
                    format!("출처 {cnt}개 · 대화 {}개", nb.conversation_ids.len())
                };
                r = r.push(
                    button(
                        column![
                            row![
                                Icon::Notebook.view(16.0, if selected { accent } else { text_secondary }),
                                space::horizontal(),
                                container(text(badge).size(10.5).color(text_secondary))
                                    .padding([2, 7])
                                    .style(move |_| container::Style {
                                        background: Some(Background::Color(bg_chip)),
                                        border: Border {
                                            radius: 8.0.into(),
                                            ..Border::default()
                                        },
                                        ..container::Style::default()
                                    }),
                            ]
                            .align_y(Alignment::Center),
                            text(truncate_single_line(&nb.title, 22))
                                .size(13.5)
                                .font(font_semibold())
                                .color(text_strong),
                            text(meta_label).size(11.5).color(text_secondary),
                        ]
                        .spacing(6),
                    )
                    .on_press(Message::OpenNotebook(nb.id.clone()))
                    .padding(14)
                    .width(Fill)
                    .style(move |_, status| {
                        let h = matches!(status, button::Status::Hovered);
                        button::Style {
                            background: Some(Background::Color(if selected {
                                accent_soft
                            } else if h {
                                bg_hover
                            } else {
                                bg_composer
                            })),
                            text_color: text_primary,
                            border: Border {
                                radius: 14.0.into(),
                                width: if selected { 1.5 } else { 1.0 },
                                color: if selected { accent } else { border_subtle },
                            },
                            ..button::Style::default()
                        }
                    }),
                );
            }
            for _ in chunk.len()..3 {
                r = r.push(space::horizontal().width(Fill));
            }
            grid = grid.push(r);
        }

        // 선택된 노트북 상세 편집 패널
        let title_in = text_input(t(loc, "노트북 제목"), &self.editing_notebook.title)
            .on_input(Message::NotebookTitleChanged)
            .size(14.5)
            .padding([9, 12])
            .style(move |_, _| text_input::Style {
                background: Background::Color(bg_chip),
                border: Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                icon: text_muted,
                placeholder: text_muted,
                value: text_primary,
                selection: accent_soft,
            });
        let desc_in = text_input(t(loc, "노트북 설명 (선택)"), &self.editing_notebook.description)
            .on_input(Message::NotebookDescChanged)
            .size(13)
            .padding([8, 12])
            .style(move |_, _| text_input::Style {
                background: Background::Color(bg_chip),
                border: Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                icon: text_muted,
                placeholder: text_muted,
                value: text_primary,
                selection: accent_soft,
            });

        let sources_header = if loc.is_en() {
            format!(
                "Source files ({} cloud / {} local)",
                self.editing_notebook.source_count,
                self.editing_notebook.sources.len()
            )
        } else {
            format!(
                "소스 파일 (서버 {}개 / 로컬 {}개)",
                self.editing_notebook.source_count,
                self.editing_notebook.sources.len()
            )
        };
        let mut sources_col = column![
            row![
                text(sources_header)
                    .size(12.5)
                    .font(font_medium())
                    .color(text_secondary),
                space::horizontal(),
                button(
                    row![
                        Icon::Plus.view(13.0, accent),
                        text(t(loc, "로컬 소스 추가")).size(12).color(accent),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::AddNotebookSource)
                .padding([4, 10])
                .style(move |_, status| ghost_btn_style(status, accent_soft, accent, 10.0)),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(6);

        for (i, src) in self.editing_notebook.sources.iter().enumerate() {
            sources_col = sources_col.push(
                row![
                    Icon::AttachFile.view(13.0, text_secondary),
                    text(truncate_single_line(src, 60))
                        .size(12.5)
                        .color(text_primary)
                        .width(Fill),
                    button(container(Icon::Close.view(12.0, text_secondary)).center(Fill))
                        .on_press(Message::RemoveNotebookSource(i))
                        .padding(0)
                        .width(Length::Fixed(22.0))
                        .height(Length::Fixed(22.0))
                        .style(move |_, status| ghost_btn_style(status, bg_hover, text_secondary, 8.0)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }

        let notes_ed = text_editor(&self.notebook_notes_editor)
            .placeholder(t(loc, "노트북 메모나 참고 컨텍스트를 적어 두세요..."))
            .on_action(Message::NotebookNotesAction)
            .height(Length::Fixed(110.0))
            .size(13.5)
            .padding(10)
            .style(move |_, _| text_editor::Style {
                background: Background::Color(bg_chip),
                border: Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                placeholder: text_muted,
                value: text_primary,
                selection: accent_soft,
            });

        let mut footer_btns = row![
            button(
                text(t(loc, "저장"))
                    .size(13)
                    .font(font_medium())
                    .color(accent_fg),
            )
            .on_press(Message::SaveNotebook)
            .padding([8, 16])
            .style(move |_, _| button::Style {
                background: Some(Background::Color(accent)),
                text_color: accent_fg,
                border: Border {
                    radius: 12.0.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
            button(
                row![
                    Icon::Sparkle.view(14.0, accent),
                    text(t(loc, "이 노트북으로 채팅"))
                        .size(13)
                        .font(font_medium())
                        .color(accent),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::ChatWithNotebook)
            .padding([8, 14])
            .style(move |_, status| {
                let h = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if h { bg_hover } else { accent_soft })),
                    text_color: accent,
                    border: Border {
                        radius: 12.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }
            }),
            space::horizontal(),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        if !self.editing_notebook.id.is_empty() && !self.editing_notebook.is_remote {
            footer_btns = footer_btns.push(
                button(text(t(loc, "삭제")).size(12.5).color(danger))
                    .on_press(Message::DeleteNotebook(self.editing_notebook.id.clone()))
                    .padding([7, 12])
                    .style(move |_, status| ghost_btn_style(status, bg_hover, danger, 10.0)),
            );
        }

        let editor_card = container(
            column![
                text(t(loc, "노트북 상세 / 편집"))
                    .size(14.5)
                    .font(font_semibold())
                    .color(text_strong),
                title_in,
                desc_in,
                sources_col,
                notes_ed,
                footer_btns,
            ]
            .spacing(12),
        )
        .padding(16)
        .width(Fill)
        .style(move |_| card_style(bg_composer, border_subtle));

        self.wrap_section_page(column![header, grid, editor_card].spacing(18))
    }

    /// Gems 허브 페이지 (`더보기 -> Gems`): 내 Gem + 공식 Gem 카드 그리드.
    pub(crate) fn view_gems_hub(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_composer = pal.bg_composer;
        let bg_hover = pal.bg_hover;
        let bg_chip = pal.bg_chip;
        let accent = pal.accent;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;

        let header = column![
            row![
                Icon::Gem.view(24.0, accent),
                text("Gems")
                    .size(26)
                    .font(font_semibold())
                    .color(text_strong),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            text(t(
                loc,
                "특정 목적에 맞춰 미리 구성된 맞춤형 Gemini 어시스턴트를 선택하세요.",
            ))
            .size(13)
            .color(text_secondary),
        ]
        .spacing(4);

        let mut grid = column![].spacing(12);
        if self.bots.is_empty() {
            grid = grid.push(
                container(
                    text(t(loc, "사용 가능한 Gem 목록을 불러오는 중입니다..."))
                        .size(13.5)
                        .color(text_secondary),
                )
                .padding(24)
                .width(Fill)
                .style(move |_| card_style(bg_composer, border_subtle)),
            );
        } else {
            for pair in self.bots.chunks(2) {
                let mut r = row![].spacing(12).width(Fill);
                for bot in pair {
                    let tag = if bot.is_custom {
                        t(loc, "내 Gem")
                    } else {
                        t(loc, "Google 공식")
                    };
                    r = r.push(
                        button(
                            column![
                                row![
                                    container(Icon::Gem.view(17.0, accent))
                                        .width(Length::Fixed(34.0))
                                        .height(Length::Fixed(34.0))
                                        .align_x(Alignment::Center)
                                        .align_y(Alignment::Center)
                                        .style(move |_| container::Style {
                                            background: Some(Background::Color(accent_soft)),
                                            border: Border {
                                                radius: 10.0.into(),
                                                ..Border::default()
                                            },
                                            ..container::Style::default()
                                        }),
                                    space::horizontal(),
                                    container(text(tag).size(11).color(text_secondary))
                                        .padding([2, 8])
                                        .style(move |_| container::Style {
                                            background: Some(Background::Color(bg_chip)),
                                            border: Border {
                                                radius: 8.0.into(),
                                                ..Border::default()
                                            },
                                            ..container::Style::default()
                                        }),
                                ]
                                .align_y(Alignment::Center),
                                text(&bot.name)
                                    .size(15)
                                    .font(font_semibold())
                                    .color(text_strong),
                                text(truncate_single_line(&bot.description, 88))
                                    .size(12.5)
                                    .color(text_secondary)
                                    .wrapping(text::Wrapping::WordOrGlyph),
                            ]
                            .spacing(8),
                        )
                        .on_press(Message::SwitchSection(ActiveSection::Gem(bot.id.clone())))
                        .padding(16)
                        .width(Fill)
                        .style(move |_, status| {
                            let h = matches!(status, button::Status::Hovered);
                            button::Style {
                                background: Some(Background::Color(if h {
                                    bg_hover
                                } else {
                                    bg_composer
                                })),
                                text_color: text_primary,
                                border: Border {
                                    radius: 16.0.into(),
                                    width: 1.0,
                                    color: border_subtle,
                                },
                                ..button::Style::default()
                            }
                        }),
                    );
                }
                if pair.len() == 1 {
                    r = r.push(space::horizontal().width(Fill));
                }
                grid = grid.push(r);
            }
        }

        self.wrap_section_page(column![header, grid].spacing(18))
    }

    /// 개별 Gem 랜딩 페이지.
    pub(crate) fn view_gem_landing(&self, bot_id: &str) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_composer = pal.bg_composer;
        let bg_hover = pal.bg_hover;
        let accent = pal.accent;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;

        let Some(bot) = self.find_bot(bot_id) else {
            return self.wrap_section_page(column![
                text(t(loc, "선택한 Gem을 찾을 수 없습니다."))
                    .size(15)
                    .color(text_secondary),
            ]);
        };

        let header = row![
            column![
                row![
                    Icon::Gem.view(22.0, accent),
                    text(&bot.name)
                        .size(26)
                        .font(font_semibold())
                        .color(text_strong),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                text(&bot.description).size(13.5).color(text_secondary),
            ]
            .spacing(6),
            space::horizontal(),
            button(text(t(loc, "Gems 목록")).size(12.5).color(text_secondary))
                .on_press(Message::SwitchSection(ActiveSection::GemsHub))
                .padding([6, 12])
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 10.0)),
        ]
        .align_y(Alignment::Center);

        let mut starters_col = column![].spacing(10);
        let list: Vec<String> = if bot.starters.is_empty() {
            vec![if loc.is_en() {
                format!("Show me examples of how you can help me as {}.", bot.name)
            } else {
                format!(
                    "{} 역할로 어떤 도움을 받을 수 있는지 예시를 알려 주세요.",
                    bot.name
                )
            }]
        } else {
            bot.starters.clone()
        };
        for st in list {
            let prompt_clone = st.clone();
            starters_col = starters_col.push(
                button(
                    row![
                        Icon::Sparkle.view(15.0, accent),
                        text(st)
                            .size(13.5)
                            .color(text_primary)
                            .wrapping(text::Wrapping::WordOrGlyph),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::StartBotPrompt(bot.id.clone(), prompt_clone))
                .padding([13, 16])
                .width(Fill)
                .style(move |_, status| {
                    let h = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: Some(Background::Color(if h { bg_hover } else { bg_composer })),
                        text_color: text_primary,
                        border: Border {
                            radius: 14.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..button::Style::default()
                    }
                }),
            );
        }

        let start_chat_btn = button(
            row![
                Icon::NewChat.view(14.0, accent),
                text(t(loc, "이 Gem으로 자유 대화 시작"))
                    .size(13)
                    .font(font_medium())
                    .color(accent),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .on_press(Message::SwitchSection(ActiveSection::Chat))
        .padding([9, 15])
        .style(move |_, status| {
            let h = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if h { bg_hover } else { accent_soft })),
                text_color: accent,
                border: Border {
                    radius: 14.0.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        });

        self.wrap_section_page(column![header, starters_col, start_chat_btn].spacing(18))
    }

    /// 동영상 스튜디오 페이지 (`더보기 -> 동영상`): Veo 동영상 생성 전용 화면.
    pub(crate) fn view_videos(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_composer = pal.bg_composer;
        let bg_hover = pal.bg_hover;
        let bg_chip = pal.bg_chip;
        let accent = pal.accent;
        let accent_fg = pal.accent_fg;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;

        let header = column![
            row![
                Icon::Video.view(24.0, accent),
                text(t(loc, "동영상 만들기"))
                    .size(26)
                    .font(font_semibold())
                    .color(text_strong),
                container(text("Veo").size(11.5).font(font_medium()).color(accent))
                    .padding([3, 9])
                    .style(move |_| container::Style {
                        background: Some(Background::Color(accent_soft)),
                        border: Border {
                            radius: 10.0.into(),
                            ..Border::default()
                        },
                        ..container::Style::default()
                    }),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            text(t(
                loc,
                "장면·카메라 움직임·분위기를 설명하면 짧은 고화질 동영상을 생성합니다.",
            ))
            .size(13)
            .color(text_secondary),
        ]
        .spacing(4);

        let prompt_in = text_input(
            t(
                loc,
                "생성할 동영상 장면을 설명하세요 (예: 노을 지는 바닷가를 드론으로 천천히 비행하는 장면)",
            ),
            &self.video_prompt_draft,
        )
        .on_input(Message::VideoPromptChanged)
        .on_submit(Message::GenerateVideoPrompt)
        .size(14)
        .padding([12, 14])
        .style(move |_, _| text_input::Style {
            background: Background::Color(bg_chip),
            border: Border {
                radius: 14.0.into(),
                width: 1.0,
                color: border_subtle,
            },
            icon: text_muted,
            placeholder: text_muted,
            value: text_primary,
            selection: accent_soft,
        });

        let chip = |label: &'static str, active: bool, msg: Message| {
            button(
                text(label)
                    .size(12)
                    .color(if active { accent } else { text_primary }),
            )
            .on_press(msg)
            .padding([5, 11])
            .style(move |_, status| {
                let h = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if active {
                        accent_soft
                    } else if h {
                        bg_hover
                    } else {
                        bg_chip
                    })),
                    text_color: text_primary,
                    border: Border {
                        radius: 14.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }
            })
        };

        let controls = row![
            text(t(loc, "비율:")).size(12).color(text_secondary),
            chip("16:9", self.video_aspect == "16:9", Message::SetVideoAspect("16:9".into())),
            chip("9:16", self.video_aspect == "9:16", Message::SetVideoAspect("9:16".into())),
            text(t(loc, "길이:")).size(12).color(text_secondary),
            chip(
                t(loc, "5초"),
                self.video_duration == "5초",
                Message::SetVideoDuration("5초".into()),
            ),
            chip(
                t(loc, "8초"),
                self.video_duration == "8초",
                Message::SetVideoDuration("8초".into()),
            ),
            space::horizontal(),
            button(
                row![
                    Icon::Video.view(14.0, accent_fg),
                    text(t(loc, "동영상 생성"))
                        .size(13)
                        .font(font_medium())
                        .color(accent_fg),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::GenerateVideoPrompt)
            .padding([8, 16])
            .style(move |_, _| button::Style {
                background: Some(Background::Color(accent)),
                text_color: accent_fg,
                border: Border {
                    radius: 16.0.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let studio = container(column![prompt_in, controls].spacing(12))
            .padding(16)
            .width(Fill)
            .style(move |_| card_style(bg_composer, border_subtle));

        let presets = [
            "새벽 안개가 피어오르는 한옥 마을을 천천히 걷는 1인칭 시점 시네마틱 영상",
            "우주 정거장 창밖으로 지구의 푸른 지평선과 오로라가 펼쳐지는 타임랩스",
            "아늑한 카페 테이블 위 따뜻한 라떼 아트가 완성되는 클로즈업 슬로우 모션",
        ];
        let mut preset_col = column![
            text(t(loc, "추천 프롬프트"))
                .size(15)
                .font(font_semibold())
                .color(text_strong),
        ]
        .spacing(8);
        for p in presets {
            let label = t(loc, p);
            preset_col = preset_col.push(
                button(
                    row![
                        Icon::Sparkle.view(14.0, accent),
                        text(label).size(13).color(text_primary),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::StartFeaturePrompt(FEATURE_MODE_VIDEO, label.into()))
                .padding([11, 14])
                .width(Fill)
                .style(move |_, status| {
                    let h = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: Some(Background::Color(if h { bg_hover } else { bg_composer })),
                        text_color: text_primary,
                        border: Border {
                            radius: 14.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..button::Style::default()
                    }
                }),
            );
        }

        self.wrap_section_page(column![header, studio, preset_col].spacing(18))
    }

    /// 맞춤설정 페이지 (`더보기 -> 맞춤설정`): 개인 인텔리전스 토글 + 사용자 맞춤 지침 저장.
    pub(crate) fn view_personalize(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_composer = pal.bg_composer;
        let bg_hover = pal.bg_hover;
        let bg_chip = pal.bg_chip;
        let accent = pal.accent;
        let accent_fg = pal.accent_fg;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;

        let header = column![
            row![
                Icon::Tune.view(24.0, accent),
                text(t(loc, "맞춤설정"))
                    .size(26)
                    .font(font_semibold())
                    .color(text_strong),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            text(t(
                loc,
                "Gemini가 나를 이해하고 원하는 어조와 형식으로 응답하도록 설정합니다.",
            ))
            .size(13)
            .color(text_secondary),
        ]
        .spacing(4);

        let pi_on = self.personal_intelligence;
        let pi_card = container(
            row![
                column![
                    row![
                        text(t(loc, "개인 인텔리전스"))
                            .size(15)
                            .font(font_semibold())
                            .color(text_strong),
                        container(text("Labs").size(11).font(font_medium()).color(accent))
                            .padding([2, 8])
                            .style(move |_| container::Style {
                                background: Some(Background::Color(accent_soft)),
                                border: Border {
                                    radius: 8.0.into(),
                                    ..Border::default()
                                },
                                ..container::Style::default()
                            }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text(t(
                        loc,
                        "이전 대화 맥락과 선호 사항을 반영해 더 개인화된 답변을 제공합니다.",
                    ))
                    .size(12.5)
                    .color(text_secondary),
                ]
                .spacing(4)
                .width(Fill),
                button(
                    text(if pi_on {
                        t(loc, "켜짐")
                    } else {
                        t(loc, "꺼짐")
                    })
                    .size(12.5)
                    .font(font_medium())
                    .color(if pi_on { accent_fg } else { text_primary }),
                )
                .on_press(Message::TogglePersonalIntelligence)
                .padding([7, 16])
                .style(move |_, status| {
                    let h = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: Some(Background::Color(if pi_on {
                            accent
                        } else if h {
                            bg_hover
                        } else {
                            bg_chip
                        })),
                        text_color: if pi_on { accent_fg } else { text_primary },
                        border: Border {
                            radius: 14.0.into(),
                            width: if pi_on { 0.0 } else { 1.0 },
                            color: border_subtle,
                        },
                        ..button::Style::default()
                    }
                }),
            ]
            .spacing(14)
            .align_y(Alignment::Center),
        )
        .padding(16)
        .width(Fill)
        .style(move |_| card_style(bg_composer, border_subtle));

        let inst_input = text_input(
            t(
                loc,
                "Gemini가 기억해야 할 선호·응답 스타일을 입력하세요 (예: 항상 간결한 한국어 불릿으로 답변)",
            ),
            &self.custom_instructions_draft,
        )
        .on_input(Message::CustomInstructionsChanged)
        .on_submit(Message::SaveCustomInstructions)
        .size(13.5)
        .padding([11, 14])
        .style(move |_, _| text_input::Style {
            background: Background::Color(bg_chip),
            border: Border {
                radius: 12.0.into(),
                width: 1.0,
                color: border_subtle,
            },
            icon: text_muted,
            placeholder: text_muted,
            value: text_primary,
            selection: accent_soft,
        });

        let inst_card = container(
            column![
                text(t(loc, "Gemini를 위한 저장된 지침"))
                    .size(15)
                    .font(font_semibold())
                    .color(text_strong),
                text(t(
                    loc,
                    "입력한 지침은 로컬 설정에 영속 저장되며 `@user-instructions` 멘션으로도 활용할 수 있습니다.",
                ))
                .size(12.5)
                .color(text_secondary),
                inst_input,
                row![
                    space::horizontal(),
                    button(
                        text(t(loc, "지침 저장"))
                            .size(13)
                            .font(font_medium())
                            .color(accent_fg),
                    )
                    .on_press(Message::SaveCustomInstructions)
                    .padding([8, 16])
                    .style(move |_, _| button::Style {
                        background: Some(Background::Color(accent)),
                        text_color: accent_fg,
                        border: Border {
                            radius: 12.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }),
                ],
            ]
            .spacing(10),
        )
        .padding(16)
        .width(Fill)
        .style(move |_| card_style(bg_composer, border_subtle));

        self.wrap_section_page(column![header, pi_card, inst_card].spacing(18))
    }
}

/// 서버에서 영문 기본 템플릿으로 내려오는 추천 예약 5종을 한국어 로케일일 때 원본 앱과 동일한 한국어 문구로 현지화한다.
fn localize_recommended_schedule<'a>(
    locale: Locale,
    title: &'a str,
    desc: &'a str,
) -> (&'a str, &'a str) {
    if locale.is_en() {
        return (title, desc);
    }
    match title.trim() {
        "Organize my subscriptions" => (
            "내 구독 관리",
            "매주 Gmail을 확인하고 해지할 만한 구독 서비스를 추천해 줘.",
        ),
        "Recommend wellness tips" => (
            "웰니스 팁 추천하기",
            "최신 건강 연구를 기반으로 한 주간 웰니스 관련 정보를 요약해 줘.",
        ),
        "Recommend local events" => (
            "지역 행사 추천하기",
            "내 관심분야에 맞는 지역 행사를 찾아서 내 일정에 추가해 줘.",
        ),
        "Draft email responses" => (
            "이메일 답장 초안 작성하기",
            "받은편지함에서 새 이메일을 확인하고 요약한 후 답장 초안을 작성해 줘.",
        ),
        "Create a work progress report" => (
            "업무 진행 상황 보고서 만들기",
            "내 Calendar 및 Gmail 활동을 바탕으로 일일 요약을 작성해 줘.",
        ),
        _ => (title, desc),
    }
}
