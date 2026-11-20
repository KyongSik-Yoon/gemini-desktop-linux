use std::collections::HashMap;

use crate::Message;
use crate::i18n::{Locale, t};
use crate::icons::Icon;
use crate::ipc::{GeneratedImage, TurnArtifact};
use crate::theme::{AppTheme, font_bold, font_medium, font_semibold};
use iced::alignment;
use iced::font::Weight;
use iced::widget::{
    button, checkbox, column, container, image, markdown, rich_text, row, scrollable, space, text,
};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding, Pixels};

const BODY_LINE_HEIGHT: f32 = 1.68;
const HEADING_LINE_HEIGHT: f32 = 1.38;
const CODE_LINE_HEIGHT: f32 = 1.52;
const TABLE_CELL_LINE_HEIGHT: f32 = 1.55;

/// 마크다운 렌더링 시 인라인 생성 이미지·Canvas 아티팩트 매칭에 사용하는 컨텍스트.
pub(crate) struct RenderContext<'a> {
    pub(crate) message_idx: usize,
    pub(crate) generated_images: &'a [GeneratedImage],
    pub(crate) artifacts: &'a [TurnArtifact],
    pub(crate) image_cache: &'a HashMap<String, image::Handle>,
}

/// 마크다운 원문을 일반 마크다운·커스텀 GFM 표·생성 이미지·Canvas 플레이스홀더 블록으로 분할 보관한다.
pub(crate) struct ParsedMarkdown {
    blocks: Vec<MarkdownBlock>,
    image_ref_count: usize,
    canvas_ref_count: usize,
}

enum MarkdownBlock {
    Rich(markdown::Content),
    Table(ParsedTable),
    GeneratedImageRef(usize),
    CanvasRef(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableAlign {
    Left,
    Center,
    Right,
}

struct ParsedTable {
    raw_markdown: String,
    alignments: Vec<TableAlign>,
    /// 각 헤더 셀은 `<br>` 기준으로 분할된 마크다운 줄 목록을 가진다.
    headers: Vec<Vec<markdown::Content>>,
    /// 각 데이터 행의 각 셀도 `<br>` 기준으로 분할된 마크다운 줄 목록을 가진다.
    rows: Vec<Vec<Vec<markdown::Content>>>,
    /// 열별 권장 너비(px).
    col_widths: Vec<f32>,
}

impl ParsedMarkdown {
    pub(crate) fn empty() -> Self {
        Self {
            blocks: Vec::new(),
            image_ref_count: 0,
            canvas_ref_count: 0,
        }
    }

    pub(crate) fn parse(raw: &str) -> Self {
        if raw.is_empty() {
            return Self::empty();
        }

        let lines: Vec<&str> = raw.split_inclusive('\n').collect();
        let mut blocks: Vec<MarkdownBlock> = Vec::new();
        let mut rich_buf = String::with_capacity(raw.len());
        let mut in_fenced_code = false;
        let mut fence_char = '`';
        let mut fence_len = 0usize;
        let mut image_ref_count = 0usize;
        let mut canvas_ref_count = 0usize;

        let flush_rich = |buf: &mut String, out: &mut Vec<MarkdownBlock>| {
            if !buf.trim().is_empty() {
                let processed = preprocess_markdown(buf);
                out.push(MarkdownBlock::Rich(markdown::Content::parse(&processed)));
            }
            buf.clear();
        };

        let mut i = 0usize;
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();

            if let Some((ch, count)) = leading_fence(trimmed) {
                if !in_fenced_code {
                    in_fenced_code = true;
                    fence_char = ch;
                    fence_len = count;
                } else if ch == fence_char
                    && count >= fence_len
                    && trimmed[count..].trim().is_empty()
                {
                    in_fenced_code = false;
                }
                rich_buf.push_str(line);
                i += 1;
                continue;
            }

            if in_fenced_code {
                rich_buf.push_str(line);
                i += 1;
                continue;
            }

            // 1. GFM 표 블록 감지 (헤더 행 + 구분선 행)
            if i + 1 < lines.len()
                && looks_like_table_row(lines[i])
                && parse_table_separator(lines[i + 1]).is_some()
            {
                let mut end = i + 2;
                while end < lines.len() && looks_like_table_row(lines[end]) {
                    end += 1;
                }
                if let Some(tbl) = parse_gfm_table(&lines[i..end]) {
                    flush_rich(&mut rich_buf, &mut blocks);
                    blocks.push(MarkdownBlock::Table(tbl));
                    i = end;
                    continue;
                }
            }

            // 2. 특수 googleusercontent 플레이스홀더 URL(생성 이미지 / Canvas 칩) 분리
            if contains_special_placeholder(line) {
                split_line_placeholders(
                    line,
                    &mut rich_buf,
                    &mut blocks,
                    &mut image_ref_count,
                    &mut canvas_ref_count,
                );
                i += 1;
                continue;
            }

            rich_buf.push_str(line);
            i += 1;
        }

        flush_rich(&mut rich_buf, &mut blocks);

        Self {
            blocks,
            image_ref_count,
            canvas_ref_count,
        }
    }

    /// 스트리밍 중 일반 텍스트 델타를 빠르게 반영한다.
    /// 표·특수 플레이스홀더·볼드 경계가 등장하면 안전하게 전체 재파싱한다.
    pub(crate) fn push_delta(&mut self, prev_raw: &str, delta: &str, full_text: &str) {
        let needs_reparse = self.blocks.len() != 1
            || !matches!(self.blocks.first(), Some(MarkdownBlock::Rich(_)))
            || prev_raw.ends_with('*')
            || delta.contains("**")
            || delta.starts_with('*')
            || delta.contains('|')
            || delta.contains("googleusercontent.com/")
            || delta.contains("<br")
            || delta.contains("<BR");

        if !needs_reparse && !delta.is_empty() && full_text.starts_with(prev_raw) {
            if let Some(MarkdownBlock::Rich(content)) = self.blocks.first_mut() {
                content.push_str(delta);
                return;
            }
        }
        *self = Self::parse(full_text);
    }
}

fn contains_special_placeholder(line: &str) -> bool {
    line.contains("googleusercontent.com/image_generation_content/")
        || line.contains("googleusercontent.com/immersive_entry_chip/")
}

fn split_line_placeholders(
    line: &str,
    rich_buf: &mut String,
    blocks: &mut Vec<MarkdownBlock>,
    image_ref_count: &mut usize,
    canvas_ref_count: &mut usize,
) {
    let mut rest = line;
    while !rest.is_empty() {
        let next_img = find_placeholder_span(rest, "image_generation_content/");
        let next_canvas = find_placeholder_span(rest, "immersive_entry_chip/");

        let chosen = match (next_img, next_canvas) {
            (Some(a), Some(b)) => {
                if a.0 <= b.0 {
                    Some((a.0, a.1, true))
                } else {
                    Some((b.0, b.1, false))
                }
            }
            (Some(a), None) => Some((a.0, a.1, true)),
            (None, Some(b)) => Some((b.0, b.1, false)),
            (None, None) => None,
        };

        let Some((start, end, is_image)) = chosen else {
            rich_buf.push_str(rest);
            break;
        };

        let before = &rest[..start];
        if !before.trim().is_empty() {
            rich_buf.push_str(before);
        }
        if !rich_buf.trim().is_empty() {
            let processed = preprocess_markdown(rich_buf);
            blocks.push(MarkdownBlock::Rich(markdown::Content::parse(&processed)));
        }
        rich_buf.clear();

        if is_image {
            blocks.push(MarkdownBlock::GeneratedImageRef(*image_ref_count));
            *image_ref_count += 1;
        } else {
            blocks.push(MarkdownBlock::CanvasRef(*canvas_ref_count));
            *canvas_ref_count += 1;
        }
        rest = &rest[end..];
    }
}

/// 마크다운 이미지 구문(`![alt](http://googleusercontent.com/...)`)이나 맨 URL 형태의
/// 특수 플레이스홀더 바이트 범위 `(start, end)`를 찾는다.
fn find_placeholder_span(s: &str, kind_path: &str) -> Option<(usize, usize)> {
    let needle = format!("googleusercontent.com/{kind_path}");
    let pos = s.find(&needle)?;

    // 앞쪽 http:// 또는 https:// 시작 위치 탐색
    let prefix = &s[..pos];
    let url_start = if prefix.ends_with("https://") {
        pos - "https://".len()
    } else if prefix.ends_with("http://") {
        pos - "http://".len()
    } else {
        return None;
    };

    // URL 끝 위치 탐색 (공백·닫는 괄호·꺾쇠·줄바꿈 전까지)
    let tail_start = pos + needle.len();
    let mut url_end = tail_start;
    for (off, ch) in s[tail_start..].char_indices() {
        if ch.is_whitespace() || matches!(ch, ')' | ']' | '>' | '"' | '\'') {
            url_end = tail_start + off;
            break;
        }
        url_end = tail_start + off + ch.len_utf8();
    }

    // `![...](url)` 또는 `[...](url)`로 감싸져 있으면 전체 마크다운 토큰을 대체
    let mut span_start = url_start;
    let mut span_end = url_end;
    if url_start > 0 && s.as_bytes()[url_start - 1] == b'(' {
        if let Some(open_bracket) = s[..url_start - 1].rfind('[') {
            if s[open_bracket..url_start - 1].ends_with(']') {
                span_start = if open_bracket > 0 && s.as_bytes()[open_bracket - 1] == b'!' {
                    open_bracket - 1
                } else {
                    open_bracket
                };
                if span_end < s.len() && s.as_bytes()[span_end] == b')' {
                    span_end += 1;
                }
            }
        }
    }

    Some((span_start, span_end))
}

fn looks_like_table_row(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || !t.contains('|') {
        return false;
    }
    // 인용구·리스트·코드블록으로 시작하는 일반 문장은 제외하되 파이프 표만 허용
    if t.starts_with('>') || t.starts_with("```") || t.starts_with("~~~") {
        return false;
    }
    split_table_cells(t).len() >= 2
}

fn parse_table_separator(line: &str) -> Option<Vec<TableAlign>> {
    let t = line.trim();
    if !t.contains('|') || !t.contains('-') {
        return None;
    }
    let cells = split_table_cells(t);
    if cells.len() < 2 {
        return None;
    }
    let mut aligns = Vec::with_capacity(cells.len());
    for raw_cell in cells {
        let c = raw_cell.trim();
        if c.is_empty() {
            return None;
        }
        let left = c.starts_with(':');
        let right = c.ends_with(':');
        let core = c.trim_matches(':');
        if core.is_empty() || !core.chars().all(|ch| ch == '-') {
            return None;
        }
        let align = match (left, right) {
            (true, true) => TableAlign::Center,
            (false, true) => TableAlign::Right,
            _ => TableAlign::Left,
        };
        aligns.push(align);
    }
    Some(aligns)
}

/// 이스케이프 파이프(`\|`)와 인라인 코드(`` `...` ``) 내부의 `|`를 보존하며 표 행을 셀 문자열로 분할한다.
fn split_table_cells(line: &str) -> Vec<String> {
    let mut t = line.trim();
    if let Some(stripped) = t.strip_prefix('|') {
        t = stripped;
    }
    if let Some(stripped) = t.strip_suffix('|') {
        // 끝 파이프가 `\|`로 이스케이프되지 않은 경우만 제거
        if !stripped.ends_with('\\') {
            t = stripped;
        }
    }

    let chars: Vec<char> = t.chars().collect();
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut in_code = false;
    let mut i = 0usize;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '`' {
            in_code = !in_code;
            cur.push(ch);
            i += 1;
            continue;
        }
        if ch == '\\' && i + 1 < chars.len() && chars[i + 1] == '|' {
            cur.push('|');
            i += 2;
            continue;
        }
        if ch == '|' && !in_code {
            cells.push(cur.trim().to_string());
            cur.clear();
            i += 1;
            continue;
        }
        cur.push(ch);
        i += 1;
    }
    cells.push(cur.trim().to_string());
    cells
}

/// 셀 텍스트 내부의 `<br>`, `<br/>`, `<br />` 태그를 실제 줄 배열로 분할한다 (인라인 코드 내부 제외).
fn split_cell_br_lines(cell: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::with_capacity(cell.len());
    let bytes = cell.as_bytes();
    let mut in_code = false;
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'`' {
            in_code = !in_code;
            cur.push('`');
            i += 1;
            continue;
        }
        if !in_code && bytes[i] == b'<' {
            if let Some(tag_len) = match_br_tag(&cell[i..]) {
                lines.push(cur.trim().to_string());
                cur.clear();
                i += tag_len;
                continue;
            }
        }
        let ch = cell[i..].chars().next().unwrap();
        cur.push(ch);
        i += ch.len_utf8();
    }
    lines.push(cur.trim().to_string());
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn match_br_tag(s: &str) -> Option<usize> {
    if !s.starts_with('<') {
        return None;
    }
    let close = s.find('>')?;
    if close > 8 {
        return None;
    }
    let inner = s[1..close].trim().to_ascii_lowercase();
    if inner == "br" || inner == "br/" || inner == "br /" {
        Some(close + 1)
    } else {
        None
    }
}

fn parse_cell_contents(raw_cell: &str) -> Vec<markdown::Content> {
    split_cell_br_lines(raw_cell)
        .into_iter()
        .map(|subline| {
            let processed = preprocess_markdown(&subline);
            markdown::Content::parse(&processed)
        })
        .collect()
}

/// 셀 텍스트의 대략적 시각 폭(한글=2, 영문=1)을 계산해 열 너비 균형을 잡는다.
fn visual_cell_units(raw_cell: &str) -> usize {
    split_cell_br_lines(raw_cell)
        .into_iter()
        .map(|line| {
            // 마크다운 링크 URL과 강조 기호는 시각 폭에서 제외
            let cleaned = strip_markdown_syntax_for_width(&line);
            cleaned
                .chars()
                .map(|c| if c.is_ascii() { 1 } else { 2 })
                .sum::<usize>()
        })
        .max()
        .unwrap_or(4)
}

fn strip_markdown_syntax_for_width(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '[' {
            // [label](url) 형태면 label만 폭 계산에 포함
            if let Some(close_b) = chars[i + 1..].iter().position(|&c| c == ']') {
                let close_idx = i + 1 + close_b;
                if close_idx + 1 < chars.len() && chars[close_idx + 1] == '(' {
                    if let Some(close_p) = chars[close_idx + 2..].iter().position(|&c| c == ')') {
                        for &c in &chars[i + 1..close_idx] {
                            out.push(c);
                        }
                        i = close_idx + 2 + close_p + 1;
                        continue;
                    }
                }
            }
        }
        if chars[i] != '*' && chars[i] != '`' {
            out.push(chars[i]);
        }
        i += 1;
    }
    out
}

fn parse_gfm_table(table_lines: &[&str]) -> Option<ParsedTable> {
    if table_lines.len() < 2 {
        return None;
    }
    let alignments = parse_table_separator(table_lines[1])?;
    let header_raw = split_table_cells(table_lines[0]);
    if header_raw.is_empty() {
        return None;
    }

    let col_count = alignments.len().max(header_raw.len());
    let mut norm_aligns = alignments;
    while norm_aligns.len() < col_count {
        norm_aligns.push(TableAlign::Left);
    }

    let mut max_units = vec![6usize; col_count];
    let mut headers = Vec::with_capacity(col_count);
    for col in 0..col_count {
        let raw = header_raw.get(col).map(|s| s.as_str()).unwrap_or("");
        max_units[col] = max_units[col].max(visual_cell_units(raw));
        headers.push(parse_cell_contents(raw));
    }

    let mut rows = Vec::with_capacity(table_lines.len().saturating_sub(2));
    for line in &table_lines[2..] {
        let raw_cells = split_table_cells(line);
        let mut parsed_row = Vec::with_capacity(col_count);
        for col in 0..col_count {
            let raw = raw_cells.get(col).map(|s| s.as_str()).unwrap_or("");
            max_units[col] = max_units[col].max(visual_cell_units(raw));
            parsed_row.push(parse_cell_contents(raw));
        }
        rows.push(parsed_row);
    }

    // 열 너비 계산: 최소 96px ~ 최대 290px 범위로 균형 배분, 가용 폭(698px) 이내(680px)로 맞춰 불필요한 가로 스크롤 방지
    let mut col_widths: Vec<f32> = max_units
        .iter()
        .map(|&u| ((u as f32) * 7.8 + 28.0).clamp(96.0, 290.0))
        .collect();
    let total_w: f32 = col_widths.iter().sum();
    let target_min_w = 680.0;
    if total_w > 0.0 && total_w < target_min_w {
        let scale = target_min_w / total_w;
        for w in &mut col_widths {
            *w = (*w * scale).min(360.0);
        }
    }

    let raw_markdown: String = table_lines.iter().copied().collect();
    Some(ParsedTable {
        raw_markdown: raw_markdown.trim_end().to_string(),
        alignments: norm_aligns,
        headers,
        rows,
        col_widths,
    })
}

fn contains_br_tag(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 4 {
        return false;
    }
    for i in 0..=bytes.len() - 3 {
        if bytes[i] == b'<'
            && bytes[i + 1].eq_ignore_ascii_case(&b'b')
            && bytes[i + 2].eq_ignore_ascii_case(&b'r')
        {
            return true;
        }
    }
    false
}

/// 문자 슬라이스의 `start` 위치가 `<br>`, `<br/>`, `<br />` (대소문자 무관) 태그라면 소비할 문자 길이를 반환한다.
fn match_br_tag_chars(chars: &[char], start: usize) -> Option<usize> {
    let len = chars.len();
    if start + 3 >= len || chars[start] != '<' {
        return None;
    }
    if !chars[start + 1].eq_ignore_ascii_case(&'b') || !chars[start + 2].eq_ignore_ascii_case(&'r')
    {
        return None;
    }
    let mut j = start + 3;
    while j < len && chars[j].is_ascii_whitespace() {
        j += 1;
    }
    if j < len && chars[j] == '/' {
        j += 1;
        while j < len && chars[j].is_ascii_whitespace() {
            j += 1;
        }
    }
    if j < len && chars[j] == '>' {
        Some(j - start + 1)
    } else {
        None
    }
}

/// CommonMark(`pulldown_cmark`)가 한국어(CJK) 텍스트에서 구두점으로 끝나는 볼드 강조(`**텍스트(괄호)**입니다`)를
/// 파싱하지 못하는 엣지 케이스와 일반 본문의 `<br>` HTML 줄바꿈 태그를 보정한다.
/// 코드 펜스와 인라인 코드는 원문 그대로 보존한다.
pub(crate) fn preprocess_markdown(raw: &str) -> String {
    if !raw.contains("**") && !contains_br_tag(raw) {
        return raw.to_string();
    }

    let mut out = String::with_capacity(raw.len() + 32);
    let mut in_fenced_code = false;
    let mut fence_char = '`';
    let mut fence_len = 0usize;

    let lines: Vec<&str> = raw.split_inclusive('\n').collect();
    for line in lines {
        let trimmed = line.trim_start();
        if let Some((ch, count)) = leading_fence(trimmed) {
            if !in_fenced_code {
                in_fenced_code = true;
                fence_char = ch;
                fence_len = count;
            } else if ch == fence_char
                && count >= fence_len
                && trimmed[count..].trim().is_empty()
            {
                in_fenced_code = false;
            }
            out.push_str(line);
            continue;
        }

        if in_fenced_code {
            out.push_str(line);
            continue;
        }

        preprocess_inline_line(line, &mut out);
    }

    out
}

fn leading_fence(trimmed: &str) -> Option<(char, usize)> {
    let first = trimmed.chars().next()?;
    if first != '`' && first != '~' {
        return None;
    }
    let count = trimmed.chars().take_while(|&c| c == first).count();
    if count >= 3 {
        Some((first, count))
    } else {
        None
    }
}

fn preprocess_inline_line(line: &str, out: &mut String) {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0usize;
    let mut in_code_ticks = 0usize;
    let mut bold_open = false;

    while i < len {
        // 인라인 코드 스팬(백틱 구간)은 그대로 통과
        if chars[i] == '`' {
            let mut count = 0usize;
            while i + count < len && chars[i + count] == '`' {
                out.push('`');
                count += 1;
            }
            if in_code_ticks == 0 {
                in_code_ticks = count;
            } else if in_code_ticks == count {
                in_code_ticks = 0;
            }
            i += count;
            continue;
        }

        if in_code_ticks > 0 {
            out.push(chars[i]);
            i += 1;
            continue;
        }

        // 이스케이프 문자는 그대로 통과
        if chars[i] == '\\' && i + 1 < len {
            out.push(chars[i]);
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }

        // 인라인 코드 외부의 `<br>`, `<br/>`, `<br />` 태그를 마크다운 하드 줄바꿈으로 변환
        if chars[i] == '<' {
            if let Some(tag_len) = match_br_tag_chars(&chars, i) {
                let next_idx = i + tag_len;
                let followed_by_newline = next_idx < len
                    && (chars[next_idx] == '\n'
                        || (chars[next_idx] == '\r'
                            && next_idx + 1 < len
                            && chars[next_idx + 1] == '\n'));
                if followed_by_newline {
                    out.push_str("  ");
                } else {
                    out.push_str("  \n");
                }
                i += tag_len;
                continue;
            }
        }

        // 정확히 두 개의 별표(`**`)만 볼드 경계로 판별 (`***` 이상은 그대로 유지)
        if chars[i] == '*' && i + 1 < len && chars[i + 1] == '*' {
            let prev_is_star = i > 0 && chars[i - 1] == '*';
            let next_is_star = i + 2 < len && chars[i + 2] == '*';
            if !prev_is_star && !next_is_star {
                let prev_char = if i > 0 { Some(chars[i - 1]) } else { None };
                let next_char = if i + 2 < len {
                    Some(chars[i + 2])
                } else {
                    None
                };

                if !bold_open {
                    let can_open = next_char.is_some_and(|c| !c.is_whitespace());
                    out.push_str("**");
                    if can_open {
                        bold_open = true;
                        if prev_char.is_some_and(is_word_char)
                            && next_char.is_some_and(is_cjk_edge_punct)
                        {
                            out.push('\u{200B}');
                        }
                    }
                    i += 2;
                    continue;
                } else {
                    let can_close = prev_char.is_some_and(|c| !c.is_whitespace());
                    if can_close {
                        bold_open = false;
                        if prev_char.is_some_and(is_cjk_edge_punct)
                            && next_char.is_some_and(is_word_char)
                        {
                            out.push('\u{200B}');
                        }
                    }
                    out.push_str("**");
                    i += 2;
                    continue;
                }
            }
        }

        out.push(chars[i]);
        i += 1;
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

fn is_cjk_edge_punct(c: char) -> bool {
    matches!(
        c,
        ')' | '('
            | ']'
            | '['
            | '}'
            | '{'
            | '"'
            | '\''
            | '”'
            | '“'
            | '’'
            | '‘'
            | '>'
            | '<'
            | '»'
            | '«'
            | '」'
            | '「'
            | '』'
            | '『'
            | '》'
            | '《'
            | '〉'
            | '〈'
            | '】'
            | '【'
            | '%'
            | '.'
            | ','
            | '!'
            | '?'
            | ':'
            | ';'
            | '…'
            | '·'
            | '~'
            | '°'
            | '₩'
            | '$'
            | '€'
            | '£'
            | '¥'
    )
}

fn mix_color(a: Color, b: Color, t: f32) -> Color {
    let k = t.clamp(0.0, 1.0);
    Color::from_rgba(
        a.r + (b.r - a.r) * k,
        a.g + (b.g - a.g) * k,
        a.b + (b.b - a.b) * k,
        a.a + (b.a - a.a) * k,
    )
}

/// 괄호 안 출처 인용 패턴(`([와우테일](url))`, `([출처1](url1), [출처2](url2))`)의 링크 스팬인지 판별한다.
fn is_parenthesized_citation_span(
    spans: &[iced::widget::text::Span<'static, markdown::Uri>],
    idx: usize,
) -> bool {
    if spans.get(idx).and_then(|s| s.link.as_ref()).is_none() {
        return false;
    }
    // 앞쪽 스팬들에서 닫는 괄호 없이 여는 `(`를 만나는지 확인
    let mut found_open = false;
    for prev in spans[..idx].iter().rev() {
        if prev.link.is_some() {
            continue;
        }
        let mut hit_boundary = false;
        for ch in prev.text.chars().rev() {
            if ch == '(' {
                found_open = true;
                hit_boundary = true;
                break;
            }
            if ch == ')' || ch == '\n' {
                hit_boundary = true;
                break;
            }
            if !ch.is_whitespace() && !matches!(ch, ',' | ';' | '·' | '/') {
                hit_boundary = true;
                break;
            }
        }
        if hit_boundary {
            break;
        }
    }
    if !found_open {
        return false;
    }

    // 뒤쪽 스팬들에서 여는 괄호 없이 닫는 `)`를 만나는지 확인
    for next in &spans[idx + 1..] {
        if next.link.is_some() {
            continue;
        }
        for ch in next.text.chars() {
            if ch == ')' {
                return true;
            }
            if ch == '(' || ch == '\n' {
                return false;
            }
            if !ch.is_whitespace() && !matches!(ch, ',' | ';' | '·' | '/') {
                return false;
            }
        }
    }
    false
}

/// 분할된 `ParsedMarkdown`을 Gemini 데스크톱 스타일로 렌더링한다.
pub(crate) fn view<'a>(
    parsed: &'a ParsedMarkdown,
    settings: markdown::Settings,
    palette: AppTheme,
    locale: Locale,
    ctx: RenderContext<'a>,
) -> Element<'a, Message> {
    let viewer = GeminiMarkdownViewer {
        palette: palette.clone(),
        locale,
        in_table: false,
        table_align: None,
    };

    let mut children: Vec<Element<'a, Message>> = Vec::with_capacity(parsed.blocks.len() + 2);
    for block in &parsed.blocks {
        match block {
            MarkdownBlock::Rich(content) => {
                children.push(viewer.render_items(settings, content.items(), 0));
            }
            MarkdownBlock::Table(tbl) => {
                children.push(viewer.render_custom_table(settings, tbl));
            }
            MarkdownBlock::GeneratedImageRef(img_idx) => {
                children.push(viewer.render_generated_image_slot(*img_idx, &ctx));
            }
            MarkdownBlock::CanvasRef(canvas_idx) => {
                children.push(viewer.render_canvas_card(*canvas_idx, &ctx));
            }
        }
    }

    // 본문 플레이스홀더보다 생성 이미지가 더 많거나 플레이스홀더가 누락된 경우 하단에 표시
    if ctx.generated_images.len() > parsed.image_ref_count {
        for idx in parsed.image_ref_count..ctx.generated_images.len() {
            children.push(viewer.render_generated_image_slot(idx, &ctx));
        }
    }
    // Canvas 아티팩트도 플레이스홀더에 매핑되지 않은 남은 항목이 있으면 하단에 카드로 표시
    if ctx.artifacts.len() > parsed.canvas_ref_count {
        for idx in parsed.canvas_ref_count..ctx.artifacts.len() {
            children.push(viewer.render_canvas_card(idx, &ctx));
        }
    }

    column(children).spacing(settings.spacing).width(Fill).into()
}

pub(crate) fn generated_image_cache_key(url: &str) -> String {
    format!("gen_img:{url}")
}

pub(crate) fn turn_attachment_cache_key(url: &str) -> String {
    format!("turn_att:{url}")
}

/// Canvas 아티팩트가 Deep Research 보고서 등 마크다운 문서인지 판별한다.
pub(crate) fn is_markdown_artifact(art: &TurnArtifact) -> bool {
    let lang = art.language.trim().to_ascii_lowercase();
    if lang == "markdown" || lang == "md" {
        return true;
    }
    if !lang.is_empty() {
        return false;
    }
    let trimmed = art.content.trim_start();
    if trimmed.is_empty() {
        return false;
    }
    let head_lower = trimmed.to_ascii_lowercase();
    if head_lower.starts_with("<!doctype html")
        || head_lower.starts_with("<html")
        || head_lower.starts_with("<?xml")
        || trimmed.starts_with('{')
        || trimmed.starts_with('[')
        || trimmed.starts_with("import ")
        || trimmed.starts_with("package ")
        || trimmed.starts_with("function ")
        || trimmed.starts_with("def ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("fn ")
    {
        return false;
    }
    trimmed.starts_with("# ")
        || trimmed.starts_with("## ")
        || trimmed.starts_with("### ")
        || trimmed.contains("\n## ")
        || trimmed.contains("\n### ")
        || (trimmed.contains("**") && trimmed.contains('\n'))
}

#[derive(Clone)]
struct GeminiMarkdownViewer {
    palette: AppTheme,
    locale: Locale,
    in_table: bool,
    table_align: Option<alignment::Horizontal>,
}

impl GeminiMarkdownViewer {
    fn render_items<'a>(
        &self,
        settings: markdown::Settings,
        items: &'a [markdown::Item],
        depth: usize,
    ) -> Element<'a, Message> {
        let blocks = items
            .iter()
            .enumerate()
            .map(|(idx, item)| self.render_item(settings, item, idx, depth));

        let col = column(blocks).spacing(settings.spacing);
        if self.in_table {
            col.into()
        } else {
            col.width(Fill).into()
        }
    }

    fn render_item<'a>(
        &self,
        settings: markdown::Settings,
        item: &'a markdown::Item,
        index: usize,
        depth: usize,
    ) -> Element<'a, Message> {
        match item {
            markdown::Item::Image { url, title, alt } => {
                self.render_image(settings, url, title, alt)
            }
            markdown::Item::Heading(level, text_item) => {
                self.render_heading(settings, level, text_item, index)
            }
            markdown::Item::Paragraph(text_item) => self.render_paragraph(settings, text_item),
            markdown::Item::CodeBlock {
                language,
                code,
                lines,
            } => self.render_code_block(settings, language.as_deref(), code, lines),
            markdown::Item::List {
                start: None,
                bullets,
            } => self.render_unordered_list(settings, bullets, depth),
            markdown::Item::List {
                start: Some(start),
                bullets,
            } => self.render_ordered_list(settings, *start, bullets, depth),
            markdown::Item::Quote(contents) => self.render_quote(settings, contents, depth),
            markdown::Item::Rule => self.render_rule(),
            markdown::Item::Table { columns, rows } => self.render_table(settings, columns, rows),
        }
    }

    /// `iced::widget::markdown::Text::spans` 결과를 후처리해 인라인 코드 크기·강조 색상·괄호 출처 링크 톤을 정돈한다.
    fn styled_spans(
        &self,
        text_item: &markdown::Text,
        style: markdown::Style,
        base_size: f32,
        is_heading: bool,
    ) -> Vec<iced::widget::text::Span<'static, markdown::Uri>> {
        let strong_color = self.palette.text_strong;
        let inline_code_size = Pixels((base_size - 1.5).max(11.5));
        let citation_color = mix_color(self.palette.text_secondary, self.palette.accent, 0.28);
        let citation_size = Pixels((base_size - 0.8).max(12.0));

        let raw_spans: Vec<_> = text_item.spans(style).iter().cloned().collect();
        let mut out = Vec::with_capacity(raw_spans.len());

        for (i, mut sp) in raw_spans.iter().cloned().enumerate() {
            if sp.highlight.is_some() {
                sp.size = Some(inline_code_size);
            } else if sp.link.is_some() && !is_heading && is_parenthesized_citation_span(&raw_spans, i)
            {
                sp.color = Some(citation_color);
                sp.size = Some(citation_size);
            } else if !is_heading
                && sp.link.is_none()
                && sp.color.is_none()
                && sp.font.is_some_and(|f| f.weight == Weight::Bold)
            {
                sp.color = Some(strong_color);
            }
            out.push(sp);
        }
        out
    }

    fn render_paragraph<'a>(
        &self,
        settings: markdown::Settings,
        text_item: &markdown::Text,
    ) -> Element<'a, Message> {
        let spans = self.styled_spans(text_item, settings.style, settings.text_size.0, false);
        let line_h = if self.in_table {
            TABLE_CELL_LINE_HEIGHT
        } else {
            BODY_LINE_HEIGHT
        };
        let mut rt = rich_text(spans)
            .size(settings.text_size)
            .line_height(text::LineHeight::Relative(line_h))
            .color(self.palette.text_primary)
            .wrapping(text::Wrapping::WordOrGlyph)
            .on_link_click(Message::LinkClicked)
            .width(Fill);
        if let Some(ax) = self.table_align {
            rt = rt.align_x(ax);
        }
        rt.into()
    }

    fn render_heading<'a>(
        &self,
        settings: markdown::Settings,
        level: &'a markdown::HeadingLevel,
        text_item: &'a markdown::Text,
        index: usize,
    ) -> Element<'a, Message> {
        let (size, heading_font, top_pad, bottom_pad) = match level {
            markdown::HeadingLevel::H1 => (settings.h1_size, font_bold(), 18.0, 4.0),
            markdown::HeadingLevel::H2 => (settings.h2_size, font_bold(), 16.0, 2.0),
            markdown::HeadingLevel::H3 => (settings.h3_size, font_semibold(), 14.0, 2.0),
            markdown::HeadingLevel::H4 => (settings.h4_size, font_semibold(), 10.0, 0.0),
            markdown::HeadingLevel::H5 => (settings.h5_size, font_semibold(), 8.0, 0.0),
            markdown::HeadingLevel::H6 => (settings.h6_size, font_semibold(), 8.0, 0.0),
        };

        let heading_style = markdown::Style {
            font: heading_font,
            ..settings.style
        };
        let spans = self.styled_spans(text_item, heading_style, size.0, true);

        let rt = rich_text(spans)
            .size(size)
            .line_height(text::LineHeight::Relative(HEADING_LINE_HEIGHT))
            .color(self.palette.text_strong)
            .wrapping(text::Wrapping::WordOrGlyph)
            .on_link_click(Message::LinkClicked);

        let top = if index > 0 { top_pad } else { 0.0 };
        let content: Element<'a, Message> = if self.in_table {
            rt.into()
        } else {
            rt.width(Fill).into()
        };

        let wrap = container(content).padding(Padding {
            top,
            right: 0.0,
            bottom: bottom_pad,
            left: 0.0,
        });
        if self.in_table {
            wrap.into()
        } else {
            wrap.width(Fill).into()
        }
    }

    fn render_unordered_list<'a>(
        &self,
        settings: markdown::Settings,
        bullets: &'a [markdown::Bullet],
        depth: usize,
    ) -> Element<'a, Message> {
        let first_line_h = settings.text_size.0 * BODY_LINE_HEIGHT;
        let marker_color = if depth == 0 {
            self.palette.text_primary
        } else {
            self.palette.text_secondary
        };

        let item_spacing = match depth {
            0 => 14.0,
            1 => 10.0,
            _ => 8.0,
        };
        let inner_spacing = if depth == 0 { 10.0 } else { 8.0 };
        let child_settings = markdown::Settings {
            spacing: inner_spacing.into(),
            ..settings
        };

        let rows = bullets.iter().map(|bullet| {
            let (marker, bullet_items): (Element<'a, Message>, &'a [markdown::Item]) = match bullet
            {
                markdown::Bullet::Point { items } => {
                    let dot: Element<'a, Message> = match depth {
                        0 => container(space::horizontal())
                            .width(Length::Fixed(5.5))
                            .height(Length::Fixed(5.5))
                            .style(move |_| container::Style {
                                background: Some(Background::Color(marker_color)),
                                border: Border {
                                    radius: 3.0.into(),
                                    ..Border::default()
                                },
                                ..container::Style::default()
                            })
                            .into(),
                        1 => container(space::horizontal())
                            .width(Length::Fixed(6.0))
                            .height(Length::Fixed(6.0))
                            .style(move |_| container::Style {
                                background: None,
                                border: Border {
                                    radius: 3.0.into(),
                                    width: 1.3,
                                    color: marker_color,
                                },
                                ..container::Style::default()
                            })
                            .into(),
                        _ => container(space::horizontal())
                            .width(Length::Fixed(4.5))
                            .height(Length::Fixed(4.5))
                            .style(move |_| container::Style {
                                background: Some(Background::Color(marker_color)),
                                border: Border {
                                    radius: 1.0.into(),
                                    ..Border::default()
                                },
                                ..container::Style::default()
                            })
                            .into(),
                    };

                    let marker_slot = container(dot)
                        .width(Length::Fixed(16.0))
                        .height(Length::Fixed(first_line_h))
                        .center_x(Length::Fixed(16.0))
                        .center_y(Length::Fixed(first_line_h))
                        .into();
                    (marker_slot, items.as_slice())
                }
                markdown::Bullet::Task { items, done } => {
                    let cb = container(checkbox(*done).size(settings.text_size))
                        .width(Length::Fixed(18.0))
                        .height(Length::Fixed(first_line_h))
                        .center_y(Length::Fixed(first_line_h))
                        .into();
                    (cb, items.as_slice())
                }
            };

            let body = self.render_items(child_settings, bullet_items, depth + 1);
            let r = row![marker, body]
                .spacing(8.0)
                .align_y(Alignment::Start);

            if self.in_table {
                r.into()
            } else {
                r.width(Fill).into()
            }
        });

        let pad = if depth == 0 {
            Padding {
                top: 2.0,
                right: 0.0,
                bottom: 2.0,
                left: 6.0,
            }
        } else {
            Padding {
                top: 4.0,
                right: 0.0,
                bottom: 2.0,
                left: 4.0,
            }
        };

        let list_col = column(rows).spacing(item_spacing).padding(pad);
        if self.in_table {
            list_col.into()
        } else {
            list_col.width(Fill).into()
        }
    }

    fn render_ordered_list<'a>(
        &self,
        settings: markdown::Settings,
        start: u64,
        bullets: &'a [markdown::Bullet],
        depth: usize,
    ) -> Element<'a, Message> {
        let max_num = start.saturating_add(bullets.len() as u64).max(1);
        let digits = ((max_num as f32).log10().floor() as usize) + 1;
        let num_width = match digits {
            1 => 22.0,
            2 => 30.0,
            _ => 38.0,
        };

        let item_spacing = if depth == 0 { 12.0 } else { 10.0 };
        let inner_spacing = if depth == 0 { 10.0 } else { 8.0 };
        let child_settings = markdown::Settings {
            spacing: inner_spacing.into(),
            ..settings
        };
        let num_color = self.palette.text_secondary;

        let rows = bullets.iter().enumerate().map(|(i, bullet)| {
            let bullet_items = match bullet {
                markdown::Bullet::Point { items } | markdown::Bullet::Task { items, .. } => {
                    items.as_slice()
                }
            };
            let number = text(format!("{}.", start + i as u64))
                .size(settings.text_size)
                .line_height(text::LineHeight::Relative(BODY_LINE_HEIGHT))
                .font(font_medium())
                .color(num_color)
                .align_x(alignment::Horizontal::Right)
                .width(Length::Fixed(num_width));

            let body = self.render_items(child_settings, bullet_items, depth + 1);
            let r = row![number, body]
                .spacing(8.0)
                .align_y(Alignment::Start);

            if self.in_table {
                r.into()
            } else {
                r.width(Fill).into()
            }
        });

        let pad = Padding {
            top: if depth == 0 { 2.0 } else { 4.0 },
            right: 0.0,
            bottom: 2.0,
            left: if depth == 0 { 2.0 } else { 4.0 },
        };

        let list_col = column(rows).spacing(item_spacing).padding(pad);
        if self.in_table {
            list_col.into()
        } else {
            list_col.width(Fill).into()
        }
    }

    fn render_code_block<'a>(
        &self,
        settings: markdown::Settings,
        language: Option<&'a str>,
        code: &'a str,
        lines: &'a [markdown::Text],
    ) -> Element<'a, Message> {
        let pal = self.palette.clone();
        let lang_label = language
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("code");
        let code_owned = code.to_string();
        let fg_code_header = Color::from_rgb(0.84, 0.88, 0.94);

        let header = container(
            row![
                text(lang_label)
                    .size(12)
                    .font(font_medium())
                    .color(Color::from_rgb(0.68, 0.74, 0.84)),
                space::horizontal(),
                button(
                    row![
                        Icon::Copy.view(13.0, fg_code_header),
                        text(t(self.locale, "복사")).size(12).color(fg_code_header),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::CopyText(code_owned))
                .padding([4, 10])
                .style(|_, status| {
                    let hovered = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: Some(Background::Color(if hovered {
                            Color::from_rgba(1.0, 1.0, 1.0, 0.14)
                        } else {
                            Color::from_rgba(1.0, 1.0, 1.0, 0.06)
                        })),
                        text_color: Color::from_rgb(0.86, 0.90, 0.96),
                        border: Border {
                            radius: 6.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }
                }),
            ]
            .align_y(Alignment::Center),
        )
        .padding([7, 14])
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(pal.bg_code_header)),
            border: Border {
                radius: iced::border::Radius {
                    top_left: 12.0,
                    top_right: 12.0,
                    bottom_right: 0.0,
                    bottom_left: 0.0,
                },
                ..Border::default()
            },
            ..container::Style::default()
        });

        let code_lines = column(lines.iter().map(|line| {
            let spans = line.spans(settings.style);
            let is_blank = spans.is_empty()
                || spans.iter().all(|sp| sp.text.is_empty() || sp.text == "\n");
            if is_blank {
                text(" ")
                    .font(settings.style.code_block_font)
                    .size(settings.code_size)
                    .line_height(text::LineHeight::Relative(CODE_LINE_HEIGHT))
                    .into()
            } else {
                rich_text(spans)
                    .on_link_click(Message::LinkClicked)
                    .font(settings.style.code_block_font)
                    .size(settings.code_size)
                    .line_height(text::LineHeight::Relative(CODE_LINE_HEIGHT))
                    .color(Color::from_rgb(0.86, 0.90, 0.96))
                    .into()
            }
        }))
        .spacing(0);

        let pal_scroll = self.palette.clone();
        let body = scrollable(container(code_lines).padding([12, 16]))
            .width(Fill)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default()
                    .width(5)
                    .scroller_width(5),
            ))
            .style(move |_, status| pal_scroll.subtle_scrollable_style(status));

        let bg_code = self.palette.bg_code;
        let border_col = self.palette.border_subtle;
        container(column![header, body].width(Fill))
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_code)),
                border: Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: border_col,
                },
                ..container::Style::default()
            })
            .into()
    }

    fn render_quote<'a>(
        &self,
        settings: markdown::Settings,
        contents: &'a [markdown::Item],
        depth: usize,
    ) -> Element<'a, Message> {
        let bar_color = self.palette.accent;
        let bg_color = self.palette.bg_thought;
        let quote_settings = markdown::Settings {
            spacing: 10.0.into(),
            ..settings
        };

        let inner = self.render_items(quote_settings, contents, depth);
        let row_view = row![
            container(space::vertical().height(Fill))
                .width(Length::Fixed(3.0))
                .style(move |_| container::Style {
                    background: Some(Background::Color(bar_color)),
                    border: Border {
                        radius: 2.0.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                }),
            inner,
        ]
        .spacing(12)
        .height(Length::Shrink);

        let row_view = if self.in_table {
            row_view
        } else {
            row_view.width(Fill)
        };

        container(row_view)
            .padding([10, 14])
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_color)),
                border: Border {
                    radius: 8.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into()
    }

    fn render_rule<'a>(&self) -> Element<'a, Message> {
        let line_color = self.palette.border;
        container(
            container(space::horizontal())
                .width(Fill)
                .height(Length::Fixed(1.0))
                .style(move |_| container::Style {
                    background: Some(Background::Color(line_color)),
                    ..container::Style::default()
                }),
        )
        .padding([8, 0])
        .width(Fill)
        .into()
    }

    /// 커스텀 GFM 표 렌더러: 헤더 음영, 행 구분선, 셀 `<br>` 줄바꿈, 열 폭 균형, 가로 스크롤 및 표 복사 버튼을 제공한다.
    fn render_custom_table<'a>(
        &self,
        settings: markdown::Settings,
        tbl: &'a ParsedTable,
    ) -> Element<'a, Message> {
        let cell_viewer = Self {
            in_table: true,
            ..self.clone()
        };
        let cell_settings = markdown::Settings {
            text_size: 13.5.into(),
            spacing: 4.0.into(),
            ..settings
        };
        let header_settings = markdown::Settings {
            text_size: 13.5.into(),
            spacing: 4.0.into(),
            style: markdown::Style {
                font: font_semibold(),
                ..settings.style
            },
            ..settings
        };

        let bg_card = self.palette.bg_composer;
        let bg_header = self.palette.bg_thought;
        let bg_zebra = mix_color(self.palette.bg_composer, self.palette.bg_thought, 0.42);
        let border_col = self.palette.border;
        let border_sub = self.palette.border_subtle;
        let text_secondary = self.palette.text_secondary;
        let bg_hover = self.palette.bg_hover;

        let render_cell = |lines: &'a [markdown::Content],
                           col_w: f32,
                           align: TableAlign,
                           st: markdown::Settings,
                           is_header: bool|
         -> Element<'a, Message> {
            let align_x = match align {
                TableAlign::Left => alignment::Horizontal::Left,
                TableAlign::Center => alignment::Horizontal::Center,
                TableAlign::Right => alignment::Horizontal::Right,
            };
            let line_elems: Vec<Element<'a, Message>> = lines
                .iter()
                .map(|c| {
                    let items = c.items();
                    if items.is_empty() {
                        text(" ").size(st.text_size).into()
                    } else if is_header {
                        // 헤더 셀 텍스트는 강조 색상과 semibold로 통일
                        let header_blocks: Vec<Element<'a, Message>> = items
                            .iter()
                            .map(|item| match item {
                                markdown::Item::Paragraph(t) => {
                                    let spans = cell_viewer.styled_spans(
                                        t,
                                        st.style,
                                        st.text_size.0,
                                        true,
                                    );
                                    rich_text(spans)
                                        .size(st.text_size)
                                        .line_height(text::LineHeight::Relative(
                                            TABLE_CELL_LINE_HEIGHT,
                                        ))
                                        .color(cell_viewer.palette.text_strong)
                                        .wrapping(text::Wrapping::WordOrGlyph)
                                        .align_x(align_x)
                                        .width(Fill)
                                        .on_link_click(Message::LinkClicked)
                                        .into()
                                }
                                other => cell_viewer.render_item(st, other, 0, 0),
                            })
                            .collect();
                        column(header_blocks).spacing(4).width(Fill).into()
                    } else {
                        let aligned_viewer = Self {
                            table_align: Some(align_x),
                            ..cell_viewer.clone()
                        };
                        aligned_viewer.render_items(st, items, 0)
                    }
                })
                .collect();

            container(column(line_elems).spacing(4).width(Fill))
                .width(Length::Fixed(col_w))
                .padding([9, 12])
                .align_x(align_x)
                .into()
        };

        let mut table_col = column![].spacing(0);

        // 헤더 행
        let header_cells: Vec<Element<'a, Message>> = tbl
            .headers
            .iter()
            .enumerate()
            .map(|(ci, cell_lines)| {
                let w = tbl.col_widths.get(ci).copied().unwrap_or(140.0);
                let align = tbl.alignments.get(ci).copied().unwrap_or(TableAlign::Left);
                render_cell(cell_lines, w, align, header_settings, true)
            })
            .collect();
        let header_row = container(row(header_cells).spacing(0).align_y(Alignment::Start))
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_header)),
                ..container::Style::default()
            });
        table_col = table_col.push(header_row);

        // 헤더 하단 구분선
        let total_w: f32 = tbl.col_widths.iter().sum();
        table_col = table_col.push(
            container(space::horizontal())
                .width(Length::Fixed(total_w))
                .height(Length::Fixed(1.0))
                .style(move |_| container::Style {
                    background: Some(Background::Color(border_col)),
                    ..container::Style::default()
                }),
        );

        // 데이터 행
        for (ri, r) in tbl.rows.iter().enumerate() {
            if ri > 0 {
                table_col = table_col.push(
                    container(space::horizontal())
                        .width(Length::Fixed(total_w))
                        .height(Length::Fixed(1.0))
                        .style(move |_| container::Style {
                            background: Some(Background::Color(border_sub)),
                            ..container::Style::default()
                        }),
                );
            }
            let cells: Vec<Element<'a, Message>> = r
                .iter()
                .enumerate()
                .map(|(ci, cell_lines)| {
                    let w = tbl.col_widths.get(ci).copied().unwrap_or(140.0);
                    let align = tbl.alignments.get(ci).copied().unwrap_or(TableAlign::Left);
                    render_cell(cell_lines, w, align, cell_settings, false)
                })
                .collect();
            let row_bg = if ri % 2 == 1 { bg_zebra } else { bg_card };
            let data_row = container(row(cells).spacing(0).align_y(Alignment::Start))
                .style(move |_| container::Style {
                    background: Some(Background::Color(row_bg)),
                    ..container::Style::default()
                });
            table_col = table_col.push(data_row);
        }

        let pal_scroll = self.palette.clone();
        let h_scroll = scrollable(table_col)
            .width(Fill)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default()
                    .width(5)
                    .scroller_width(5),
            ))
            .style(move |_, status| pal_scroll.subtle_scrollable_style(status));

        // 하단 우측 복사 바
        let raw_md = tbl.raw_markdown.clone();
        let footer = container(
            row![
                space::horizontal(),
                button(
                    row![
                        Icon::Copy.view(12.0, text_secondary),
                        text(t(self.locale, "표 복사"))
                            .size(11.5)
                            .color(text_secondary),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::CopyText(raw_md))
                .padding([3, 8])
                .style(move |_, status| {
                    let hovered = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: if hovered {
                            Some(Background::Color(bg_hover))
                        } else {
                            None
                        },
                        text_color: text_secondary,
                        border: Border {
                            radius: 6.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }
                }),
            ]
            .align_y(Alignment::Center),
        )
        .padding([4, 8])
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_header)),
            border: Border {
                radius: iced::border::Radius {
                    top_left: 0.0,
                    top_right: 0.0,
                    bottom_right: 11.0,
                    bottom_left: 11.0,
                },
                ..Border::default()
            },
            ..container::Style::default()
        });

        let footer_div = container(space::horizontal())
            .width(Fill)
            .height(Length::Fixed(1.0))
            .style(move |_| container::Style {
                background: Some(Background::Color(border_sub)),
                ..container::Style::default()
            });

        container(column![h_scroll, footer_div, footer].width(Fill))
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_card)),
                border: Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: border_col,
                },
                ..container::Style::default()
            })
            .into()
    }

    fn render_table<'a>(
        &self,
        settings: markdown::Settings,
        columns: &'a [markdown::Column],
        rows: &'a [markdown::Row],
    ) -> Element<'a, Message> {
        let cell_viewer = Self {
            in_table: true,
            ..self.clone()
        };
        let table_settings = markdown::Settings {
            text_size: 13.5.into(),
            spacing: 10.0.into(),
            ..settings
        };
        let table_el = markdown::table(&cell_viewer, table_settings, columns, rows);
        let bg = self.palette.bg_composer;
        let border_col = self.palette.border;
        container(table_el)
            .padding([6, 10])
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    radius: 10.0.into(),
                    width: 1.0,
                    color: border_col,
                },
                ..container::Style::default()
            })
            .into()
    }

    fn render_generated_image_slot<'a>(
        &self,
        img_idx: usize,
        ctx: &RenderContext<'a>,
    ) -> Element<'a, Message> {
        let loc = self.locale;
        let bg_card = self.palette.bg_composer;
        let bg_thought = self.palette.bg_thought;
        let bg_hover = self.palette.bg_hover;
        let border_col = self.palette.border;
        let text_primary = self.palette.text_primary;
        let text_secondary = self.palette.text_secondary;
        let accent = self.palette.accent;

        let matched = ctx.generated_images.get(img_idx);

        let Some(img_meta) = matched else {
            // 서버 미디어 매칭이 없는 경우에도 가짜 googleusercontent URL 대신 안내 칩을 표시한다.
            return container(
                row![
                    Icon::Image.view(16.0, text_secondary),
                    text(t(loc, "생성된 이미지"))
                        .size(13)
                        .font(font_medium())
                        .color(text_primary),
                    space::horizontal(),
                    text(t(loc, "웹 Gemini에서 보기"))
                        .size(12)
                        .color(text_secondary),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([10, 14])
            .max_width(420.0)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_thought)),
                border: Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: border_col,
                },
                ..container::Style::default()
            })
            .into();
        };

        let cache_key = generated_image_cache_key(&img_meta.url);
        let title_str = if img_meta.title.trim().is_empty() {
            t(loc, "생성된 이미지").to_string()
        } else {
            img_meta.title.clone()
        };

        let media_body: Element<'a, Message> = if let Some(handle) = ctx.image_cache.get(&cache_key)
        {
            container(
                image(handle.clone())
                    .width(Length::Fill)
                    .height(Length::Fixed(340.0))
                    .content_fit(iced::ContentFit::Contain),
            )
            .padding(6)
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_thought)),
                border: Border {
                    radius: iced::border::Radius {
                        top_left: 13.0,
                        top_right: 13.0,
                        bottom_right: 0.0,
                        bottom_left: 0.0,
                    },
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into()
        } else {
            container(
                row![
                    Icon::Image.view(18.0, text_secondary),
                    text(t(loc, "이미지 불러오는 중..."))
                        .size(13)
                        .color(text_secondary),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .height(Length::Fixed(180.0))
            .center(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_thought)),
                border: Border {
                    radius: iced::border::Radius {
                        top_left: 13.0,
                        top_right: 13.0,
                        bottom_right: 0.0,
                        bottom_left: 0.0,
                    },
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into()
        };

        let open_url = img_meta.url.clone();
        let footer = container(
            row![
                Icon::Image.view(14.0, accent),
                text(title_str)
                    .size(12.5)
                    .font(font_medium())
                    .color(text_primary),
                space::horizontal(),
                button(text(t(loc, "원본 열기")).size(12).color(accent))
                    .on_press(Message::LinkClicked(open_url))
                    .padding([4, 10])
                    .style(move |_, status| {
                        let hovered = matches!(status, button::Status::Hovered);
                        button::Style {
                            background: if hovered {
                                Some(Background::Color(bg_hover))
                            } else {
                                None
                            },
                            text_color: accent,
                            border: Border {
                                radius: 8.0.into(),
                                ..Border::default()
                            },
                            ..button::Style::default()
                        }
                    }),
            ]
            .spacing(7)
            .align_y(Alignment::Center),
        )
        .padding([8, 12])
        .width(Fill);

        container(column![media_body, footer].width(Fill))
            .max_width(480.0)
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_card)),
                border: Border {
                    radius: 14.0.into(),
                    width: 1.0,
                    color: border_col,
                },
                ..container::Style::default()
            })
            .into()
    }

    fn render_canvas_card<'a>(
        &self,
        canvas_idx: usize,
        ctx: &RenderContext<'a>,
    ) -> Element<'a, Message> {
        let loc = self.locale;
        let bg_card = self.palette.bg_composer;
        let bg_hover = self.palette.bg_hover;
        let accent = self.palette.accent;
        let accent_soft = self.palette.accent_soft;
        let border_col = self.palette.border;
        let text_strong = self.palette.text_strong;
        let text_secondary = self.palette.text_secondary;

        let art_opt = ctx.artifacts.get(canvas_idx);
        let resolved_idx = canvas_idx;

        let Some(art) = art_opt else {
            return container(
                row![
                    Icon::Canvas.view(16.0, accent),
                    text(t(loc, "Canvas 문서"))
                        .size(13.5)
                        .font(font_medium())
                        .color(text_strong),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([12, 16])
            .max_width(460.0)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_card)),
                border: Border {
                    radius: 14.0.into(),
                    width: 1.0,
                    color: border_col,
                },
                ..container::Style::default()
            })
            .into();
        };

        let title = if art.title.trim().is_empty() {
            t(loc, "Canvas 문서").to_string()
        } else {
            art.title.clone()
        };
        let line_count = art.content.lines().count();
        let is_md = is_markdown_artifact(art);
        let lang_label = if !art.language.is_empty() {
            art.language.as_str()
        } else if is_md {
            "Markdown"
        } else {
            ""
        };
        let meta_sub = if !lang_label.is_empty() && line_count > 0 {
            if loc.is_en() {
                format!("Canvas · {} · {} lines", lang_label, line_count)
            } else {
                format!("Canvas · {} · {}줄", lang_label, line_count)
            }
        } else if !lang_label.is_empty() {
            format!("Canvas · {}", lang_label)
        } else if line_count > 0 {
            if loc.is_en() {
                format!("Canvas · {} lines", line_count)
            } else {
                format!("Canvas · {}줄", line_count)
            }
        } else {
            "Canvas".to_string()
        };

        let icon_badge = container(Icon::Canvas.view(18.0, accent))
            .width(Length::Fixed(36.0))
            .height(Length::Fixed(36.0))
            .center(Length::Fixed(36.0))
            .style(move |_| container::Style {
                background: Some(Background::Color(accent_soft)),
                border: Border {
                    radius: 10.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            });

        let mut actions = row![].spacing(6).align_y(Alignment::Center);
        if !art.content.is_empty() {
            let open_label = if is_md {
                t(loc, "문서 보기")
            } else {
                t(loc, "코드 보기")
            };
            actions = actions.push(
                button(text(open_label).size(12).color(text_strong))
                    .on_press(Message::OpenCanvasPreview(ctx.message_idx, resolved_idx))
                    .padding([6, 12])
                    .style(move |_, status| {
                        let hovered = matches!(status, button::Status::Hovered);
                        button::Style {
                            background: Some(Background::Color(if hovered {
                                bg_hover
                            } else {
                                accent_soft
                            })),
                            text_color: text_strong,
                            border: Border {
                                radius: 10.0.into(),
                                ..Border::default()
                            },
                            ..button::Style::default()
                        }
                    }),
            );
        }

        let head_lower = art.content.trim_start().to_ascii_lowercase();
        let is_html = art.language.eq_ignore_ascii_case("html")
            || art.language.eq_ignore_ascii_case("htm")
            || head_lower.starts_with("<!doctype html")
            || head_lower.starts_with("<html");
        if is_html && !art.content.is_empty() {
            actions = actions.push(
                button(text(t(loc, "브라우저에서 실행")).size(12).color(accent))
                    .on_press(Message::RunCanvasHtml(ctx.message_idx, resolved_idx))
                    .padding([6, 12])
                    .style(move |_, status| {
                        let hovered = matches!(status, button::Status::Hovered);
                        button::Style {
                            background: Some(Background::Color(if hovered {
                                bg_hover
                            } else {
                                Color::TRANSPARENT
                            })),
                            text_color: accent,
                            border: Border {
                                radius: 10.0.into(),
                                width: 1.0,
                                color: border_col,
                            },
                            ..button::Style::default()
                        }
                    }),
            );
        } else if !art.url.is_empty() {
            actions = actions.push(
                button(text(t(loc, "웹에서 열기")).size(12).color(accent))
                    .on_press(Message::LinkClicked(art.url.clone()))
                    .padding([6, 12])
                    .style(move |_, status| {
                        let hovered = matches!(status, button::Status::Hovered);
                        button::Style {
                            background: Some(Background::Color(if hovered {
                                bg_hover
                            } else {
                                Color::TRANSPARENT
                            })),
                            text_color: accent,
                            border: Border {
                                radius: 10.0.into(),
                                width: 1.0,
                                color: border_col,
                            },
                            ..button::Style::default()
                        }
                    }),
            );
        }

        container(
            row![
                icon_badge,
                column![
                    text(title)
                        .size(14)
                        .font(font_semibold())
                        .color(text_strong),
                    text(meta_sub).size(12).color(text_secondary),
                ]
                .spacing(2),
                space::horizontal(),
                actions,
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding([12, 16])
        .max_width(580.0)
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_card)),
            border: Border {
                radius: 14.0.into(),
                width: 1.0,
                color: border_col,
            },
            ..container::Style::default()
        })
        .into()
    }

    fn render_image<'a>(
        &self,
        settings: markdown::Settings,
        url: &'a markdown::Uri,
        title: &'a str,
        alt: &markdown::Text,
    ) -> Element<'a, Message> {
        let label = if !title.trim().is_empty() {
            title.to_string()
        } else {
            let alt_str: String = alt
                .spans(settings.style)
                .iter()
                .map(|sp| sp.text.as_ref())
                .collect();
            if alt_str.trim().is_empty() {
                url.clone()
            } else {
                alt_str
            }
        };
        let bg = self.palette.bg_chip;
        let bg_hover = self.palette.bg_hover;
        let border_col = self.palette.border_subtle;
        let fg = self.palette.accent;

        button(
            row![
                Icon::Image.view(14.0, fg),
                text(label).size(13).color(fg),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .on_press(Message::LinkClicked(url.clone()))
        .padding([6, 12])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if hovered { bg_hover } else { bg })),
                text_color: fg,
                border: Border {
                    radius: 8.0.into(),
                    width: 1.0,
                    color: border_col,
                },
                ..button::Style::default()
            }
        })
        .into()
    }
}

impl<'a> markdown::Viewer<'a, Message> for GeminiMarkdownViewer {
    fn on_link_click(url: markdown::Uri) -> Message {
        Message::LinkClicked(url)
    }

    fn image(
        &self,
        settings: markdown::Settings,
        url: &'a markdown::Uri,
        title: &'a str,
        alt: &markdown::Text,
    ) -> Element<'a, Message> {
        self.render_image(settings, url, title, alt)
    }

    fn heading(
        &self,
        settings: markdown::Settings,
        level: &'a markdown::HeadingLevel,
        text_item: &'a markdown::Text,
        index: usize,
    ) -> Element<'a, Message> {
        self.render_heading(settings, level, text_item, index)
    }

    fn paragraph(
        &self,
        settings: markdown::Settings,
        text_item: &markdown::Text,
    ) -> Element<'a, Message> {
        self.render_paragraph(settings, text_item)
    }

    fn code_block(
        &self,
        settings: markdown::Settings,
        language: Option<&'a str>,
        code: &'a str,
        lines: &'a [markdown::Text],
    ) -> Element<'a, Message> {
        self.render_code_block(settings, language, code, lines)
    }

    fn unordered_list(
        &self,
        settings: markdown::Settings,
        bullets: &'a [markdown::Bullet],
    ) -> Element<'a, Message> {
        self.render_unordered_list(settings, bullets, 0)
    }

    fn ordered_list(
        &self,
        settings: markdown::Settings,
        start: u64,
        bullets: &'a [markdown::Bullet],
    ) -> Element<'a, Message> {
        self.render_ordered_list(settings, start, bullets, 0)
    }

    fn quote(
        &self,
        settings: markdown::Settings,
        contents: &'a [markdown::Item],
    ) -> Element<'a, Message> {
        self.render_quote(settings, contents, 0)
    }

    fn rule(&self, _settings: markdown::Settings) -> Element<'a, Message> {
        self.render_rule()
    }

    fn table(
        &self,
        settings: markdown::Settings,
        columns: &'a [markdown::Column],
        rows: &'a [markdown::Row],
    ) -> Element<'a, Message> {
        self.render_table(settings, columns, rows)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MarkdownBlock, ParsedMarkdown, is_parenthesized_citation_span, preprocess_markdown,
        split_cell_br_lines,
    };
    use iced::widget::markdown;

    #[test]
    fn 한글_구두점_인접_볼드_파싱_보정() {
        let src = "**엔트로픽(Anthropic)**이 상장했고 가**“강화학습”**을 도입했습니다.";
        let processed = preprocess_markdown(src);
        assert!(processed.contains(")\u{200B}**이"));
        assert!(processed.contains("가**\u{200B}“강화학습”\u{200B}**을"));

        let content = markdown::Content::parse(&processed);
        let style = markdown::Style::from_palette(iced::Theme::Light.palette());
        let Some(markdown::Item::Paragraph(p)) = content.items().first() else {
            panic!("문단 아이템이 생성되어야 합니다");
        };
        let rendered: String = p.spans(style).iter().map(|s| s.text.as_ref()).collect();
        assert!(
            !rendered.contains("**"),
            "볼드 기호가 원문 그대로 노출되면 안 됩니다: {rendered}"
        );
    }

    #[test]
    fn 코드블록과_인라인코드는_전처리에서_보존() {
        let src = "본문 `**코드(괄호)**가` 끝\n```rust\nlet s = \"**유지(괄호)**함\";\n```\n";
        let processed = preprocess_markdown(src);
        assert_eq!(processed, src);
    }

    #[test]
    fn 일반_볼드는_불필요한_문자_없이_유지() {
        let src = "**무슨 일인지 (핵심 요약):** 일반 텍스트와 **강조** 문장입니다.";
        let processed = preprocess_markdown(src);
        assert_eq!(processed, src);
    }

    #[test]
    fn gfm_표와_셀_내부_br_줄바꿈_파싱() {
        let src = "요약입니다.\n\n| 구분 | 설명 |\n| --- | :--- |\n| **1. 단순 조회**<br>`README.md` 점검 | 첫째 줄<br/>둘째 줄 |\n\n끝.";
        let parsed = ParsedMarkdown::parse(src);
        assert_eq!(parsed.blocks.len(), 3);
        let MarkdownBlock::Table(tbl) = &parsed.blocks[1] else {
            panic!("가운데 블록이 Table이어야 합니다");
        };
        assert_eq!(tbl.headers.len(), 2);
        assert_eq!(tbl.rows.len(), 1);
        assert_eq!(tbl.rows[0][0].len(), 2);
        assert_eq!(tbl.rows[0][1].len(), 2);

        // 인라인 코드 내부의 `<br>`은 줄바꿈으로 쪼개지 않아야 한다
        assert_eq!(split_cell_br_lines("`<br>` 유지<br/>다음 줄"), vec!["`<br>` 유지", "다음 줄"]);
    }

    #[test]
    fn 생성_이미지와_canvas_플레이스홀더_분리() {
        let src = "결과입니다.\n\nhttp://googleusercontent.com/image_generation_content/0_868\n\n앱도 만들었습니다.\nhttp://googleusercontent.com/immersive_entry_chip/0\n";
        let parsed = ParsedMarkdown::parse(src);
        assert_eq!(parsed.image_ref_count, 1);
        assert_eq!(parsed.canvas_ref_count, 1);
        assert!(parsed
            .blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::GeneratedImageRef(0))));
        assert!(parsed
            .blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::CanvasRef(0))));
    }

    #[test]
    fn 괄호_출처_링크와_일반_본문_링크_구분() {
        let src = "[공식 문서](https://example.com)를 참고하세요 ([와우테일](https://wowtale.net), [연합뉴스](https://yna.co.kr)).";
        let content = markdown::Content::parse(src);
        let style = markdown::Style::from_palette(iced::Theme::Light.palette());
        let Some(markdown::Item::Paragraph(p)) = content.items().first() else {
            panic!("문단이 생성되어야 합니다");
        };
        let spans: Vec<_> = p.spans(style).iter().cloned().collect();
        let link_flags: Vec<(String, bool)> = spans
            .iter()
            .enumerate()
            .filter(|(_, s)| s.link.is_some())
            .map(|(i, s)| (s.text.to_string(), is_parenthesized_citation_span(&spans, i)))
            .collect();

        assert_eq!(
            link_flags,
            vec![
                ("공식 문서".to_string(), false),
                ("와우테일".to_string(), true),
                ("연합뉴스".to_string(), true),
            ]
        );
    }

    #[test]
    fn 일반_본문_br_줄바꿈_변환과_코드_내_보존() {
        let src = "첫째 줄<br>• 둘째 줄<BR />셋째 줄<br>\n넷째 줄 `inline <br> keep`\n```html\n<div><br/></div>\n```\n";
        let processed = preprocess_markdown(src);
        assert!(processed.contains("첫째 줄  \n• 둘째 줄  \n셋째 줄  \n넷째 줄"));
        assert!(processed.contains("`inline <br> keep`"));
        assert!(processed.contains("<div><br/></div>"));
    }

    #[test]
    fn 짧은_표_열_너비가_가용_폭_이내로_유지() {
        let src = "| 항목 | 값 |\n| --- | ---: |\n| A | 10 |\n| B | 20 |\n";
        let parsed = ParsedMarkdown::parse(src);
        let MarkdownBlock::Table(tbl) = &parsed.blocks[0] else {
            panic!("표 블록이어야 합니다");
        };
        let total: f32 = tbl.col_widths.iter().sum();
        assert!(
            total <= 685.0,
            "짧은 표의 전체 폭({total})이 가용 폭 이내여야 합니다"
        );
    }

    #[test]
    fn 마크다운_canvas_아티팩트_판별() {
        use super::is_markdown_artifact;
        use crate::ipc::TurnArtifact;

        let report = TurnArtifact {
            id: String::new(),
            title: "심층 리포트".into(),
            language: String::new(),
            content: "# **2026년 복합기 비교**\n\n본문 요약입니다.".into(),
            url: String::new(),
        };
        assert!(is_markdown_artifact(&report));

        let html = TurnArtifact {
            id: String::new(),
            title: "게임".into(),
            language: String::new(),
            content: "<!DOCTYPE html><html><body>hi</body></html>".into(),
            url: String::new(),
        };
        assert!(!is_markdown_artifact(&html));

        let py = TurnArtifact {
            id: String::new(),
            title: "script.py".into(),
            language: "python".into(),
            content: "# comment\nprint('hi')".into(),
            url: String::new(),
        };
        assert!(!is_markdown_artifact(&py));
    }
}
