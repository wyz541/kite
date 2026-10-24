use std::cell::RefCell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::actions::*;
use crate::fs::{
    FileEntry, SortSpec, create_folder, load_directory, open_file, recycle_entry, rename_entry,
    search_directory, transfer_entry, validate_name,
};
use crate::table::{FileTableDelegate, icon_color, icon_for, vcs_badge};
use crate::vcs::{apply_statuses, load_vcs_snapshot};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu};
use gpui_kit::component::table::{DataTable, TableEvent, TableState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, StyledExt as _, Theme, ThemeMode,
    h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

#[derive(Clone, Copy, Debug)]
enum NavigationIntent {
    Initial,
    Push,
    ReplaceIndex(usize),
    Refresh,
}

struct PathSegment {
    label: SharedString,
    path: PathBuf,
    clickable: bool,
}

#[derive(Clone)]
struct QuickLink {
    label: SharedString,
    path: PathBuf,
    icon: IconName,
}

#[derive(Clone)]
struct SearchState {
    query: String,
    root: PathBuf,
}

#[derive(Clone)]
enum ModalKind {
    NewFolder,
    Rename(PathBuf),
    GoToFolder,
}

struct ModalState {
    kind: ModalKind,
    input: Entity<InputState>,
    _subscription: Subscription,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClipboardMode {
    Copy,
    Cut,
}

#[derive(Clone)]
pub(crate) struct FileClipboard {
    path: PathBuf,
    mode: ClipboardMode,
}

pub(crate) type SharedClipboard = Rc<RefCell<Option<FileClipboard>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewMode {
    Columns,
    List,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabRequest {
    New,
    Close,
    Next,
    Previous,
}

#[derive(Clone)]
struct ColumnPane {
    path: PathBuf,
    entries: Vec<FileEntry>,
    selected_path: Option<PathBuf>,
    loading: bool,
}

pub struct KiteApp {
    current_dir: PathBuf,
    history: Vec<PathBuf>,
    history_index: usize,
    quick_links: Vec<QuickLink>,
    drives: Vec<QuickLink>,
    address_input: Entity<InputState>,
    search_input: Entity<InputState>,
    table: Entity<TableState<FileTableDelegate>>,
    focus_handle: FocusHandle,
    active_search: Option<SearchState>,
    search_truncated: bool,
    loading: bool,
    skipped: usize,
    error: Option<SharedString>,
    notice: Option<SharedString>,
    modal: Option<ModalState>,
    clipboard: SharedClipboard,
    load_generation: u64,
    search_generation: u64,
    vcs_generation: u64,
    search_cancel: Option<Arc<AtomicBool>>,
    _load_task: Task<()>,
    _search_task: Task<()>,
    _vcs_task: Task<()>,
    view_mode: ViewMode,
    show_path_bar: bool,
    column_panes: Vec<ColumnPane>,
    column_selected_path: Option<PathBuf>,
    column_scroll: ScrollHandle,
    column_scroll_pending: bool,
    path_bar_scroll: ScrollHandle,
    path_bar_scroll_pending: bool,
    column_generation: u64,
    _column_task: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl KiteApp {
    pub(crate) fn default_directory() -> PathBuf {
        std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .or_else(dirs::home_dir)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("C:\\"))
    }

    pub(crate) fn new_at(
        home: PathBuf,
        clipboard: SharedClipboard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let quick_links = build_quick_links();
        let drives = build_drives();
        let address_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入文件夹路径，按 Enter 打开")
                .clean_on_escape()
        });
        let search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("搜索当前文件夹")
                .clean_on_escape()
        });
        let table = cx.new(|cx| {
            TableState::new(
                FileTableDelegate::new(Vec::new(), SortSpec::default()),
                window,
                cx,
            )
            .col_movable(false)
            .col_resizable(true)
            .row_selectable(true)
            .sortable(true)
        });

        let address_subscription = cx.subscribe_in(
            &address_input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    let value = this.address_input.read(cx).value().to_string();
                    this.navigate_from_text(value, window, cx);
                }
            },
        );
        let search_subscription = cx.subscribe_in(
            &search_input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    let value = this.search_input.read(cx).value().to_string();
                    this.submit_search(value, window, cx);
                }
            },
        );
        let table_subscription = cx.subscribe_in(&table, window, Self::on_table_event);

        let mut app = Self {
            current_dir: home.clone(),
            history: Vec::new(),
            history_index: 0,
            quick_links,
            drives,
            address_input,
            search_input,
            table,
            focus_handle: cx.focus_handle(),
            active_search: None,
            search_truncated: false,
            loading: false,
            skipped: 0,
            error: None,
            notice: None,
            modal: None,
            clipboard,
            load_generation: 0,
            search_generation: 0,
            vcs_generation: 0,
            search_cancel: None,
            _load_task: Task::ready(()),
            _search_task: Task::ready(()),
            _vcs_task: Task::ready(()),
            view_mode: ViewMode::Columns,
            show_path_bar: true,
            column_panes: Vec::new(),
            column_selected_path: None,
            column_scroll: ScrollHandle::new(),
            column_scroll_pending: false,
            path_bar_scroll: ScrollHandle::new(),
            path_bar_scroll_pending: false,
            column_generation: 0,
            _column_task: Task::ready(()),
            _subscriptions: vec![
                address_subscription,
                search_subscription,
                table_subscription,
            ],
        };
        app.request_load(home, NavigationIntent::Initial, window, cx);
        app
    }

    pub(crate) fn current_dir(&self) -> &Path {
        &self.current_dir
    }

    fn request_tab(&mut self, request: TabRequest, cx: &mut Context<Self>) {
        cx.emit(request);
    }

    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("alt-left", NavigateBack, Some("Kite")),
            KeyBinding::new("alt-right", NavigateForward, Some("Kite")),
            KeyBinding::new("alt-up", NavigateUp, Some("Kite")),
            KeyBinding::new("secondary-r", RefreshFolder, Some("Kite")),
            KeyBinding::new("secondary-l", FocusAddress, Some("Kite")),
            KeyBinding::new("secondary-f", FocusSearch, Some("Kite")),
            KeyBinding::new("secondary-1", ShowListView, Some("Kite")),
            KeyBinding::new("secondary-2", ShowColumnsView, Some("Kite")),
            KeyBinding::new("secondary-t", NewTab, Some("Kite")),
            KeyBinding::new("secondary-w", CloseTab, Some("Kite")),
            KeyBinding::new("secondary-tab", NextTab, Some("Kite")),
            KeyBinding::new("secondary-shift-tab", PreviousTab, Some("Kite")),
            KeyBinding::new("alt-p", TogglePathBar, Some("Kite")),
            KeyBinding::new("secondary-shift-g", GoToFolder, Some("Kite")),
            KeyBinding::new("secondary-shift-n", CreateFolder, Some("Kite")),
            KeyBinding::new("secondary-c", CopySelection, Some("Kite")),
            KeyBinding::new("secondary-x", CutSelection, Some("Kite")),
            KeyBinding::new("secondary-v", PasteSelection, Some("Kite")),
            KeyBinding::new("f2", RenameSelection, Some("Kite")),
            KeyBinding::new("delete", RecycleSelection, Some("Kite")),
            KeyBinding::new("enter", OpenSelection, Some("Kite")),
            KeyBinding::new("escape", ClearSearchOverlay, Some("Kite")),
        ]);
    }
    pub fn install_menus(cx: &mut App) {
        cx.set_menus([
            Menu::new("文件").items([
                MenuItem::action("新建页签", NewTab),
                MenuItem::action("关闭页签", CloseTab),
                MenuItem::separator(),
                MenuItem::action("下一个页签", NextTab),
                MenuItem::action("上一个页签", PreviousTab),
            ]),
            Menu::new("前往").items([
                MenuItem::action("上层文件夹", NavigateUp),
                MenuItem::separator(),
                MenuItem::action("前往文件夹…", GoToFolder),
            ]),
            Menu::new("显示").items([
                MenuItem::action("列表视图", ShowListView),
                MenuItem::action("分栏视图", ShowColumnsView),
                MenuItem::separator(),
                MenuItem::action("显示或隐藏路径栏", TogglePathBar),
            ]),
        ]);
    }

    fn request_load(
        &mut self,
        path: PathBuf,
        intent: NavigationIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_search();
        self.invalidate_vcs();
        self.invalidate_columns();
        self.load_generation = self.load_generation.wrapping_add(1);
        let generation = self.load_generation;
        let sort = self.table.read(cx).delegate().sort_spec();
        let load_path = path.clone();

        self.loading = true;
        self.error = None;
        self.active_search = None;
        self.search_truncated = false;
        self.table.update(cx, |state, cx| {
            state.delegate_mut().set_loading(true);
            state.refresh(cx);
            cx.notify();
        });
        cx.notify();

        self._load_task = cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { load_directory(&load_path, sort) })
                .await;

            let _ = this.update_in(cx, |this, window, cx| {
                if this.load_generation != generation {
                    return;
                }
                this.loading = false;
                this.table.update(cx, |state, cx| {
                    state.delegate_mut().set_loading(false);
                    cx.notify();
                });

                match result {
                    Ok(snapshot) => {
                        this.apply_history(snapshot.path.clone(), intent);
                        this.current_dir = snapshot.path.clone();
                        this.skipped = snapshot.skipped;
                        this.error = None;
                        this.address_input.update(cx, |input, cx| {
                            input.set_value(path_text(&snapshot.path), window, cx);
                        });
                        this.search_input.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                        });
                        this.table.update(cx, |state, cx| {
                            state.delegate_mut().replace_entries(snapshot.entries, None);
                            state.clear_selection(cx);
                            state.refresh(cx);
                            cx.notify();
                        });
                        if this.view_mode == ViewMode::Columns {
                            this.load_columns(this.current_dir.clone(), cx);
                        } else {
                            this.refresh_vcs_statuses(this.current_dir.clone(), cx);
                        }
                        this.path_bar_scroll_pending = true;
                    }
                    Err(error) => {
                        this.error = Some(error.into());
                        this.table.update(cx, |state, cx| {
                            state.refresh(cx);
                            cx.notify();
                        });
                    }
                }
                cx.notify();
            });
        });
    }

    fn apply_history(&mut self, path: PathBuf, intent: NavigationIntent) {
        match intent {
            NavigationIntent::Initial => {
                self.history.clear();
                self.history.push(path);
                self.history_index = 0;
            }
            NavigationIntent::Push => {
                if self.history.is_empty() {
                    self.history.push(path);
                    self.history_index = 0;
                    return;
                }
                if same_path(&self.history[self.history_index], &path) {
                    return;
                }
                self.history.truncate(self.history_index + 1);
                self.history.push(path);
                self.history_index = self.history.len().saturating_sub(1);
            }
            NavigationIntent::ReplaceIndex(index) => {
                if index < self.history.len() {
                    self.history_index = index;
                }
            }
            NavigationIntent::Refresh => {}
        }
    }

    fn navigate_to(
        &mut self,
        path: PathBuf,
        intent: NavigationIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.notice = None;
        self.request_load(path, intent, window, cx);
    }

    fn navigate_from_text(&mut self, value: String, window: &mut Window, cx: &mut Context<Self>) {
        match resolve_location_input(&value, &self.current_dir) {
            Ok(path) => self.navigate_to(path, NavigationIntent::Push, window, cx),
            Err(error) => {
                self.error = Some(error.into());
                cx.notify();
            }
        }
    }

    fn go_back(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_back(window, cx);
    }

    fn navigate_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history_index == 0 {
            return;
        }
        let index = self.history_index - 1;
        self.notice = None;
        self.request_load(
            self.history[index].clone(),
            NavigationIntent::ReplaceIndex(index),
            window,
            cx,
        );
    }

    fn go_forward(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_forward(window, cx);
    }

    fn navigate_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history_index + 1 >= self.history.len() {
            return;
        }
        let index = self.history_index + 1;
        self.notice = None;
        self.request_load(
            self.history[index].clone(),
            NavigationIntent::ReplaceIndex(index),
            window,
            cx,
        );
    }

    fn go_up(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_up(window, cx);
    }

    fn navigate_up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(parent) = self.current_dir.parent() else {
            self.error = Some("已经在驱动器根目录".into());
            cx.notify();
            return;
        };
        self.navigate_to(parent.to_path_buf(), NavigationIntent::Push, window, cx);
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        if let Some(search) = self.active_search.clone() {
            self.start_search(search.query, window, cx);
        } else {
            self.request_load(
                self.current_dir.clone(),
                NavigationIntent::Refresh,
                window,
                cx,
            );
        }
    }

    fn refresh_clicked(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh(window, cx);
    }
    fn submit_search(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        let query = query.trim().to_string();
        if query.is_empty() {
            self.clear_search(window, cx);
        } else {
            self.start_search(query, window, cx);
        }
    }

    fn invalidate_vcs(&mut self) {
        self.vcs_generation = self.vcs_generation.wrapping_add(1);
        self._vcs_task = Task::ready(());
    }

    fn refresh_vcs_statuses(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        self.vcs_generation = self.vcs_generation.wrapping_add(1);
        let generation = self.vcs_generation;
        let status_root = root.clone();
        self._vcs_task = cx.spawn(async move |this, cx| {
            let snapshot = cx
                .background_spawn(async move { load_vcs_snapshot(&status_root) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.vcs_generation != generation {
                    return;
                }
                this.table.update(cx, |state, cx| {
                    state.delegate_mut().apply_vcs_statuses(&root, &snapshot);
                    state.refresh(cx);
                    cx.notify();
                });
                cx.notify();
            });
        });
    }

    fn invalidate_columns(&mut self) {
        self.column_generation = self.column_generation.wrapping_add(1);
        self._column_task = Task::ready(());
    }

    fn load_columns(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        self.column_generation = self.column_generation.wrapping_add(1);
        let generation = self.column_generation;
        let mut chain = root.ancestors().map(Path::to_path_buf).collect::<Vec<_>>();
        chain.reverse();
        let sort = self.table.read(cx).delegate().sort_spec();
        self.column_panes = chain
            .iter()
            .map(|path| ColumnPane {
                path: path.clone(),
                entries: Vec::new(),
                selected_path: None,
                loading: true,
            })
            .collect();
        self.column_selected_path = None;
        self.column_scroll_pending = true;
        self.path_bar_scroll_pending = true;
        cx.notify();

        let load_paths = chain;
        self._column_task = cx.spawn(async move |this, cx| {
            let panes = cx
                .background_spawn(async move {
                    let mut panes = Vec::with_capacity(load_paths.len());
                    for path in load_paths {
                        let mut entries = load_directory(&path, sort)
                            .map(|snapshot| snapshot.entries)
                            .unwrap_or_default();
                        let snapshot = load_vcs_snapshot(&path);
                        apply_statuses(&mut entries, &path, &snapshot);
                        panes.push(ColumnPane {
                            path,
                            entries,
                            selected_path: None,
                            loading: false,
                        });
                    }
                    panes
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.column_generation != generation {
                    return;
                }
                let mut panes = panes;
                for index in 0..panes.len().saturating_sub(1) {
                    panes[index].selected_path = Some(panes[index + 1].path.clone());
                }
                this.column_panes = panes;
                this.column_scroll_pending = true;
                this.path_bar_scroll_pending = true;
                cx.notify();
            });
        });
    }

    fn show_list_view(&mut self, _: &ShowListView, _: &mut Window, cx: &mut Context<Self>) {
        if self.view_mode == ViewMode::List {
            return;
        }
        self.view_mode = ViewMode::List;
        self.column_selected_path = None;
        self.invalidate_columns();
        if self.active_search.is_none() {
            self.refresh_vcs_statuses(self.current_dir.clone(), cx);
        }
        cx.notify();
    }

    fn show_columns_view(&mut self, _: &ShowColumnsView, _: &mut Window, cx: &mut Context<Self>) {
        self.view_mode = ViewMode::Columns;
        self.column_selected_path = None;
        if self.active_search.is_none() {
            self.load_columns(self.current_dir.clone(), cx);
        }
        cx.notify();
    }

    fn toggle_path_bar(&mut self, _: &TogglePathBar, _: &mut Window, cx: &mut Context<Self>) {
        self.show_path_bar = !self.show_path_bar;
        self.path_bar_scroll_pending = self.show_path_bar;
        cx.notify();
    }
    fn toggle_view_clicked(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        match self.view_mode {
            ViewMode::Columns => self.show_list_view(&ShowListView, window, cx),
            ViewMode::List => self.show_columns_view(&ShowColumnsView, window, cx),
        }
    }

    fn start_search(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        let query = query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.cancel_search();
        self.invalidate_vcs();
        self.invalidate_columns();
        self.search_generation = self.search_generation.wrapping_add(1);
        self.load_generation = self.load_generation.wrapping_add(1);
        let generation = self.search_generation;
        let root = self.current_dir.clone();
        let sort = self.table.read(cx).delegate().sort_spec();
        let cancel = Arc::new(AtomicBool::new(false));
        self.search_cancel = Some(cancel.clone());
        self.active_search = Some(SearchState {
            query: query.clone(),
            root: root.clone(),
        });
        self.loading = true;
        self.error = None;
        self.notice = None;
        self.search_truncated = false;
        self.table.update(cx, |state, cx| {
            state.delegate_mut().clear_entries();
            state.delegate_mut().set_loading(true);
            state.clear_selection(cx);
            state.refresh(cx);
            cx.notify();
        });
        cx.notify();

        self._search_task = cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(
                    async move { search_directory(&root, &query, sort, cancel.as_ref()) },
                )
                .await;

            let _ = this.update_in(cx, |this, _, cx| {
                if this.search_generation != generation {
                    return;
                }
                this.loading = false;
                this.search_cancel = None;
                this.table.update(cx, |state, cx| {
                    state.delegate_mut().set_loading(false);
                    cx.notify();
                });

                match result {
                    Ok(Some(snapshot)) => {
                        this.active_search = Some(SearchState {
                            query: snapshot.query.clone(),
                            root: snapshot.root,
                        });
                        this.search_truncated = snapshot.truncated;
                        this.error = None;
                        this.table.update(cx, |state, cx| {
                            state
                                .delegate_mut()
                                .replace_entries(snapshot.entries, Some(snapshot.query));
                            state.clear_selection(cx);
                            state.refresh(cx);
                            cx.notify();
                        });
                        this.refresh_vcs_statuses(this.current_dir.clone(), cx);
                    }
                    Ok(None) => {
                        this.active_search = None;
                    }
                    Err(error) => {
                        this.active_search = None;
                        this.error = Some(error.into());
                        this.table.update(cx, |state, cx| {
                            state.delegate_mut().clear_entries();
                            state.refresh(cx);
                            cx.notify();
                        });
                    }
                }
                cx.notify();
            });
        });
    }

    fn cancel_search(&mut self) {
        if let Some(cancel) = self.search_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.search_generation = self.search_generation.wrapping_add(1);
    }

    fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_search.is_none() && !self.loading {
            return;
        }
        self.cancel_search();
        self.search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        self.active_search = None;
        self.search_truncated = false;
        self.request_load(
            self.current_dir.clone(),
            NavigationIntent::Refresh,
            window,
            cx,
        );
    }

    fn focus_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.address_input.update(cx, |input, cx| {
            input.select_all(window, cx);
            input.focus(window, cx);
        });
    }

    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input.update(cx, |input, cx| {
            input.select_all(window, cx);
            input.focus(window, cx);
        });
    }

    fn focus_table(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.table.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }

    fn on_table_event(
        &mut self,
        _: &Entity<TableState<FileTableDelegate>>,
        event: &TableEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TableEvent::DoubleClickedRow(index) => {
                if let Some(entry) = self.table.read(cx).delegate().entry(*index).cloned() {
                    self.open_entry(entry, window, cx);
                }
            }
            TableEvent::RightClickedRow(index) => {
                let index = *index;
                let table = self.table.clone();
                let app = cx.entity();
                window.defer(cx, move |_, cx| {
                    table.update(cx, |state, cx| match index {
                        Some(index) => state.set_selected_row(index, cx),
                        None => state.clear_selection(cx),
                    });
                    app.update(cx, |_, cx| cx.notify());
                });
            }
            TableEvent::SelectRow(_) | TableEvent::ClearSelection => cx.notify(),
            _ => {}
        }
    }

    fn open_entry(&mut self, entry: FileEntry, window: &mut Window, cx: &mut Context<Self>) {
        if entry.is_dir() {
            self.navigate_to(entry.path, NavigationIntent::Push, window, cx);
            return;
        }
        let path = entry.path;
        self.error = None;
        self.notice = Some("正在打开文件…".into());
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { open_file(&path) }).await;
            let _ = this.update_in(cx, |this, _, cx| {
                match result {
                    Ok(()) => this.notice = None,
                    Err(error) => {
                        this.notice = None;
                        this.error = Some(error.into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn selected_entry(&self, cx: &App) -> Option<FileEntry> {
        if self.view_mode == ViewMode::Columns && self.active_search.is_none() {
            let selected = self.column_selected_path.as_ref()?;
            return self
                .column_panes
                .iter()
                .flat_map(|pane| pane.entries.iter())
                .find(|entry| same_path(&entry.path, selected))
                .cloned();
        }

        let selected = self.table.read(cx).selected_row()?;
        self.table.read(cx).delegate().entry(selected).cloned()
    }

    fn open_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.selected_entry(cx) {
            self.open_entry(entry, window, cx);
        }
    }
    fn on_row_context_action(
        &mut self,
        action: &RowContextAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.table.read(cx).delegate().entry(action.row).cloned() else {
            self.error = Some("项目已不存在，列表已刷新".into());
            self.refresh(window, cx);
            return;
        };

        match action.command {
            RowContextCommand::Open => self.open_entry(entry, window, cx),
            RowContextCommand::Copy => self.set_clipboard_entry(entry, ClipboardMode::Copy, cx),
            RowContextCommand::Cut => self.set_clipboard_entry(entry, ClipboardMode::Cut, cx),
            RowContextCommand::Rename => self.begin_rename_entry(entry, window, cx),
            RowContextCommand::Recycle => self.confirm_recycle_entry(entry, window, cx),
        }
    }

    pub(crate) fn sync_clipboard_state(&mut self, cx: &mut Context<Self>) {
        let can_paste = self.clipboard.borrow().is_some();
        self.table.update(cx, |state, cx| {
            state.delegate_mut().set_can_paste(can_paste);
            cx.notify();
        });
    }

    fn select_column_entry(&mut self, column: usize, row: usize, cx: &mut Context<Self>) {
        let Some(entry) = self
            .column_panes
            .get(column)
            .and_then(|pane| pane.entries.get(row))
            .cloned()
        else {
            return;
        };
        if let Some(pane) = self.column_panes.get_mut(column) {
            pane.selected_path = Some(entry.path.clone());
        }
        self.column_selected_path = Some(entry.path);
        self.path_bar_scroll_pending = true;
        cx.notify();
    }

    fn activate_column_entry(
        &mut self,
        column: usize,
        row: usize,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self
            .column_panes
            .get(column)
            .and_then(|pane| pane.entries.get(row))
            .cloned()
        else {
            return;
        };
        if entry.is_dir() {
            self.column_selected_path = None;
            self.navigate_to(entry.path, NavigationIntent::Push, window, cx);
            return;
        }
        self.select_column_entry(column, row, cx);
        if event.click_count() == 2 {
            self.open_entry(entry, window, cx);
        }
    }

    fn set_clipboard(&mut self, mode: ClipboardMode, cx: &mut Context<Self>) {
        if let Some(entry) = self.selected_entry(cx) {
            self.set_clipboard_entry(entry, mode, cx);
        }
    }

    fn set_clipboard_entry(
        &mut self,
        entry: FileEntry,
        mode: ClipboardMode,
        cx: &mut Context<Self>,
    ) {
        *self.clipboard.borrow_mut() = Some(FileClipboard {
            path: entry.path,
            mode,
        });
        self.sync_clipboard_state(cx);
        self.error = None;
        self.notice = Some(if mode == ClipboardMode::Cut {
            "已剪切 1 项".into()
        } else {
            "已复制 1 项".into()
        });
        cx.notify();
    }

    fn copy_clicked(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_clipboard(ClipboardMode::Copy, cx);
    }

    fn cut_clicked(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_clipboard(ClipboardMode::Cut, cx);
    }

    fn paste_clicked(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.paste_clipboard(window, cx);
    }

    fn paste_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(clipboard) = self.clipboard.borrow().clone() else {
            return;
        };
        let source = clipboard.path;
        let destination_dir = self.current_dir.clone();
        let move_source = clipboard.mode == ClipboardMode::Cut;
        self.error = None;
        self.notice = Some(if move_source {
            "正在移动项目…".into()
        } else {
            "正在复制项目…".into()
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(
                    async move { transfer_entry(&source, &destination_dir, move_source) },
                )
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(_) => {
                    if move_source {
                        *this.clipboard.borrow_mut() = None;
                        this.sync_clipboard_state(cx);
                    }
                    this.notice = Some(if move_source {
                        "项目已移动".into()
                    } else {
                        "项目已粘贴".into()
                    });
                    this.refresh(window, cx);
                }
                Err(error) => {
                    this.notice = None;
                    this.error = Some(error.into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn begin_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("文件夹名称")
                .default_value("新建文件夹")
                .clean_on_escape()
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit_modal(window, cx);
                }
            },
        );
        self.modal = Some(ModalState {
            kind: ModalKind::NewFolder,
            input: input.clone(),
            _subscription: subscription,
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn begin_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.selected_entry(cx) {
            self.begin_rename_entry(entry, window, cx);
        }
    }

    fn begin_rename_entry(
        &mut self,
        entry: FileEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("新名称")
                .default_value(entry.name.clone())
                .clean_on_escape()
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit_modal(window, cx);
                }
            },
        );
        self.modal = Some(ModalState {
            kind: ModalKind::Rename(entry.path),
            input: input.clone(),
            _subscription: subscription,
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn begin_go_to_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("文件夹路径")
                .default_value(path_text(&self.current_dir))
                .clean_on_escape()
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit_modal(window, cx);
                }
            },
        );
        self.modal = Some(ModalState {
            kind: ModalKind::GoToFolder,
            input: input.clone(),
            _subscription: subscription,
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.modal = None;
        self.focus_table(window, cx);
        cx.notify();
    }

    fn submit_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(modal) = self.modal.as_ref() else {
            return;
        };
        let value = modal.input.read(cx).value().trim().to_string();
        let kind = modal.kind.clone();
        if !matches!(&kind, ModalKind::GoToFolder)
            && let Err(error) = validate_name(&value)
        {
            self.error = Some(error.into());
            cx.notify();
            return;
        }
        self.error = None;

        match kind {
            ModalKind::NewFolder => {
                let parent = self.current_dir.clone();
                let name = value;
                self.notice = Some("正在新建文件夹…".into());
                cx.spawn_in(window, async move |this, cx| {
                    let result = cx
                        .background_spawn(async move { create_folder(&parent, &name) })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(_) => {
                            this.notice = Some("文件夹已创建".into());
                            this.refresh(window, cx);
                        }
                        Err(error) => {
                            this.notice = None;
                            this.error = Some(error.into());
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            ModalKind::Rename(path) => {
                let name = value;
                self.notice = Some("正在重命名…".into());
                cx.spawn_in(window, async move |this, cx| {
                    let result = cx
                        .background_spawn(async move { rename_entry(&path, &name) })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(_) => {
                            this.notice = Some("名称已更新".into());
                            this.refresh(window, cx);
                        }
                        Err(error) => {
                            this.notice = None;
                            this.error = Some(error.into());
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            ModalKind::GoToFolder => match resolve_location_input(&value, &self.current_dir) {
                Ok(path) => self.navigate_to(path, NavigationIntent::Push, window, cx),
                Err(error) => {
                    self.error = Some(error.into());
                    cx.notify();
                }
            },
        }
    }

    fn confirm_recycle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.selected_entry(cx) {
            self.confirm_recycle_entry(entry, window, cx);
        }
    }

    fn confirm_recycle_entry(
        &mut self,
        entry: FileEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let message = format!("将“{}”移到回收站？", entry.name);
        let detail = path_text(&entry.path);
        let answers = [PromptButton::ok("移到回收站"), PromptButton::cancel("取消")];
        let answer = window.prompt(PromptLevel::Warning, &message, Some(&detail), &answers, cx);
        let path = entry.path;
        cx.spawn_in(window, async move |this, cx| {
            let Ok(choice) = answer.await else {
                return;
            };
            if choice != 0 {
                return;
            }
            let result = cx
                .background_spawn(async move { recycle_entry(&path) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(()) => {
                    this.notice = Some("已移到回收站".into());
                    this.refresh(window, cx);
                }
                Err(error) => {
                    this.notice = None;
                    this.error = Some(error.into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn clear_search_or_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_some() {
            self.close_modal(window, cx);
        } else if self.active_search.is_some() {
            self.clear_search(window, cx);
        }
    }

    fn toggle_theme(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let mode = if cx.theme().mode.is_dark() {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        Theme::change(mode, Some(window), cx);
    }
    fn sidebar_item(
        &self,
        label: SharedString,
        icon: IconName,
        path: PathBuf,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accent = cx.theme().accent;
        let active_fg = cx.theme().accent_foreground;
        let fg = cx.theme().sidebar_foreground;
        let muted = cx.theme().muted_foreground;
        let id = SharedString::from(format!("nav-{}", path.display()));

        div()
            .id(id)
            .h(px(32.))
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .text_sm()
            .text_color(if active { active_fg } else { fg })
            .when(active, |this| this.bg(accent))
            .when(!active, |this| {
                this.hover(move |style| style.bg(muted.opacity(0.12)))
            })
            .child(
                Icon::new(icon)
                    .size_4()
                    .text_color(if active { active_fg } else { muted }),
            )
            .child(div().min_w_0().truncate().child(label))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.navigate_to(path.clone(), NavigationIntent::Push, window, cx);
            }))
            .into_any_element()
    }

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let section_color = cx.theme().muted_foreground;
        let mut content = v_flex().gap_0p5();
        content = content.child(
            div()
                .px_2()
                .pt_1()
                .pb_1()
                .text_xs()
                .font_medium()
                .text_color(section_color)
                .child("快速访问"),
        );
        for link in self.quick_links.clone() {
            let active = same_path(&link.path, &self.current_dir);
            content =
                content.child(self.sidebar_item(link.label, link.icon, link.path, active, cx));
        }
        content = content.child(
            div()
                .px_2()
                .pt_4()
                .pb_1()
                .text_xs()
                .font_medium()
                .text_color(section_color)
                .child("此电脑"),
        );
        for drive in self.drives.clone() {
            let active = same_path(&drive.path, &self.current_dir);
            content =
                content.child(self.sidebar_item(drive.label, drive.icon, drive.path, active, cx));
        }

        v_flex()
            .w(px(208.))
            .h_full()
            .flex_shrink_0()
            .gap_1()
            .p_2()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(div().flex_1().min_h_0().child(content))
            .child(
                h_flex()
                    .h(px(30.))
                    .px_2()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .text_color(section_color)
                    .child(Icon::new(IconName::ShieldCheck).size_3p5())
                    .child("本地文件，不上传"),
            )
            .into_any_element()
    }
    fn render_path_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.selected_entry(cx);
        let (path, final_is_file) = selected
            .as_ref()
            .map(|entry| (entry.path.clone(), !entry.is_dir()))
            .unwrap_or_else(|| (self.current_dir.clone(), false));
        let segments = path_segments(&path, final_is_file);
        let scroll_handle = self.path_bar_scroll.clone();
        let mut bar = h_flex()
            .id("path-bar")
            .h(px(32.))
            .w_full()
            .px_3()
            .gap_1()
            .items_center()
            .overflow_x_scroll()
            .track_scroll(&scroll_handle)
            .bg(cx.theme().status_bar)
            .border_t_1()
            .border_color(cx.theme().status_bar_border);
        let count = segments.len();
        for (index, segment) in segments.into_iter().enumerate() {
            if segment.clickable {
                bar = bar.child(
                    Button::new(format!("path-segment-{index}"))
                        .ghost()
                        .compact()
                        .label(segment.label)
                        .tooltip(path_text(&segment.path))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigate_to(
                                segment.path.clone(),
                                NavigationIntent::Push,
                                window,
                                cx,
                            );
                        })),
                );
            } else {
                bar = bar.child(div().px_1().text_sm().font_medium().child(segment.label));
            }
            if index + 1 < count {
                bar = bar.child(
                    Icon::new(IconName::ChevronRight)
                        .size_3()
                        .text_color(cx.theme().muted_foreground),
                );
            }
        }
        bar.into_any_element()
    }

    fn render_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let scroll_handle = self.column_scroll.clone();
        let mut strip = h_flex()
            .id("column-strip")
            .size_full()
            .items_stretch()
            .overflow_x_scroll()
            .track_scroll(&scroll_handle);
        if self.column_panes.is_empty() {
            return strip
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("正在读取分栏…")
                .into_any_element();
        }
        for index in 0..self.column_panes.len() {
            strip = strip.child(self.render_column(index, window, cx));
        }
        strip.into_any_element()
    }

    fn render_column(
        &mut self,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(pane) = self.column_panes.get(index) else {
            return div().into_any_element();
        };
        let path = pane.path.clone();
        let loading = pane.loading;
        let count = pane.entries.len();
        let active = same_path(&path, &self.current_dir);
        let title = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().accent;
        let body = if loading {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(muted)
                .child("正在读取…")
                .into_any_element()
        } else if count == 0 {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(muted)
                .child("空文件夹")
                .into_any_element()
        } else {
            uniform_list(
                SharedString::from(format!("column-list-{index}")),
                count,
                cx.processor(move |this, range, window, cx| {
                    this.render_column_rows(index, range, window, cx)
                }),
            )
            .flex_1()
            .min_h_0()
            .w_full()
            .into_any_element()
        };
        v_flex()
            .id(("column", index))
            .w(px(250.))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(border)
            .child(
                h_flex()
                    .h(px(34.))
                    .w_full()
                    .px_2()
                    .gap_2()
                    .items_center()
                    .border_b_1()
                    .border_color(border)
                    .when(active, |this| this.bg(accent.opacity(0.5)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .font_medium()
                            .text_color(if active { cx.theme().foreground } else { muted })
                            .child(title),
                    ),
            )
            .child(body)
            .into_any_element()
    }

    fn render_column_rows(
        &mut self,
        column: usize,
        range: Range<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(pane) = self.column_panes.get(column) else {
            return Vec::new();
        };
        let entries = pane.entries.clone();
        let selected_path = pane.selected_path.clone();
        let can_paste = self.clipboard.borrow().is_some();
        let accent = cx.theme().accent;
        let hover = cx.theme().muted;
        let muted = cx.theme().muted_foreground;
        let mut rows = Vec::new();
        for row_ix in range {
            let Some(entry) = entries.get(row_ix).cloned() else {
                continue;
            };
            let selected = selected_path
                .as_ref()
                .is_some_and(|path| same_path(path, &entry.path));
            let menu_entry = entry.clone();
            let row = h_flex()
                .id(("column-row", column * 100_000 + row_ix))
                .h(px(30.))
                .w_full()
                .px_2()
                .gap_2()
                .items_center()
                .cursor_pointer()
                .text_sm()
                .when(selected, |this| this.bg(accent))
                .when(!selected, |this| {
                    this.hover(move |style| style.bg(hover.opacity(0.45)))
                })
                .child(
                    Icon::new(icon_for(&entry))
                        .size_4()
                        .text_color(icon_color(&entry, cx)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(entry.name.clone()),
                )
                .when_some(entry.git_status, |this, status| {
                    this.child(vcs_badge(IconName::GitBranch, status))
                })
                .when_some(entry.svn_status, |this, status| {
                    this.child(vcs_badge(IconName::Boxes, status))
                })
                .when(entry.is_dir(), |this| {
                    this.child(Icon::new(IconName::ChevronRight).size_3().text_color(muted))
                })
                .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    this.activate_column_entry(column, row_ix, event, window, cx)
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                        this.select_column_entry(column, row_ix, cx)
                    }),
                )
                .context_menu(move |menu, _, _| {
                    build_column_context_menu(menu, &menu_entry, can_paste)
                })
                .into_any_element();
            rows.push(row);
        }
        rows
    }

    fn render_modal(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(modal) = self.modal.as_ref() else {
            return div().into_any_element();
        };
        let (title, description, action_label) = match &modal.kind {
            ModalKind::NewFolder => ("新建文件夹", "输入文件夹名称。按 Enter 创建。", "创建"),
            ModalKind::Rename(path) => (
                "重命名",
                "输入新名称。文件内容不会改变。",
                if path.is_dir() {
                    "重命名文件夹"
                } else {
                    "重命名文件"
                },
            ),
            ModalKind::GoToFolder => (
                "前往文件夹",
                "输入路径。~ 表示个人文件夹。/ 表示当前磁盘根目录。",
                "前往",
            ),
        };
        let input = Input::new(&modal.input)
            .aria_label(title)
            .cleanable(true)
            .w_full();
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x000000a8))
            .child(
                v_flex()
                    .w(px(430.))
                    .max_w(relative(0.9))
                    .gap_4()
                    .p_5()
                    .rounded_lg()
                    .bg(cx.theme().popover)
                    .border_1()
                    .border_color(cx.theme().border)
                    .shadow_xl()
                    .child(
                        h_flex()
                            .w_full()
                            .items_start()
                            .justify_between()
                            .gap_4()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .gap_1()
                                    .child(div().text_lg().font_semibold().child(title))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(description),
                                    ),
                            )
                            .child(
                                Button::new("modal-close")
                                    .ghost()
                                    .compact()
                                    .icon(IconName::X)
                                    .tooltip("关闭")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.close_modal(window, cx)
                                    })),
                            ),
                    )
                    .child(input)
                    .child(
                        h_flex()
                            .w_full()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("modal-cancel")
                                    .label("取消")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.close_modal(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("modal-submit")
                                    .primary()
                                    .label(action_label)
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.submit_modal(window, cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
}

fn build_quick_links() -> Vec<QuickLink> {
    let mut links = Vec::new();
    push_quick_link(&mut links, "主文件夹", dirs::home_dir(), IconName::House);
    push_quick_link(&mut links, "桌面", dirs::desktop_dir(), IconName::Monitor);
    push_quick_link(&mut links, "文档", dirs::document_dir(), IconName::FileText);
    push_quick_link(&mut links, "下载", dirs::download_dir(), IconName::Download);
    push_quick_link(&mut links, "图片", dirs::picture_dir(), IconName::Image);
    links
}

fn push_quick_link(links: &mut Vec<QuickLink>, label: &str, path: Option<PathBuf>, icon: IconName) {
    let Some(path) = path else {
        return;
    };
    if !path.is_dir() || links.iter().any(|link| same_path(&link.path, &path)) {
        return;
    }
    links.push(QuickLink {
        label: label.into(),
        path,
        icon,
    });
}

#[cfg(target_os = "windows")]
fn build_drives() -> Vec<QuickLink> {
    let mask = unsafe { windows_sys::Win32::Storage::FileSystem::GetLogicalDrives() };
    let mut drives = Vec::new();
    for index in 0..26 {
        if mask & (1 << index) == 0 {
            continue;
        }
        let letter = (b'A' + index as u8) as char;
        let path = PathBuf::from(format!("{letter}:\\"));
        drives.push(QuickLink {
            label: format!("{letter}:").into(),
            path,
            icon: IconName::HardDrive,
        });
    }
    drives
}

#[cfg(not(target_os = "windows"))]
fn build_drives() -> Vec<QuickLink> {
    vec![QuickLink {
        label: "根目录".into(),
        path: PathBuf::from("/"),
        icon: IconName::HardDrive,
    }]
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}

fn same_path(left: &Path, right: &Path) -> bool {
    let normalize = |path: &Path| {
        let mut value = path.to_string_lossy().replace('/', "\\").to_lowercase();
        while value.len() > 3 && value.ends_with('\\') {
            value.pop();
        }
        value
    };
    normalize(left) == normalize(right)
}

fn resolve_location_input(value: &str, current_dir: &Path) -> Result<PathBuf, String> {
    let raw = value.trim().trim_matches('"');
    if raw.is_empty() {
        return Err("请输入文件夹路径".to_string());
    }
    if raw == "~" {
        return dirs::home_dir().ok_or_else(|| "无法确定个人文件夹".to_string());
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        let home = dirs::home_dir().ok_or_else(|| "无法确定个人文件夹".to_string())?;
        return Ok(home.join(rest));
    }

    let root = current_dir
        .ancestors()
        .last()
        .unwrap_or(current_dir)
        .to_path_buf();
    if raw == "/" {
        return Ok(root);
    }
    if raw.starts_with('/') && !raw.starts_with("//") {
        return Ok(root.join(raw.trim_start_matches('/')));
    }

    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(current_dir.join(path))
    }
}

fn build_column_context_menu(menu: PopupMenu, entry: &FileEntry, can_paste: bool) -> PopupMenu {
    let open_label = if entry.is_dir() {
        "打开文件夹"
    } else {
        "打开文件"
    };
    menu.label(entry.name.clone())
        .separator()
        .menu_with_icon_and_disabled(
            open_label,
            IconName::FolderOpen,
            Box::new(OpenSelection),
            false,
        )
        .separator()
        .menu_with_icon_and_disabled("复制", IconName::Copy, Box::new(CopySelection), false)
        .menu_with_icon_and_disabled("剪切", IconName::Scissors, Box::new(CutSelection), false)
        .menu_with_icon_and_disabled(
            "粘贴",
            IconName::ClipboardPaste,
            Box::new(PasteSelection),
            !can_paste,
        )
        .separator()
        .menu_with_icon_and_disabled(
            "新建文件夹",
            IconName::FolderPlus,
            Box::new(CreateFolder),
            false,
        )
        .menu_with_icon_and_disabled("重命名", IconName::Pencil, Box::new(RenameSelection), false)
        .menu_with_icon_and_disabled(
            "移到回收站",
            IconName::Trash,
            Box::new(RecycleSelection),
            false,
        )
        .separator()
        .menu_with_icon_and_disabled("刷新", IconName::RotateCw, Box::new(RefreshFolder), false)
}

fn path_segments(path: &Path, final_is_file: bool) -> Vec<PathSegment> {
    let mut ancestors = path.ancestors().map(Path::to_path_buf).collect::<Vec<_>>();
    ancestors.reverse();
    let count = ancestors.len();
    ancestors
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            let label = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| path.display().to_string());
            PathSegment {
                label: label.into(),
                path,
                clickable: !final_is_file || index + 1 < count,
            }
        })
        .collect()
}
impl Focusable for KiteApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<TabRequest> for KiteApp {}

impl Render for KiteApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = cx.theme().mode.is_dark();
        let can_back = self.history_index > 0;
        let can_forward = self.history_index + 1 < self.history.len();
        let can_up = self.current_dir.parent().is_some();
        let selected = self.selected_entry(cx);
        let selected_exists = selected.is_some();
        let has_clipboard = self.clipboard.borrow().is_some();
        let column_mode = self.view_mode == ViewMode::Columns && self.active_search.is_none();
        let item_count = if column_mode {
            self.column_panes
                .last()
                .map(|pane| pane.entries.len())
                .unwrap_or(0)
        } else {
            self.table.read(cx).delegate().entries().len()
        };
        let selected_size = selected
            .as_ref()
            .map(FileEntry::size_label)
            .unwrap_or_default();
        let error = self.error.clone();
        let notice = self.notice.clone();
        let loading = self.loading;
        let active_search = self.active_search.clone();
        let search_truncated = self.search_truncated;
        let skipped = self.skipped;
        let view_mode = self.view_mode;
        let show_path_bar = self.show_path_bar;

        if self.column_scroll_pending {
            self.column_scroll_pending = false;
            let handle = self.column_scroll.clone();
            window.defer(cx, move |_, _| {
                handle.set_offset(point(-handle.max_offset().x, handle.offset().y));
            });
        }
        if self.show_path_bar && self.path_bar_scroll_pending {
            self.path_bar_scroll_pending = false;
            let handle = self.path_bar_scroll.clone();
            window.defer(cx, move |_, _| {
                handle.set_offset(point(-handle.max_offset().x, handle.offset().y));
            });
        }

        let footer_left = if let Some(error) = error.clone() {
            error
        } else if loading {
            if let Some(search) = &active_search {
                SharedString::from(format!("正在搜索“{}”…", search.query))
            } else {
                SharedString::from("正在读取文件夹…")
            }
        } else if let Some(search) = &active_search {
            if search_truncated {
                SharedString::from(format!("{} 个匹配项目 · 仅显示前 1000 个结果", item_count))
            } else {
                SharedString::from(format!(
                    "{} 个匹配项目 · 搜索“{}”",
                    item_count, search.query
                ))
            }
        } else {
            SharedString::from(format!("{} 个项目", item_count))
        };
        let footer_right = if let Some(notice) = notice {
            notice
        } else if selected_exists {
            SharedString::from(if selected_size.is_empty() {
                "已选择 1 项".to_string()
            } else {
                format!("已选择 1 项 · {selected_size}")
            })
        } else if skipped > 0 {
            SharedString::from(format!("已跳过 {skipped} 个不可读取项目"))
        } else if column_mode {
            SharedString::from("Ctrl+1 列表 · Ctrl+2 分栏 · Alt+P 路径栏")
        } else {
            SharedString::from("Alt+← 返回 · Alt+↑ 上一级 · Ctrl+L 地址栏")
        };

        let search_banner = active_search.as_ref().map(|search| {
            h_flex()
                .h(px(34.))
                .px_3()
                .gap_2()
                .items_center()
                .bg(cx.theme().accent.opacity(0.35))
                .border_b_1()
                .border_color(cx.theme().border)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(Icon::new(IconName::Search).size_3p5())
                .child(format!(
                    "在 {} 中搜索“{}”",
                    search.root.display(),
                    search.query
                ))
                .child(
                    Button::new("clear-search-banner")
                        .ghost()
                        .compact()
                        .icon(IconName::X)
                        .tooltip("清除搜索 (Esc)")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.clear_search_or_modal(window, cx)
                        })),
                )
                .into_any_element()
        });

        let sidebar = self.render_sidebar(cx);
        let main_view = if column_mode {
            self.render_columns(window, cx)
        } else {
            DataTable::new(&self.table)
                .with_size(Size::Small)
                .bordered(false)
                .stripe(false)
                .scroll_bounce(false)
                .scrollbar_visible(false, true)
                .into_any_element()
        };
        let path_bar = if show_path_bar {
            Some(self.render_path_bar(cx))
        } else {
            None
        };

        v_flex()
            .id("kite-root")
            .key_context("Kite")
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(
                cx.listener(|this, _: &NavigateBack, window, cx| this.navigate_back(window, cx)),
            )
            .on_action(cx.listener(|this, _: &NavigateForward, window, cx| {
                this.navigate_forward(window, cx)
            }))
            .on_action(cx.listener(|this, _: &NavigateUp, window, cx| this.navigate_up(window, cx)))
            .on_action(cx.listener(|this, _: &RefreshFolder, window, cx| this.refresh(window, cx)))
            .on_action(
                cx.listener(|this, _: &OpenSelection, window, cx| this.open_selected(window, cx)),
            )
            .on_action(cx.listener(Self::on_row_context_action))
            .on_action(cx.listener(Self::show_list_view))
            .on_action(cx.listener(Self::show_columns_view))
            .on_action(cx.listener(Self::toggle_path_bar))
            .on_action(
                cx.listener(|this, _: &GoToFolder, window, cx| this.begin_go_to_folder(window, cx)),
            )
            .on_action(cx.listener(|this, _: &CopySelection, _window, cx| {
                this.set_clipboard(ClipboardMode::Copy, cx)
            }))
            .on_action(cx.listener(|this, _: &CutSelection, _window, cx| {
                this.set_clipboard(ClipboardMode::Cut, cx)
            }))
            .on_action(
                cx.listener(|this, _: &PasteSelection, window, cx| {
                    this.paste_clipboard(window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &FocusAddress, window, cx| this.focus_address(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &FocusSearch, window, cx| this.focus_search(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &CreateFolder, window, cx| this.begin_new_folder(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &RenameSelection, window, cx| this.begin_rename(window, cx)),
            )
            .on_action(cx.listener(|this, _: &RecycleSelection, window, cx| {
                this.confirm_recycle(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ClearSearchOverlay, window, cx| {
                this.clear_search_or_modal(window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewTab, _, cx| this.request_tab(TabRequest::New, cx)))
            .on_action(
                cx.listener(|this, _: &CloseTab, _, cx| this.request_tab(TabRequest::Close, cx)),
            )
            .on_action(
                cx.listener(|this, _: &NextTab, _, cx| this.request_tab(TabRequest::Next, cx)),
            )
            .on_action(cx.listener(|this, _: &PreviousTab, _, cx| {
                this.request_tab(TabRequest::Previous, cx)
            }))
            .child(
                h_flex()
                    .h(px(54.))
                    .w_full()
                    .flex_shrink_0()
                    .px_3()
                    .gap_2()
                    .items_center()
                    .bg(cx.theme().title_bar)
                    .border_b_1()
                    .border_color(cx.theme().title_bar_border)
                    .child(
                        h_flex()
                            .w(px(84.))
                            .gap_2()
                            .items_center()
                            .child(
                                Icon::new(IconName::Navigation)
                                    .size_5()
                                    .text_color(cx.theme().primary),
                            )
                            .child(div().text_lg().font_semibold().child("Kite")),
                    )
                    .child(
                        Button::new("nav-back")
                            .ghost()
                            .compact()
                            .icon(IconName::ChevronLeft)
                            .tooltip("返回 (Alt+←)")
                            .disabled(!can_back)
                            .on_click(cx.listener(Self::go_back)),
                    )
                    .child(
                        Button::new("nav-forward")
                            .ghost()
                            .compact()
                            .icon(IconName::ChevronRight)
                            .tooltip("前进 (Alt+→)")
                            .disabled(!can_forward)
                            .on_click(cx.listener(Self::go_forward)),
                    )
                    .child(
                        Button::new("nav-up")
                            .ghost()
                            .compact()
                            .icon(IconName::ArrowUp)
                            .tooltip("上一级 (Alt+↑)")
                            .disabled(!can_up)
                            .on_click(cx.listener(Self::go_up)),
                    )
                    .child(
                        Button::new("go-menu")
                            .ghost()
                            .compact()
                            .label("前往")
                            .dropdown_caret(true)
                            .dropdown_menu(move |menu, _, _| {
                                menu.menu_with_icon_and_disabled(
                                    "上层文件夹",
                                    IconName::ArrowUp,
                                    Box::new(NavigateUp),
                                    !can_up,
                                )
                                .separator()
                                .menu_with_icon_and_disabled(
                                    "前往文件夹…",
                                    IconName::FolderOpen,
                                    Box::new(GoToFolder),
                                    false,
                                )
                            }),
                    )
                    .child(
                        Input::new(&self.address_input)
                            .aria_label("地址")
                            .prefix(Icon::new(IconName::Folder).size_4())
                            .flex_1()
                            .min_w(px(160.)),
                    )
                    .child(
                        Input::new(&self.search_input)
                            .aria_label("搜索")
                            .prefix(Icon::new(IconName::Search).size_4())
                            .cleanable(true)
                            .w(px(180.)),
                    )
                    .child(
                        Button::new("refresh")
                            .ghost()
                            .compact()
                            .icon(IconName::RotateCw)
                            .tooltip("刷新 (Ctrl+R)")
                            .on_click(cx.listener(Self::refresh_clicked)),
                    )
                    .child(
                        Button::new("copy")
                            .ghost()
                            .compact()
                            .icon(IconName::Copy)
                            .tooltip("复制 (Ctrl+C)")
                            .disabled(!selected_exists)
                            .on_click(cx.listener(Self::copy_clicked)),
                    )
                    .child(
                        Button::new("cut")
                            .ghost()
                            .compact()
                            .icon(IconName::Scissors)
                            .tooltip("剪切 (Ctrl+X)")
                            .disabled(!selected_exists)
                            .on_click(cx.listener(Self::cut_clicked)),
                    )
                    .child(
                        Button::new("paste")
                            .ghost()
                            .compact()
                            .icon(IconName::ClipboardPaste)
                            .tooltip("粘贴 (Ctrl+V)")
                            .disabled(!has_clipboard)
                            .on_click(cx.listener(Self::paste_clicked)),
                    )
                    .child(
                        Button::new("new-folder")
                            .ghost()
                            .compact()
                            .icon(IconName::FolderPlus)
                            .tooltip("新建文件夹 (Ctrl+Shift+N)")
                            .disabled(self.loading)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.begin_new_folder(window, cx)
                            })),
                    )
                    .child(
                        Button::new("rename")
                            .ghost()
                            .compact()
                            .icon(IconName::Pencil)
                            .tooltip("重命名 (F2)")
                            .disabled(!selected_exists)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.begin_rename(window, cx)
                            })),
                    )
                    .child(
                        Button::new("recycle")
                            .ghost()
                            .compact()
                            .icon(IconName::Trash)
                            .tooltip("移到回收站 (Delete)")
                            .disabled(!selected_exists)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.confirm_recycle(window, cx)
                            })),
                    )
                    .child(
                        Button::new("toggle-view")
                            .ghost()
                            .compact()
                            .icon(if view_mode == ViewMode::Columns {
                                IconName::List
                            } else {
                                IconName::Columns3
                            })
                            .tooltip(if view_mode == ViewMode::Columns {
                                "切换到列表视图 (Ctrl+1)"
                            } else {
                                "切换到分栏视图 (Ctrl+2)"
                            })
                            .on_click(cx.listener(Self::toggle_view_clicked)),
                    )
                    .child(
                        Button::new("toggle-path-bar")
                            .ghost()
                            .compact()
                            .icon(IconName::PanelBottom)
                            .tooltip("显示或隐藏路径栏 (Alt+P)")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.toggle_path_bar(&TogglePathBar, window, cx);
                            })),
                    )
                    .child(
                        Button::new("theme")
                            .ghost()
                            .compact()
                            .icon(if is_dark {
                                IconName::Sun
                            } else {
                                IconName::Moon
                            })
                            .tooltip(if is_dark {
                                "切换到浅色主题"
                            } else {
                                "切换到深色主题"
                            })
                            .on_click(cx.listener(Self::toggle_theme)),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .items_stretch()
                    .child(sidebar)
                    .child(
                        v_flex()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .min_h_0()
                            .child(if loading {
                                div()
                                    .h(px(2.))
                                    .w_full()
                                    .bg(cx.theme().primary)
                                    .into_any_element()
                            } else {
                                div()
                                    .h(px(2.))
                                    .w_full()
                                    .bg(cx.theme().background)
                                    .into_any_element()
                            })
                            .when_some(search_banner, |this, banner| this.child(banner))
                            .child(div().flex_1().min_h_0().w_full().child(main_view)),
                    ),
            )
            .when_some(path_bar, |this, path_bar| this.child(path_bar))
            .child(
                h_flex()
                    .h(px(28.))
                    .w_full()
                    .flex_shrink_0()
                    .px_3()
                    .gap_3()
                    .items_center()
                    .justify_between()
                    .bg(cx.theme().status_bar)
                    .border_t_1()
                    .border_color(cx.theme().status_bar_border)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .when(error.is_some(), |this| this.text_color(cx.theme().danger))
                            .child(footer_left),
                    )
                    .child(div().max_w(px(520.)).truncate().child(footer_right)),
            )
            .child(self.render_modal(cx))
    }
}

#[cfg(test)]
mod clipboard_tests {
    use super::{ClipboardMode, FileClipboard, SharedClipboard};
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;

    #[test]
    fn shared_clipboard_changes_are_visible_to_every_handle() {
        let source: SharedClipboard = Rc::new(RefCell::new(None));
        let destination = source.clone();

        *source.borrow_mut() = Some(FileClipboard {
            path: PathBuf::from(r"C:\example.txt"),
            mode: ClipboardMode::Copy,
        });

        assert!(destination.borrow().is_some());
        *destination.borrow_mut() = None;
        assert!(source.borrow().is_none());
    }
}
