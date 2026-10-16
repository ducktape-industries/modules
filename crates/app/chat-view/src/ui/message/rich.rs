//! A message's styled lines: the runs machinery both designs share
//! ([`line`], [`Run`]) and the desktop's own runs ([`rich_line`]); the
//! phone's are `ui/phone/rich.rs`.
use super::*;
use crate::message::ChatSpan;

pub(crate) fn plain_line(id: ElementId, text: &str, mono: bool) -> InteractiveText {
    let styled = StyledText::new(text.to_owned());
    let mut text = InteractiveText::new(id, styled).w_full();
    if mono {
        text = text
            .font_family(design::fonts::FAMILY_MONO)
            .text_size(design::text::SECONDARY);
    }
    text
}

/// One styled run of a rich line, as a design draws it: the words it adds,
/// how they look, where a press on them goes, whether they are in the
/// data face.
pub(crate) struct Run {
    pub text: String,
    pub style: HighlightStyle,
    pub target: Option<String>,
    pub mono: bool,
}

impl Run {
    /// `span`'s own words in `style`, pressing through to `target` unless
    /// it is empty.
    pub(crate) fn of(span: &ChatSpan, style: HighlightStyle, target: Option<&String>) -> Run {
        Run {
            text: span.text.clone(),
            style,
            target: target.filter(|target| !target.is_empty()).cloned(),
            mono: false,
        }
    }
}

/// A line of `spans`, each run drawn by `run` (one design's), a press on a
/// link or mention opening it.
pub(crate) fn line(
    id: ElementId,
    spans: &[Span],
    names: &Names,
    cx: &mut Context<Chat>,
    run: impl Fn(&ChatSpan) -> Run,
) -> InteractiveText {
    let styled = crate::message::styled_spans(spans, names);
    if styled.is_empty() {
        return plain_line(id, &crate::message::span_text(spans, names), false);
    }
    let mut text = String::new();
    let mut highlights = Vec::new();
    let mut clickable = Vec::new();
    let mut targets = Vec::new();
    let mut mono = Vec::new();
    for span in &styled {
        let start = text.len();
        let drawn = run(span);
        text.push_str(&drawn.text);
        let range = start..text.len();
        if let Some(target) = drawn.target {
            clickable.push(range.clone());
            targets.push(target);
        }
        if drawn.mono {
            mono.push((range.clone(), design::fonts::FAMILY_MONO.into()));
        }
        highlights.push((range, drawn.style));
    }
    let styled = StyledText::new(text)
        .with_highlights(highlights)
        .with_font_family_overrides(mono);
    let open = cx.processor(move |chat, index: usize, _window, cx| {
        if let Some(target) = targets.get(index) {
            cx.notify();
            chat.open_link(target.clone(), cx);
        }
    });
    InteractiveText::new(id, styled)
        .w_full()
        .on_click(clickable, open)
}

/// The desktop's runs: bold is bold, a link and a mention in the link
/// tone, inline code on the fenced block's ground at the paragraph's size.
pub(crate) fn rich_line(
    id: ElementId,
    spans: &[Span],
    names: &Names,
    cx: &mut Context<Chat>,
    theme: &Theme,
) -> InteractiveText {
    let theme = *theme;
    line(id, spans, names, cx, move |span| {
        let mut style = HighlightStyle::default();
        match &span.style {
            SpanStyle::Plain => {}
            SpanStyle::Bold => style.font_weight = Some(FontWeight::BOLD),
            SpanStyle::Italic => style.font_style = Some(FontStyle::Italic),
            SpanStyle::BoldItalic => {
                style.font_weight = Some(FontWeight::BOLD);
                style.font_style = Some(FontStyle::Italic);
            }
            SpanStyle::Link(target) => {
                style.color = Some(theme.link);
                style.font_weight = Some(FontWeight::MEDIUM);
                style.underline = Some(UnderlineStyle {
                    thickness: px(1.),
                    color: Some(theme.link),
                    wavy: false,
                });
                return Run::of(span, style, Some(target));
            }
            SpanStyle::Mention(account) => {
                style.color = Some(theme.link);
                style.font_weight = Some(FontWeight::MEDIUM);
                return Run::of(span, style, Some(account));
            }
            SpanStyle::Code => {
                style.background_color = Some(theme.surface);
                return Run {
                    mono: true,
                    ..Run::of(span, style, None)
                };
            }
        }
        Run::of(span, style, None)
    })
}
