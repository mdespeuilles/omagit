//! The diff viewer — "la pièce maîtresse", in board 03's own words.
//!
//! Four things layer on one line of text, and the order they compose in is the
//! whole design (DESIGN §4):
//!
//! 1. the **line background** at 12% — `diff_added` / `diff_deleted`;
//! 2. the **intra-line refinement** at 24% — `diff_added_word` /
//!    `diff_deleted_word`, over the words that actually changed;
//! 3. the **syntax colour**, from the five roles of [`crate::syntax`];
//! 4. the **sign and the gutter**, which carry the meaning when none of the
//!    above do — a greyscale check has to stay readable, so `+` and `−` and the
//!    two number columns are not decoration.
//!
//! ## Virtualised, and only the visible range is highlighted
//!
//! SPEC §12 requires both. The rows are flattened once per file into a `Vec`
//! and handed to the renderer's virtual list, so a 40 000-line diff costs the
//! same as a short one; the syntax query then runs over *exactly* the lines
//! being drawn, which is why scrolling does not get slower further down.
//!
//! ## Two views of the same rows
//!
//! Unified and side-by-side are the same hunks arranged differently, not two
//! renderers: [`Mode`] changes how the rows are built and nothing else. That is
//! what keeps the two from drifting — a fix to the refinement or the gutter
//! lands in both by construction.

use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::base::{VirtualListScrollHandle, v_virtual_list};
use gpui_kit::prelude::*;
use gpui_kit::{
    Context, HighlightStyle, IntoElement, Pixels, SharedString, Size, StyledText, Window, div, px,
};

use omagit_git::diff::{DiffContent, FileChange, FileDiff, Hunk, Line, LineKind, Sides};
use omagit_theme::Rgb;

use crate::syntax::{Highlighter, Span};
use crate::{ActiveFonts, ActivePalette, Fonts, Palette, hsla};

/// One diff line, in pixels.
///
/// Fixed rather than derived from density: DESIGN §3 fixes the mono face at
/// 12.5px, and a virtual list wants a height it can multiply. 18px is that text
/// with the leading the mock-up draws.
const LINE_HEIGHT: f32 = 18.0;

/// The gutter's two number columns.
const NUMBER_WIDTH: f32 = 44.0;

/// What a tab is worth. Expanding them here rather than leaving them to the
/// text system is what keeps a gutter-aligned diff aligned; the spans are
/// remapped with the text so nothing drifts (see [`expand_tabs`]).
const TAB_WIDTH: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Unified,
    SideBySide,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Unified => "Unifié",
            Mode::SideBySide => "Côte à côte",
        }
    }
}

/// A row of the flattened diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    /// A `@@` header.
    Header(usize),
    /// The context between two hunks that is not shown.
    Fold { lines: u32 },
    /// One line, unified.
    Unified { hunk: usize, line: usize },
    /// One line on each side; either may be absent.
    Split {
        hunk: usize,
        old: Option<usize>,
        new: Option<usize>,
    },
}

/// The facts the header states about the file: `Rust · UTF-8 · LF`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    pub language: Option<&'static str>,
    pub utf8: bool,
    pub crlf: bool,
}

impl Facts {
    fn of(path: &[u8], text: &[u8]) -> Self {
        Self {
            language: Highlighter::language_of(path),
            utf8: std::str::from_utf8(text).is_ok(),
            // A file is CRLF if its first terminator is; mixed endings are
            // reported as LF, which is what Git normalises them to anyway.
            crlf: text
                .iter()
                .position(|byte| *byte == b'\n')
                .is_some_and(|at| at > 0 && text[at - 1] == b'\r'),
        }
    }

    fn label(&self) -> String {
        let mut parts = Vec::new();
        if let Some(language) = self.language {
            parts.push(language.to_owned());
        }
        parts.push(if self.utf8 { "UTF-8" } else { "binaire ?" }.to_owned());
        parts.push(if self.crlf { "CRLF" } else { "LF" }.to_owned());
        parts.join(" · ")
    }
}

/// The diff of one file, on screen.
pub struct DiffView {
    file: Option<FileDiff>,
    mode: Mode,
    rows: Vec<Row>,
    sizes: Rc<Vec<Size<Pixels>>>,
    /// The two sides, parsed. `None` when there is no grammar for the file,
    /// which is a supported outcome rather than a failure.
    old_syntax: Option<Highlighter>,
    new_syntax: Option<Highlighter>,
    facts: Option<Facts>,
    scroll: VirtualListScrollHandle,
}

impl DiffView {
    pub fn new() -> Self {
        Self {
            file: None,
            mode: Mode::Unified,
            rows: Vec::new(),
            sizes: Rc::new(Vec::new()),
            old_syntax: None,
            new_syntax: None,
            facts: None,
            scroll: VirtualListScrollHandle::new(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn path(&self) -> Option<&omagit_git::RepoPath> {
        self.file.as_ref().map(|file| &file.path)
    }

    /// Show a file, or nothing.
    ///
    /// Parsing happens here rather than per frame: it is the one expensive step
    /// and it depends only on the file.
    pub fn show(&mut self, file: Option<FileDiff>, cx: &mut Context<Self>) {
        let same = match (&self.file, &file) {
            (Some(current), Some(next)) => current.path == next.path,
            _ => false,
        };
        self.file = file;
        self.old_syntax = None;
        self.new_syntax = None;
        self.facts = None;

        if let Some(file) = &self.file
            && let DiffContent::Text { sides, .. } = &file.content
        {
            let path = file.path.as_bytes();
            self.old_syntax = Highlighter::new(path, &sides.old);
            self.new_syntax = Highlighter::new(path, &sides.new);
            self.facts = Some(Facts::of(path, &sides.new));
        }

        self.rebuild();
        if !same {
            // A different file starts at the top; the same file re-read — after
            // a save, say — keeps the reader where they were.
            self.scroll.scroll_to_item(0, gpui_kit::ScrollStrategy::Top);
        }
        cx.notify();
    }

    pub fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.mode = mode;
            self.rebuild();
            cx.notify();
        }
    }

    /// Flatten the hunks into rows for the current mode.
    fn rebuild(&mut self) {
        self.rows = match (&self.file, self.mode) {
            (Some(file), mode) => match &file.content {
                DiffContent::Text { hunks, .. } => rows_of(hunks, mode),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        self.sizes = Rc::new(vec![
            Size {
                width: px(0.0),
                height: px(LINE_HEIGHT),
            };
            self.rows.len()
        ]);
    }

    fn hunks(&self) -> &[Hunk] {
        match &self.file {
            Some(FileDiff {
                content: DiffContent::Text { hunks, .. },
                ..
            }) => hunks,
            _ => &[],
        }
    }

    fn line(&self, hunk: usize, line: usize) -> Option<&Line> {
        self.hunks().get(hunk)?.lines.get(line)
    }

    /// The syntax spans for every source line the visible rows touch.
    ///
    /// One query per side per frame, over the visible range only — not one per
    /// line, and not the whole file.
    fn visible_spans(&self, rows: &[Row]) -> (LineSpans, LineSpans) {
        let mut old_range: Option<(usize, usize)> = None;
        let mut new_range: Option<(usize, usize)> = None;
        let extend = |range: &mut Option<(usize, usize)>, number: Option<u32>| {
            if let Some(number) = number {
                let index = number.saturating_sub(1) as usize;
                *range = Some(match *range {
                    None => (index, index + 1),
                    Some((from, to)) => (from.min(index), to.max(index + 1)),
                });
            }
        };

        for row in rows {
            match *row {
                Row::Unified { hunk, line } => {
                    if let Some(line) = self.line(hunk, line) {
                        extend(&mut old_range, line.old_number);
                        extend(&mut new_range, line.new_number);
                    }
                }
                Row::Split { hunk, old, new } => {
                    if let Some(line) = old.and_then(|line| self.line(hunk, line)) {
                        extend(&mut old_range, line.old_number);
                    }
                    if let Some(line) = new.and_then(|line| self.line(hunk, line)) {
                        extend(&mut new_range, line.new_number);
                    }
                }
                Row::Header(_) | Row::Fold { .. } => {}
            }
        }

        let query = |highlighter: &Option<Highlighter>, range: Option<(usize, usize)>| match (
            highlighter,
            range,
        ) {
            (Some(highlighter), Some((from, to))) => highlighter.spans(from..to),
            _ => HashMap::new(),
        };
        (
            query(&self.old_syntax, old_range),
            query(&self.new_syntax, new_range),
        )
    }
}

type LineSpans = HashMap<usize, Vec<Span>>;

impl Default for DiffView {
    fn default() -> Self {
        Self::new()
    }
}

/// Flatten hunks into rows, inserting a fold marker wherever context was left
/// out between two of them.
fn rows_of(hunks: &[Hunk], mode: Mode) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut previous_end: Option<u32> = None;

    for (index, hunk) in hunks.iter().enumerate() {
        if let Some(end) = previous_end {
            let hidden = hunk.new_start.saturating_sub(end);
            if hidden > 0 {
                rows.push(Row::Fold { lines: hidden });
            }
        }
        rows.push(Row::Header(index));
        match mode {
            Mode::Unified => {
                rows.extend((0..hunk.lines.len()).map(|line| Row::Unified { hunk: index, line }))
            }
            Mode::SideBySide => rows.extend(pair_lines(&hunk.lines).into_iter().map(
                |(old, new)| Row::Split {
                    hunk: index,
                    old,
                    new,
                },
            )),
        }
        previous_end = Some(hunk.new_start + hunk.new_lines);
    }
    rows
}

/// Pair a hunk's lines for side-by-side.
///
/// Removed lines sit opposite added ones in the order they appear, which is the
/// same pairing the intra-line refinement used — so a word highlighted on the
/// left is opposite the word it became. Where one side runs out, the other
/// keeps going against a blank.
fn pair_lines(lines: &[Line]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut pairs = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        match lines[index].kind {
            LineKind::Context => {
                pairs.push((Some(index), Some(index)));
                index += 1;
            }
            _ => {
                let removed_from = index;
                while index < lines.len() && lines[index].kind == LineKind::Removed {
                    index += 1;
                }
                let removed = removed_from..index;
                let added_from = index;
                while index < lines.len() && lines[index].kind == LineKind::Added {
                    index += 1;
                }
                let added = added_from..index;

                let height = removed.len().max(added.len());
                for offset in 0..height {
                    pairs.push((removed.clone().nth(offset), added.clone().nth(offset)));
                }
            }
        }
    }
    pairs
}

/// Replace tabs with spaces, and report where every original byte ended up.
///
/// The map is the point: syntax spans and refinement ranges are offsets into
/// the *original* line, and a diff that expanded tabs without moving them would
/// highlight the wrong characters on every indented line.
fn expand_tabs(line: &[u8]) -> (String, Vec<usize>) {
    let text = String::from_utf8_lossy(line);
    let mut out = String::with_capacity(text.len());
    let mut map = Vec::with_capacity(text.len() + 1);
    let mut column = 0;

    for (offset, character) in text.char_indices() {
        while map.len() <= offset {
            map.push(out.len());
        }
        if character == '\t' {
            let stop = TAB_WIDTH - (column % TAB_WIDTH);
            out.push_str(&" ".repeat(stop));
            column += stop;
        } else {
            out.push(character);
            column += 1;
        }
    }
    while map.len() <= text.len() {
        map.push(out.len());
    }
    (out, map)
}

impl Render for DiffView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette().clone();
        let fonts = cx.fonts().clone();
        let t = palette.tokens;

        let Some(file) = self.file.clone() else {
            return placeholder("Sélectionne un fichier pour voir son diff.", &palette)
                .into_any_element();
        };

        let body = match &file.content {
            DiffContent::Text { .. } if self.rows.is_empty() => {
                placeholder("Aucune modification à afficher.", &palette).into_any_element()
            }
            DiffContent::Text { .. } => self.lines(&palette, &fonts, cx).into_any_element(),
            DiffContent::Binary {
                old_bytes,
                new_bytes,
            } => placeholder_owned(
                format!("Fichier binaire · {old_bytes} octets → {new_bytes} octets"),
                &palette,
            )
            .into_any_element(),
            DiffContent::Oversized { bytes } => placeholder_owned(
                format!(
                    "Fichier trop volumineux pour être affiché · {} Mio",
                    bytes / (1024 * 1024)
                ),
                &palette,
            )
            .into_any_element(),
            DiffContent::Submodule { .. } => {
                placeholder("Sous-module — hors périmètre (SPEC §11).", &palette).into_any_element()
            }
            DiffContent::Empty => {
                placeholder("Aucun changement de contenu.", &palette).into_any_element()
            }
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .bg(hsla(t.bg))
            .child(self.header(&file, &palette, &fonts, cx))
            .child(body)
            .into_any_element()
    }
}

impl DiffView {
    fn header(
        &self,
        file: &FileDiff,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        let (added, removed, chunks) = match &file.content {
            DiffContent::Text {
                added,
                removed,
                hunks,
                ..
            } => (*added, *removed, hunks.len()),
            _ => (0, 0, 0),
        };
        let path = file.path.to_string();
        let (directory, name) = match path.rsplit_once('/') {
            Some((directory, name)) => (format!("{directory}/"), name.to_owned()),
            None => (String::new(), path.clone()),
        };

        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .flex_none()
            .h(px(32.0))
            .px(px(12.0))
            .border_b_1()
            .border_color(hsla(t.border))
            .child(
                div()
                    .font_family(fonts.mono.clone())
                    .text_size(px(11.0))
                    .text_color(hsla(t.text_muted))
                    .child(SharedString::from(change_letter(&file.change).to_string())),
            )
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .min_w_0()
                    .font_family(fonts.mono.clone())
                    .text_size(px(12.5))
                    .child(
                        div()
                            .text_color(hsla(t.text_muted))
                            .child(SharedString::from(directory)),
                    )
                    .child(div().truncate().child(SharedString::from(name))),
            )
            .child(div().flex_1())
            // The two views of the same hunks. A button group rather than a
            // menu: it is a two-way switch used constantly.
            .child(
                div()
                    .flex()
                    .border_1()
                    .border_color(hsla(t.border))
                    .children([Mode::Unified, Mode::SideBySide].map(|mode| {
                        let selected = self.mode == mode;
                        div()
                            .id(("mode", mode as usize))
                            .flex()
                            .items_center()
                            .h(px(22.0))
                            .px(px(10.0))
                            .text_size(px(12.0))
                            .when(selected, |element| {
                                element.bg(hsla(t.accent)).text_color(hsla(t.bg))
                            })
                            .when(!selected, |element| {
                                element
                                    .text_color(hsla(t.text_muted))
                                    .hover(|style| style.bg(hsla(t.surface_hover)))
                            })
                            .on_click(cx.listener(move |view, _, _, cx| view.set_mode(mode, cx)))
                            .child(mode.label())
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .flex_none()
                    .font_family(fonts.mono.clone())
                    .text_size(px(11.0))
                    .child(
                        div()
                            .text_color(hsla(t.text_dim))
                            .child(SharedString::from(format!("{chunks} chunks"))),
                    )
                    .child(
                        div()
                            .text_color(hsla(t.success))
                            .child(SharedString::from(format!("+{added}"))),
                    )
                    .child(
                        div()
                            .text_color(hsla(t.danger))
                            .child(SharedString::from(format!("−{removed}"))),
                    )
                    .children(self.facts.as_ref().map(|facts| {
                        div()
                            .text_color(hsla(t.text_dim))
                            .child(SharedString::from(facts.label()))
                    })),
            )
    }

    fn lines(
        &mut self,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = palette.clone();
        let fonts = fonts.clone();
        let sizes = Rc::clone(&self.sizes);

        div().flex_1().min_h_0().min_w_0().child(
            v_virtual_list(
                cx.entity(),
                "diff-lines",
                sizes,
                move |view, range, _, _| {
                    let rows: Vec<Row> = view.rows[range.clone()].to_vec();
                    // One syntax query per side for the whole visible window,
                    // then a lookup per line (SPEC §12).
                    let (old_spans, new_spans) = view.visible_spans(&rows);
                    rows.iter()
                        .map(|row| view.row(*row, &old_spans, &new_spans, &palette, &fonts))
                        .collect::<Vec<_>>()
                },
            )
            .track_scroll(&self.scroll)
            .w_full()
            .h_full(),
        )
    }

    fn row(
        &self,
        row: Row,
        old_spans: &LineSpans,
        new_spans: &LineSpans,
        palette: &Palette,
        fonts: &Fonts,
    ) -> gpui_kit::AnyElement {
        let t = palette.tokens;
        match row {
            Row::Header(index) => {
                let header = self
                    .hunks()
                    .get(index)
                    .map(Hunk::header)
                    .unwrap_or_default();
                div()
                    .flex()
                    .items_center()
                    .h(px(LINE_HEIGHT))
                    .w_full()
                    .px(px(8.0))
                    .bg(hsla(t.surface))
                    .font_family(fonts.mono.clone())
                    .text_size(px(11.0))
                    .text_color(hsla(t.text_muted))
                    .child(SharedString::from(header))
                    .into_any_element()
            }
            Row::Fold { lines } => div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .h(px(LINE_HEIGHT))
                .w_full()
                .px(px(8.0))
                .bg(hsla(t.surface))
                .font_family(fonts.mono.clone())
                .text_size(px(11.0))
                .text_color(hsla(t.text_dim))
                .child(SharedString::from(format!(
                    "⌄ {lines} ligne{} de contexte repliée{}",
                    if lines > 1 { "s" } else { "" },
                    if lines > 1 { "s" } else { "" }
                )))
                .into_any_element(),
            Row::Unified { hunk, line } => {
                let Some(line) = self.line(hunk, line) else {
                    return div().h(px(LINE_HEIGHT)).into_any_element();
                };
                let spans = spans_for(line, old_spans, new_spans);
                div()
                    .flex()
                    .items_center()
                    .h(px(LINE_HEIGHT))
                    .w_full()
                    .bg(hsla(line_background(line.kind, palette)))
                    .child(number(line.old_number, palette, fonts))
                    .child(number(line.new_number, palette, fonts))
                    .child(sign(line.kind, palette, fonts))
                    .child(text_of(line, spans, palette, fonts))
                    .into_any_element()
            }
            Row::Split { hunk, old, new } => {
                let old_line = old.and_then(|index| self.line(hunk, index));
                let new_line = new.and_then(|index| self.line(hunk, index));
                div()
                    .flex()
                    .h(px(LINE_HEIGHT))
                    .w_full()
                    .child(side(old_line, true, old_spans, new_spans, palette, fonts))
                    .child(div().w(px(1.0)).h_full().flex_none().bg(hsla(t.border)))
                    .child(side(new_line, false, old_spans, new_spans, palette, fonts))
                    .into_any_element()
            }
        }
    }
}

/// One half of a side-by-side row.
fn side(
    line: Option<&Line>,
    old_side: bool,
    old_spans: &LineSpans,
    new_spans: &LineSpans,
    palette: &Palette,
    fonts: &Fonts,
) -> gpui_kit::Div {
    let t = palette.tokens;
    let Some(line) = line else {
        // A blank opposite an insertion or a deletion. Tinted, so the eye sees
        // that the *file* has nothing here rather than that the line is empty.
        return div().flex_1().min_w_0().h_full().bg(hsla(t.surface));
    };
    let number_of = if old_side {
        line.old_number
    } else {
        line.new_number
    };
    div()
        .flex()
        .items_center()
        .flex_1()
        .min_w_0()
        .h_full()
        .bg(hsla(line_background(line.kind, palette)))
        .child(number(number_of, palette, fonts))
        .child(sign(line.kind, palette, fonts))
        .child(text_of(
            line,
            spans_for(line, old_spans, new_spans),
            palette,
            fonts,
        ))
}

/// The syntax spans for a line, taken from the side it belongs to.
fn spans_for<'a>(
    line: &Line,
    old_spans: &'a LineSpans,
    new_spans: &'a LineSpans,
) -> Option<&'a Vec<Span>> {
    match line.kind {
        // A context line exists on both sides and is identical on both, so
        // either answers; the new side is the one that is current.
        LineKind::Added | LineKind::Context => new_spans.get(&index_of(line.new_number)?),
        LineKind::Removed => old_spans.get(&index_of(line.old_number)?),
    }
}

fn index_of(number: Option<u32>) -> Option<usize> {
    number.map(|number| number.saturating_sub(1) as usize)
}

/// The line's own 12% background.
fn line_background(kind: LineKind, palette: &Palette) -> Rgb {
    let t = palette.tokens;
    match kind {
        LineKind::Added => t.diff_added,
        LineKind::Removed => t.diff_deleted,
        LineKind::Context => t.bg,
    }
}

/// One gutter column.
fn number(value: Option<u32>, palette: &Palette, fonts: &Fonts) -> gpui_kit::Div {
    div()
        .w(px(NUMBER_WIDTH))
        .flex_none()
        .pr(px(8.0))
        .text_right()
        .font_family(fonts.mono.clone())
        .text_size(px(11.0))
        .text_color(hsla(palette.tokens.text_dim))
        .child(SharedString::from(
            value.map(|value| value.to_string()).unwrap_or_default(),
        ))
}

/// `+`, `−` or a space.
///
/// Not decoration: DESIGN §1 requires additions and deletions to be
/// distinguishable without colour, and this is how.
fn sign(kind: LineKind, palette: &Palette, fonts: &Fonts) -> gpui_kit::Div {
    let t = palette.tokens;
    let (glyph, color) = match kind {
        LineKind::Added => ("+", t.success),
        LineKind::Removed => ("−", t.danger),
        LineKind::Context => (" ", t.text_dim),
    };
    div()
        .w(px(14.0))
        .flex_none()
        .text_center()
        .font_family(fonts.mono.clone())
        .text_size(px(12.5))
        .text_color(hsla(color))
        .child(glyph)
}

/// The line itself: tabs expanded, syntax coloured, changed words tinted.
fn text_of(
    line: &Line,
    spans: Option<&Vec<Span>>,
    palette: &Palette,
    fonts: &Fonts,
) -> gpui_kit::Div {
    let t = palette.tokens;
    let (text, map) = expand_tabs(&line.text);
    let translate = |offset: usize| *map.get(offset).unwrap_or(&text.len());

    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    for span in spans.into_iter().flatten() {
        let range = translate(span.start)..translate(span.end);
        if range.start < range.end {
            highlights.push((
                range,
                HighlightStyle {
                    color: Some(hsla(span.role.color(palette))),
                    ..Default::default()
                },
            ));
        }
    }
    // The 24% intra-line surface goes on last so it wins where it overlaps a
    // syntax span: what changed matters more than what it is.
    let word = match line.kind {
        LineKind::Added => Some(t.diff_added_word),
        LineKind::Removed => Some(t.diff_deleted_word),
        LineKind::Context => None,
    };
    if let Some(word) = word {
        for refinement in &line.refinements {
            let range = translate(refinement.start)..translate(refinement.end);
            if range.start < range.end {
                highlights.push((
                    range,
                    HighlightStyle {
                        background_color: Some(hsla(word)),
                        ..Default::default()
                    },
                ));
            }
        }
    }

    div()
        .flex()
        .items_center()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .font_family(fonts.mono.clone())
        .text_size(px(12.5))
        .text_color(hsla(t.text))
        .child(StyledText::new(SharedString::from(text)).with_highlights(highlights))
        .when(line.no_newline_at_eof, |element| {
            element.child(
                div()
                    .pl(px(8.0))
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(hsla(t.text_dim))
                    .child("↵ absent"),
            )
        })
}

fn change_letter(change: &FileChange) -> char {
    match change {
        FileChange::Added => 'A',
        FileChange::Deleted => 'D',
        FileChange::Modified => 'M',
        FileChange::ModeChanged => 'T',
        FileChange::Renamed { .. } => 'R',
        FileChange::Copied { .. } => 'C',
    }
}

fn placeholder(text: &'static str, palette: &Palette) -> gpui_kit::Div {
    placeholder_owned(text.to_owned(), palette)
}

fn placeholder_owned(text: String, palette: &Palette) -> gpui_kit::Div {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .text_size(px(12.0))
        .text_color(hsla(palette.tokens.text_dim))
        .child(SharedString::from(text))
}

/// How many lines each side of a text diff has — used by the screen to say
/// something sensible about a file before its diff is on screen.
pub fn line_counts(content: &DiffContent) -> Option<(usize, usize)> {
    match content {
        DiffContent::Text { sides, .. } => Some((
            Sides::lines(&sides.old).len(),
            Sides::lines(&sides.new).len(),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omagit_git::diff::Refinement;

    fn line(kind: LineKind, old: Option<u32>, new: Option<u32>, text: &str) -> Line {
        Line {
            kind,
            old_number: old,
            new_number: new,
            text: text.as_bytes().to_vec(),
            no_newline_at_eof: false,
            refinements: Vec::new(),
        }
    }

    fn hunk(lines: Vec<Line>, old_start: u32, new_start: u32) -> Hunk {
        let old_lines = lines
            .iter()
            .filter(|line| line.kind != LineKind::Added)
            .count() as u32;
        let new_lines = lines
            .iter()
            .filter(|line| line.kind != LineKind::Removed)
            .count() as u32;
        Hunk {
            old_start,
            old_lines,
            new_start,
            new_lines,
            lines,
        }
    }

    #[test]
    fn side_by_side_puts_a_change_opposite_itself() {
        let pairs = pair_lines(&[
            line(LineKind::Context, Some(1), Some(1), "a"),
            line(LineKind::Removed, Some(2), None, "before"),
            line(LineKind::Added, None, Some(2), "after"),
            line(LineKind::Context, Some(3), Some(3), "b"),
        ]);
        assert_eq!(
            pairs,
            vec![(Some(0), Some(0)), (Some(1), Some(2)), (Some(3), Some(3))],
            "the removed line and the added line share a row"
        );
    }

    #[test]
    fn an_uneven_change_leaves_blanks_on_the_shorter_side() {
        let pairs = pair_lines(&[
            line(LineKind::Removed, Some(1), None, "one"),
            line(LineKind::Added, None, Some(1), "one"),
            line(LineKind::Added, None, Some(2), "two"),
            line(LineKind::Added, None, Some(3), "three"),
        ]);
        assert_eq!(
            pairs,
            vec![(Some(0), Some(1)), (None, Some(2)), (None, Some(3))],
            "two of the three added lines face nothing"
        );
    }

    #[test]
    fn a_pure_insertion_has_no_left_side_at_all() {
        let pairs = pair_lines(&[
            line(LineKind::Added, None, Some(1), "new"),
            line(LineKind::Added, None, Some(2), "newer"),
        ]);
        assert_eq!(pairs, vec![(None, Some(0)), (None, Some(1))]);
    }

    #[test]
    fn the_gap_between_two_hunks_becomes_a_fold() {
        let first = hunk(vec![line(LineKind::Context, Some(1), Some(1), "a")], 1, 1);
        let second = hunk(
            vec![line(LineKind::Context, Some(30), Some(30), "b")],
            30,
            30,
        );
        let rows = rows_of(&[first, second], Mode::Unified);

        assert_eq!(rows[0], Row::Header(0));
        assert_eq!(rows[1], Row::Unified { hunk: 0, line: 0 });
        assert_eq!(
            rows[2],
            Row::Fold { lines: 28 },
            "lines 2 to 29 are not shown, and the reader is told how many"
        );
        assert_eq!(rows[3], Row::Header(1));
    }

    #[test]
    fn adjacent_hunks_have_no_fold_between_them() {
        let first = hunk(vec![line(LineKind::Context, Some(1), Some(1), "a")], 1, 1);
        let second = hunk(vec![line(LineKind::Context, Some(2), Some(2), "b")], 2, 2);
        let rows = rows_of(&[first, second], Mode::Unified);
        assert!(
            !rows.iter().any(|row| matches!(row, Row::Fold { .. })),
            "nothing is hidden, so nothing is announced: {rows:?}"
        );
    }

    #[test]
    fn tabs_expand_to_the_next_stop_and_take_their_offsets_with_them() {
        let (text, map) = expand_tabs(b"\tif x {");
        assert_eq!(text, "    if x {");
        // `if` starts at byte 1 in the original and byte 4 once expanded; a
        // highlight that did not move with it would colour the indentation.
        assert_eq!(map[1], 4);
        assert_eq!(&text[map[1]..map[3]], "if");

        // A tab mid-line goes to the next multiple of four, not four columns on.
        let (text, _) = expand_tabs(b"ab\tc");
        assert_eq!(text, "ab  c");
    }

    #[test]
    fn expanding_a_line_without_tabs_changes_nothing() {
        let (text, map) = expand_tabs(b"let x = 1;");
        assert_eq!(text, "let x = 1;");
        assert!(
            map.iter()
                .enumerate()
                .all(|(index, mapped)| index == *mapped),
            "offsets must be untouched when there is nothing to expand"
        );
    }

    #[test]
    fn the_refinement_survives_tab_expansion() {
        // The case the map exists for: an indented line whose changed word is
        // past the indentation.
        let mut changed = line(LineKind::Added, None, Some(1), "\tlet x = 2;");
        changed.refinements = vec![Refinement { start: 9, end: 10 }];
        let (text, map) = expand_tabs(&changed.text);
        assert_eq!(&text[map[9]..map[10]], "2");
    }

    #[test]
    fn a_files_facts_read_off_its_bytes() {
        let facts = Facts::of(b"src/main.rs", b"fn main() {}\n");
        assert_eq!(facts.label(), "Rust · UTF-8 · LF");

        let crlf = Facts::of(b"notes.txt", b"line\r\nline\r\n");
        assert_eq!(crlf.label(), "UTF-8 · CRLF", "no grammar, so no language");

        let latin1 = Facts::of(b"caf\xe9.rs", b"let x = \"caf\xe9\";\n");
        assert!(!latin1.utf8);
        assert!(latin1.label().contains("binaire ?"));
    }
}
