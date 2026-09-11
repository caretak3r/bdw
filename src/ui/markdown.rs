//! Markdown-to-`ratatui::Line` renderer for the issue detail popup.
//! Renders full text markdown (headings, emphasis, lists, blockquotes,
//! tables, code blocks) with theme colors. Deliberately does not render
//! diagrams: a fenced ```mermaid``` block (or any other fenced language) is
//! always shown as a labeled, boxed block of raw text, never parsed or
//! drawn as a graphic — true diagram rendering needs a terminal image
//! protocol (sixel/kitty), which is out of scope here.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme::Theme;

/// Parses `source` as GFM-flavored markdown and lays it out as styled
/// `Line`s. `width` sizes code-block/table rules to roughly fit the popup;
/// actual line wrapping still happens via the caller's `Paragraph::wrap`.
pub(crate) fn render(source: &str, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    let mut renderer = Renderer::new(theme, width);
    for event in Parser::new_ext(source, opts) {
        renderer.handle(event);
    }
    renderer.finish()
}

struct Renderer<'t> {
    theme: &'t Theme,
    width: usize,
    lines: Vec<Line<'static>>,
    cur: Vec<Span<'static>>,
    bold: u32,
    italic: u32,
    strike: u32,
    link: u32,
    heading: Option<HeadingLevel>,
    blockquote: u32,
    list_stack: Vec<Option<u64>>,
    code_lang: Option<String>,
    code_raw: String,
    in_table_cell: bool,
    table_cell: String,
    table_rows: Vec<Vec<String>>,
}

impl<'t> Renderer<'t> {
    fn new(theme: &'t Theme, width: usize) -> Self {
        Self {
            theme,
            width: width.clamp(20, 100),
            lines: Vec::new(),
            cur: Vec::new(),
            bold: 0,
            italic: 0,
            strike: 0,
            link: 0,
            heading: None,
            blockquote: 0,
            list_stack: Vec::new(),
            code_lang: None,
            code_raw: String::new(),
            in_table_cell: false,
            table_cell: String::new(),
            table_rows: Vec::new(),
        }
    }

    fn handle(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.push_text(&text),
            Event::Code(text) => self.push_inline_code(&text),
            Event::SoftBreak => self.push_text(" "),
            Event::HardBreak => self.flush_line(),
            Event::Rule => {
                self.flush_line();
                self.lines.push(Line::from(Span::styled(
                    "─".repeat(self.width),
                    Style::default().fg(self.theme.dimmer),
                )));
            }
            Event::TaskListMarker(checked) => self.push_text(if checked { "[x] " } else { "[ ] " }),
            Event::Html(_) | Event::InlineHtml(_) | Event::FootnoteReference(_) => {}
            // Only emitted with Options::ENABLE_MATH, which isn't set — kept
            // for match exhaustiveness against future pulldown-cmark events.
            Event::InlineMath(text) | Event::DisplayMath(text) => self.push_inline_code(&text),
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush_line();
                self.heading = Some(level);
            }
            Tag::BlockQuote(_) => {
                self.flush_line();
                self.blockquote += 1;
            }
            Tag::CodeBlock(kind) => {
                self.flush_line();
                self.code_lang = Some(match kind {
                    CodeBlockKind::Fenced(lang) => lang.into_string(),
                    CodeBlockKind::Indented => String::new(),
                });
                self.code_raw.clear();
            }
            Tag::List(start) => {
                self.flush_line();
                self.list_stack.push(start);
            }
            Tag::Item => {
                self.flush_line();
                let depth = self.list_stack.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                let marker = match self.list_stack.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{n}. ");
                        *n += 1;
                        m
                    }
                    _ => "• ".to_string(),
                };
                self.cur.push(Span::styled(
                    format!("{indent}{marker}"),
                    Style::default().fg(self.theme.dim),
                ));
            }
            Tag::Emphasis => self.italic += 1,
            Tag::Strong => self.bold += 1,
            Tag::Strikethrough => self.strike += 1,
            Tag::Link { .. } => self.link += 1,
            Tag::Image { .. } => self
                .cur
                .push(Span::styled("▤ ", Style::default().fg(self.theme.dimmer))),
            Tag::Table(_) => self.table_rows.clear(),
            Tag::TableHead | Tag::TableRow => self.table_rows.push(Vec::new()),
            Tag::TableCell => {
                self.in_table_cell = true;
                self.table_cell.clear();
            }
            Tag::Paragraph
            | Tag::HtmlBlock
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Superscript
            | Tag::Subscript
            | Tag::MetadataBlock(_) => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush_line();
                self.lines.push(Line::raw(""));
            }
            TagEnd::Heading(_) => {
                self.flush_line();
                self.heading = None;
                self.lines.push(Line::raw(""));
            }
            TagEnd::BlockQuote(_) => {
                self.flush_line();
                self.blockquote = self.blockquote.saturating_sub(1);
            }
            TagEnd::CodeBlock => self.emit_code_block(),
            TagEnd::List(_) => {
                self.flush_line();
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.lines.push(Line::raw(""));
                }
            }
            TagEnd::Item => self.flush_line(),
            TagEnd::Emphasis => self.italic = self.italic.saturating_sub(1),
            TagEnd::Strong => self.bold = self.bold.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Link => self.link = self.link.saturating_sub(1),
            TagEnd::Table => {
                self.emit_table();
                self.lines.push(Line::raw(""));
            }
            TagEnd::TableCell => {
                self.in_table_cell = false;
                let cell = std::mem::take(&mut self.table_cell);
                if let Some(row) = self.table_rows.last_mut() {
                    row.push(cell);
                }
            }
            TagEnd::HtmlBlock
            | TagEnd::FootnoteDefinition
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::TableHead
            | TagEnd::TableRow
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Image
            | TagEnd::MetadataBlock(_) => {}
        }
    }

    fn current_style(&self) -> Style {
        let mut style = Style::default().fg(self.theme.fg);
        if let Some(level) = self.heading {
            style = style.fg(self.theme.blue).add_modifier(Modifier::BOLD);
            if level == HeadingLevel::H1 {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
        }
        if self.blockquote > 0 {
            style = style.fg(self.theme.dimmer).add_modifier(Modifier::ITALIC);
        }
        if self.link > 0 {
            style = style.fg(self.theme.cyan).add_modifier(Modifier::UNDERLINED);
        }
        if self.bold > 0 {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.italic > 0 {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if self.strike > 0 {
            style = style.add_modifier(Modifier::CROSSED_OUT);
        }
        style
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.code_lang.is_some() {
            self.code_raw.push_str(text);
            return;
        }
        if self.in_table_cell {
            self.table_cell.push_str(text);
            return;
        }
        self.cur
            .push(Span::styled(text.to_string(), self.current_style()));
    }

    fn push_inline_code(&mut self, text: &str) {
        if self.in_table_cell {
            self.table_cell.push_str(text);
            return;
        }
        self.cur.push(Span::styled(
            format!(" {text} "),
            Style::default().fg(self.theme.green).bg(self.theme.sel_bg),
        ));
    }

    fn flush_line(&mut self) {
        if self.cur.is_empty() {
            return;
        }
        let spans = std::mem::take(&mut self.cur);
        let mut out = Vec::with_capacity(spans.len() + 1);
        if self.blockquote > 0 {
            out.push(Span::styled(
                "▎ ".repeat(self.blockquote as usize),
                Style::default().fg(self.theme.dimmer),
            ));
        }
        out.extend(spans);
        self.lines.push(Line::from(out));
    }

    /// Boxed, tinted rendering shared by every fenced/indented code block —
    /// including ```mermaid``` and other diagram-spec languages, which get
    /// the same treatment as any other code: labeled raw text, never a
    /// rendered graphic.
    fn emit_code_block(&mut self) {
        let lang = self.code_lang.take().unwrap_or_default();
        let raw = std::mem::take(&mut self.code_raw);
        let border = self.theme.dimmer;
        let label = if lang.is_empty() {
            "code".to_string()
        } else {
            lang
        };
        let head_dashes = self.width.saturating_sub(label.chars().count() + 4).max(1);
        self.lines.push(Line::from(Span::styled(
            format!("╭─ {label} {}", "─".repeat(head_dashes)),
            Style::default().fg(border),
        )));
        let body_lines: Vec<&str> = raw.lines().collect();
        if body_lines.is_empty() {
            self.lines
                .push(Line::from(Span::styled("│", Style::default().fg(border))));
        }
        for code_line in body_lines {
            self.lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(border)),
                Span::styled(
                    code_line.to_string(),
                    Style::default().fg(self.theme.green).bg(self.theme.sel_bg),
                ),
            ]));
        }
        self.lines.push(Line::from(Span::styled(
            format!("╰{}", "─".repeat(self.width.saturating_sub(1).max(1))),
            Style::default().fg(border),
        )));
        self.lines.push(Line::raw(""));
    }

    fn emit_table(&mut self) {
        let rows = std::mem::take(&mut self.table_rows);
        if rows.is_empty() {
            return;
        }
        let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut widths = vec![0usize; ncols];
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cell.chars().count());
            }
        }
        for (ri, row) in rows.iter().enumerate() {
            let mut spans = Vec::new();
            for (ci, col_width) in widths.iter().enumerate() {
                let cell = row.get(ci).map(String::as_str).unwrap_or("");
                let style = if ri == 0 {
                    Style::default()
                        .fg(self.theme.fg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.fg)
                };
                spans.push(Span::styled(format!("{cell:<col_width$}"), style));
                if ci + 1 < widths.len() {
                    spans.push(Span::raw("  "));
                }
            }
            self.lines.push(Line::from(spans));
            if ri == 0 {
                let sep = widths
                    .iter()
                    .map(|w| "─".repeat(*w))
                    .collect::<Vec<_>>()
                    .join("──");
                self.lines.push(Line::from(Span::styled(
                    sep,
                    Style::default().fg(self.theme.dimmer),
                )));
            }
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush_line();
        while matches!(self.lines.last(), Some(l) if is_blank(l)) {
            self.lines.pop();
        }
        self.lines
    }
}

fn is_blank(line: &Line) -> bool {
    line.spans.iter().all(|s| s.content.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(theme: &Theme, src: &str) -> Vec<String> {
        render(src, theme, 40)
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn heading_and_paragraph_render_as_separate_lines() {
        let theme = Theme::nord();
        let out = plain(&theme, "# Title\n\nBody text.");
        assert_eq!(out, vec!["Title", "", "Body text."]);
    }

    #[test]
    fn bullet_list_gets_bullet_markers() {
        let theme = Theme::nord();
        let out = plain(&theme, "- one\n- two");
        assert_eq!(out, vec!["• one", "• two"]);
    }

    #[test]
    fn ordered_list_numbers_increment() {
        let theme = Theme::nord();
        let out = plain(&theme, "1. first\n2. second");
        assert_eq!(out, vec!["1. first", "2. second"]);
    }

    #[test]
    fn fenced_code_block_is_boxed_and_labeled() {
        let theme = Theme::nord();
        let lines = render("```rust\nfn main() {}\n```", &theme, 40);
        let rendered: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(rendered[0].contains("rust"));
        assert!(rendered.iter().any(|l| l.contains("fn main() {}")));
        assert!(rendered.last().unwrap().starts_with('╰'));
    }

    #[test]
    fn mermaid_block_is_boxed_as_labeled_code_not_a_diagram() {
        let theme = Theme::nord();
        let lines = render("```mermaid\ngraph TD; A-->B;\n```", &theme, 40);
        let rendered: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(rendered[0].contains("mermaid"));
        assert!(rendered.iter().any(|l| l.contains("graph TD; A-->B;")));
    }

    #[test]
    fn blockquote_lines_get_prefixed() {
        let theme = Theme::nord();
        let out = plain(&theme, "> quoted line");
        assert_eq!(out, vec!["▎ quoted line"]);
    }

    #[test]
    fn table_renders_header_separator_and_row() {
        let theme = Theme::nord();
        let out = plain(&theme, "| a | b |\n| - | - |\n| 1 | 2 |");
        assert_eq!(out.len(), 3);
        assert!(out[0].contains('a') && out[0].contains('b'));
        assert!(out[1].chars().all(|c| c == '─'));
        assert!(out[2].contains('1') && out[2].contains('2'));
    }

    #[test]
    fn no_trailing_blank_lines() {
        let theme = Theme::nord();
        let out = render("Body text.", &theme, 40);
        assert!(!out.is_empty());
        assert!(!is_blank(out.last().unwrap()));
    }
}
