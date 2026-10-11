from realer_xpad.markdown import continue_list, parse, render, toggle_checkbox


def tags(text):
    p = parse(text)
    return {(s.tag, text[s.start:s.end]) for s in p.spans}


def marker_text(text):
    return [text[m.start:m.end] for m in sorted(parse(text).markers, key=lambda m: m.start)]


def test_headings():
    assert ("h1", "Title") in tags("# Title")
    assert ("h2", "Sub") in tags("## Sub")
    assert ("h3", "Deep") in tags("##### Deep")
    assert marker_text("## Sub") == ["## "]
    assert tags("#nospace") == set()


def test_inline_emphasis():
    t = tags("a **bold** and *it* and _it2_ and ~~gone~~")
    assert {("bold", "bold"), ("italic", "it"), ("italic", "it2"), ("strike", "gone")} <= t


def test_bold_italic_nested():
    t = tags("**bold *inner* text**")
    assert ("bold", "bold *inner* text") in t
    assert ("italic", "inner") in t


def test_snake_case_is_not_italic():
    assert tags("my_var_name and 2*3*4") == set()


def test_inline_code_protects_content():
    t = tags("run `**not bold**` now")
    assert ("code", "**not bold**") in t
    assert not any(tag == "bold" for tag, _ in t)


def test_links():
    text = "see [docs](https://example.com) or https://foo.org/x."
    p = parse(text)
    links = [(text[s.start:s.end], s.data) for s in p.spans if s.tag == "link"]
    assert ("docs", "https://example.com") in links
    assert ("https://foo.org/x", "https://foo.org/x") in links
    assert len(links) == 2


def test_checkboxes():
    text = "- [ ] todo\n- [x] done"
    p = parse(text)
    assert [c.checked for c in p.checkboxes] == [False, True]
    assert text[p.checkboxes[0].start:p.checkboxes[0].end] == "[ ]"
    assert ("done", "done") in tags(text)


def test_toggle_checkbox():
    text = "- [ ] a\n- [x] b"
    p = parse(text)
    assert toggle_checkbox(text, p.checkboxes[0]) == "- [x] a\n- [x] b"
    assert toggle_checkbox(text, p.checkboxes[1]) == "- [ ] a\n- [ ] b"


def test_code_fence_disables_parsing():
    text = "```\n# not heading\n```\n# heading"
    t = tags(text)
    assert ("code_block", "# not heading") in t
    assert ("h1", "heading") in t
    assert ("h1", "not heading") not in t


def test_quote_and_hr():
    assert ("quote", "wise words") in tags("> wise words")
    p = parse("---")
    assert p.markers[0].tag == "hr"


def test_render_strips_markers():
    r = render("# Hello **world**\n- item\n- [ ] task\n- [x] did")
    assert r.text == "Hello world\n• item\n☐ task\n☑ did"
    spans = {(s.tag, r.text[s.start:s.end]) for s in r.spans}
    assert ("h1", "Hello world") in spans
    assert ("bold", "world") in spans
    assert ("done", "did") in spans
    assert [r.text[a:b] for a, b, _ in r.checkboxes] == ["☐", "☑"]
    assert [i for _, _, i in r.checkboxes] == [0, 1]


def test_render_link_keeps_label_only():
    r = render("go [here](http://x.y) now")
    assert r.text == "go here now"
    link = next(s for s in r.spans if s.tag == "link")
    assert r.text[link.start:link.end] == "here" and link.data == "http://x.y"


def test_render_unicode_offsets():
    r = render("🎉 **ünï** ok")
    bold = next(s for s in r.spans if s.tag == "bold")
    assert r.text[bold.start:bold.end] == "ünï"


def test_continue_list():
    assert continue_list("- item") == "- "
    assert continue_list("  * nested") == "  * "
    assert continue_list("- [x] done") == "- [ ] "
    assert continue_list("9. nine") == "10. "
    assert continue_list("> quote") == "> "
    assert continue_list("- ") == ""
    assert continue_list("- [ ] ") == ""
    assert continue_list("plain") is None


def test_render_to_source_offsets():
    src = "# Hi **there**"
    r = render(src)
    assert r.text == "Hi there"
    assert src[r.to_source(0)] == "H"
    assert src[r.to_source(3)] == "t"
    assert r.to_source(len(r.text)) == src.index("**", 7)
