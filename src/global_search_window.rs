use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::content_search::{
    self, CONTENT_SEARCH_MAX_FILES, ContentFileMatch, ContentSearchSnapshot,
};
use crate::fs::{FileEntry, SortSpec, format_size, search_directory, sort_entries};
use crate::table::icon_for;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Selectable as _, StyledExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlobalSearchMode {
    Name,
    Content,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlobalSearchScope {
    CurrentFolder,
    Favorites,
    LocalDisks,
}

impl GlobalSearchScope {
    fn label(self) -> &'static str {
        match self {
            Self::CurrentFolder => "当前文件夹",
            Self::Favorites => "收藏范围",
            Self::LocalDisks => "本地磁盘",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::CurrentFolder => Self::Favorites,
            Self::Favorites => Self::LocalDisks,
            Self::LocalDisks => Self::CurrentFolder,
        }
    }
}

struct GlobalNameResults {
    query: String,
    entries: Vec<FileEntry>,
    truncated: bool,
}

pub(crate) struct GlobalSearchWindow {
    current_folder: PathBuf,
    favorite_roots: Vec<PathBuf>,
    local_roots: Vec<PathBuf>,
    mode: GlobalSearchMode,
    scope: GlobalSearchScope,
    query_input: Entity<InputState>,
    name_results: Option<GlobalNameResults>,
    content_results: Option<ContentSearchSnapshot>,
    loading: bool,
    error: Option<SharedString>,
    generation: u64,
    search_cancel: Option<Arc<AtomicBool>>,
    _search_task: Task<()>,
    _subscription: Subscription,
}

impl GlobalSearchWindow {
    pub(crate) fn new(
        current_folder: PathBuf,
        favorite_roots: Vec<PathBuf>,
        local_roots: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入文件名或文件内容")
                .clean_on_escape()
        });
        query_input.update(cx, |input, cx| input.focus(window, cx));

        let subscription = cx.subscribe_in(
            &query_input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let query = this.query_input.read(cx).value().to_string();
                    this.start_search(query, window, cx);
                }
                InputEvent::Change => {
                    if this.mode == GlobalSearchMode::Content
                        && let Ok(service) = content_search::shared_service()
                    {
                        service.note_interaction();
                    }
                }
                _ => {}
            },
        );

        Self {
            current_folder,
            favorite_roots,
            local_roots,
            mode: GlobalSearchMode::Name,
            scope: GlobalSearchScope::CurrentFolder,
            query_input,
            name_results: None,
            content_results: None,
            loading: false,
            error: None,
            generation: 0,
            search_cancel: None,
            _search_task: Task::ready(()),
            _subscription: subscription,
        }
    }

    fn scope_roots(&self) -> Vec<PathBuf> {
        match self.scope {
            GlobalSearchScope::CurrentFolder => vec![self.current_folder.clone()],
            GlobalSearchScope::Favorites => self.favorite_roots.clone(),
            GlobalSearchScope::LocalDisks => self.local_roots.clone(),
        }
    }

    fn set_mode(&mut self, mode: GlobalSearchMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        self.cancel_search();
        self.mode = mode;
        self.name_results = None;
        self.content_results = None;
        self.loading = false;
        self.error = None;
        if mode == GlobalSearchMode::Content {
            self.prepare_content_index();
        }
        self.query_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn cycle_scope(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scope = self.scope.next();
        self.cancel_search();
        self.name_results = None;
        self.content_results = None;
        self.error = None;
        if self.mode == GlobalSearchMode::Content {
            self.prepare_content_index();
        }
        self.query_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn prepare_content_index(&self) {
        if let Ok(service) = content_search::shared_service() {
            let _ = service.ensure_roots(&self.scope_roots());
        }
    }

    fn cancel_search(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(cancel) = self.search_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Ok(service) = content_search::shared_service() {
            service.cancel_search();
        }
    }

    fn start_search(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        let query = query.trim().to_string();
        if query.is_empty() {
            self.error = Some("请输入搜索内容".into());
            cx.notify();
            return;
        }
        let roots = self.scope_roots();
        if roots.is_empty() {
            self.error = Some("当前搜索范围没有可搜索的文件夹".into());
            cx.notify();
            return;
        }

        self.cancel_search();
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let mode = self.mode;
        let cancel = Arc::new(AtomicBool::new(false));
        self.search_cancel = Some(Arc::clone(&cancel));
        self.name_results = None;
        self.content_results = None;
        self.loading = true;
        self.error = None;
        if let Ok(service) = content_search::shared_service() {
            service.note_interaction();
            let _ = service.ensure_roots(&roots);
        }
        cx.notify();

        self._search_task = cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    match mode {
                        GlobalSearchMode::Name => {
                            search_name_roots(&roots, &query, cancel.as_ref())
                                .map(SearchOutcome::Name)
                        }
                        GlobalSearchMode::Content => content_search::shared_service()
                            .and_then(|service| {
                                service.search(&roots, &query, false, CONTENT_SEARCH_MAX_FILES)
                            })
                            .map(SearchOutcome::Content),
                    }
                })
                .await;

            let _ = this.update_in(cx, |this, _, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                this.search_cancel = None;
                match result {
                    Ok(SearchOutcome::Name(result)) => {
                        this.name_results = result;
                        this.error = None;
                    }
                    Ok(SearchOutcome::Content(result)) => {
                        this.content_results = result;
                        this.error = None;
                    }
                    Err(error) => {
                        this.error = Some(error.into());
                    }
                }
                cx.notify();
            });
        });
    }

    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    open::that(&path)
                        .map_err(|error| format!("无法打开 {}：{error}", path.display()))
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                if let Err(error) = result {
                    this.error = Some(error.into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn render_search_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .p_2()
            .rounded_lg()
            .bg(cx.theme().title_bar)
            .border_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_0p5()
                    .rounded_md()
                    .bg(cx.theme().background.opacity(0.5))
                    .child(
                        Button::new("global-search-mode-name")
                            .ghost()
                            .compact()
                            .label("名称")
                            .selected(self.mode == GlobalSearchMode::Name)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.set_mode(GlobalSearchMode::Name, window, cx)
                            })),
                    )
                    .child(
                        Button::new("global-search-mode-content")
                            .ghost()
                            .compact()
                            .label("内容")
                            .selected(self.mode == GlobalSearchMode::Content)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.set_mode(GlobalSearchMode::Content, window, cx)
                            })),
                    ),
            )
            .child(
                div().flex_1().min_w(px(260.)).child(
                    Input::new(&self.query_input)
                        .aria_label(match self.mode {
                            GlobalSearchMode::Name => "搜索文件名",
                            GlobalSearchMode::Content => "搜索文件内容",
                        })
                        .prefix(Icon::new(IconName::Search).size_4())
                        .cleanable(true),
                ),
            )
            .when(self.mode == GlobalSearchMode::Content, |this| {
                this.child(
                    Button::new("global-search-scope")
                        .ghost()
                        .compact()
                        .label(self.scope.label())
                        .tooltip("切换搜索范围")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.cycle_scope(window, cx)
                        })),
                )
            })
            .child(
                Button::new("global-search-submit")
                    .primary()
                    .compact()
                    .label("搜索")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        let query = this.query_input.read(cx).value().to_string();
                        this.start_search(query, window, cx);
                    })),
            )
            .into_any_element()
    }

    fn render_name_results(
        &self,
        results: &GlobalNameResults,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = v_flex().w_full().gap_1();
        for entry in &results.entries {
            let path = entry.path.clone();
            let icon = icon_for(entry);
            let detail = if entry.is_dir() {
                entry.path.display().to_string()
            } else {
                format!("{} · {}", entry.type_label(), entry.size_label())
            };
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!(
                        "global-name-{}",
                        entry.path.display()
                    )))
                    .w_full()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(cx.theme().background.opacity(0.55))
                    .border_1()
                    .border_color(cx.theme().border)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        if event.click_count() == 2 {
                            this.open_path(path.clone(), window, cx);
                        }
                    }))
                    .child(Icon::new(icon).size_5().text_color(cx.theme().primary))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_medium()
                                    .truncate()
                                    .child(entry.name.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .truncate()
                                    .child(detail),
                            ),
                    ),
            );
        }

        v_flex()
            .size_full()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("“{}”的名称结果", results.query))
                    .child(format!("{} 项", results.entries.len()))
                    .when(results.truncated, |this| this.child("结果已截断")),
            )
            .child(div().flex_1().min_h_0().overflow_y_scrollbar().child(list))
            .into_any_element()
    }

    fn render_content_results(
        &self,
        results: &ContentSearchSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = v_flex().w_full().gap_2();
        for file in &results.files {
            list = list.child(self.render_content_file(file, cx));
        }

        v_flex()
            .size_full()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("“{}”的内容结果", results.query))
                    .child(format!("{} 个文件", results.files.len()))
                    .child(format!("{} 处匹配", results.total_matches))
                    .when(results.partial, |this| this.child("索引更新中"))
                    .when(results.truncated, |this| this.child("结果已截断")),
            )
            .child(div().flex_1().min_h_0().overflow_y_scrollbar().child(list))
            .into_any_element()
    }

    fn render_content_file(&self, file: &ContentFileMatch, cx: &mut Context<Self>) -> AnyElement {
        let path = file.path.clone();
        let mut snippets = v_flex().w_full().gap_1();
        for line in &file.matches {
            snippets = snippets.child(
                h_flex()
                    .w_full()
                    .items_start()
                    .gap_3()
                    .child(
                        div()
                            .w(px(46.))
                            .flex_shrink_0()
                            .text_right()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(line.line_number.to_string()),
                    )
                    .child(div().flex_1().min_w_0().text_xs().child(line.text.clone())),
            );
        }

        h_flex()
            .id(SharedString::from(format!(
                "global-content-{}",
                file.path.display()
            )))
            .w_full()
            .items_start()
            .gap_3()
            .p_3()
            .rounded_md()
            .bg(cx.theme().background.opacity(0.62))
            .border_1()
            .border_color(cx.theme().border)
            .cursor_pointer()
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if event.click_count() == 2 {
                    this.open_path(path.clone(), window, cx);
                }
            }))
            .child(
                Icon::new(content_file_icon(&file.path))
                    .size_5()
                    .text_color(cx.theme().primary),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .text_sm()
                                    .font_medium()
                                    .truncate()
                                    .child(file.name.clone()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("{} 处", file.match_count)),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .truncate()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(file.relative_path.clone()),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(file.kind_label.clone())
                            .child(format_size(file.size)),
                    )
                    .child(snippets),
            )
            .into_any_element()
    }
}

enum SearchOutcome {
    Name(Option<GlobalNameResults>),
    Content(Option<ContentSearchSnapshot>),
}

impl Render for GlobalSearchWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = if let Some(error) = self.error.as_ref() {
            div()
                .flex_1()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().danger)
                .child(error.clone())
                .into_any_element()
        } else if self.loading && self.name_results.is_none() && self.content_results.is_none() {
            div()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(Icon::new(IconName::Search).size_4())
                .child("正在搜索…")
                .into_any_element()
        } else if self.mode == GlobalSearchMode::Content {
            if let Some(results) = self.content_results.as_ref() {
                self.render_content_results(results, cx)
            } else {
                empty_global_search_state(cx)
            }
        } else if let Some(results) = self.name_results.as_ref() {
            self.render_name_results(results, cx)
        } else {
            empty_global_search_state(cx)
        };

        v_flex()
            .id("global-search-window")
            .size_full()
            .gap_2()
            .p_3()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(Icon::new(IconName::Search).size_4())
                    .child(div().text_lg().font_semibold().child("全局搜索"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Ctrl+Shift+F"),
                    ),
            )
            .child(self.render_search_bar(cx))
            .when(self.loading, |this| {
                this.child(
                    div()
                        .h(px(2.))
                        .w_full()
                        .rounded_full()
                        .bg(cx.theme().primary),
                )
            })
            .child(div().flex_1().min_h_0().w_full().child(content))
    }
}

fn empty_global_search_state(cx: &App) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(IconName::Search).size_5())
        .child(
            div()
                .text_sm()
                .font_medium()
                .child("输入关键词开始全局搜索"),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("名称模式搜索文件名，内容模式搜索本地文本内容"),
        )
        .into_any_element()
}

fn search_name_roots(
    roots: &[PathBuf],
    query: &str,
    cancel: &AtomicBool,
) -> Result<Option<GlobalNameResults>, String> {
    let mut entries = Vec::new();
    let mut truncated = false;
    for root in roots {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let Some(snapshot) = search_directory(root, query, SortSpec::default(), cancel)? else {
            continue;
        };
        truncated |= snapshot.truncated;
        entries.extend(snapshot.entries);
        if entries.len() >= 1000 {
            entries.truncate(1000);
            truncated = true;
            break;
        }
    }
    sort_entries(&mut entries, SortSpec::default());
    let mut seen = BTreeSet::new();
    entries.retain(|entry| seen.insert(entry.path.clone()));
    Ok(Some(GlobalNameResults {
        query: query.to_string(),
        entries,
        truncated,
    }))
}

fn content_file_icon(path: &Path) -> IconName {
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" => IconName::FileImage,
        "mp3" | "wav" | "flac" | "aac" | "ogg" => IconName::FileMusic,
        "mp4" | "mkv" | "mov" | "avi" | "webm" => IconName::FileVideoCamera,
        "zip" | "7z" | "rar" | "tar" | "gz" => IconName::FileArchive,
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "html" | "css" | "c" | "cpp" | "h" | "cs"
        | "java" | "go" | "kt" => IconName::FileCode,
        "txt" | "md" | "log" | "ini" | "toml" | "yaml" | "yml" | "json" | "pdf" | "doc"
        | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => IconName::FileText,
        _ => IconName::File,
    }
}

pub(crate) fn open_global_search_window(
    current_folder: PathBuf,
    favorite_roots: Vec<PathBuf>,
    local_roots: Vec<PathBuf>,
    cx: &mut App,
) -> Result<(), String> {
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("全局搜索".into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::centered(size(px(920.), px(700.)), cx)),
        window_min_size: Some(size(px(720.), px(520.))),
        ..Default::default()
    };
    gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| {
            GlobalSearchWindow::new(current_folder, favorite_roots, local_roots, window, cx)
        })
    })
    .map(|_| ())
    .map_err(|error| format!("无法打开全局搜索窗口：{error}"))
}
