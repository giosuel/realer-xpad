//! A small Markdown parser tailored for styling a text buffer in place.
//!
//! [`parse`] never changes the text; it returns *spans* (style tags over source
//! ranges) and *markers* (syntax characters such as `**` or `# `). The editor
//! applies spans as text tags and dims markers. [`render`] produces the clean
//! read view by dropping/replacing markers and remapping the spans.
//!
//! All offsets are in characters (Unicode scalar values), the unit
//! `gtk::TextBuffer` uses — not bytes.

use std::sync::LazyLock;

use fancy_regex::{Captures, Regex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub tag: &'static str,
    pub start: usize,
    pub end: usize,
    /// Extra data, e.g. a link target.
    pub data: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub start: usize,
    pub end: usize,
    /// What the read view shows instead of the marker.
    pub replace: &'static str,
    pub tag: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Checkbox {
    /// Range of `[ ]` / `[x]`.
    pub start: usize,
    pub end: usize,
    pub checked: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Parsed {
    pub spans: Vec<Span>,
    pub markers: Vec<Marker>,
    pub checkboxes: Vec<Checkbox>,
}

const HR: &str = "────────────────";

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid regex")
}

static RE_FENCE: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*(```|~~~)"));
static RE_HEADING: LazyLock<Regex> = LazyLock::new(|| re(r"^(#{1,6})[ \t]+"));
static RE_HR: LazyLock<Regex> = LazyLock::new(|| re(r"^[ \t]*([-*_])([ \t]*\1){2,}[ \t]*$"));
static RE_QUOTE: LazyLock<Regex> = LazyLock::new(|| re(r"^[ \t]*>[ \t]?"));
static RE_CHECKBOX: LazyLock<Regex> = LazyLock::new(|| re(r"^([ \t]*)[-*+][ \t]+(\[([ xX])\])[ \t]+"));
static RE_BULLET: LazyLock<Regex> = LazyLock::new(|| re(r"^([ \t]*)[-*+][ \t]+"));
static RE_ORDERED: LazyLock<Regex> = LazyLock::new(|| re(r"^([ \t]*)(\d+[.)])[ \t]+"));

static RE_CODE: LazyLock<Regex> = LazyLock::new(|| re(r"(`+)(.+?)\1"));
static RE_LINK: LazyLock<Regex> = LazyLock::new(|| re(r"\[([^\]\n]+)\]\(([^)\s]+)\)"));
// No `*` or backtick, so `**https://x.y**` stays bold, and none of the
// `placeholder` characters, so a URL never runs into masked code.
static RE_URL: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?<![A-Za-z0-9])https?://[^\s<>()\[\]*`\x00\x{A7}\x{2060}\x{1F600}]+[^\s<>()\[\]*`\x00\x{A7}\x{2060}\x{1F600}.,;:!?'"_~]"#,
    )
});
static RE_BOLD: LazyLock<Regex> = LazyLock::new(|| re(r"(\*\*|__)(?=\S)(.+?)(?<=\S)\1"));
static RE_STRIKE: LazyLock<Regex> = LazyLock::new(|| re(r"(~~)(?=\S)(.+?)(?<=\S)\1"));
static RE_ITALIC: LazyLock<Regex> = LazyLock::new(|| re(r"(?<![\w*_])([*_])(?=\S)(.+?)(?<=\S)\1(?![\w*_])"));

/// Maps byte offsets within one line to character offsets in the whole text.
struct Line<'a> {
    text: &'a str,
    base: usize,
    chars: Vec<usize>, // byte index -> char index within the line
}

impl<'a> Line<'a> {
    fn new(text: &'a str, base: usize) -> Self {
        let mut chars = vec![0; text.len() + 1];
        let mut n = 0;
        for (i, c) in text.char_indices() {
            for slot in &mut chars[i..i + c.len_utf8()] {
                *slot = n;
            }
            n += 1;
        }
        chars[text.len()] = n;
        Self { text, base, chars }
    }

    /// Absolute character offset of byte `b` in this line.
    fn at(&self, b: usize) -> usize {
        self.base + self.chars[b]
    }

    fn char_len(&self) -> usize {
        self.chars[self.text.len()]
    }
}

fn first<'t>(regex: &Regex, text: &'t str) -> Option<Captures<'t, str>> {
    regex.captures(text).ok().flatten()
}

fn all<'t>(regex: &Regex, text: &'t str) -> Vec<Captures<'t, str>> {
    regex.captures_iter(text).filter_map(Result::ok).collect()
}

pub fn parse(text: &str) -> Parsed {
    let mut out = Parsed::default();
    let mut in_fence = false;
    let mut base = 0;
    for raw in text.split('\n') {
        let line = Line::new(raw, base);
        base += line.char_len() + 1;
        let end = line.at(raw.len());
        if RE_FENCE.is_match(raw).unwrap_or(false) {
            in_fence = !in_fence;
            out.markers.push(Marker { start: line.base, end, replace: "", tag: "fence" });
            continue;
        }
        if in_fence {
            out.spans.push(Span { tag: "code_block", start: line.base, end, data: None });
            continue;
        }
        parse_line(&line, &mut out);
    }
    out
}

fn parse_line(line: &Line, out: &mut Parsed) {
    let text = line.text;
    let end = line.at(text.len());
    let marker = |start, stop, replace, tag| Marker { start, end: stop, replace, tag };
    let span = |tag, start, stop| Span { tag, start, end: stop, data: None };

    if RE_HR.is_match(text).unwrap_or(false) {
        out.markers.push(marker(line.base, end, HR, "hr"));
        return;
    }

    let mut body = 0; // byte index where inline content begins
    if let Some(m) = first(&RE_HEADING, text) {
        let level = m[1].len().min(3);
        let stop = m.get(0).unwrap().end();
        out.markers.push(marker(line.base, line.at(stop), "", "prefix"));
        out.spans.push(span(["h1", "h2", "h3"][level - 1], line.at(stop), end));
        body = stop;
    } else if let Some(m) = first(&RE_QUOTE, text) {
        let stop = m.get(0).unwrap().end();
        out.markers.push(marker(line.base, line.at(stop), "", "prefix"));
        out.spans.push(span("quote", line.at(stop), end));
        body = stop;
    } else if let Some(m) = first(&RE_CHECKBOX, text) {
        let checked = &m[3] != " ";
        let stop = m.get(0).unwrap().end();
        let indent = m.get(1).unwrap().end();
        let replace = if checked { "☑ " } else { "☐ " };
        out.markers.push(marker(line.at(indent), line.at(stop), replace, "checkbox"));
        let boxed = m.get(2).unwrap();
        out.checkboxes.push(Checkbox { start: line.at(boxed.start()), end: line.at(boxed.end()), checked });
        if checked {
            out.spans.push(span("done", line.at(stop), end));
        }
        body = stop;
    } else if let Some(m) = first(&RE_BULLET, text) {
        let stop = m.get(0).unwrap().end();
        out.markers.push(marker(line.at(m.get(1).unwrap().end()), line.at(stop), "• ", "bullet"));
        body = stop;
    } else if let Some(m) = first(&RE_ORDERED, text) {
        let num = m.get(2).unwrap();
        out.spans.push(span("bullet", line.at(num.start()), line.at(num.end())));
        body = m.get(0).unwrap().end();
    }

    parse_inline(line, body, out);
}

/// Replaces a character with a placeholder of the same UTF-8 length, so byte
/// offsets in the masked string stay valid for the original line.
fn placeholder(len: usize) -> char {
    match len {
        1 => '\0',
        2 => '\u{00A7}',
        3 => '\u{2060}',
        _ => '\u{1F600}',
    }
}

fn mask(masked: &mut String, range: std::ops::Range<usize>) {
    let replaced: String = masked[range.clone()].chars().map(|c| placeholder(c.len_utf8())).collect();
    masked.replace_range(range, &replaced);
}

fn parse_inline(line: &Line, body: usize, out: &mut Parsed) {
    // Work on the whole line but blank out everything before `body`.
    let mut masked = line.text.to_string();
    mask(&mut masked, 0..body);
    let at = |b| line.at(b);

    // Inline code first: nothing inside it is parsed.
    for m in all(&RE_CODE, &masked.clone()) {
        let (whole, tick) = (m.get(0).unwrap(), m[1].len());
        let inner = m.get(2).unwrap();
        out.markers.push(Marker {
            start: at(whole.start()),
            end: at(whole.start() + tick),
            replace: "",
            tag: "marker",
        });
        out.markers.push(Marker { start: at(whole.end() - tick), end: at(whole.end()), replace: "", tag: "marker" });
        out.spans.push(Span { tag: "code", start: at(inner.start()), end: at(inner.end()), data: None });
        mask(&mut masked, whole.range());
    }

    for m in all(&RE_LINK, &masked.clone()) {
        let (whole, label) = (m.get(0).unwrap(), m.get(1).unwrap());
        let url = line.text[m.get(2).unwrap().range()].to_string();
        out.markers.push(Marker { start: at(whole.start()), end: at(label.start()), replace: "", tag: "marker" });
        out.markers.push(Marker { start: at(label.end()), end: at(whole.end()), replace: "", tag: "marker" });
        out.spans.push(Span { tag: "link", start: at(label.start()), end: at(label.end()), data: Some(url) });
        // Hide the URL from the bare-URL pass but keep the label for emphasis.
        mask(&mut masked, whole.start()..label.start());
        mask(&mut masked, label.end()..whole.end());
    }

    for m in all(&RE_URL, &masked.clone()) {
        let whole = m.get(0).unwrap();
        let url = line.text[whole.range()].to_string();
        out.spans.push(Span { tag: "link", start: at(whole.start()), end: at(whole.end()), data: Some(url) });
        mask(&mut masked, whole.range());
    }

    for (regex, tag) in [(&*RE_BOLD, "bold"), (&*RE_STRIKE, "strike"), (&*RE_ITALIC, "italic")] {
        for m in all(regex, &masked.clone()) {
            let (whole, n) = (m.get(0).unwrap(), m[1].len());
            let inner = m.get(2).unwrap();
            out.markers.push(Marker {
                start: at(whole.start()),
                end: at(whole.start() + n),
                replace: "",
                tag: "marker",
            });
            out.markers.push(Marker { start: at(whole.end() - n), end: at(whole.end()), replace: "", tag: "marker" });
            out.spans.push(Span { tag, start: at(inner.start()), end: at(inner.end()), data: None });
            mask(&mut masked, whole.start()..whole.start() + n);
            mask(&mut masked, whole.end() - n..whole.end());
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Rendered {
    pub text: String,
    pub spans: Vec<Span>,
    /// `(start, end)` in rendered text, and the index into [`Parsed::checkboxes`].
    pub checkboxes: Vec<(usize, usize, usize)>,
    /// Kept source runs: `(src_start, src_end, rendered_start)`.
    pub runs: Vec<(usize, usize, usize)>,
    /// Length of the source in chars.
    pub source_len: usize,
    /// Source ranges of the markers that start a line (`# `, `- `, `> `…).
    pub line_markers: Vec<(usize, usize)>,
}

/// Markers that belong to a line rather than to the text in it. Headings and
/// quotes are tagged `prefix`; the editor styles them as plain markers.
const LINE_MARKERS: [&str; 5] = ["prefix", "bullet", "checkbox", "hr", "fence"];

impl Rendered {
    /// Map a rendered offset back to the closest source offset.
    pub fn to_source(&self, offset: usize) -> usize {
        for &(s, e, o) in &self.runs {
            if o <= offset && offset < o + (e - s) {
                return s + offset - o;
            }
            if offset < o {
                return s; // inside replaced marker text: snap to the next run
            }
        }
        self.runs.last().map_or(0, |r| r.1)
    }

    /// The source range behind the rendered selection `start..end`, so that
    /// copying from the read view gives Markdown: a selection that starts at
    /// the beginning of a run takes the markers before it (`**`, and with
    /// `line_markers` also `- ` or `# `), and one that ends at the end of a
    /// run takes the closing markers after it, unless the run ends a line
    /// (those belong to the next line). A selection that starts on a shown
    /// marker (`•`, `☐`) always takes it.
    pub fn source_range(&self, start: usize, end: usize, line_markers: bool) -> (usize, usize) {
        let ends_line = end > 0 && self.text.chars().nth(end - 1) == Some('\n');
        let mut from = None;
        let mut to = None;
        let mut prev_end = 0;
        for (i, &(s, e, o)) in self.runs.iter().enumerate() {
            let len = e - s;
            if from.is_none() {
                if start < o || (start == o && line_markers) {
                    from = Some(prev_end);
                } else if start == o {
                    let skip = self.line_markers.iter().filter(|m| prev_end <= m.0 && m.1 <= s).map(|m| m.1).max();
                    from = Some(skip.unwrap_or(prev_end));
                } else if start < o + len {
                    from = Some(s + start - o);
                }
            }
            if to.is_none() {
                let next = self.runs.get(i + 1).map_or(self.source_len, |r| r.0);
                if end <= o {
                    to = Some(s);
                } else if end < o + len || (end == o + len && ends_line) {
                    to = Some(s + end - o);
                } else if end == o + len {
                    to = Some(next);
                }
            }
            prev_end = e;
        }
        let from = from.unwrap_or(prev_end);
        (from, to.unwrap_or(self.source_len).max(from))
    }
}

/// Build the read view: markers are removed or replaced, spans remapped.
pub fn render(source: &str, parsed: &Parsed) -> Rendered {
    let chars: Vec<char> = source.chars().collect();
    let mut markers: Vec<&Marker> = parsed.markers.iter().collect();
    markers.sort_by_key(|m| m.start);

    let mut out = Rendered { source_len: chars.len(), ..Rendered::default() };
    let mut extra = Vec::new();
    let (mut src, mut pos) = (0, 0);
    for mk in markers {
        if mk.start < src {
            continue; // overlapping marker; keep the first one
        }
        if mk.start > src {
            out.runs.push((src, mk.start, pos));
            out.text.extend(&chars[src..mk.start]);
            pos += mk.start - src;
        }
        if LINE_MARKERS.contains(&mk.tag) {
            out.line_markers.push((mk.start, mk.end));
        }
        if !mk.replace.is_empty() {
            if mk.tag == "checkbox"
                && let Some(i) = parsed.checkboxes.iter().position(|cb| mk.start <= cb.start && cb.start < mk.end)
            {
                out.checkboxes.push((pos, pos + 1, i));
            }
            let len = mk.replace.chars().count();
            // A checkbox's tag covers just the box, which the read view enlarges.
            let styled = if mk.tag == "checkbox" { 1 } else { len };
            extra.push(Span { tag: mk.tag, start: pos, end: pos + styled, data: None });
            out.text.push_str(mk.replace);
            pos += len;
        }
        src = mk.end;
    }
    if src < chars.len() {
        out.runs.push((src, chars.len(), pos));
        out.text.extend(&chars[src..]);
        pos += chars.len() - src;
    }

    let runs = out.runs.clone();
    let remap = |offset: usize, is_end: bool| -> usize {
        for &(s, e, o) in &runs {
            if (s <= offset && offset < e) || (is_end && offset == e) {
                return o + offset - s;
            }
        }
        // Inside a removed marker: snap to the next kept run.
        runs.iter().find(|&&(s, _, _)| s >= offset).map_or(pos, |r| r.2)
    };

    out.spans = extra;
    for sp in &parsed.spans {
        let (a, b) = (remap(sp.start, false), remap(sp.end, true));
        if b > a {
            out.spans.push(Span { tag: sp.tag, start: a, end: b, data: sp.data.clone() });
        }
    }
    out
}

/// Flip the checkbox at `checkbox` in `source`.
pub fn toggle_checkbox(source: &str, checkbox: &Checkbox) -> String {
    let mark = if checkbox.checked { ' ' } else { 'x' };
    source.chars().enumerate().map(|(i, c)| if i == checkbox.start + 1 { mark } else { c }).collect()
}

/// Prefix for the next line when pressing Enter at the end of `line`.
///
/// `None` when `line` is not a list item, `Some("")` when the item is empty
/// (pressing Enter again should end the list).
pub fn continue_list(line: &str) -> Option<String> {
    let (m, prefix) = if let Some(m) = first(&RE_CHECKBOX, line) {
        let prefix = format!("{}[ ] ", &line[..m.get(2).unwrap().start()]);
        (m, prefix)
    } else if let Some(m) = first(&RE_BULLET, line) {
        let prefix = line[..m.get(0).unwrap().end()].to_string();
        (m, prefix)
    } else if let Some(m) = first(&RE_ORDERED, line) {
        let num = &m[2];
        let next = num[..num.len() - 1].parse::<u64>().unwrap_or(0).saturating_add(1);
        let prefix = format!("{}{}{} ", &m[1], next, &num[num.len() - 1..]);
        (m, prefix)
    } else {
        let m = first(&RE_QUOTE, line)?;
        let prefix = line[..m.get(0).unwrap().end()].to_string();
        (m, prefix)
    };
    let rest = &line[m.get(0).unwrap().end()..];
    Some(if rest.trim().is_empty() { String::new() } else { prefix })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(text: &str) -> Vec<(&'static str, String)> {
        let chars: Vec<char> = text.chars().collect();
        parse(text).spans.iter().map(|s| (s.tag, chars[s.start..s.end].iter().collect())).collect()
    }

    fn has(text: &str, tag: &str, content: &str) -> bool {
        tags(text).iter().any(|(t, c)| *t == tag && c == content)
    }

    fn marker_text(text: &str) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        let mut markers = parse(text).markers;
        markers.sort_by_key(|m| m.start);
        markers.iter().map(|m| chars[m.start..m.end].iter().collect()).collect()
    }

    fn rendered(text: &str) -> Rendered {
        render(text, &parse(text))
    }

    fn slice(text: &str, a: usize, b: usize) -> String {
        text.chars().skip(a).take(b - a).collect()
    }

    #[test]
    fn headings() {
        assert!(has("# Title", "h1", "Title"));
        assert!(has("## Sub", "h2", "Sub"));
        assert!(has("##### Deep", "h3", "Deep"));
        assert_eq!(marker_text("## Sub"), ["## "]);
        assert!(tags("#nospace").is_empty());
    }

    #[test]
    fn inline_emphasis() {
        let t = "a **bold** and *it* and _it2_ and ~~gone~~";
        assert!(has(t, "bold", "bold"));
        assert!(has(t, "italic", "it"));
        assert!(has(t, "italic", "it2"));
        assert!(has(t, "strike", "gone"));
    }

    #[test]
    fn bold_italic_nested() {
        let t = "**bold *inner* text**";
        assert!(has(t, "bold", "bold *inner* text"));
        assert!(has(t, "italic", "inner"));
    }

    #[test]
    fn snake_case_is_not_italic() {
        assert!(tags("my_var_name and 2*3*4").is_empty());
    }

    #[test]
    fn inline_code_protects_content() {
        let t = "run `**not bold**` now";
        assert!(has(t, "code", "**not bold**"));
        assert!(!tags(t).iter().any(|(tag, _)| *tag == "bold"));
    }

    #[test]
    fn links() {
        let text = "see [docs](https://example.com) or https://foo.org/x.";
        let links: Vec<_> = parse(text)
            .spans
            .into_iter()
            .filter(|s| s.tag == "link")
            .map(|s| (slice(text, s.start, s.end), s.data.unwrap()))
            .collect();
        assert_eq!(
            links,
            [
                ("docs".to_string(), "https://example.com".to_string()),
                ("https://foo.org/x".to_string(), "https://foo.org/x".to_string()),
            ]
        );
    }

    #[test]
    fn checkboxes() {
        let text = "- [ ] todo\n- [x] done";
        let p = parse(text);
        assert_eq!(p.checkboxes.iter().map(|c| c.checked).collect::<Vec<_>>(), [false, true]);
        assert_eq!(slice(text, p.checkboxes[0].start, p.checkboxes[0].end), "[ ]");
        assert!(has(text, "done", "done"));
    }

    #[test]
    fn toggle() {
        let text = "- [ ] a\n- [x] b";
        let p = parse(text);
        assert_eq!(toggle_checkbox(text, &p.checkboxes[0]), "- [x] a\n- [x] b");
        assert_eq!(toggle_checkbox(text, &p.checkboxes[1]), "- [ ] a\n- [ ] b");
    }

    #[test]
    fn code_fence_disables_parsing() {
        let text = "```\n# not heading\n```\n# heading";
        assert!(has(text, "code_block", "# not heading"));
        assert!(has(text, "h1", "heading"));
        assert!(!has(text, "h1", "not heading"));
    }

    #[test]
    fn quote_and_hr() {
        assert!(has("> wise words", "quote", "wise words"));
        assert_eq!(parse("---").markers[0].tag, "hr");
    }

    #[test]
    fn render_strips_markers() {
        let r = rendered("# Hello **world**\n- item\n- [ ] task\n- [x] did");
        assert_eq!(r.text, "Hello world\n• item\n☐ task\n☑ did");
        let spans: Vec<_> = r.spans.iter().map(|s| (s.tag, slice(&r.text, s.start, s.end))).collect();
        assert!(spans.contains(&("h1", "Hello world".into())));
        assert!(spans.contains(&("bold", "world".into())));
        assert!(spans.contains(&("done", "did".into())));
        let boxes: Vec<_> = r.checkboxes.iter().map(|&(a, b, _)| slice(&r.text, a, b)).collect();
        assert_eq!(boxes, ["☐", "☑"]);
        assert_eq!(r.checkboxes.iter().map(|c| c.2).collect::<Vec<_>>(), [0, 1]);
    }

    #[test]
    fn render_link_keeps_label_only() {
        let r = rendered("go [here](http://x.y) now");
        assert_eq!(r.text, "go here now");
        let link = r.spans.iter().find(|s| s.tag == "link").unwrap();
        assert_eq!(slice(&r.text, link.start, link.end), "here");
        assert_eq!(link.data.as_deref(), Some("http://x.y"));
    }

    #[test]
    fn render_unicode_offsets() {
        let r = rendered("🎉 **ünï** ok");
        let bold = r.spans.iter().find(|s| s.tag == "bold").unwrap();
        assert_eq!(slice(&r.text, bold.start, bold.end), "ünï");
    }

    #[test]
    fn unicode_in_masked_code_next_to_url() {
        // A URL ending right before inline code with multi-byte characters
        // must not produce offsets inside a character.
        let text = "https://x.y`é` and **ö**";
        assert!(has(text, "bold", "ö"));
        assert!(has(text, "code", "é"));
    }

    #[test]
    fn urls_inside_emphasis_and_next_to_code() {
        let link = |text: &str| {
            parse(text).spans.into_iter().find(|s| s.tag == "link").map(|s| (slice(text, s.start, s.end), s.data))
        };
        let url = Some(("https://example.com".to_string(), Some("https://example.com".to_string())));
        assert_eq!(link("**https://example.com**"), url);
        assert!(has("**https://example.com**", "bold", "https://example.com"));
        assert_eq!(link("*https://example.com*"), url);
        assert_eq!(link("_https://example.com_"), url);
        assert_eq!(link("see https://example.com`code`"), url);
        assert_eq!(
            link("https://example.com/a_b~c"),
            Some(("https://example.com/a_b~c".into(), Some("https://example.com/a_b~c".into())))
        );
    }

    #[test]
    fn to_source_offsets() {
        let src = "# Hi **there**";
        let r = rendered(src);
        assert_eq!(r.text, "Hi there");
        let chars: Vec<char> = src.chars().collect();
        assert_eq!(chars[r.to_source(0)], 'H');
        assert_eq!(chars[r.to_source(3)], 't');
        assert_eq!(r.to_source(r.text.chars().count()), 12);
    }

    #[test]
    fn copying_the_read_view_gives_markdown() {
        let copy = |src: &str, a: usize, b: usize| {
            let r = rendered(src);
            let (a, b) = r.source_range(a, b, true);
            slice(src, a, b)
        };
        let edit = |src: &str, a: usize, b: usize| {
            let r = rendered(src);
            let (a, b) = r.source_range(a, b, false);
            slice(src, a, b)
        };
        let src = "# Milestones\n- Tabs\n- **Lab** menu\n- [ ] Netlab";
        let r = rendered(src);
        assert_eq!(r.text, "Milestones\n• Tabs\n• Lab menu\n☐ Netlab");
        let all = r.text.chars().count();
        assert_eq!(copy(src, 0, all), src);
        // Whole lines, from the bullet or from the text after it.
        assert_eq!(copy(src, 11, 18), "- Tabs\n");
        assert_eq!(copy(src, 13, 17), "- Tabs");
        // Inside a line, no markers are added; bold kept whole at its ends.
        assert_eq!(copy(src, 14, 16), "ab");
        assert_eq!(copy(src, 20, 28), "- **Lab** menu");
        assert_eq!(copy(src, 29, all), "- [ ] Netlab");
        assert_eq!(copy("a **b** c", 2, 3), "**b**");
        assert_eq!(copy("a **b** c", 2, 5), "**b** c");
        assert_eq!(copy("", 0, 0), "");
        // Typing over a selection keeps the line's marker unless the shown
        // marker itself was selected.
        assert_eq!(edit(src, 0, 10), "Milestones");
        assert_eq!(edit(src, 13, 17), "Tabs");
        assert_eq!(edit(src, 11, 17), "- Tabs");
        assert_eq!(edit(src, 20, 23), "**Lab**");
        assert_eq!(edit(src, 31, all), "Netlab");
        assert_eq!(edit("a **b** c", 2, 3), "**b**");
    }

    #[test]
    fn list_continuation() {
        assert_eq!(continue_list("- item").as_deref(), Some("- "));
        assert_eq!(continue_list("  * nested").as_deref(), Some("  * "));
        assert_eq!(continue_list("- [x] done").as_deref(), Some("- [ ] "));
        assert_eq!(continue_list("9. nine").as_deref(), Some("10. "));
        assert_eq!(continue_list("> quote").as_deref(), Some("> "));
        assert_eq!(continue_list("- ").as_deref(), Some(""));
        assert_eq!(continue_list("- [ ] ").as_deref(), Some(""));
        assert_eq!(continue_list("plain"), None);
        assert_eq!(continue_list("18446744073709551615. huge").as_deref(), Some("18446744073709551615. "));
    }
}
