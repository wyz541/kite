use std::path::PathBuf;

use crate::actions::SaveEditor;
use crate::app::{
    is_image_preview_entry, is_text_preview_entry, read_log_preview, read_text_preview,
};
use crate::fs::FileEntry;
use crate::shell_fs;
use crate::text_editor::{self, TextDocument};
use gpui_kit::assets::IconName;
use gpui_kit::base::Selectable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Editor, EditorState, InputEvent, Redo, Undo};
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};
use gpui_kit::component::text::{TextView, TextViewState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, StyledExt as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewKind {
    Editor,
    ReadOnlyText,
    Image,
    Other,
}

pub(crate) struct PreviewWindow {
    path: PathBuf,
    name: SharedString,
    path_label: SharedString,
    kind: PreviewKind,
    loading: bool,
    saving: bool,
    dirty: bool,
    allow_close: bool,
    document: Option<TextDocument>,
    editor: Option<Entity<EditorState>>,
    text_view: Option<Entity<TextViewState>>,
    scroll: ScrollHandle,
    message: Option<SharedString>,
    word_wrap: bool,
    line_numbers: bool,
    show_whitespace: bool,
    _editor_subscription: Option<Subscription>,
    _load_task: Task<()>,
    _save_task: Task<()>,
}

impl PreviewWindow {
    pub(crate) fn new(entry: FileEntry, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let shell_path = shell_fs::is_shell_namespace_path(&entry.path);
        let kind = if is_image_preview_entry(&entry) {
            PreviewKind::Image
        } else if !shell_path
            && text_editor::is_text_path(&entry.path)
            && entry.size <= text_editor::MAX_TEXT_FILE_BYTES
        {
            PreviewKind::Editor
        } else if is_text_preview_entry(&entry) {
            PreviewKind::ReadOnlyText
        } else {
            PreviewKind::Other
        };
        let editor = (kind == PreviewKind::Editor).then(|| {
            let language = text_editor::language_for_path(&entry.path);
            cx.new(|cx| {
                EditorState::new(window, cx)
                    .default_value("正在加载文件…")
                    .language(language)
            })
        });
        let text_view = (kind == PreviewKind::ReadOnlyText).then(|| {
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
            kind,
            loading: editor.is_some() || text_view.is_some(),
            saving: false,
            dirty: false,
            allow_close: false,
            document: None,
            editor,
            text_view,
            scroll: ScrollHandle::new(),
            message: None,
            word_wrap: false,
            line_numbers: true,
            show_whitespace: false,
            _editor_subscription: None,
            _load_task: Task::ready(()),
            _save_task: Task::ready(()),
        };
        if let Some(editor) = this.editor.clone() {
            this._editor_subscription = Some(cx.subscribe_in(
                &editor,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.dirty = true;
                        this.update_window_title(window);
                        cx.notify();
                    }
                },
            ));
        }
        if this.loading {
            this.start_load(cx);
        }
        this.install_close_guard(window, cx);
        this
    }

    fn start_load(&mut self, cx: &mut Context<Self>) {
        let path = self.path.clone();
        match self.kind {
            PreviewKind::Editor => self.start_editor_load(path, cx),
            PreviewKind::ReadOnlyText => self.start_text_preview(path, cx),
            PreviewKind::Image | PreviewKind::Other => {}
        }
    }

    fn start_editor_load(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.clone() else {
            return;
        };
        self._load_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { TextDocument::open(&path) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.loading = false;
                match result {
                    Ok(document) => {
                        let text = document.text.clone();
                        editor.update(cx, |editor, cx| {
                            editor.set_value(text, window, cx);
                            editor.focus(window, cx);
                        });
                        this.document = Some(document);
                        this.message = None;
                        this.dirty = false;
                        this.update_window_title(window);
                    }
                    Err(error) => {
                        this.document = None;
                        this.message = Some(error.into());
                    }
                }
                cx.notify();
            });
        });
    }

    fn start_text_preview(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let extension = path
            .extension()
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let log_preview = extension.eq_ignore_ascii_case("log");
        let markdown = is_markdown_path(&path);
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

    fn install_close_guard(&self, window: &mut Window, cx: &mut Context<Self>) {
        let weak_entity = cx.weak_entity();
        window.on_window_should_close(cx, move |window, app| {
            let Some(entity) = weak_entity.upgrade() else {
                return true;
            };
            if entity.read(app).allow_close || !entity.read(app).dirty {
                return true;
            }
            if window.has_active_dialog(app) {
                return false;
            }

            let dialog_entity = entity.downgrade();
            entity.update(app, |this, cx| {
                this.allow_close = false;
                window.open_alert_dialog(cx, move |alert, _, _| {
                    let entity = dialog_entity.clone();
                    alert
                        .title("有未保存的更改")
                        .description("关闭窗口会丢失当前修改。请先保存文件。")
                        .confirm()
                        .ok_text("放弃并关闭")
                        .cancel_text("继续编辑")
                        .on_ok(move |_, window, app| {
                            if let Some(entity) = entity.upgrade() {
                                entity.update(app, |this, _| {
                                    this.allow_close = true;
                                });
                            }
                            window.remove_window();
                            true
                        })
                });
            });
            false
        });
    }

    fn update_window_title(&self, window: &mut Window) {
        let marker = if self.dirty { "● " } else { "" };
        window.set_window_title(&format!("{marker}{} — Kite 编辑器", self.name));
    }

    fn on_save(&mut self, _: &SaveEditor, window: &mut Window, cx: &mut Context<Self>) {
        self.save(window, cx);
    }

    fn save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.saving || !self.dirty {
            return;
        }
        let (Some(document), Some(editor)) = (self.document.clone(), self.editor.clone()) else {
            return;
        };
        let text = editor.read(cx).value().to_string();
        self.saving = true;
        self.message = Some("正在保存…".into());
        cx.notify();
        self._save_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { document.save(&text) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.saving = false;
                match result {
                    Ok(document) => {
                        this.document = Some(document);
                        this.dirty = false;
                        this.message = Some("已保存".into());
                        this.update_window_title(window);
                        editor.update(cx, |editor, cx| editor.focus(window, cx));
                    }
                    Err(error) => {
                        this.message = Some(error.into());
                    }
                }
                cx.notify();
            });
        });
    }

    fn open_search(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_ref() else {
            return;
        };
        editor.update(cx, |editor, cx| {
            editor.focus(window, cx);
            editor.open_search(replace, cx);
        });
    }

    fn undo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_editor(window, cx);
        window.dispatch_action(Box::new(Undo), cx);
    }

    fn redo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_editor(window, cx);
        window.dispatch_action(Box::new(Redo), cx);
    }

    fn focus_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_ref() {
            editor.update(cx, |editor, cx| editor.focus(window, cx));
        }
    }

    fn toggle_word_wrap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.word_wrap = !self.word_wrap;
        if let Some(editor) = self.editor.as_ref() {
            editor.update(cx, |editor, cx| {
                editor.set_soft_wrap(self.word_wrap, window, cx)
            });
        }
        cx.notify();
    }

    fn toggle_line_numbers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.line_numbers = !self.line_numbers;
        if let Some(editor) = self.editor.as_ref() {
            editor.update(cx, |editor, cx| {
                editor.set_line_number(self.line_numbers, window, cx)
            });
        }
        cx.notify();
    }

    fn toggle_whitespace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_whitespace = !self.show_whitespace;
        if let Some(editor) = self.editor.as_ref() {
            editor.update(cx, |editor, cx| {
                editor.set_show_whitespaces(self.show_whitespace, window, cx)
            });
        }
        cx.notify();
    }

    fn render_toolbar(&self, cx: &Context<Self>) -> AnyElement {
        if self.kind != PreviewKind::Editor {
            return div().into_any_element();
        }
        let muted = cx.theme().muted_foreground;
        h_flex()
            .h(px(38.))
            .w_full()
            .flex_shrink_0()
            .px_3()
            .gap_1()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("editor-save")
                    .ghost()
                    .compact()
                    .icon(IconName::Save)
                    .label("保存")
                    .tooltip("保存 Ctrl+S")
                    .disabled(self.saving || !self.dirty)
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| this.save(window, cx)),
                    ),
            )
            .child(div().w(px(1.)).h(px(20.)).bg(cx.theme().border).mx_1())
            .child(
                Button::new("editor-undo")
                    .ghost()
                    .compact()
                    .icon(IconName::Undo)
                    .tooltip("撤销 Ctrl+Z")
                    .disabled(self.loading || self.document.is_none())
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| this.undo(window, cx)),
                    ),
            )
            .child(
                Button::new("editor-redo")
                    .ghost()
                    .compact()
                    .icon(IconName::Redo)
                    .tooltip("重做 Ctrl+Y")
                    .disabled(self.loading || self.document.is_none())
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| this.redo(window, cx)),
                    ),
            )
            .child(div().w(px(1.)).h(px(20.)).bg(cx.theme().border).mx_1())
            .child(
                Button::new("editor-find")
                    .ghost()
                    .compact()
                    .icon(IconName::Search)
                    .label("查找")
                    .tooltip("查找 Ctrl+F")
                    .disabled(self.loading || self.document.is_none())
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_search(false, window, cx)
                    })),
            )
            .child(
                Button::new("editor-replace")
                    .ghost()
                    .compact()
                    .icon(IconName::Replace)
                    .label("替换")
                    .tooltip("查找并替换 Ctrl+H")
                    .disabled(self.loading || self.document.is_none())
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_search(true, window, cx)
                    })),
            )
            .child(div().w(px(1.)).h(px(20.)).bg(cx.theme().border).mx_1())
            .child(
                Button::new("editor-word-wrap")
                    .ghost()
                    .compact()
                    .icon(IconName::TextWrap)
                    .tooltip(if self.word_wrap {
                        "关闭自动换行"
                    } else {
                        "开启自动换行"
                    })
                    .selected(self.word_wrap)
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.toggle_word_wrap(window, cx)
                    })),
            )
            .child(
                Button::new("editor-line-numbers")
                    .ghost()
                    .compact()
                    .icon(IconName::ListOrdered)
                    .tooltip(if self.line_numbers {
                        "隐藏行号"
                    } else {
                        "显示行号"
                    })
                    .selected(self.line_numbers)
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.toggle_line_numbers(window, cx)
                    })),
            )
            .child(
                Button::new("editor-whitespace")
                    .ghost()
                    .compact()
                    .icon(IconName::Pilcrow)
                    .tooltip(if self.show_whitespace {
                        "隐藏空白字符"
                    } else {
                        "显示空白字符"
                    })
                    .selected(self.show_whitespace)
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.toggle_whitespace(window, cx)
                    })),
            )
            .child(
                div()
                    .ml_auto()
                    .text_xs()
                    .text_color(muted)
                    .child(self.editor_metadata_label()),
            )
            .into_any_element()
    }

    fn editor_metadata_label(&self) -> SharedString {
        let Some(document) = self.document.as_ref() else {
            return "正在识别…".into();
        };
        let language = display_language(&document.language);
        let line_ending = match document.line_ending {
            text_editor::TextLineEnding::Lf => "LF",
            text_editor::TextLineEnding::CrLf => "CRLF",
        };
        let bom = if document.has_bom { " · BOM" } else { "" };
        format!("{language} · UTF-8{bom} · {line_ending}").into()
    }

    fn render_editor_body(&self, cx: &App) -> AnyElement {
        let Some(editor) = self.editor.as_ref() else {
            return div().into_any_element();
        };
        if self.loading {
            return centered_message("正在加载文本…", cx);
        }
        if self.document.is_none() {
            return centered_message(
                self.message.as_deref().unwrap_or("无法打开此文本文件。"),
                cx,
            );
        }
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .p_2()
            .bg(cx.theme().background)
            .child(
                Editor::new(editor)
                    .appearance(false)
                    .bordered(false)
                    .h_full()
                    .w_full()
                    .aria_label("Kite 文本编辑器"),
            )
            .into_any_element()
    }

    fn render_read_only_text_body(&self, cx: &App) -> AnyElement {
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

    fn render_status_bar(&self, cx: &App) -> AnyElement {
        if self.kind != PreviewKind::Editor {
            return div().into_any_element();
        }
        let muted = cx.theme().muted_foreground;
        let dirty = if self.dirty { "未保存" } else { "已保存" };
        let line_count = self
            .editor
            .as_ref()
            .map(|editor| {
                editor
                    .read(cx)
                    .text()
                    .chars()
                    .filter(|character| *character == '\n')
                    .count()
                    + 1
            })
            .unwrap_or(1);
        h_flex()
            .h(px(28.))
            .w_full()
            .flex_shrink_0()
            .px_3()
            .gap_3()
            .items_center()
            .border_t_1()
            .border_color(cx.theme().border)
            .text_xs()
            .text_color(muted)
            .child(if self.dirty { "● 未保存" } else { dirty })
            .child(format!("{line_count} 行"))
            .child(self.editor_metadata_label())
            .when(self.saving, |this| this.child("正在写入磁盘…"))
            .when_some(self.message.as_ref(), |this, message| {
                this.child(div().ml_auto().truncate().child(message.clone()))
            })
            .into_any_element()
    }
}

impl Render for PreviewWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let body = match self.kind {
            PreviewKind::Editor => self.render_editor_body(cx),
            PreviewKind::ReadOnlyText => self.render_read_only_text_body(cx),
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
            .key_context("KiteEditor")
            .on_action(cx.listener(Self::on_save))
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
                    .child(
                        Icon::new(if self.kind == PreviewKind::Editor {
                            IconName::FileCode
                        } else {
                            IconName::FileText
                        })
                        .size_5()
                        .text_color(muted),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_sm()
                            .font_medium()
                            .child(self.name.clone()),
                    )
                    .when(self.dirty, |this| {
                        this.child(div().text_xs().text_color(muted).child("●"))
                    })
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
            .child(self.render_toolbar(cx))
            .child(body)
            .child(self.render_status_bar(cx))
    }
}

fn centered_message(message: &str, cx: &App) -> AnyElement {
    v_flex()
        .flex_1()
        .min_h_0()
        .w_full()
        .items_center()
        .justify_center()
        .p_8()
        .child(
            div()
                .max_w(px(560.))
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(message.to_string()),
        )
        .into_any_element()
}

fn display_language(language: &str) -> &'static str {
    match language {
        "rust" => "Rust",
        "javascript" => "JavaScript",
        "typescript" => "TypeScript",
        "tsx" => "TSX",
        "python" => "Python",
        "json" => "JSON",
        "toml" => "TOML",
        "yaml" => "YAML",
        "markdown" => "Markdown",
        "html" => "HTML",
        "css" => "CSS",
        "c" => "C",
        "cpp" => "C++",
        "csharp" => "C#",
        "java" => "Java",
        "go" => "Go",
        "kotlin" => "Kotlin",
        "bash" => "Shell",
        "sql" => "SQL",
        "swift" => "Swift",
        "lua" => "Lua",
        "ruby" => "Ruby",
        "php" => "PHP",
        "proto" => "Protocol Buffers",
        "cmake" => "CMake",
        "make" => "Make",
        "diff" => "Diff",
        _ => "纯文本",
    }
}

fn is_markdown_path(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .as_deref(),
        Some("txt" | "md" | "markdown")
    )
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
    use super::{display_language, plain_text_markdown};

    #[test]
    fn plain_text_uses_a_safe_code_fence() {
        let value = plain_text_markdown("`inline` and ```block```", "rs");
        assert!(value.starts_with("````rs\n"));
        assert!(value.ends_with("\n````"));
    }

    #[test]
    fn editor_language_labels_are_stable() {
        assert_eq!(display_language("rust"), "Rust");
        assert_eq!(display_language("unknown"), "纯文本");
    }
}
