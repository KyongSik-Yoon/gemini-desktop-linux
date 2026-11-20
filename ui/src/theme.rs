use iced::border;
use iced::font::{Family, Weight};
use iced::theme::Palette;
use iced::widget::{markdown, scrollable};
use iced::{Background, Border, Color, Font, Padding, Theme};
use std::collections::HashMap;
use std::path::PathBuf;

pub fn font_regular() -> Font {
    Font::with_name("Noto Sans CJK KR")
}

pub fn font_medium() -> Font {
    Font {
        family: Family::Name("Noto Sans CJK KR"),
        weight: Weight::Medium,
        ..Font::DEFAULT
    }
}

pub fn font_semibold() -> Font {
    Font {
        family: Family::Name("Noto Sans CJK KR"),
        weight: Weight::Semibold,
        ..Font::DEFAULT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "시스템",
            Self::Light => "라이트",
            Self::Dark => "다크",
        }
    }

    pub fn label_for(self, locale: crate::i18n::Locale) -> &'static str {
        crate::i18n::t(locale, self.label())
    }
}

/// 시스템(Omarchy `colors.toml` 또는 XDG 다크/라이트 모드)에서 유도한 앱 시맨틱 컬러 토큰.
/// 원본 Gemini 데스크톱 앱과 동일하게 라이트 모드에서는 깨끗한 사이드바와 은은한 블루 틴트 캔버스·중앙 글로우를 구성한다.
#[derive(Debug, Clone, PartialEq)]
pub struct AppTheme {
    pub is_dark: bool,
    pub theme_name: String,
    pub generation: String,

    pub bg_canvas: Color,
    pub bg_sidebar: Color,
    pub bg_composer: Color,
    pub bg_popover: Color,
    pub bg_user_bubble: Color,
    pub bg_hover: Color,
    pub bg_active: Color,
    pub bg_nav_active: Color,
    pub bg_thought: Color,
    pub bg_chip: Color,
    pub bg_segment: Color,
    pub bg_code: Color,
    pub bg_code_header: Color,
    pub bg_inline_code: Color,
    pub glow_color: Color,
    pub glow_task_color: Color,

    pub border: Color,
    pub border_subtle: Color,

    pub text_primary: Color,
    pub text_strong: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_inline_code: Color,

    pub accent: Color,
    pub accent_fg: Color,
    pub accent_soft: Color,
    pub danger: Color,
    pub warning: Color,
    pub success: Color,
}

impl AppTheme {
    /// 사용자 테마 모드(`system`/`light`/`dark`)에 따라 팔레트를 로드한다.
    pub fn load_with_mode(mode: ThemeMode, fallback_dark: bool) -> Self {
        match mode {
            ThemeMode::System => Self::load(fallback_dark),
            ThemeMode::Light => {
                let mut t = Self::fallback(false);
                t.theme_name = "라이트 (수동)".into();
                t
            }
            ThemeMode::Dark => {
                let mut t = Self::fallback(true);
                t.theme_name = "다크 (수동)".into();
                t
            }
        }
    }

    /// Omarchy 시스템 테마를 우선 로드하고, 없으면 시스템 다크/라이트 폴백 팔레트를 사용한다.
    pub fn load(fallback_dark: bool) -> Self {
        if let Some(omarchy) = Self::load_omarchy() {
            return omarchy;
        }
        Self::fallback(fallback_dark)
    }

    /// 현재 UI 로케일에 맞게 테마 표시 이름을 반환한다.
    pub fn display_name(&self, locale: crate::i18n::Locale) -> String {
        if !locale.is_en() {
            return self.theme_name.clone();
        }
        self.theme_name
            .replace("라이트 (수동)", "Light (Manual)")
            .replace("다크 (수동)", "Dark (Manual)")
            .replace("시스템 · 다크", "System · Dark")
            .replace("시스템 · 라이트", "System · Light")
            .replace(" · 다크", " · Dark")
            .replace(" · 라이트", " · Light")
    }

    /// Omarchy `theme.generation` 값만 빠르게 읽어 변경 여부를 감지한다.
    pub fn current_omarchy_generation() -> Option<String> {
        let dir = omarchy_current_dir()?;
        if let Ok(generation) = std::fs::read_to_string(dir.join("theme.generation")) {
            let trimmed = generation.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
        let meta = std::fs::metadata(dir.join("theme").join("colors.toml")).ok()?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Some(format!("{}-{}", meta.len(), mtime))
    }

    fn load_omarchy() -> Option<Self> {
        let dir = omarchy_current_dir()?;
        let colors_path = dir.join("theme").join("colors.toml");
        let raw = std::fs::read_to_string(colors_path).ok()?;
        let generation = Self::current_omarchy_generation().unwrap_or_default();
        let name = std::fs::read_to_string(dir.join("theme.name"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "omarchy".into());
        Self::from_omarchy_toml(&raw, &name, generation)
    }

    pub fn from_omarchy_toml(raw: &str, name: &str, generation: String) -> Option<Self> {
        let kv = parse_simple_toml(raw);

        let background = parse_hex(kv.get("background")?)?;
        let is_dark = match kv.get("mode").map(|s| s.as_str()) {
            Some("light") => false,
            Some("dark") => true,
            _ => luminance(background) < 0.5,
        };

        let foreground = kv
            .get("foreground")
            .and_then(|s| parse_hex(s))
            .unwrap_or_else(|| if is_dark { hex(0xe3e3e3) } else { hex(0x1f1f1f) });
        let bright_fg = kv
            .get("bright_foreground")
            .and_then(|s| parse_hex(s))
            .unwrap_or(foreground);
        let dark_fg = kv
            .get("dark_foreground")
            .and_then(|s| parse_hex(s))
            .unwrap_or_else(|| mix(foreground, background, 0.44));

        let dark_bg = kv
            .get("dark_background")
            .and_then(|s| parse_hex(s))
            .unwrap_or_else(|| {
                if is_dark {
                    mix(background, Color::BLACK, 0.25)
                } else {
                    mix(background, Color::BLACK, 0.05)
                }
            });
        let darker_bg = kv
            .get("darker_background")
            .and_then(|s| parse_hex(s))
            .unwrap_or_else(|| mix(dark_bg, Color::BLACK, 0.20));
        let lighter_bg = kv
            .get("lighter_background")
            .and_then(|s| parse_hex(s))
            .unwrap_or_else(|| {
                if is_dark {
                    mix(background, Color::WHITE, 0.08)
                } else {
                    mix(background, Color::WHITE, 0.78)
                }
            });

        let accent = kv
            .get("accent")
            .or_else(|| kv.get("blue"))
            .and_then(|s| parse_hex(s))
            .unwrap_or_else(|| if is_dark { hex(0x8ab4f8) } else { hex(0x1a73e8) });

        let danger = kv
            .get("red")
            .and_then(|s| parse_hex(s))
            .unwrap_or_else(|| if is_dark { hex(0xf28b82) } else { hex(0xd93025) });
        let warning = kv
            .get("orange")
            .or_else(|| kv.get("yellow"))
            .and_then(|s| parse_hex(s))
            .unwrap_or_else(|| if is_dark { hex(0xfdd663) } else { hex(0xe37400) });
        let success = kv
            .get("green")
            .and_then(|s| parse_hex(s))
            .unwrap_or_else(|| if is_dark { hex(0x81c995) } else { hex(0x188038) });

        // 원본 Gemini 데스크톱 톤 대조:
        // 라이트 모드에서는 사이드바가 순백(#FFFFFF), 메인 캔버스는 깨끗한 오프화이트(#FAFAFA 계열) + 중앙 파스텔 블루 글로우.
        // 다크 모드에서는 사이드바가 캔버스보다 한 단계 깊은 표면.
        let (bg_sidebar, bg_canvas) = if is_dark {
            (dark_bg, background)
        } else {
            (Color::WHITE, background)
        };

        let bg_composer = if is_dark {
            lighter_bg
        } else {
            Color::WHITE
        };
        let bg_popover = if is_dark {
            mix(lighter_bg, Color::WHITE, 0.05)
        } else {
            Color::WHITE
        };
        let bg_user_bubble = if is_dark {
            mix(lighter_bg, accent, 0.14)
        } else {
            mix(bg_canvas, accent, 0.09)
        };
        let bg_hover = if is_dark {
            mix(bg_sidebar, foreground, 0.08)
        } else {
            mix(bg_sidebar, foreground, 0.05)
        };
        let bg_nav_active = if is_dark {
            mix(bg_sidebar, foreground, 0.11)
        } else {
            mix(bg_sidebar, foreground, 0.065)
        };
        let bg_active = if is_dark {
            mix(bg_sidebar, accent, 0.24)
        } else {
            mix(bg_sidebar, accent, 0.13)
        };
        let bg_thought = if is_dark {
            mix(bg_canvas, lighter_bg, 0.65)
        } else {
            mix(bg_canvas, dark_bg, 0.50)
        };
        let bg_chip = if is_dark {
            lighter_bg
        } else {
            mix(bg_canvas, foreground, 0.045)
        };
        let bg_segment = if is_dark {
            Color::from_rgba(1.0, 1.0, 1.0, 0.08)
        } else {
            Color::from_rgba(0.16, 0.36, 0.68, 0.085)
        };

        let glow_color = if is_dark {
            mix(accent, hex(0x4a7de0), 0.40)
        } else {
            mix(accent, hex(0x8ec5fc), 0.72)
        };
        let glow_task_color = if is_dark {
            hex(0x6c5ce7)
        } else {
            hex(0xa29bfe)
        };

        let (bg_code, bg_code_header) = if is_dark {
            (darker_bg, dark_bg)
        } else {
            (hex(0x1d2028), hex(0x272b36))
        };

        let bg_inline_code = if is_dark {
            mix(bg_canvas, foreground, 0.12)
        } else {
            mix(bg_canvas, foreground, 0.07)
        };
        let border = if is_dark {
            mix(bg_canvas, foreground, 0.16)
        } else {
            mix(bg_canvas, foreground, 0.10)
        };
        let border_subtle = if is_dark {
            mix(bg_canvas, foreground, 0.09)
        } else {
            mix(bg_canvas, foreground, 0.06)
        };

        let accent_fg = if luminance(accent) > 0.55 {
            hex(0x111111)
        } else {
            Color::WHITE
        };
        let accent_soft = if is_dark {
            mix(bg_canvas, accent, 0.22)
        } else {
            mix(bg_canvas, accent, 0.12)
        };

        Some(Self {
            is_dark,
            theme_name: format!("{name} · {}", if is_dark { "다크" } else { "라이트" }),
            generation,
            bg_canvas,
            bg_sidebar,
            bg_composer,
            bg_popover,
            bg_user_bubble,
            bg_hover,
            bg_active,
            bg_nav_active,
            bg_thought,
            bg_chip,
            bg_segment,
            bg_code,
            bg_code_header,
            bg_inline_code,
            glow_color,
            glow_task_color,
            border,
            border_subtle,
            text_primary: foreground,
            text_strong: bright_fg,
            text_secondary: dark_fg,
            text_muted: mix(dark_fg, bg_canvas, 0.22),
            text_inline_code: bright_fg,
            accent,
            accent_fg,
            accent_soft,
            danger,
            warning,
            success,
        })
    }

    /// 비-Omarchy 환경용 공식 Gemini 라이트/다크 팔레트 폴백.
    pub fn fallback(is_dark: bool) -> Self {
        if is_dark {
            let bg = hex(0x131314);
            let surface = hex(0x1b1c1f);
            let accent = hex(0x8ab4f8);
            Self {
                is_dark: true,
                theme_name: "시스템 · 다크".into(),
                generation: String::new(),
                bg_canvas: bg,
                bg_sidebar: hex(0x18191c),
                bg_composer: hex(0x212327),
                bg_popover: hex(0x282a2e),
                bg_user_bubble: hex(0x282a2e),
                bg_hover: hex(0x27292e),
                bg_active: hex(0x1e334f),
                bg_nav_active: hex(0x2a2c32),
                bg_thought: surface,
                bg_chip: hex(0x24262b),
                bg_segment: Color::from_rgba(1.0, 1.0, 1.0, 0.08),
                bg_code: hex(0x16171a),
                bg_code_header: hex(0x222429),
                bg_inline_code: hex(0x282a2c),
                glow_color: hex(0x5d92f2),
                glow_task_color: hex(0x6c5ce7),
                border: hex(0x3a3d42),
                border_subtle: hex(0x2a2c30),
                text_primary: hex(0xe3e3e3),
                text_strong: hex(0xf8f9fa),
                text_secondary: hex(0x9aa0a6),
                text_muted: hex(0x7c8087),
                text_inline_code: hex(0xe8eaed),
                accent,
                accent_fg: hex(0x062e6f),
                accent_soft: hex(0x1e2c44),
                danger: hex(0xf28b82),
                warning: hex(0xfdd663),
                success: hex(0x81c995),
            }
        } else {
            let accent = hex(0x1a73e8);
            Self {
                is_dark: false,
                theme_name: "시스템 · 라이트".into(),
                generation: String::new(),
                bg_canvas: hex(0xfaf9f9),
                bg_sidebar: hex(0xffffff),
                bg_composer: hex(0xffffff),
                bg_popover: hex(0xffffff),
                bg_user_bubble: hex(0xe9eef6),
                bg_hover: hex(0xf2f4f8),
                bg_active: hex(0xe8f0fe),
                bg_nav_active: hex(0xf0f2f5),
                bg_thought: hex(0xf0f3f8),
                bg_chip: hex(0xf1f3f7),
                bg_segment: Color::from_rgba(0.16, 0.36, 0.68, 0.085),
                bg_code: hex(0x1d2028),
                bg_code_header: hex(0x272b36),
                bg_inline_code: hex(0xeef1f5),
                glow_color: hex(0x82bcfc),
                glow_task_color: hex(0xa29bfe),
                border: hex(0xe1e5ec),
                border_subtle: hex(0xedf0f5),
                text_primary: hex(0x1f1f1f),
                text_strong: hex(0x111111),
                text_secondary: hex(0x5f6368),
                text_muted: hex(0x80868b),
                text_inline_code: hex(0x1f1f1f),
                accent,
                accent_fg: Color::WHITE,
                accent_soft: hex(0xe8f0fe),
                danger: hex(0xd93025),
                warning: hex(0xe37400),
                success: hex(0x188038),
            }
        }
    }

    pub fn to_iced_theme(&self) -> Theme {
        Theme::custom(
            self.theme_name.clone(),
            Palette {
                background: self.bg_canvas,
                text: self.text_primary,
                primary: self.accent,
                success: self.success,
                warning: self.warning,
                danger: self.danger,
            },
        )
    }

    /// 투명 트랙과 얇고 둥근 호버 스크롤러를 사용하는 미니멀 스크롤바 스타일.
    pub fn subtle_scrollable_style(
        &self,
        status: scrollable::Status,
    ) -> scrollable::Style {
        let fg = self.text_primary;
        let alpha = match status {
            scrollable::Status::Active { .. } => 0.0,
            scrollable::Status::Hovered {
                is_vertical_scrollbar_hovered,
                is_horizontal_scrollbar_hovered,
                ..
            } => {
                if is_vertical_scrollbar_hovered || is_horizontal_scrollbar_hovered {
                    0.30
                } else {
                    0.14
                }
            }
            scrollable::Status::Dragged { .. } => 0.40,
        };

        let rail = scrollable::Rail {
            background: None,
            border: Border::default(),
            scroller: scrollable::Scroller {
                background: Background::Color(Color::from_rgba(fg.r, fg.g, fg.b, alpha)),
                border: Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
            },
        };

        scrollable::Style {
            container: iced::widget::container::Style::default(),
            vertical_rail: rail,
            horizontal_rail: rail,
            gap: None,
            auto_scroll: scrollable::AutoScroll {
                background: Background::Color(Color::TRANSPARENT),
                border: Border::default(),
                shadow: iced::Shadow::default(),
                icon: Color::TRANSPARENT,
            },
        }
    }

    pub fn markdown_settings(&self) -> markdown::Settings {
        let style = markdown::Style {
            font: font_regular(),
            inline_code_highlight: markdown::Highlight {
                background: self.bg_inline_code.into(),
                border: border::rounded(5),
            },
            inline_code_padding: Padding {
                top: 1.0,
                right: 5.0,
                bottom: 1.0,
                left: 5.0,
            },
            inline_code_color: self.text_inline_code,
            inline_code_font: Font::MONOSPACE,
            code_block_font: Font::MONOSPACE,
            link_color: self.accent,
        };
        let mut settings = markdown::Settings::with_text_size(15.0, style);
        settings.h1_size = 24.0.into();
        settings.h2_size = 20.0.into();
        settings.h3_size = 17.0.into();
        settings.h4_size = 16.0.into();
        settings.code_size = 13.0.into();
        settings.spacing = 12.0.into();
        settings
    }
}

fn omarchy_current_dir() -> Option<PathBuf> {
    if let Ok(state_home) = std::env::var("XDG_STATE_HOME")
        && !state_home.is_empty()
    {
        let p = PathBuf::from(state_home).join("omarchy").join("current");
        if p.exists() {
            return Some(p);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let p = PathBuf::from(home)
        .join(".local")
        .join("state")
        .join("omarchy")
        .join("current");
    if p.exists() { Some(p) } else { None }
}

/// 단순 key = "value" 형태의 TOML을 파싱한다. 따옴표 내부의 `#` 색상 코드는 보존한다.
fn parse_simple_toml(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim().to_string();
        let v = v.trim();
        let val = if let Some(rest) = v.strip_prefix('"') {
            rest.split('"').next().unwrap_or("").to_string()
        } else if let Some(rest) = v.strip_prefix('\'') {
            rest.split('\'').next().unwrap_or("").to_string()
        } else {
            v.split('#').next().unwrap_or("").trim().to_string()
        };
        if !key.is_empty() && !val.is_empty() {
            map.insert(key, val);
        }
    }
    map
}

fn parse_hex(s: &str) -> Option<Color> {
    let clean = s.trim().trim_start_matches('#');
    if clean.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(clean, 16).ok()?;
    Some(hex(v))
}

fn hex(v: u32) -> Color {
    let r = ((v >> 16) & 0xff) as f32 / 255.0;
    let g = ((v >> 8) & 0xff) as f32 / 255.0;
    let b = (v & 0xff) as f32 / 255.0;
    Color::from_rgb(r, g, b)
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::from_rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

fn luminance(c: Color) -> f32 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omarchy_lupine_colors_toml_parses_hex_correctly() {
        let sample = r##"
mode = "light"
accent = "#3264eb" # inline comment
background = "#fafafa"
dark_background = "#ececec"
foreground = "#212121"
"##;
        let theme = AppTheme::from_omarchy_toml(sample, "lupine", "gen1".into())
            .expect("Omarchy TOML 파싱 성공해야 함");
        assert!(!theme.is_dark);
        assert_eq!(theme.theme_name, "lupine · 라이트");
        // 라이트 모드에서는 사이드바가 캔버스보다 밝은 화이트 톤이어야 한다
        assert!(luminance(theme.bg_sidebar) > luminance(theme.bg_canvas));
        assert_eq!(theme.accent, hex(0x3264eb));
        assert_eq!(theme.text_primary, hex(0x212121));
    }
}
