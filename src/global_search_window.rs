use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::actions::{
    SetGlobalSearchKindAll, SetGlobalSearchKindDocuments, SetGlobalSearchKindFolders,
    SetGlobalSearchKindImages, SetGlobalSearchKindMarkdown, SetGlobalSearchKindMedia,
    SetGlobalSearchKindSourceCode, SetGlobalSearchScopeCurrentFolder,
    SetGlobalSearchScopeFavorites, SetGlobalSearchScopeLocalDisks,
    SetGlobalSearchScopeNetworkVolumes, SetGlobalSearchTimeAny, SetGlobalSearchTimeLast7Days,
    SetGlobalSearchTimeLast24Hours, SetGlobalSearchTimeLast30Days,
};
use crate::content_search::{
    self, CONTENT_SEARCH_MAX_FILES, ContentFileMatch, ContentSearchSnapshot,
};
use crate::fs::{FileEntry, format_size};
use crate::table::icon_for;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Selectable as _, StyledExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

static GLOBAL_SEARCH_SESSION_FILTERS: OnceLock<Mutex<SearchFilters>> = OnceLock::new();
use kite_core::search_filters::{SearchFilters, SearchKindFilter, SearchTimeFilter};

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
    NetworkVolumes,
}

impl GlobalSearchScope {
    fn label(self) -> &'static str {
        match self {
            Self::CurrentFolder => "当前文件夹",
            Self::Favorites => "收藏文件夹",
            Self::LocalDisks => "本地磁盘",
            Self::NetworkVolumes => "网络卷",
        }
    }

    fn allows_live_search(self) -> bool {
        self == Self::CurrentFolder
    }

    fn prepares_content_index_in_background(self) -> bool {
        matches!(self, Self::CurrentFolder | Self::Favorites)
    }

    fn icon(self) -> IconName {
        match self {
            Self::CurrentFolder => IconName::FolderOpen,
            Self::Favorites => IconName::Star,
            Self::LocalDisks => IconName::HardDrive,
            Self::NetworkVolumes => IconName::Network,
        }
    }
}

struct GlobalNameResults {
    query: String,
    groups: Vec<GlobalNameGroup>,
    total_entries: usize,
    truncated: bool,
    partial: bool,
}

struct GlobalNameGroup {
    key: String,
    entries: Vec<FileEntry>,
}

pub(crate) struct GlobalSearchWindow {
    current_folder: PathBuf,
    favorite_roots: Vec<PathBuf>,
    local_roots: Vec<PathBuf>,
    network_roots: Vec<PathBuf>,
    mode: GlobalSearchMode,
    scope: GlobalSearchScope,
    kind_filter: SearchKindFilter,
    time_filter: SearchTimeFilter,
    query_input: Entity<InputState>,
    name_results: Option<GlobalNameResults>,
    content_results: Option<ContentSearchSnapshot>,
    loading: bool,
    error: Option<SharedString>,
    generation: u64,
    search_cancel: Option<Arc<AtomicBool>>,
    expanded_groups: HashSet<String>,
    _search_task: Task<()>,
    _indexed_search_task: Task<()>,
    _live_search_task: Task<()>,
    _subscription: Subscription,
}

impl GlobalSearchWindow {
    pub(crate) fn new(
        current_folder: PathBuf,
        favorite_roots: Vec<PathBuf>,
        local_roots: Vec<PathBuf>,
        network_roots: Vec<PathBuf>,
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
                    this.start_immediate_search(query, window, cx);
                }
                InputEvent::Change => {
                    if !this.scope.allows_live_search() && this.loading {
                        this.cancel_search();
                        this.loading = false;
                        this.name_results = None;
                        this.content_results = None;
                        this.expanded_groups.clear();
                        this.error = None;
                    }
                    if this.mode == GlobalSearchMode::Content
                        && let Ok(service) = content_search::shared_service()
                    {
                        service.note_interaction();
                    }
                    this.schedule_live_search(window, cx);
                }
                _ => {}
            },
        );

        let mut index_roots = favorite_roots.clone();
        index_roots.extend(local_roots.iter().cloned());
        index_roots.push(current_folder.clone());
        kite_core::global_name_search::ensure_roots(&index_roots);

        let session_filters = global_search_session_filters();
        Self {
            current_folder,
            favorite_roots,
            local_roots,
            network_roots,
            mode: GlobalSearchMode::Name,
            scope: GlobalSearchScope::CurrentFolder,
            kind_filter: session_filters.kind,
            time_filter: session_filters.modified,
            query_input,
            name_results: None,
            content_results: None,
            loading: false,
            error: None,
            generation: 0,
            search_cancel: None,
            expanded_groups: HashSet::new(),
            _search_task: Task::ready(()),
            _indexed_search_task: Task::ready(()),
            _live_search_task: Task::ready(()),
            _subscription: subscription,
        }
    }

    fn scope_roots(&self) -> Vec<PathBuf> {
        match self.scope {
            GlobalSearchScope::CurrentFolder => vec![self.current_folder.clone()],
            GlobalSearchScope::Favorites => self.favorite_roots.clone(),
            GlobalSearchScope::LocalDisks => self.local_roots.clone(),
            GlobalSearchScope::NetworkVolumes => self.network_roots.clone(),
        }
    }

    fn filters(&self) -> SearchFilters {
        SearchFilters {
            kind: self.kind_filter,
            modified: self.time_filter,
        }
    }

    fn set_kind_filter(
        &mut self,
        filter: SearchKindFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.kind_filter == filter {
            return;
        }
        self.kind_filter = filter;
        store_global_search_session_filters(self.filters());
        self.refresh_after_filter_change(window, cx);
    }

    fn set_time_filter(
        &mut self,
        filter: SearchTimeFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.time_filter == filter {
            return;
        }
        self.time_filter = filter;
        store_global_search_session_filters(self.filters());
        self.refresh_after_filter_change(window, cx);
    }

    fn refresh_after_filter_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_search();
        self.cancel_live_search();
        self.loading = false;
        self.name_results = None;
        self.content_results = None;
        self.expanded_groups.clear();
        self.error = None;
        let query = self.query_input.read(cx).value().trim().to_string();
        if !query.is_empty() {
            self.start_search(query, window, cx);
        } else {
            cx.notify();
        }
    }

    fn scope_has_roots(&self, scope: GlobalSearchScope) -> bool {
        match scope {
            GlobalSearchScope::CurrentFolder => self.current_folder.is_dir(),
            GlobalSearchScope::Favorites => !self.favorite_roots.is_empty(),
            GlobalSearchScope::LocalDisks => !self.local_roots.is_empty(),
            GlobalSearchScope::NetworkVolumes => !self.network_roots.is_empty(),
        }
    }

    fn scope_detail(&self, scope: GlobalSearchScope) -> String {
        match scope {
            GlobalSearchScope::CurrentFolder => self
                .current_folder
                .file_name()
                .filter(|name| !name.is_empty())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.current_folder.display().to_string()),
            GlobalSearchScope::Favorites => format!("{} 个位置", self.favorite_roots.len()),
            GlobalSearchScope::LocalDisks => format!("{} 个卷", self.local_roots.len()),
            GlobalSearchScope::NetworkVolumes => format!("{} 个卷", self.network_roots.len()),
        }
    }

    fn scope_disabled_reason(&self, scope: GlobalSearchScope) -> Option<&'static str> {
        if self.scope_has_roots(scope) {
            return None;
        }
        match scope {
            GlobalSearchScope::CurrentFolder => Some("当前文件夹不可用"),
            GlobalSearchScope::Favorites => Some("暂无收藏文件夹"),
            GlobalSearchScope::LocalDisks => Some("未检测到本地磁盘"),
            GlobalSearchScope::NetworkVolumes => Some("未检测到网络卷"),
        }
    }

    fn scope_menu_label(&self, scope: GlobalSearchScope) -> String {
        compose_scope_menu_label(
            scope.label(),
            &self.scope_detail(scope),
            self.scope_disabled_reason(scope),
        )
    }

    fn scope_trigger_label(&self) -> String {
        self.scope_menu_label(self.scope)
    }

    fn set_mode(&mut self, mode: GlobalSearchMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        self.cancel_search();
        self.cancel_live_search();
        self.mode = mode;
        self.name_results = None;
        self.content_results = None;
        self.expanded_groups.clear();
        self.loading = false;
        self.error = None;
        if mode == GlobalSearchMode::Content {
            self.prepare_content_index();
        }
        self.query_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        self.schedule_live_search(window, cx);
        cx.notify();
    }

    fn set_scope(&mut self, scope: GlobalSearchScope, window: &mut Window, cx: &mut Context<Self>) {
        if self.scope == scope {
            return;
        }
        self.scope = scope;
        let roots = self.scope_roots();
        kite_core::global_name_search::ensure_roots(&roots);
        self.cancel_search();
        self.cancel_live_search();
        self.name_results = None;
        self.content_results = None;
        self.expanded_groups.clear();
        self.loading = false;
        self.error = None;
        if self.mode == GlobalSearchMode::Content
            && self.scope.prepares_content_index_in_background()
        {
            self.prepare_content_index();
        }
        self.query_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        self.schedule_live_search(window, cx);
        cx.notify();
    }

    fn prepare_content_index(&self) {
        if !self.scope.prepares_content_index_in_background() {
            return;
        }
        if let Ok(service) = content_search::shared_service() {
            let _ = service.ensure_roots(&self.scope_roots());
        }
    }

    fn cancel_search(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self._indexed_search_task = Task::ready(());
        if let Some(cancel) = self.search_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Ok(service) = content_search::shared_service() {
            service.cancel_search();
        }
    }

    fn cancel_live_search(&mut self) {
        self._live_search_task = Task::ready(());
    }

    fn start_immediate_search(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_live_search();
        self.start_search(query, window, cx);
    }

    fn schedule_live_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.scope.allows_live_search() {
            self.cancel_live_search();
            return;
        }
        let query = self.query_input.read(cx).value().trim().to_string();
        if query.chars().count() < 2 {
            self.cancel_live_search();
            return;
        }
        self._live_search_task = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(280))
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let current = this.query_input.read(cx).value().trim().to_string();
                if current == query {
                    this.start_search(query, window, cx);
                }
            });
        });
    }

    fn start_search(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading && !self.scope.allows_live_search() {
            self.cancel_search();
            self.loading = false;
        }
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
        let filters = self.filters();
        let cancel = Arc::new(AtomicBool::new(false));
        self.search_cancel = Some(Arc::clone(&cancel));
        self.name_results = None;
        self.content_results = None;
        self.expanded_groups.clear();
        self.loading = true;
        self.error = None;
        if let Ok(service) = content_search::shared_service() {
            service.note_interaction();
            let _ = service.ensure_roots(&roots);
        }
        cx.notify();

        if mode == GlobalSearchMode::Name && self.scope != GlobalSearchScope::CurrentFolder {
            let indexed_roots = roots.clone();
            let indexed_query = query.clone();
            let indexed_cancel = Arc::clone(&cancel);
            self._indexed_search_task = cx.spawn_in(window, async move |this, cx| {
                let result = cx
                    .background_spawn(async move {
                        kite_core::indexed_search::search_indexed_names(
                            &indexed_roots,
                            &indexed_query,
                            filters,
                            indexed_cancel.as_ref(),
                        )
                    })
                    .await;
                let _ = this.update_in(cx, |this, _, cx| {
                    if this.generation != generation || !this.loading {
                        return;
                    }
                    if let Ok(Some(snapshot)) = result {
                        this.name_results = Some(global_name_results(snapshot));
                        this.error = None;
                        cx.notify();
                    }
                });
            });
        }

        self._search_task = cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    match mode {
                        GlobalSearchMode::Name => {
                            search_name_roots(&roots, &query, filters, cancel.as_ref())
                                .map(SearchOutcome::Name)
                        }
                        GlobalSearchMode::Content => content_search::shared_service()
                            .and_then(|service| {
                                service.search_with_filters(
                                    &roots,
                                    &query,
                                    false,
                                    CONTENT_SEARCH_MAX_FILES,
                                    filters,
                                )
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
                this._indexed_search_task = Task::ready(());
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
            .child({
                let selected_scope = self.scope;
                let trigger_label = self.scope_trigger_label();
                let current_folder_label = self.scope_menu_label(GlobalSearchScope::CurrentFolder);
                let favorites_label = self.scope_menu_label(GlobalSearchScope::Favorites);
                let local_disks_label = self.scope_menu_label(GlobalSearchScope::LocalDisks);
                let network_volumes_label =
                    self.scope_menu_label(GlobalSearchScope::NetworkVolumes);
                let has_current_folder = self.scope_has_roots(GlobalSearchScope::CurrentFolder);
                let has_favorites = self.scope_has_roots(GlobalSearchScope::Favorites);
                let has_local_roots = self.scope_has_roots(GlobalSearchScope::LocalDisks);
                let has_network_roots = self.scope_has_roots(GlobalSearchScope::NetworkVolumes);
                Button::new("global-search-scope")
                    .ghost()
                    .compact()
                    .icon(self.scope.icon())
                    .label(format!("范围 · {trigger_label}"))
                    .tooltip(format!("当前搜索范围：{trigger_label}"))
                    .dropdown_caret(true)
                    .dropdown_menu(move |menu, _, _| {
                        menu.min_w(px(280.))
                            .max_w(px(340.))
                            .label("搜索范围")
                            .separator()
                            .item(
                                PopupMenuItem::new(current_folder_label.clone())
                                    .icon(GlobalSearchScope::CurrentFolder.icon())
                                    .checked(selected_scope == GlobalSearchScope::CurrentFolder)
                                    .disabled(!has_current_folder)
                                    .action(Box::new(SetGlobalSearchScopeCurrentFolder)),
                            )
                            .item(
                                PopupMenuItem::new(favorites_label.clone())
                                    .icon(GlobalSearchScope::Favorites.icon())
                                    .checked(selected_scope == GlobalSearchScope::Favorites)
                                    .disabled(!has_favorites)
                                    .action(Box::new(SetGlobalSearchScopeFavorites)),
                            )
                            .item(
                                PopupMenuItem::new(local_disks_label.clone())
                                    .icon(GlobalSearchScope::LocalDisks.icon())
                                    .checked(selected_scope == GlobalSearchScope::LocalDisks)
                                    .disabled(!has_local_roots)
                                    .action(Box::new(SetGlobalSearchScopeLocalDisks)),
                            )
                            .item(
                                PopupMenuItem::new(network_volumes_label.clone())
                                    .icon(GlobalSearchScope::NetworkVolumes.icon())
                                    .checked(selected_scope == GlobalSearchScope::NetworkVolumes)
                                    .disabled(!has_network_roots)
                                    .action(Box::new(SetGlobalSearchScopeNetworkVolumes)),
                            )
                    })
            })
            .child({
                let selected = self.kind_filter;
                Button::new("global-search-kind-filter")
                    .ghost()
                    .compact()
                    .label(selected.compact_label())
                    .tooltip("按项目类型过滤")
                    .dropdown_caret(true)
                    .dropdown_menu(move |menu, _, _| {
                        menu.min_w(px(210.))
                            .label("项目类型")
                            .separator()
                            .item(filter_kind_item(
                                selected,
                                SearchKindFilter::All,
                                SetGlobalSearchKindAll,
                            ))
                            .item(filter_kind_item(
                                selected,
                                SearchKindFilter::Folders,
                                SetGlobalSearchKindFolders,
                            ))
                            .item(filter_kind_item(
                                selected,
                                SearchKindFilter::Documents,
                                SetGlobalSearchKindDocuments,
                            ))
                            .item(filter_kind_item(
                                selected,
                                SearchKindFilter::SourceCode,
                                SetGlobalSearchKindSourceCode,
                            ))
                            .item(filter_kind_item(
                                selected,
                                SearchKindFilter::Markdown,
                                SetGlobalSearchKindMarkdown,
                            ))
                            .item(filter_kind_item(
                                selected,
                                SearchKindFilter::Images,
                                SetGlobalSearchKindImages,
                            ))
                            .item(filter_kind_item(
                                selected,
                                SearchKindFilter::Media,
                                SetGlobalSearchKindMedia,
                            ))
                    })
            })
            .child({
                let selected = self.time_filter;
                Button::new("global-search-time-filter")
                    .ghost()
                    .compact()
                    .label(selected.compact_label())
                    .tooltip("按修改时间过滤")
                    .dropdown_caret(true)
                    .dropdown_menu(move |menu, _, _| {
                        menu.min_w(px(210.))
                            .label("修改时间")
                            .separator()
                            .item(filter_time_item(
                                selected,
                                SearchTimeFilter::Any,
                                SetGlobalSearchTimeAny,
                            ))
                            .item(filter_time_item(
                                selected,
                                SearchTimeFilter::Last24Hours,
                                SetGlobalSearchTimeLast24Hours,
                            ))
                            .item(filter_time_item(
                                selected,
                                SearchTimeFilter::Last7Days,
                                SetGlobalSearchTimeLast7Days,
                            ))
                            .item(filter_time_item(
                                selected,
                                SearchTimeFilter::Last30Days,
                                SetGlobalSearchTimeLast30Days,
                            ))
                    })
            })
            .child(
                Button::new("global-search-submit")
                    .primary()
                    .compact()
                    .label("搜索")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        let query = this.query_input.read(cx).value().to_string();
                        this.start_immediate_search(query, window, cx);
                    })),
            )
            .into_any_element()
    }

    fn render_name_results(
        &self,
        results: &GlobalNameResults,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = v_flex().w_full().gap_2();
        for group in &results.groups {
            let expanded = self.expanded_groups.contains(&group.key);
            let visible_count = if expanded { group.entries.len() } else { 1 };
            let mut rows = v_flex().w_full().gap_1();
            for entry in group.entries.iter().take(visible_count) {
                let path = entry.path.clone();
                let location = entry
                    .path
                    .parent()
                    .map(|parent| parent.display().to_string())
                    .unwrap_or_else(|| entry.path.display().to_string());
                let icon = icon_for(entry);
                let metadata = format!(
                    "{} · {} · {}",
                    entry.type_label(),
                    if entry.is_dir() {
                        "文件夹".to_string()
                    } else {
                        entry.size_label()
                    },
                    entry.modified_label(),
                );
                rows = rows.child(
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
                            Icon::new(icon)
                                .size_4()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_0p5()
                                .child(
                                    h_flex()
                                        .min_w_0()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .min_w_0()
                                                .text_sm()
                                                .font_semibold()
                                                .truncate()
                                                .child(entry.name.clone()),
                                        )
                                        .when(group.entries.len() > 1, |this| {
                                            this.child(
                                                div()
                                                    .flex_shrink_0()
                                                    .rounded_full()
                                                    .bg(cx.theme().primary.opacity(0.12))
                                                    .px(px(6.))
                                                    .py_0p5()
                                                    .text_xs()
                                                    .text_color(cx.theme().primary)
                                                    .child(format!(
                                                        "{} 个位置",
                                                        group.entries.len()
                                                    )),
                                            )
                                        }),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().foreground.opacity(0.72))
                                        .truncate()
                                        .child(location),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .truncate()
                                        .child(metadata),
                                ),
                        ),
                );
            }
            if group.entries.len() > 1 {
                let key = group.key.clone();
                let label = if expanded {
                    "收起其他位置".to_string()
                } else {
                    format!("展开另外 {} 个位置", group.entries.len() - 1)
                };
                rows = rows.child(
                    Button::new(SharedString::from(format!(
                        "global-name-group-toggle-{}",
                        group.key
                    )))
                    .ghost()
                    .compact()
                    .label(label)
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            if !this.expanded_groups.remove(&key) {
                                this.expanded_groups.insert(key.clone());
                            }
                            cx.notify();
                        },
                    )),
                );
            }
            list = list.child(rows);
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
                    .child(format!("{} 项", results.total_entries))
                    .when_some(self.filters().summary(), |this, summary| {
                        this.child(summary)
                    })
                    .when(results.partial, |this| {
                        this.child("索引结果，正在核对全部文件…")
                    })
                    .when(results.truncated, |this| this.child("结果不完整")),
            )
            .child(if results.groups.is_empty() {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("没有找到匹配的文件名")
                    .into_any_element()
            } else {
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(list)
                    .into_any_element()
            })
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
                    .when_some(self.filters().summary(), |this, summary| {
                        this.child(summary)
                    })
                    .when(results.partial, |this| this.child("索引更新中"))
                    .when(results.truncated, |this| this.child("结果不完整")),
            )
            .child(if results.files.is_empty() {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("没有找到匹配的文件内容")
                    .into_any_element()
            } else {
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(list)
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_content_file(&self, file: &ContentFileMatch, cx: &mut Context<Self>) -> AnyElement {
        let path = file.path.clone();
        let mut snippets = v_flex().w_full().gap_1();
        for line in &file.matches {
            let mut styled = StyledText::new(line.text.clone());
            if let Some(range) =
                content_highlight_range(&line.text, line.column_start, line.column_end)
            {
                styled = styled.with_highlights([(
                    range,
                    HighlightStyle {
                        color: Some(cx.theme().primary),
                        font_weight: Some(FontWeight::SEMIBOLD),
                        background_color: Some(cx.theme().primary.opacity(0.16)),
                        ..Default::default()
                    },
                )]);
            }
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
                    .child(div().flex_1().min_w_0().text_xs().child(styled)),
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
                    .size_4()
                    .text_color(cx.theme().muted_foreground),
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
                                    .text_color(cx.theme().foreground.opacity(0.72))
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
            let roots = self.scope_roots();
            let scope_count = roots.len();
            let index_progress = kite_core::global_name_search::status(&roots)
                .into_iter()
                .filter_map(|status| status.message)
                .collect::<Vec<_>>()
                .join("；");
            div()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(
                    v_flex()
                        .items_center()
                        .gap_1()
                        .child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .child(Icon::new(IconName::Search).size_4())
                                .child(if !index_progress.is_empty() {
                                    "正在建立名称索引…"
                                } else if self.scope.allows_live_search() {
                                    "正在搜索…"
                                } else {
                                    "正在扫描所有搜索位置…"
                                }),
                        )
                        .when(!index_progress.is_empty(), |this| {
                            this.child(index_progress.clone())
                        })
                        .when(!self.scope.allows_live_search(), |this| {
                            this.child(if scope_count > 1 {
                                format!("共 {scope_count} 个位置。修改关键词可取消本次搜索。")
                            } else {
                                "修改关键词可取消本次搜索。".to_string()
                            })
                        }),
                )
                .into_any_element()
        } else if self.mode == GlobalSearchMode::Content {
            if let Some(results) = self.content_results.as_ref() {
                self.render_content_results(results, cx)
            } else {
                empty_global_search_state(cx, self.scope.allows_live_search())
            }
        } else if let Some(results) = self.name_results.as_ref() {
            self.render_name_results(results, cx)
        } else {
            empty_global_search_state(cx, self.scope.allows_live_search())
        };

        v_flex()
            .id("global-search-window")
            .size_full()
            .gap_2()
            .p_3()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(
                cx.listener(|this, _: &SetGlobalSearchScopeCurrentFolder, window, cx| {
                    this.set_scope(GlobalSearchScope::CurrentFolder, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &SetGlobalSearchScopeFavorites, window, cx| {
                    this.set_scope(GlobalSearchScope::Favorites, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &SetGlobalSearchScopeLocalDisks, window, cx| {
                    this.set_scope(GlobalSearchScope::LocalDisks, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &SetGlobalSearchScopeNetworkVolumes, window, cx| {
                    this.set_scope(GlobalSearchScope::NetworkVolumes, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &SetGlobalSearchKindAll, window, cx| {
                this.set_kind_filter(SearchKindFilter::All, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &SetGlobalSearchKindFolders, window, cx| {
                    this.set_kind_filter(SearchKindFilter::Folders, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &SetGlobalSearchKindDocuments, window, cx| {
                    this.set_kind_filter(SearchKindFilter::Documents, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &SetGlobalSearchKindSourceCode, window, cx| {
                    this.set_kind_filter(SearchKindFilter::SourceCode, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &SetGlobalSearchKindMarkdown, window, cx| {
                    this.set_kind_filter(SearchKindFilter::Markdown, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &SetGlobalSearchKindImages, window, cx| {
                    this.set_kind_filter(SearchKindFilter::Images, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &SetGlobalSearchKindMedia, window, cx| {
                    this.set_kind_filter(SearchKindFilter::Media, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &SetGlobalSearchTimeAny, window, cx| {
                this.set_time_filter(SearchTimeFilter::Any, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &SetGlobalSearchTimeLast24Hours, window, cx| {
                    this.set_time_filter(SearchTimeFilter::Last24Hours, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &SetGlobalSearchTimeLast7Days, window, cx| {
                    this.set_time_filter(SearchTimeFilter::Last7Days, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &SetGlobalSearchTimeLast30Days, window, cx| {
                    this.set_time_filter(SearchTimeFilter::Last30Days, window, cx)
                }),
            )
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

fn empty_global_search_state(cx: &App, allows_live_search: bool) -> AnyElement {
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
                .child(if allows_live_search {
                    "输入至少两个字符后自动搜索"
                } else {
                    "当前范围较大，请输入关键词后按 Enter 或点击搜索"
                }),
        )
        .into_any_element()
}

fn search_name_roots(
    roots: &[PathBuf],
    query: &str,
    filters: SearchFilters,
    cancel: &AtomicBool,
) -> Result<Option<GlobalNameResults>, String> {
    let Some(snapshot) =
        kite_core::global_name_search::search_names(roots, query, filters, cancel)?
    else {
        return Ok(None);
    };
    Ok(Some(global_name_results(snapshot)))
}

fn global_name_results(snapshot: crate::fs::GlobalNameSearchSnapshot) -> GlobalNameResults {
    let total_entries = snapshot.entries.len();
    GlobalNameResults {
        query: snapshot.query,
        groups: group_name_entries(&snapshot.entries),
        total_entries,
        truncated: snapshot.truncated,
        partial: snapshot.partial,
    }
}

fn group_name_entries(entries: &[FileEntry]) -> Vec<GlobalNameGroup> {
    let mut groups = Vec::<GlobalNameGroup>::new();
    let mut indexes = HashMap::<String, usize>::new();
    for entry in entries {
        let key = format!(
            "{}:{}",
            if entry.is_dir() { "dir" } else { "file" },
            entry.name.to_lowercase()
        );
        if let Some(index) = indexes.get(&key) {
            groups[*index].entries.push(entry.clone());
        } else {
            indexes.insert(key.clone(), groups.len());
            groups.push(GlobalNameGroup {
                key,
                entries: vec![entry.clone()],
            });
        }
    }
    groups
}

fn content_highlight_range(
    text: &str,
    column_start: Option<u64>,
    column_end: Option<u64>,
) -> Option<std::ops::Range<usize>> {
    let start_char = usize::try_from(column_start?.checked_sub(1)?).ok()?;
    let end_char = usize::try_from(column_end?.checked_sub(1)?).ok()?;
    if start_char >= end_char {
        return None;
    }
    let start = text
        .char_indices()
        .nth(start_char)
        .map(|(index, _)| index)?;
    let end = text
        .char_indices()
        .nth(end_char)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    Some(start..end)
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

fn compose_scope_menu_label(label: &str, detail: &str, disabled_reason: Option<&str>) -> String {
    match disabled_reason {
        Some(reason) => format!("{label} · {reason}"),
        None => format!("{label} · {detail}"),
    }
}

fn filter_kind_item<A: Action>(
    selected: SearchKindFilter,
    value: SearchKindFilter,
    action: A,
) -> PopupMenuItem {
    PopupMenuItem::new(value.label())
        .checked(selected == value)
        .action(Box::new(action))
}

fn filter_time_item<A: Action>(
    selected: SearchTimeFilter,
    value: SearchTimeFilter,
    action: A,
) -> PopupMenuItem {
    PopupMenuItem::new(value.label())
        .checked(selected == value)
        .action(Box::new(action))
}

fn global_search_session_filters() -> SearchFilters {
    GLOBAL_SEARCH_SESSION_FILTERS
        .get_or_init(|| Mutex::new(SearchFilters::default()))
        .lock()
        .map(|filters| *filters)
        .unwrap_or_default()
}

fn store_global_search_session_filters(filters: SearchFilters) {
    if let Ok(mut stored) = GLOBAL_SEARCH_SESSION_FILTERS
        .get_or_init(|| Mutex::new(SearchFilters::default()))
        .lock()
    {
        *stored = filters;
    }
}

pub(crate) fn open_global_search_window(
    current_folder: PathBuf,
    favorite_roots: Vec<PathBuf>,
    local_roots: Vec<PathBuf>,
    network_roots: Vec<PathBuf>,
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
            GlobalSearchWindow::new(
                current_folder,
                favorite_roots,
                local_roots,
                network_roots,
                window,
                cx,
            )
        })
    })
    .map(|_| ())
    .map_err(|error| format!("无法打开全局搜索窗口：{error}"))
}

#[cfg(test)]
mod tests {
    use super::compose_scope_menu_label;

    #[test]
    fn scope_label_uses_detail_when_available() {
        assert_eq!(
            compose_scope_menu_label("本地磁盘", "2 个卷", None),
            "本地磁盘 · 2 个卷"
        );
    }

    #[test]
    fn scope_label_uses_disabled_reason_instead_of_detail() {
        assert_eq!(
            compose_scope_menu_label("收藏文件夹", "0 个位置", Some("暂无收藏文件夹")),
            "收藏文件夹 · 暂无收藏文件夹"
        );
    }
}
