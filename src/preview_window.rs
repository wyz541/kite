use std::path::PathBuf;

use crate::app::{
    is_image_preview_entry, is_markdown_preview_entry, is_text_preview_entry, read_log_preview,
    read_text_preview,
};
use crate::fs::FileEntry;
use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};
use gpui_kit::component::text::{TextView, TextViewState};
use gpui_kit::component::{ActiveTheme as _, Icon, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewKind {
    MarkdownText,
    PlainText,
    Image,
    Other,
}

pub(crate) struct PreviewWindow {
    path: PathBuf,
    name: SharedString,
    path_label: SharedString,
    extension: String,
    kind: PreviewKind,
    loading: bool,
    text_view: Option<Entity<TextViewState>>,
    scroll: ScrollHandle,
    _load_task: Task<()>,
}

impl PreviewWindow {
    pub(crate) fn new(entry: FileEntry, cx: &mut Context<Self>) -> Self {
        let extension = entry.extension.to_ascii_lowercase();
        let kind = if is_image_preview_entry(&entry) {
            PreviewKind::Image
        } else if is_text_preview_entry(&entry) && is_markdown_preview_entry(&entry) {
            PreviewKind::MarkdownText
        } else if is_text_preview_entry(&entry) {
            PreviewKind::PlainText
        } else {
            PreviewKind::Other
        };
        let text_view =
            matches!(kind, PreviewKind::MarkdownText | PreviewKind::PlainText).then(|| {
                cx.new(|cx| {
                    TextViewState::markdown("正在加载文件…", cx)
                        .selectable(true)
                        .scrollable(false)
                })
            });
        let mut this = Self {
            path: entry.path.clone(),
            name: entry.name.into(),
            path_label: entry.path.display().to_string().into(),
            extension,
            kind,
            loading: text_view.is_some(),
            text_view,
            scroll: ScrollHandle::new(),
            _load_task: Task::ready(()),
        };
        if matches!(kind, PreviewKind::MarkdownText | PreviewKind::PlainText) {
            this.start_load(cx);
        }
        this
    }

    fn start_load(&mut self, cx: &mut Context<Self>) {
        let path = self.path.clone();
        let log_preview = self.extension.eq_ignore_ascii_case("log");
        let markdown = self.kind == PreviewKind::MarkdownText;
        let extension = self.extension.clone();
        let Some(text_view) = self.text_view.clone() else {
            return;
        };
        self._load_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    if log_preview {
                        read_log_preview(&path)
                    } else {
                        read_text_preview(&path)
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                let source = match result {
                    Ok(text) if markdown => text,
                    Ok(text) => plain_text_markdown(&text, &extension),
                    Err(error) => format!("**无法预览文件**\n\n{error}"),
                };
                text_view.update(cx, |view, cx| view.set_text(&source, cx));
                cx.notify();
            });
        });
    }

    fn render_text_body(&self, cx: &App) -> AnyElement {
        let Some(text_view) = self.text_view.as_ref() else {
            return div().into_any_element();
        };
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(
                div()
                    .id("enlarged-preview-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .px_8()
                    .py_6()
                    .child(
                        div().w_full().max_w(px(920.)).mx_auto().child(
                            TextView::new(text_view)
                                .w_full()
                                .text_sm()
                                .selectable(true)
                                .scrollable(false),
                        ),
                    ),
            )
            .child(
                div().absolute().inset_0().child(
                    Scrollbar::vertical(&self.scroll)
                        .id("enlarged-preview-scrollbar")
                        .mode(ScrollbarMode::Always)
                        .styles(|styles| {
                            let track = cx.theme().muted.opacity(0.55);
                            let thumb = cx.theme().muted_foreground.opacity(0.75);
                            let thumb_hover = cx.theme().muted_foreground.opacity(0.95);
                            styles
                                .track(|style| style.bg(track).width(px(10.)))
                                .thumb(|style| {
                                    style.bg(thumb).width(px(6.)).inset(px(2.)).radius(px(99.))
                                })
                                .thumb_hover(|style| style.bg(thumb_hover))
                        })
                        .viewport_from_layout(),
                ),
            )
            .into_any_element()
    }
}

impl Render for PreviewWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let body = match self.kind {
            PreviewKind::MarkdownText | PreviewKind::PlainText => self.render_text_body(cx),
            PreviewKind::Image => h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .p_6()
                .child(
                    img(self.path.clone())
                        .max_w_full()
                        .max_h_full()
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element(),
            PreviewKind::Other => v_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .items_center()
                .justify_center()
                .gap_3()
                .child(Icon::new(IconName::File).size(px(72.)).text_color(muted))
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("此文件没有内置预览"),
                )
                .into_any_element(),
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                h_flex()
                    .h(px(48.))
                    .w_full()
                    .flex_shrink_0()
                    .px_4()
                    .gap_3()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(Icon::new(IconName::FileText).size_5().text_color(muted))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_sm()
                            .font_medium()
                            .child(self.name.clone()),
                    )
                    .when(self.loading, |this| {
                        this.child(div().text_xs().text_color(muted).child("正在加载…"))
                    })
                    .child(
                        div()
                            .max_w(px(520.))
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .child(self.path_label.clone()),
                    ),
            )
            .child(body)
    }
}

fn plain_text_markdown(text: &str, extension: &str) -> String {
    let max_backticks = text
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat((max_backticks + 1).max(3));
    let language = if extension.is_empty() {
        "text"
    } else {
        extension
    };
    format!("{fence}{language}\n{text}\n{fence}")
}

#[cfg(test)]
mod tests {
    use super::plain_text_markdown;

    #[test]
    fn plain_text_uses_a_safe_code_fence() {
        let value = plain_text_markdown("`inline` and ```block```", "rs");
        assert!(value.starts_with("````rs\n"));
        assert!(value.ends_with("\n````"));
    }
}
