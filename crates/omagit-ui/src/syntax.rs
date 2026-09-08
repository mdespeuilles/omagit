//! Syntax highlighting for the diff viewer.
//!
//! DESIGN §4 is unusually specific about this, and the constraint is the
//! interesting part: **the highlighting may only use tokens that already
//! exist.** Keywords are `info`, types `warning`, functions `accent`, strings
//! `success`, comments `text_dim`. There is no syntax palette, no per-language
//! colouring, and nothing here may introduce one — a diff that invents six new
//! colours stops obeying the theme, and on Matte Black it stops being readable.
//!
//! Five roles is also why the mapping from a grammar's capture names is coarse
//! on purpose: `@function.method`, `@function.macro` and `@function` are all
//! *functions*, because the reader has five colours and no more.
//!
//! ## Whole files, not fragments
//!
//! A parser handed the twelve lines of a hunk produces nonsense: the fragment
//! is not a program. So the two *sides* of the diff are parsed in full — which
//! is why `omagit_git::diff::Sides` keeps them — and the highlights are indexed
//! by line, then handed to whichever diff lines came from that side.
//!
//! Parsing is per file and cached; the query runs over the **visible range
//! only**, which is what SPEC §12 asks for. On a 5 000-line file the parse is
//! about a millisecond and the query over forty visible lines is not
//! measurable.
//!
//! ## When there is no grammar
//!
//! Nothing happens, and that is a supported outcome rather than a gap to
//! apologise for: DESIGN §1's first rule is that hierarchy never rests on hue,
//! and a diff is legible in one colour — the `+`/`−` sign, the gutter and the
//! position carry it. So omagit ships a grammar per language it actually
//! renders and adds more when they are wanted, rather than a hundred C
//! compiles against the chance that one is.

use std::collections::HashMap;
use std::ops::Range;

use omagit_theme::Rgb;
use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator, Tree};

use crate::Palette;

/// What a span of source is, in the five terms DESIGN §4 allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Keyword,
    Type,
    Function,
    String,
    Comment,
}

impl Role {
    /// The token this role is drawn in. The only place the mapping exists.
    pub fn color(self, palette: &Palette) -> Rgb {
        let t = palette.tokens;
        match self {
            Role::Keyword => t.info,
            Role::Type => t.warning,
            Role::Function => t.accent,
            Role::String => t.success,
            Role::Comment => t.text_dim,
        }
    }

    /// Map a grammar's capture name onto a role.
    ///
    /// Deliberately by prefix: every grammar spells its captures differently
    /// past the first segment — `function.method`, `function.macro`,
    /// `string.special.path` — and all of them mean the same thing to a reader
    /// with five colours.
    fn of_capture(name: &str) -> Option<Self> {
        // Markdown's captures are typographic — `text.title`, `text.emphasis`,
        // `text.strong` — and DESIGN §4 has no token for any of them. Only two
        // say something in the five available roles, and both say the same
        // thing: this run is *literal*, not prose.
        if let "text.literal" | "text.uri" = name {
            return Some(Role::String);
        }
        let head = name.split('.').next().unwrap_or(name);
        Some(match head {
            "keyword" | "conditional" | "repeat" | "include" | "operator" | "storageclass" => {
                Role::Keyword
            }
            "type" | "constructor" => Role::Type,
            "function" | "method" => Role::Function,
            "string" | "character" => Role::String,
            "comment" => Role::Comment,
            _ => return None,
        })
    }
}

/// A highlighted span, as byte offsets into one line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub role: Role,
}

/// The languages omagit can highlight.
///
/// Chosen by what it renders rather than by popularity: this project is Rust,
/// TOML, JSON and Markdown, and those are the files whose diffs it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Grammar {
    Rust,
    Toml,
    Json,
    Markdown,
}

impl Grammar {
    /// Pick a grammar from a path, by extension and then by whole file name.
    fn of_path(path: &[u8]) -> Option<Self> {
        let text = String::from_utf8_lossy(path);
        let name = text.rsplit('/').next().unwrap_or(&text);
        // Whole names first: `Cargo.lock` has an extension and it lies.
        if let "Cargo.lock" = name {
            return Some(Grammar::Toml);
        }
        match name.rsplit_once('.').map(|(_, extension)| extension)? {
            "rs" => Some(Grammar::Rust),
            "toml" => Some(Grammar::Toml),
            "json" => Some(Grammar::Json),
            "md" | "markdown" => Some(Grammar::Markdown),
            _ => None,
        }
    }

    fn language(self) -> Language {
        match self {
            Grammar::Rust => tree_sitter_rust::LANGUAGE.into(),
            Grammar::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
            Grammar::Json => tree_sitter_json::LANGUAGE.into(),
            Grammar::Markdown => tree_sitter_md::LANGUAGE.into(),
        }
    }

    fn highlights(self) -> &'static str {
        match self {
            Grammar::Rust => tree_sitter_rust::HIGHLIGHTS_QUERY,
            Grammar::Toml => tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            Grammar::Json => tree_sitter_json::HIGHLIGHTS_QUERY,
            Grammar::Markdown => tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
        }
    }
}

/// One parsed file, ready to answer "what is on lines 40 to 80?".
pub struct Highlighter {
    tree: Tree,
    query: Query,
    source: Vec<u8>,
    /// Byte offset of the start of each line, so a line range becomes a byte
    /// range without re-scanning the file.
    line_starts: Vec<usize>,
}

impl Highlighter {
    /// Parse `source` as whatever `path` says it is.
    ///
    /// `None` when there is no grammar for it, when the file is too large to be
    /// worth parsing, or when the parse fails outright — all three mean the
    /// same thing to the caller, which is "draw it plain".
    pub fn new(path: &[u8], source: &[u8]) -> Option<Self> {
        // A parse is linear and fast, but a megabyte of minified JavaScript on
        // one line is not what this is for, and the reader cannot use the
        // result either.
        const MAX_BYTES: usize = 512 * 1024;
        if source.len() > MAX_BYTES {
            return None;
        }
        let grammar = Grammar::of_path(path)?;
        let language = grammar.language();

        let mut parser = Parser::new();
        parser.set_language(&language).ok()?;
        let tree = parser.parse(source, None)?;
        let query = Query::new(&language, grammar.highlights()).ok()?;

        let mut line_starts = vec![0];
        line_starts.extend(
            source
                .iter()
                .enumerate()
                .filter(|(_, byte)| **byte == b'\n')
                .map(|(index, _)| index + 1),
        );

        Some(Self {
            tree,
            query,
            source: source.to_vec(),
            line_starts,
        })
    }

    /// The spans on lines `range` (0-based, end exclusive), grouped by line.
    ///
    /// Only this range is queried — SPEC §12's "coloration syntaxique sur la
    /// plage visible uniquement" — which is what keeps scrolling a 40 000-line
    /// file the same cost as scrolling a short one.
    pub fn spans(&self, range: Range<usize>) -> HashMap<usize, Vec<Span>> {
        let mut out: HashMap<usize, Vec<Span>> = HashMap::new();
        let last_line = self.line_starts.len().saturating_sub(1);
        let start = range.start.min(last_line);
        let end = range.end.min(self.line_starts.len());
        if start >= end {
            return out;
        }

        let mut cursor = QueryCursor::new();
        cursor.set_point_range(tree_sitter::Point::new(start, 0)..tree_sitter::Point::new(end, 0));
        let names = self.query.capture_names();
        let mut matches =
            cursor.matches(&self.query, self.tree.root_node(), self.source.as_slice());

        while let Some(matched) = matches.next() {
            for capture in matched.captures() {
                let Some(role) = Role::of_capture(names[capture.index as usize]) else {
                    continue;
                };
                let node = capture.node;
                // A node can span several lines — a block comment, a string —
                // so it is cut at line boundaries and each piece is reported
                // against the line it belongs to.
                for line in node.start_position().row..=node.end_position().row {
                    if line < start || line >= end {
                        continue;
                    }
                    let Some(line_range) = self.line_range(line) else {
                        continue;
                    };
                    let from = node.start_byte().max(line_range.start);
                    let to = node.end_byte().min(line_range.end);
                    if from >= to {
                        continue;
                    }
                    out.entry(line).or_default().push(Span {
                        start: from - line_range.start,
                        end: to - line_range.start,
                        role,
                    });
                }
            }
        }

        // A later match wins where two overlap, which is tree-sitter's own
        // convention: queries are ordered from general to specific.
        for spans in out.values_mut() {
            spans.sort_by_key(|span| span.start);
        }
        out
    }

    /// The byte range of one line, without its terminator.
    fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let start = *self.line_starts.get(line)?;
        let end = self
            .line_starts
            .get(line + 1)
            .map(|next| next.saturating_sub(1))
            .unwrap_or(self.source.len());
        Some(start..end.max(start))
    }

    /// Whether a path can be highlighted at all — for the viewer's "Rust ·
    /// UTF-8 · LF" read-out, which should say what the file is even when
    /// nothing is coloured.
    pub fn language_of(path: &[u8]) -> Option<&'static str> {
        Some(match Grammar::of_path(path)? {
            Grammar::Rust => "Rust",
            Grammar::Toml => "TOML",
            Grammar::Json => "JSON",
            Grammar::Markdown => "Markdown",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans_of(path: &str, source: &str) -> HashMap<usize, Vec<Span>> {
        let highlighter =
            Highlighter::new(path.as_bytes(), source.as_bytes()).expect("a grammar for this path");
        highlighter.spans(0..source.lines().count().max(1))
    }

    /// The text a span covers, so assertions read as the source does.
    fn covered<'a>(source: &'a str, line: usize, span: &Span) -> &'a str {
        let text = source.lines().nth(line).expect("the line exists");
        &text[span.start..span.end]
    }

    #[test]
    fn colours_rust_in_the_five_roles_and_no_others() {
        let source = "// a comment\nfn total(items: &[u32]) -> u32 {\n    let name = \"x\";\n}\n";
        let spans = spans_of("src/lib.rs", source);

        let role_of = |line: usize, text: &str| {
            spans
                .get(&line)
                .into_iter()
                .flatten()
                .find(|span| covered(source, line, span) == text)
                .map(|span| span.role)
        };

        assert_eq!(role_of(0, "// a comment"), Some(Role::Comment));
        assert_eq!(role_of(1, "fn"), Some(Role::Keyword));
        assert_eq!(role_of(1, "total"), Some(Role::Function));
        assert_eq!(role_of(1, "u32"), Some(Role::Type));
        assert_eq!(role_of(2, "let"), Some(Role::Keyword));
        assert_eq!(role_of(2, "\"x\""), Some(Role::String));
    }

    #[test]
    fn spans_are_offsets_into_their_own_line() {
        // The viewer indexes into one line's text, so an offset that is
        // file-absolute would highlight the wrong characters on every line but
        // the first — and only visibly so far down the file.
        let source = "fn a() {}\nfn b() {}\n";
        let spans = spans_of("x.rs", source);
        for line in [0, 1] {
            let line_text = source.lines().nth(line).unwrap();
            for span in spans.get(&line).into_iter().flatten() {
                assert!(
                    span.end <= line_text.len(),
                    "line {line}: span {span:?} runs past {line_text:?}"
                );
            }
        }
        let first = &spans[&0][0];
        let second = &spans[&1][0];
        assert_eq!(
            (first.start, first.end),
            (second.start, second.end),
            "the same construct on two lines has the same offsets"
        );
    }

    #[test]
    fn only_the_visible_range_is_queried() {
        let source: String = (0..200).map(|n| format!("fn f{n}() {{}}\n")).collect();
        let highlighter = Highlighter::new(b"big.rs", source.as_bytes()).expect("a grammar");

        let window = highlighter.spans(100..110);
        assert!(!window.is_empty(), "the window has code in it");
        assert!(
            window.keys().all(|line| (100..110).contains(line)),
            "spans leaked outside the requested range: {:?}",
            window.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_multiline_construct_is_cut_at_line_boundaries() {
        let source = "/* one\n   two */\nfn a() {}\n";
        let spans = spans_of("x.rs", source);
        assert_eq!(
            spans[&0][0].role,
            Role::Comment,
            "the comment starts on line 0"
        );
        assert!(spans.contains_key(&1), "and continues on line 1: {spans:?}");
        assert!(spans[&1][0].end <= "   two */".len());
    }

    #[test]
    fn the_other_grammars_parse() {
        assert!(!spans_of("Cargo.toml", "[package]\nname = \"omagit\"\n").is_empty());
        assert!(!spans_of("x.json", "{\"a\": \"b\"}\n").is_empty());
        // Markdown colours what is *literal* and leaves prose alone: a fenced
        // block reads as code, a heading reads as a heading because it is a
        // heading, not because it is orange.
        let markdown = spans_of("README.md", "# Title\n\n```\ncode\n```\n");
        assert!(
            markdown
                .values()
                .flatten()
                .all(|span| span.role == Role::String),
            "prose picked up a code colour: {markdown:?}"
        );
        assert!(!markdown.is_empty(), "the fenced block is highlighted");
        assert_eq!(Highlighter::language_of(b"Cargo.lock"), Some("TOML"));
    }

    #[test]
    fn a_file_with_no_grammar_is_simply_not_highlighted() {
        // Not an error, and not a gap: a diff is legible without colour
        // (DESIGN §1), so an unknown language costs nothing.
        assert!(Highlighter::new(b"notes.txt", b"plain text\n").is_none());
        assert!(Highlighter::new(b"Makefile", b"all:\n").is_none());
        assert_eq!(Highlighter::language_of(b"notes.txt"), None);
    }

    #[test]
    fn an_unparseable_file_still_highlights_what_it_can() {
        // Half-written code is the normal state of a file being edited, and
        // tree-sitter recovers rather than giving up. The viewer must not go
        // blank mid-keystroke.
        let source = "fn broken( {\n    let x = \"unterminated\n";
        let highlighter = Highlighter::new(b"x.rs", source.as_bytes()).expect("a grammar");
        assert!(
            !highlighter.spans(0..2).is_empty(),
            "an error node is not the end of highlighting"
        );
    }

    #[test]
    fn a_very_large_file_is_left_plain() {
        let huge = "fn a() {}\n".repeat(120_000);
        assert!(
            Highlighter::new(b"huge.rs", huge.as_bytes()).is_none(),
            "past the cap the file is drawn plain rather than parsed"
        );
    }
}
