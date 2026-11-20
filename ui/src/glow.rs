use iced::mouse;
use iced::widget::canvas::{self, Fill, Geometry, Path};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};

/// 원본 Gemini 데스크톱 환영 화면의 중앙 라디얼 블루 글로우 배경.
/// 저투명도 알파 누적 시 일부 GPU 파이프라인에서 화이트 클리핑이 발생하는 문제를 방지하기 위해,
/// `bg`에서 `color`로 가우시안 감쇠 보간된 불투명 타원 레이어를 바깥에서 안쪽으로 정밀하게 적층한다.
/// `canvas::Cache`를 사용해 크기·테마·모드가 바뀌지 않는 프레임에서는 지오메트리를 재사용한다.
pub struct WelcomeGlow<'a> {
    pub cache: &'a canvas::Cache,
    pub bg: Color,
    pub color: Color,
    pub is_dark: bool,
}

impl<Message> canvas::Program<Message> for WelcomeGlow<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geom = self.cache.draw(renderer, bounds.size(), |frame| {
            // 컨테이너와 캔버스 간 감마 경계 링이 생기지 않도록 동일한 셰이더 색공간으로 배경을 먼저 채운다.
            frame.fill_rectangle(Point::ORIGIN, bounds.size(), self.bg);

            let cx = bounds.width * 0.5;
            // 원본과 동일하게 입력창과 [채팅|태스크] 토글 바로 뒤쪽에 중심이 오도록 배치
            let cy = bounds.height * 0.525;
            let max_rx = (bounds.width * 0.50).clamp(300.0, 540.0);
            let max_ry = (bounds.height * 0.40).clamp(260.0, 460.0);

            let steps = 52usize;
            let peak_strength = if self.is_dark { 0.17 } else { 0.42 };
            let k = 4.0_f32;
            let edge_exp = (-k).exp();
            let norm = 1.0 - edge_exp;

            for i in (1..steps).rev() {
                let t = i as f32 / steps as f32;
                let rx = max_rx * t;
                let ry = max_ry * t;
                let g = (((-k * t * t).exp() - edge_exp) / norm).clamp(0.0, 1.0);
                let strength = peak_strength * g;
                let col = blend_opaque(self.bg, self.color, strength);

                let ellipse = ellipse_path(cx, cy, rx, ry);
                frame.fill(&ellipse, col);
            }
        });

        vec![geom]
    }
}

fn blend_opaque(bg: Color, fg: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::from_rgb(
        bg.r + (fg.r - bg.r) * t,
        bg.g + (fg.g - bg.g) * t,
        bg.b + (fg.b - bg.b) * t,
    )
}

/// 4개의 3차 베지어 곡선으로 부드러운 수학적 타원 경로를 생성한다.
fn ellipse_path(cx: f32, cy: f32, rx: f32, ry: f32) -> Path {
    const KAPPA: f32 = 0.552_284_8;
    let ox = rx * KAPPA;
    let oy = ry * KAPPA;
    Path::new(|b| {
        b.move_to(Point::new(cx, cy - ry));
        b.bezier_curve_to(
            Point::new(cx + ox, cy - ry),
            Point::new(cx + rx, cy - oy),
            Point::new(cx + rx, cy),
        );
        b.bezier_curve_to(
            Point::new(cx + rx, cy + oy),
            Point::new(cx + ox, cy + ry),
            Point::new(cx, cy + ry),
        );
        b.bezier_curve_to(
            Point::new(cx - ox, cy + ry),
            Point::new(cx - rx, cy + oy),
            Point::new(cx - rx, cy),
        );
        b.bezier_curve_to(
            Point::new(cx - rx, cy - oy),
            Point::new(cx - ox, cy - ry),
            Point::new(cx, cy - ry),
        );
        b.close();
    })
}

/// 정사각형 프로필 이미지 위 네 모서리를 배경색으로 덮어 깔끔한 원형 아바타로 마스킹한다.
pub struct AvatarCircleMask<'a> {
    pub cache: &'a canvas::Cache,
    pub bg: Color,
}

impl<Message> canvas::Program<Message> for AvatarCircleMask<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geom = self.cache.draw(renderer, bounds.size(), |frame| {
            let w = bounds.width;
            let h = bounds.height;
            let r = w.min(h) * 0.5;

            let mask = Path::new(|b| {
                b.rectangle(Point::ORIGIN, Size::new(w, h));
                b.circle(Point::new(w * 0.5, h * 0.5), r);
            });
            frame.fill(
                &mask,
                Fill {
                    style: canvas::Style::Solid(self.bg),
                    rule: canvas::fill::Rule::EvenOdd,
                },
            );
        });
        vec![geom]
    }
}
