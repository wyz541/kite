use std::cell::RefCell;
use std::io::Read as _;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::actions::*;
use crate::context_menu::{
    ContextMenuCapabilities, ContextMenuTarget, render_context_menu, show_system_menu,
};
use crate::devices::{DeviceSnapshot, PortableDevice, StorageVolume, VolumeKind};
use crate::fs::{
    FileEntry, SortSpec, create_folder, format_size, load_directory, open_file, recycle_entry,
    rename_entry, search_directory, transfer_entry, validate_name, worse_status,
};
use crate::platform::{default_root_path, root_directory};
use crate::table::{FileTableDelegate, icon_color, icon_for, vcs_icon_badge};
use crate::vcs::{
    VcsOperation, VcsQuery, VcsTool, apply_statuses, load_vcs_snapshot, open_vcs_log_window,
    run_vcs_operation, run_vcs_query,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::SelectableText;
use gpui_kit::component::button::{Button, ButtonVariants as _, DropdownButton};
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};
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
    detail: Option<SharedString>,
}

#[derive(Clone)]
struct SearchState {
    query: String,
    root: PathBuf,
}

#[derive(Clone)]
enum ModalKind {
    NewFolder(PathBuf),
    Rename(PathBuf),

    GitCommit(PathBuf),
    SvnCommit(PathBuf),
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
    skipped: usize,
    git_available: bool,
    svn_available: bool,
}

#[derive(Clone)]
struct ColumnListScroll {
    path: PathBuf,
    handle: UniformListScrollHandle,
}

#[derive(Clone)]
struct ColumnPreviewState {
    path: PathBuf,
    text: Option<SharedString>,
    error: Option<SharedString>,
    loading: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct DirectoryScrollRoute {
    directory_offset: f32,
    column_delta: f32,
}

pub struct KiteApp {
    current_dir: PathBuf,
    history: Vec<PathBuf>,
    history_index: usize,
    quick_links: Vec<QuickLink>,
    connected_devices: Vec<QuickLink>,
    drives: Vec<QuickLink>,
    portable_devices: Vec<PortableDevice>,
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
    column_vertical_scrolls: Vec<ColumnListScroll>,
    column_scroll_pending: bool,
    column_preview: Option<ColumnPreviewState>,
    preview_generation: u64,
    _preview_task: Task<()>,
    path_bar_scroll: ScrollHandle,
    path_bar_scroll_pending: bool,
    column_generation: u64,
    _column_task: Task<()>,
    _device_task: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl KiteApp {
    pub(crate) fn launch_directory_override() -> Option<PathBuf> {
        std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
    }

    pub(crate) fn default_directory() -> PathBuf {
        Self::launch_directory_override()
            .or_else(dirs::home_dir)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(default_root_path)
    }

    pub(crate) fn new_at(
        home: PathBuf,
        clipboard: SharedClipboard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_at_with_state(home, ViewMode::Columns, true, clipboard, window, cx)
    }

    pub(crate) fn new_restored(
        state: &crate::session::SessionTabDto,
        clipboard: SharedClipboard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let restored_path = PathBuf::from(&state.current_directory);
        let path = if restored_path.is_dir() {
            restored_path
        } else {
            Self::default_directory()
        };
        let view_mode = if state.view_mode == "list" {
            ViewMode::List
        } else {
            ViewMode::Columns
        };
        Self::new_at_with_state(path, view_mode, state.show_path_bar, clipboard, window, cx)
    }

    pub(crate) fn session_tab(&self) -> crate::session::SessionTabDto {
        crate::session::SessionTabDto {
            current_directory: self.current_dir.to_string_lossy().into_owned(),
            view_mode: match self.view_mode {
                ViewMode::Columns => "columns".to_string(),
                ViewMode::List => "list".to_string(),
            },
            show_path_bar: self.show_path_bar,
        }
    }

    fn new_at_with_state(
        home: PathBuf,
        view_mode: ViewMode,
        show_path_bar: bool,
        clipboard: SharedClipboard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let quick_links = build_quick_links();
        let snapshot = crate::devices::snapshot();
        let (connected_devices, drives) = volume_quick_links(&snapshot.volumes);
        let portable_devices = snapshot.portable_devices;
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
                FileTableDelegate::new(Vec::new(), SortSpec::default(), home.clone()),
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
            connected_devices,
            drives,
            portable_devices,
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
            view_mode,
            show_path_bar,
            column_panes: Vec::new(),
            column_selected_path: None,
            column_scroll: ScrollHandle::new(),
            column_vertical_scrolls: Vec::new(),
            column_scroll_pending: false,
            column_preview: None,
            preview_generation: 0,
            _preview_task: Task::ready(()),
            path_bar_scroll: ScrollHandle::new(),
            path_bar_scroll_pending: false,
            column_generation: 0,
            _column_task: Task::ready(()),
            _device_task: Task::ready(()),
            _subscriptions: vec![
                address_subscription,
                search_subscription,
                table_subscription,
            ],
        };
        app.request_load(home, NavigationIntent::Initial, window, cx);
        app.start_device_watcher(cx);
        app
    }

    pub(crate) fn current_dir(&self) -> &Path {
        &self.current_dir
    }

    fn start_device_watcher(&mut self, cx: &mut Context<Self>) {
        self._device_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                let snapshot = cx
                    .background_spawn(async { crate::devices::snapshot() })
                    .await;
                if this
                    .update(cx, |this, cx| this.apply_device_snapshot(snapshot, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
    }

    fn apply_device_snapshot(&mut self, snapshot: DeviceSnapshot, cx: &mut Context<Self>) {
        let (connected_devices, drives) = volume_quick_links(&snapshot.volumes);
        let volumes_changed = !same_quick_links(&self.connected_devices, &connected_devices)
            || !same_quick_links(&self.drives, &drives);
        let portable_changed = self.portable_devices != snapshot.portable_devices;

        if volumes_changed {
            self.connected_devices = connected_devices;
            self.drives = drives;
        }
        if portable_changed {
            self.portable_devices = snapshot.portable_devices;
        }
        if volumes_changed || portable_changed {
            cx.notify();
        }
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
            Menu::new("导航").items([MenuItem::action("聚焦路径栏", FocusAddress)]),
            Menu::new("显示").items([
                MenuItem::action("列表视图", ShowListView),
                MenuItem::action("分栏视图", ShowColumnsView),
                MenuItem::separator(),
                MenuItem::action("显示或隐藏路径栏", TogglePathBar),
            ]),
            Menu::new("Kite").items([
                MenuItem::action("设为默认资源管理器…", SetDefaultExplorer),
                MenuItem::action("恢复之前的文件夹打开方式…", RestoreDefaultExplorer),
                MenuItem::separator(),
                MenuItem::action("创建开始菜单快捷方式…", CreateStartMenuShortcut),
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
        self.clear_column_preview();
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
                        let column_entries =
                            (this.view_mode == ViewMode::Columns).then(|| snapshot.entries.clone());
                        let column_skipped = snapshot.skipped;
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
                        let table_directory = snapshot.path.clone();
                        this.table.update(cx, |state, cx| {
                            state.delegate_mut().set_working_directory(table_directory);
                            state.delegate_mut().replace_entries(snapshot.entries, None);
                            state.clear_selection(cx);
                            state.refresh(cx);
                            cx.notify();
                        });
                        if this.view_mode == ViewMode::Columns {
                            if let Some(entries) = column_entries {
                                this.update_columns_for_loaded_path(
                                    this.current_dir.clone(),
                                    entries,
                                    column_skipped,
                                    cx,
                                );
                            }
                            this.refresh_vcs_statuses(this.current_dir.clone(), cx);
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

    fn navigate_from_path_bar(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let loaded_column = if self.view_mode == ViewMode::Columns && self.active_search.is_none() {
            loaded_column_index(&self.column_panes, &path)
        } else {
            None
        };

        if let Some(column) = loaded_column {
            self.navigate_to_loaded_column(column, window, cx);
        } else {
            self.navigate_to(path, NavigationIntent::Push, window, cx);
        }
    }

    fn navigate_to_loaded_column(
        &mut self,
        column: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_column_preview();
        let Some((entries, skipped)) = self
            .column_panes
            .get(column)
            .map(|pane| (pane.entries.clone(), pane.skipped))
        else {
            return;
        };
        let path = self.column_panes[column].path.clone();

        self.cancel_search();
        self.invalidate_vcs();
        self._load_task = Task::ready(());
        self.load_generation = self.load_generation.wrapping_add(1);
        self.loading = false;
        self.error = None;
        self.notice = None;
        self.active_search = None;
        self.search_truncated = false;
        self.skipped = skipped;
        self.apply_history(path.clone(), NavigationIntent::Push);
        let table_directory = path.clone();
        self.current_dir = path;

        self.address_input.update(cx, |input, cx| {
            input.set_value(path_text(&self.current_dir), window, cx);
        });
        self.search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        self.table.update(cx, |state, cx| {
            state.delegate_mut().set_loading(false);
            state.delegate_mut().set_working_directory(table_directory);
            state.delegate_mut().replace_entries(entries, None);
            state.clear_selection(cx);
            state.refresh(cx);
            cx.notify();
        });

        self.column_panes.truncate(column + 1);
        for index in 0..self.column_panes.len().saturating_sub(1) {
            self.column_panes[index].selected_path =
                Some(self.column_panes[index + 1].path.clone());
        }
        if let Some(pane) = self.column_panes.last_mut() {
            pane.selected_path = None;
        }
        self.column_selected_path = None;
        self.column_scroll_pending = true;
        self.path_bar_scroll_pending = true;
        cx.notify();
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
                if let Some(pane) = this
                    .column_panes
                    .iter_mut()
                    .find(|pane| same_path(&pane.path, &root))
                {
                    apply_statuses(&mut pane.entries, &root, &snapshot);
                    pane.git_available = snapshot.git_available();
                    pane.svn_available = snapshot.svn_available();
                }
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
        let chain = column_chain(&root);
        let sort = self.table.read(cx).delegate().sort_spec();
        self.column_panes = chain
            .iter()
            .map(|path| ColumnPane {
                path: path.clone(),
                entries: Vec::new(),
                selected_path: None,
                loading: true,
                skipped: 0,
                git_available: false,
                svn_available: false,
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
                        panes.push(load_column_pane(path, sort));
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

    fn update_columns_for_loaded_path(
        &mut self,
        root: PathBuf,
        entries: Vec<FileEntry>,
        skipped: usize,
        cx: &mut Context<Self>,
    ) {
        self.column_generation = self.column_generation.wrapping_add(1);
        let generation = self.column_generation;
        let chain = column_chain(&root);
        let common = common_column_prefix(&self.column_panes, &chain);
        let inherited = self.column_panes.get(common.saturating_sub(1));
        let git_available = inherited.is_some_and(|pane| pane.git_available);
        let svn_available = inherited.is_some_and(|pane| pane.svn_available);
        let final_index = chain.len() - 1;
        let mut entries = Some(entries);
        let mut panes = self.column_panes[..common].to_vec();

        for (index, path) in chain.iter().enumerate().skip(common) {
            let ready_entries = (index == final_index).then(|| entries.take().unwrap_or_default());
            panes.push(ColumnPane {
                path: path.clone(),
                entries: ready_entries.unwrap_or_default(),
                selected_path: None,
                loading: index != final_index,
                skipped: if index == final_index { skipped } else { 0 },
                git_available,
                svn_available,
            });
        }
        if let Some(entries) = entries
            && let Some(last) = panes.get_mut(final_index)
        {
            last.entries = entries;
            last.skipped = skipped;
            last.loading = false;
        }

        self.column_panes = panes;
        for index in 0..self.column_panes.len().saturating_sub(1) {
            self.column_panes[index].selected_path =
                Some(self.column_panes[index + 1].path.clone());
        }
        if let Some(last) = self.column_panes.last_mut() {
            last.selected_path = None;
        }
        self.column_selected_path = None;
        self.column_scroll_pending = true;
        self.path_bar_scroll_pending = true;

        let load_paths = (common..final_index)
            .map(|index| (index, chain[index].clone()))
            .collect::<Vec<_>>();
        if load_paths.is_empty() {
            self._column_task = Task::ready(());
        } else {
            let sort = self.table.read(cx).delegate().sort_spec();
            self._column_task = cx.spawn(async move |this, cx| {
                let loaded = cx
                    .background_spawn(async move {
                        load_paths
                            .into_iter()
                            .map(|(index, path)| (index, load_column_pane(path, sort)))
                            .collect::<Vec<_>>()
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.column_generation != generation {
                        return;
                    }
                    for (index, pane) in loaded {
                        if let Some(slot) = this.column_panes.get_mut(index) {
                            *slot = pane;
                        }
                    }
                    for index in 0..this.column_panes.len().saturating_sub(1) {
                        this.column_panes[index].selected_path =
                            Some(this.column_panes[index + 1].path.clone());
                    }
                    if let Some(last) = this.column_panes.last_mut() {
                        last.selected_path = None;
                    }
                    cx.notify();
                });
            });
        }
        cx.notify();
    }

    fn show_list_view(&mut self, _: &ShowListView, _: &mut Window, cx: &mut Context<Self>) {
        self.clear_column_preview();
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
        self.open_path(entry.path, window, cx);
    }

    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if path.is_dir() {
            self.navigate_to(path, NavigationIntent::Push, window, cx);
            return;
        }
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

    fn on_context_menu_action(
        &mut self,
        action: &ContextMenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action.command {
            ContextMenuCommand::Open => self.open_path(action.target.clone(), window, cx),
            ContextMenuCommand::Copy => {
                self.set_clipboard_path(action.target.clone(), ClipboardMode::Copy, cx)
            }
            ContextMenuCommand::Cut => {
                self.set_clipboard_path(action.target.clone(), ClipboardMode::Cut, cx)
            }
            ContextMenuCommand::Paste => {
                self.paste_clipboard_into(action.working_directory.clone(), window, cx)
            }
            ContextMenuCommand::NewFolder => {
                self.begin_new_folder_in(action.working_directory.clone(), window, cx)
            }
            ContextMenuCommand::Rename => self.begin_rename_path(action.target.clone(), window, cx),
            ContextMenuCommand::Recycle => {
                self.confirm_recycle_path(action.target.clone(), window, cx)
            }
            ContextMenuCommand::Vcs(command) => {
                self.run_vcs_command(command, action.target.clone(), window, cx)
            }
            ContextMenuCommand::SystemMenu => {
                self.open_system_menu(action.target.clone(), window, cx)
            }
            ContextMenuCommand::Refresh => self.refresh(window, cx),
        }
    }

    fn open_system_menu(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match show_system_menu(window, &path) {
            Ok(true) => self.refresh(window, cx),
            Ok(false) => {}
            Err(error) => {
                self.notice = None;
                self.error = Some(error.into());
                cx.notify();
            }
        }
    }

    fn context_target_missing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = Some("项目已不存在，列表已刷新".into());
        self.refresh(window, cx);
    }

    fn set_default_explorer(
        &mut self,
        _: &SetDefaultExplorer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let message = "将 Kite 设为默认资源管理器？";
        let detail = "Kite 将接管文件夹打开命令。\nKite 将创建开始菜单入口。\nWindows 桌面、任务栏和系统文件对话框仍由 Windows 管理。";
        let answers = [PromptButton::ok("设为默认"), PromptButton::cancel("取消")];
        let answer = window.prompt(PromptLevel::Warning, message, Some(detail), &answers, cx);
        cx.spawn_in(window, async move |this, cx| {
            let Ok(choice) = answer.await else {
                return;
            };
            if choice != 0 {
                return;
            }
            let result = cx
                .background_spawn(async {
                    crate::explorer_integration::set_as_default_file_manager()
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                match result {
                    Ok(message) => {
                        this.error = None;
                        this.notice = Some(message.into());
                    }
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

    fn restore_default_explorer(
        &mut self,
        _: &RestoreDefaultExplorer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let message = "恢复之前的文件夹打开方式？";
        let detail = "Kite 将恢复最近保存的文件夹打开命令。";
        let answers = [PromptButton::ok("恢复"), PromptButton::cancel("取消")];
        let answer = window.prompt(PromptLevel::Warning, message, Some(detail), &answers, cx);
        cx.spawn_in(window, async move |this, cx| {
            let Ok(choice) = answer.await else {
                return;
            };
            if choice != 0 {
                return;
            }
            let result = cx
                .background_spawn(async {
                    crate::explorer_integration::restore_previous_file_manager()
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                match result {
                    Ok(message) => {
                        this.error = None;
                        this.notice = Some(message.into());
                    }
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

    fn create_start_menu_shortcut(
        &mut self,
        _: &CreateStartMenuShortcut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.error = None;
        self.notice = Some("正在创建开始菜单快捷方式…".into());
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async {
                    crate::explorer_integration::register_start_menu_shortcut()
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                match result {
                    Ok(message) => {
                        this.error = None;
                        this.notice = Some(message.into());
                    }
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

    fn run_vcs_command(
        &mut self,
        command: VcsMenuCommand,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            VcsMenuCommand::GitAdd => {
                self.run_vcs_task(VcsTool::Git, VcsOperation::Add, path, window, cx)
            }
            VcsMenuCommand::GitUnstage => {
                self.run_vcs_task(VcsTool::Git, VcsOperation::Unstage, path, window, cx)
            }
            VcsMenuCommand::GitCommit => self.begin_vcs_commit(VcsTool::Git, path, window, cx),
            VcsMenuCommand::GitPull => {
                self.run_vcs_task(VcsTool::Git, VcsOperation::Pull, path, window, cx)
            }
            VcsMenuCommand::GitPush => {
                self.run_vcs_task(VcsTool::Git, VcsOperation::Push, path, window, cx)
            }
            VcsMenuCommand::GitFetch => {
                self.run_vcs_task(VcsTool::Git, VcsOperation::Fetch, path, window, cx)
            }
            VcsMenuCommand::GitDiff => {
                self.run_vcs_query_task(VcsTool::Git, VcsQuery::Diff, path, window, cx)
            }
            VcsMenuCommand::GitLog => self.open_vcs_log_window_task(VcsTool::Git, path, window, cx),
            VcsMenuCommand::GitRevert => self.confirm_vcs_revert(VcsTool::Git, path, window, cx),
            VcsMenuCommand::GitClean => self.confirm_git_clean(path, window, cx),
            VcsMenuCommand::SvnUpdate => {
                self.run_vcs_task(VcsTool::Svn, VcsOperation::Update, path, window, cx)
            }
            VcsMenuCommand::SvnAdd => {
                self.run_vcs_task(VcsTool::Svn, VcsOperation::Add, path, window, cx)
            }
            VcsMenuCommand::SvnCommit => self.begin_vcs_commit(VcsTool::Svn, path, window, cx),
            VcsMenuCommand::SvnLock => {
                self.run_vcs_task(VcsTool::Svn, VcsOperation::Lock, path, window, cx)
            }
            VcsMenuCommand::SvnUnlock => {
                self.run_vcs_task(VcsTool::Svn, VcsOperation::Unlock, path, window, cx)
            }
            VcsMenuCommand::SvnDiff => {
                self.run_vcs_query_task(VcsTool::Svn, VcsQuery::Diff, path, window, cx)
            }
            VcsMenuCommand::SvnLog => self.open_vcs_log_window_task(VcsTool::Svn, path, window, cx),
            VcsMenuCommand::SvnRevert => self.confirm_vcs_revert(VcsTool::Svn, path, window, cx),
            VcsMenuCommand::SvnCleanup => {
                self.run_vcs_task(VcsTool::Svn, VcsOperation::Cleanup, path, window, cx)
            }
        }
    }

    fn run_vcs_task(
        &mut self,
        tool: VcsTool,
        operation: VcsOperation,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pending = match &operation {
            VcsOperation::Add => format!("正在{}添加…", tool.label()),
            VcsOperation::Unstage => format!("正在{}取消暂存…", tool.label()),
            VcsOperation::Commit(_) => format!("正在{}提交…", tool.label()),
            VcsOperation::Pull => format!("正在{}拉取…", tool.label()),
            VcsOperation::Push => format!("正在{}推送…", tool.label()),
            VcsOperation::Fetch => format!("正在{}获取…", tool.label()),
            VcsOperation::Revert => format!("正在{}还原…", tool.label()),
            VcsOperation::Clean => format!("正在{}清理…", tool.label()),
            VcsOperation::Update => format!("正在{}更新…", tool.label()),
            VcsOperation::Lock => format!("正在{}锁定…", tool.label()),
            VcsOperation::Unlock => format!("正在{}解锁…", tool.label()),
            VcsOperation::Cleanup => format!("正在{}清理…", tool.label()),
        };
        self.error = None;
        self.notice = Some(pending.into());
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { run_vcs_operation(tool, operation, &path) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(message) => {
                    this.notice = Some(message.into());
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

    fn run_vcs_query_task(
        &mut self,
        tool: VcsTool,
        query: VcsQuery,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = format!("{} {}", tool.label(), query.label());
        self.error = None;
        self.notice = Some(format!("正在读取{title}…").into());
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { open_vcs_query_text(tool, query, &path) })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                match result {
                    Ok(message) => this.notice = Some(message.into()),
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

    fn open_vcs_log_window_task(
        &mut self,
        tool: VcsTool,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.error = None;
        self.notice = Some(format!("正在打开{}日志窗口…", tool.label()).into());
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    open_vcs_log_window(tool, &path).or_else(|window_error| {
                        open_vcs_query_text(tool, VcsQuery::Log, &path).map_err(|text_error| {
                            format!("{window_error}；文本回退失败：{text_error}")
                        })
                    })
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                match result {
                    Ok(message) => this.notice = Some(message.into()),
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

    fn begin_vcs_commit(
        &mut self,
        tool: VcsTool,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("提交说明")
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
        let kind = match tool {
            VcsTool::Git => ModalKind::GitCommit(path),
            VcsTool::Svn => ModalKind::SvnCommit(path),
        };
        self.modal = Some(ModalState {
            kind,
            input: input.clone(),
            _subscription: subscription,
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn confirm_vcs_revert(
        &mut self,
        tool: VcsTool,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path_text(&path));
        let message = format!("还原“{name}”？{} 将丢弃未提交更改。", tool.label());
        let detail = path_text(&path);
        let answers = [PromptButton::ok("还原"), PromptButton::cancel("取消")];
        let answer = window.prompt(PromptLevel::Warning, &message, Some(&detail), &answers, cx);
        cx.spawn_in(window, async move |this, cx| {
            let Ok(choice) = answer.await else {
                return;
            };
            if choice != 0 {
                return;
            }
            let result = cx
                .background_spawn(
                    async move { run_vcs_operation(tool, VcsOperation::Revert, &path) },
                )
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(message) => {
                    this.notice = Some(message.into());
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

    fn confirm_git_clean(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let message = "清理所选路径的未跟踪文件和文件夹？";
        let detail = format!("{}\n此操作不可撤销。忽略文件不受影响。", path_text(&path));
        let answers = [PromptButton::ok("清理"), PromptButton::cancel("取消")];
        let answer = window.prompt(PromptLevel::Warning, message, Some(&detail), &answers, cx);
        cx.spawn_in(window, async move |this, cx| {
            let Ok(choice) = answer.await else {
                return;
            };
            if choice != 0 {
                return;
            }
            let result = cx
                .background_spawn(async move {
                    run_vcs_operation(VcsTool::Git, VcsOperation::Clean, &path)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(message) => {
                    this.notice = Some(message.into());
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
        self.column_selected_path = Some(entry.path.clone());
        if column + 1 == self.column_panes.len() && !entry.is_dir() {
            self.start_column_preview(entry, cx);
            self.column_scroll_pending = true;
        } else {
            self.clear_column_preview();
        }
        self.path_bar_scroll_pending = true;
        cx.notify();
    }

    fn clear_column_preview(&mut self) {
        self.preview_generation = self.preview_generation.wrapping_add(1);
        self._preview_task = Task::ready(());
        self.column_preview = None;
    }

    fn start_column_preview(&mut self, entry: FileEntry, cx: &mut Context<Self>) {
        self.preview_generation = self.preview_generation.wrapping_add(1);
        let generation = self.preview_generation;
        let path = entry.path.clone();
        let load_text = !is_image_preview_entry(&entry) && is_text_preview_entry(&entry);
        self._preview_task = Task::ready(());
        self.column_preview = Some(ColumnPreviewState {
            path: path.clone(),
            text: None,
            error: None,
            loading: load_text,
        });
        if !load_text {
            return;
        }

        self._preview_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { read_text_preview(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.preview_generation != generation {
                    return;
                }
                if let Some(preview) = this.column_preview.as_mut() {
                    preview.loading = false;
                    match result {
                        Ok(text) => preview.text = Some(text.into()),
                        Err(error) => preview.error = Some(error.into()),
                    }
                }
                cx.notify();
            });
        });
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
        self.set_clipboard_path(entry.path, mode, cx);
    }

    fn set_clipboard_path(&mut self, path: PathBuf, mode: ClipboardMode, cx: &mut Context<Self>) {
        *self.clipboard.borrow_mut() = Some(FileClipboard { path, mode });
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
        self.paste_clipboard_into(self.current_dir.clone(), window, cx);
    }

    fn paste_clipboard_into(
        &mut self,
        destination_dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(clipboard) = self.clipboard.borrow().clone() else {
            return;
        };
        let source = clipboard.path;
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
        self.begin_new_folder_in(self.current_dir.clone(), window, cx);
    }

    fn begin_new_folder_in(
        &mut self,
        directory: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            kind: ModalKind::NewFolder(directory),
            input: input.clone(),
            _subscription: subscription,
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn begin_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.selected_entry(cx) {
            self.begin_rename_path(entry.path, window, cx);
        }
    }

    fn begin_rename_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            self.error = Some("无法重命名驱动器根目录".into());
            cx.notify();
            return;
        };
        if !path.exists() {
            self.context_target_missing(window, cx);
            return;
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("新名称")
                .default_value(name)
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
            kind: ModalKind::Rename(path),
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
        let validation = match &kind {
            ModalKind::NewFolder(_) | ModalKind::Rename(_) => validate_name(&value),
            ModalKind::GitCommit(_) | ModalKind::SvnCommit(_) if value.is_empty() => {
                Err("请输入提交说明".to_string())
            }
            ModalKind::GitCommit(_) | ModalKind::SvnCommit(_) => Ok(()),
        };
        if let Err(error) = validation {
            self.error = Some(error.into());
            cx.notify();
            return;
        }
        self.error = None;

        match kind {
            ModalKind::NewFolder(parent) => {
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
                let rename_source = path.clone();
                let renamed_path = path
                    .parent()
                    .map(|parent| parent.join(&name))
                    .unwrap_or_else(|| path.clone());
                self.notice = Some("正在重命名…".into());
                cx.spawn_in(window, async move |this, cx| {
                    let result = cx
                        .background_spawn(async move { rename_entry(&rename_source, &name) })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(_) => {
                            this.notice = Some("名称已更新".into());
                            let destination =
                                path_after_rename(&this.current_dir, &path, &renamed_path);
                            this.navigate_to(destination, NavigationIntent::Refresh, window, cx);
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
            ModalKind::GitCommit(path) => {
                self.modal = None;
                self.run_vcs_task(VcsTool::Git, VcsOperation::Commit(value), path, window, cx);
            }
            ModalKind::SvnCommit(path) => {
                self.modal = None;
                self.run_vcs_task(VcsTool::Svn, VcsOperation::Commit(value), path, window, cx);
            }
        }
    }

    fn confirm_recycle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.selected_entry(cx) {
            self.confirm_recycle_path(entry.path, window, cx);
        }
    }

    fn confirm_recycle_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path_text(&path));
        if !path.exists() {
            self.context_target_missing(window, cx);
            return;
        }
        let message = format!("将“{name}”移到回收站？");
        let recycle_source = path.clone();
        let detail = path_text(&path);
        let answers = [PromptButton::ok("移到回收站"), PromptButton::cancel("取消")];
        let answer = window.prompt(PromptLevel::Warning, &message, Some(&detail), &answers, cx);
        cx.spawn_in(window, async move |this, cx| {
            let Ok(choice) = answer.await else {
                return;
            };
            if choice != 0 {
                return;
            }
            let result = cx
                .background_spawn(async move { recycle_entry(&recycle_source) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(()) => {
                    this.notice = Some("已移到回收站".into());
                    let destination = path_after_recycle(&this.current_dir, &path);
                    this.navigate_to(destination, NavigationIntent::Refresh, window, cx);
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
        detail: Option<SharedString>,
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
            .child(div().flex_1().min_w_0().truncate().child(label))
            .when_some(detail, |this, detail| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(if active { active_fg } else { muted })
                        .child(detail),
                )
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.navigate_to(path.clone(), NavigationIntent::Push, window, cx);
            }))
            .into_any_element()
    }

    fn portable_device_item(&self, device: PortableDevice, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let hover_color = muted.opacity(0.12);
        let clicked = device.clone();
        div()
            .id(SharedString::from(format!(
                "portable-device-{}",
                device.shell_path
            )))
            .h(px(32.))
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .hover(move |style| style.bg(hover_color))
            .text_sm()
            .text_color(cx.theme().sidebar_foreground)
            .child(Icon::new(IconName::Usb).size_4().text_color(muted))
            .child(div().flex_1().min_w_0().truncate().child(device.name))
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child("MTP"),
            )
            .child(Icon::new(IconName::ChevronRight).size_3())
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.open_portable_device(clicked.clone(), cx);
            }))
            .into_any_element()
    }

    fn open_portable_device(&mut self, device: PortableDevice, cx: &mut Context<Self>) {
        match device.open_in_system() {
            Ok(()) => {
                self.error = None;
                self.notice = Some(format!("已在 Windows 中打开 {}", device.name).into());
            }
            Err(error) => {
                self.notice = None;
                self.error = Some(error.into());
            }
        }
        cx.notify();
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
            content = content.child(self.sidebar_item(
                link.label,
                link.icon,
                link.path,
                link.detail,
                active,
                cx,
            ));
        }
        if !self.connected_devices.is_empty() || !self.portable_devices.is_empty() {
            content = content.child(
                div()
                    .px_2()
                    .pt_4()
                    .pb_1()
                    .text_xs()
                    .font_medium()
                    .text_color(section_color)
                    .child("设备"),
            );
            for device in self.connected_devices.clone() {
                let active = same_path(&device.path, &self.current_dir);
                content = content.child(self.sidebar_item(
                    device.label,
                    device.icon,
                    device.path,
                    device.detail,
                    active,
                    cx,
                ));
            }
            for device in self.portable_devices.clone() {
                content = content.child(self.portable_device_item(device, cx));
            }
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
            content = content.child(self.sidebar_item(
                drive.label,
                drive.icon,
                drive.path,
                drive.detail,
                active,
                cx,
            ));
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
                            this.navigate_from_path_bar(segment.path.clone(), window, cx);
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
            if let Some(error) = self.error.clone() {
                return v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .px_8()
                    .child(
                        div()
                            .max_w(px(560.))
                            .text_sm()
                            .text_color(cx.theme().danger)
                            .child(error),
                    )
                    .into_any_element();
            }
            return strip
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(if self.loading {
                    "正在读取分栏…"
                } else {
                    "文件夹不可用"
                })
                .into_any_element();
        }
        self.sync_column_list_scrolls();
        for index in 0..self.column_panes.len() {
            strip = strip.child(self.render_column(index, window, cx));
        }
        if let Some(entry) = self.selected_last_column_file() {
            strip = strip.child(self.render_column_preview(&entry, cx));
        }
        div()
            .relative()
            .size_full()
            .child(strip.into_any_element())
            .child(
                div().absolute().inset_0().child(
                    Scrollbar::horizontal(&scroll_handle)
                        .id("column-horizontal-scrollbar")
                        .mode(ScrollbarMode::Always)
                        .viewport_from_layout(),
                ),
            )
            .into_any_element()
    }

    fn selected_last_column_file(&self) -> Option<FileEntry> {
        let pane = self.column_panes.last()?;
        let selected = pane.selected_path.as_ref()?;
        pane.entries
            .iter()
            .find(|entry| same_path(&entry.path, selected))
            .filter(|entry| !entry.is_dir())
            .cloned()
    }

    fn render_column_preview(&mut self, entry: &FileEntry, cx: &mut Context<Self>) -> AnyElement {
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let copy_text = preview_info_text(entry);
        let preview = self
            .column_preview
            .as_ref()
            .filter(|preview| same_path(&preview.path, &entry.path))
            .cloned();

        let preview_area = if is_image_preview_entry(entry) {
            let fallback_entry = entry.clone();
            h_flex()
                .h(px(180.))
                .w_full()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .rounded_md()
                .bg(cx.theme().muted.opacity(0.25))
                .child(
                    img(entry.path.clone())
                        .max_w_full()
                        .max_h(px(170.))
                        .object_fit(ObjectFit::Contain)
                        .with_fallback(move || {
                            v_flex()
                                .size_full()
                                .items_center()
                                .justify_center()
                                .gap_2()
                                .child(
                                    Icon::new(icon_for(&fallback_entry))
                                        .size(px(48.))
                                        .text_color(muted),
                                )
                                .child(div().text_xs().text_color(muted).child("无法加载图片预览"))
                                .into_any_element()
                        }),
                )
                .into_any_element()
        } else if is_text_preview_entry(entry) {
            let preview_text = if let Some(preview) = preview.as_ref() {
                if preview.loading {
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("正在加载预览…")
                        .into_any_element()
                } else if let Some(error) = preview.error.clone() {
                    div()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(SelectableText::new(
                            format!("column-preview-error-{}", entry.path.display()),
                            error,
                        ))
                        .into_any_element()
                } else if let Some(text) = preview.text.clone() {
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground)
                        .child(SelectableText::new(
                            format!("column-preview-content-{}", entry.path.display()),
                            text,
                        ))
                        .into_any_element()
                } else {
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("无文本预览")
                        .into_any_element()
                }
            } else {
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("选择文件后加载预览")
                    .into_any_element()
            };
            div()
                .id("column-preview-text")
                .h(px(180.))
                .w_full()
                .overflow_y_scroll()
                .rounded_md()
                .bg(cx.theme().muted.opacity(0.25))
                .p_2()
                .child(preview_text)
                .into_any_element()
        } else {
            v_flex()
                .h(px(180.))
                .w_full()
                .items_center()
                .justify_center()
                .gap_2()
                .rounded_md()
                .bg(cx.theme().muted.opacity(0.25))
                .child(
                    Icon::new(icon_for(entry))
                        .size(px(56.))
                        .text_color(icon_color(entry, cx)),
                )
                .child(div().text_xs().text_color(muted).child("无内置预览"))
                .into_any_element()
        };

        let mut properties = v_flex().w_full().gap_3();
        properties = properties
            .child(preview_property("名称", entry.name.clone(), cx))
            .child(preview_property("类型", entry.type_label(), cx))
            .child(preview_property("大小", entry.size_label(), cx))
            .child(preview_property("修改日期", entry.modified_label(), cx))
            .child(preview_property("位置", path_text(&entry.path), cx));
        let mut vcs = Vec::new();
        if let Some(status) = entry.git_status {
            vcs.push(format!("Git {} ({})", status.label(), status.code()));
        }
        if let Some(status) = entry.svn_status {
            vcs.push(format!("SVN {} ({})", status.label(), status.code()));
        }
        if !vcs.is_empty() {
            properties = properties.child(preview_property("版本控制", vcs.join("\n"), cx));
        }

        v_flex()
            .w(px(300.))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(border)
            .child(
                h_flex()
                    .h(px(34.))
                    .w_full()
                    .px_2()
                    .items_center()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(muted)
                            .child("预览"),
                    )
                    .child(div().flex_1())
                    .child(
                        Clipboard::new(format!("copy-preview-info-{}", entry.path.display()))
                            .value(copy_text)
                            .tooltip("复制预览信息")
                            .accessibility_label("复制预览信息"),
                    ),
            )
            .child(
                v_flex()
                    .id("column-preview")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .p_3()
                    .gap_4()
                    .child(preview_area)
                    .child(properties),
            )
            .into_any_element()
    }
    fn sync_column_list_scrolls(&mut self) {
        self.column_vertical_scrolls
            .truncate(self.column_panes.len());
        for (index, pane) in self.column_panes.iter().enumerate() {
            let path_changed = self
                .column_vertical_scrolls
                .get(index)
                .is_none_or(|state| !same_path(&state.path, &pane.path));
            if path_changed {
                let state = ColumnListScroll {
                    path: pane.path.clone(),
                    handle: UniformListScrollHandle::new(),
                };
                if let Some(slot) = self.column_vertical_scrolls.get_mut(index) {
                    *slot = state;
                } else {
                    self.column_vertical_scrolls.push(state);
                }
            }
        }
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
        let vertical_scroll = self
            .column_vertical_scrolls
            .get(index)
            .map(|state| state.handle.clone())
            .unwrap_or_default();
        let active = same_path(&path, &self.current_dir);
        let title = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let can_paste = self.clipboard.borrow().is_some();
        let column_menu_target = column_context_menu_target(pane, can_paste);
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
                SharedString::from(format!("column-list-{index}-{}", path.display())),
                count,
                cx.processor(move |this, range, window, cx| {
                    this.render_column_rows(index, range, window, cx)
                }),
            )
            .flex_1()
            .min_h_0()
            .w_full()
            .track_scroll(&vertical_scroll)
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
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .when(!loading && count > 0, |this| {
                        this.on_scroll_wheel(cx.listener(
                            move |this, event: &ScrollWheelEvent, window, cx| {
                                this.handle_column_scroll_wheel(index, event, window, cx)
                            },
                        ))
                    })
                    .child(body),
            )
            .context_menu(move |menu, window, cx| {
                render_context_menu(menu, &column_menu_target, window, cx)
            })
            .into_any_element()
    }

    fn handle_column_scroll_wheel(
        &mut self,
        index: usize,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let handle = match self.column_vertical_scrolls.get(index) {
            Some(state) => state.handle.clone(),
            None => return,
        };
        let base = handle.0.borrow().base_handle.clone();
        let delta = event.delta.pixel_delta(window.line_height());
        let delta_x = delta.x.as_f32();
        let delta_y = delta.y.as_f32();
        let current = base.offset();

        if wheel_scrolls_columns(delta_x, delta_y) {
            // The list maps one-axis gestures to its vertical offset. Restore it
            // before the column strip receives this horizontal gesture.
            let applied_y = if delta.y.is_zero() { delta_x } else { delta_y };
            let max_y = base.max_offset().y.as_f32().max(0.0);
            let restored_y = (current.y.as_f32() - applied_y).clamp(-max_y, 0.0);
            base.set_offset(point(current.x, px(restored_y)));
            cx.notify();
            return;
        }

        if delta.y.is_zero() {
            return;
        }

        let max_y = base.max_offset().y.as_f32();
        if !handle.is_scrollable() || max_y <= 0.0 {
            base.set_offset(point(current.x, px(0.0)));
            cx.notify();
            return;
        }

        let route = route_directory_scroll(current.y.as_f32(), max_y);
        base.set_offset(point(current.x, px(route.directory_offset)));
        if route.column_delta != 0.0 {
            self.scroll_columns_by(route.column_delta, cx);
        }
        // The list handles wheel input before this wrapper during bubbling.
        // Stop the parent strip only after the directory consumes the gesture.
        cx.notify();
        cx.stop_propagation();
    }

    fn scroll_columns_by(&self, delta: f32, cx: &mut Context<Self>) {
        if delta == 0.0 {
            return;
        }
        let handle = self.column_scroll.clone();
        let offset = handle.offset();
        let max_x = handle.max_offset().x.as_f32().max(0.0);
        let target_x = (offset.x.as_f32() + delta).clamp(-max_x, 0.0);
        if (target_x - offset.x.as_f32()).abs() > f32::EPSILON {
            handle.set_offset(point(px(target_x), offset.y));
            cx.notify();
        }
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
        let column_path = pane.path.clone();
        let selected_path = pane.selected_path.clone();
        let loading = pane.loading;
        let can_paste = self.clipboard.borrow().is_some();
        let git_available = pane.git_available;
        let svn_available = pane.svn_available;
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
            let row_column_path = column_path.clone();
            let menu_entry = entry.clone();
            let row_git_available = git_available || entry.git_status.is_some();
            let row_svn_available = svn_available || entry.svn_status.is_some();
            let icon_status = worse_status(entry.git_status, entry.svn_status);
            let icon_status_is_svn = match (entry.git_status, entry.svn_status) {
                (Some(git), Some(svn)) => svn.rank() > git.rank(),
                (None, Some(_)) => true,
                _ => false,
            };
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
                    div()
                        .relative()
                        .size_4()
                        .child(
                            Icon::new(icon_for(&entry))
                                .size_4()
                                .text_color(icon_color(&entry, cx)),
                        )
                        .when_some(icon_status, |this, status| {
                            this.child(vcs_icon_badge(status, icon_status_is_svn, cx))
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(entry.name.clone()),
                )
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
                .context_menu(move |menu, window, cx| {
                    let target = ContextMenuTarget::entry(
                        menu_entry.path.clone(),
                        menu_entry.name.clone(),
                        menu_entry.is_dir(),
                        row_column_path.clone(),
                        ContextMenuCapabilities {
                            loading,
                            can_paste,
                            git_available: row_git_available,
                            svn_available: row_svn_available,
                        },
                    );
                    render_context_menu(menu, &target, window, cx)
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
            ModalKind::NewFolder(_) => ("新建文件夹", "输入文件夹名称。按 Enter 创建。", "创建"),
            ModalKind::Rename(path) => (
                "重命名",
                "输入新名称。文件内容不会改变。",
                if path.is_dir() {
                    "重命名文件夹"
                } else {
                    "重命名文件"
                },
            ),
            ModalKind::GitCommit(_) => (
                "Git 提交",
                "输入提交说明。Git 将提交所选路径的已暂存更改。",
                "提交",
            ),
            ModalKind::SvnCommit(_) => (
                "SVN 提交",
                "输入提交说明。SVN 将提交所选路径的本地更改。",
                "提交",
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
        detail: None,
    });
}

fn volume_quick_links(volumes: &[StorageVolume]) -> (Vec<QuickLink>, Vec<QuickLink>) {
    let mut connected_devices = Vec::new();
    let mut drives = Vec::new();

    for volume in volumes {
        let name = volume
            .volume_name
            .as_deref()
            .filter(|name| !name.is_empty())
            .or_else(|| {
                volume
                    .kind
                    .is_removable()
                    .then_some(volume.device_name.as_deref())
                    .flatten()
            })
            .unwrap_or(match volume.kind {
                VolumeKind::Usb => "USB 设备",
                VolumeKind::Removable => "可移动磁盘",
                VolumeKind::Fixed => "本地磁盘",
                VolumeKind::Optical => "光驱",
                VolumeKind::Network => "网络驱动器",
                VolumeKind::Other => "磁盘",
            });
        let icon = match volume.kind {
            VolumeKind::Usb => IconName::Usb,
            VolumeKind::Optical => IconName::Disc,
            _ => IconName::HardDrive,
        };
        let link = QuickLink {
            label: crate::platform::volume_label(name, &volume.path).into(),
            path: volume.path.clone(),
            icon,
            detail: volume
                .total_bytes
                .map(|bytes| SharedString::from(format_size(bytes))),
        };

        if volume.kind.is_removable() {
            connected_devices.push(link);
        } else {
            drives.push(link);
        }
    }

    (connected_devices, drives)
}

fn same_quick_links(left: &[QuickLink], right: &[QuickLink]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.label == right.label && left.path == right.path && left.detail == right.detail
        })
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}

fn same_path(left: &Path, right: &Path) -> bool {
    crate::platform::paths_equal(left, right)
}

fn path_after_rename(current_dir: &Path, original: &Path, renamed: &Path) -> PathBuf {
    if current_dir == original {
        return renamed.to_path_buf();
    }
    match current_dir.strip_prefix(original) {
        Ok(relative) => renamed.join(relative),
        Err(_) => current_dir.to_path_buf(),
    }
}

fn path_after_recycle(current_dir: &Path, removed: &Path) -> PathBuf {
    if current_dir == removed || current_dir.starts_with(removed) {
        return removed
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(default_root_path);
    }
    current_dir.to_path_buf()
}

fn open_vcs_query_text(tool: VcsTool, query: VcsQuery, path: &Path) -> Result<String, String> {
    let title = format!("{} {}", tool.label(), query.label());
    let content = run_vcs_query(tool, query, path)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let kind = match query {
        VcsQuery::Diff => "diff",
        VcsQuery::Log => "log",
    };
    let file = std::env::temp_dir().join(format!(
        "kite-{}-{kind}-{}-{stamp}.txt",
        tool.label().to_lowercase(),
        std::process::id()
    ));
    let text = format!("{title}\n目标：{}\n\n{content}\n", path.display());
    std::fs::write(&file, text.as_bytes())
        .map_err(|error| format!("无法写入{title}临时文件：{error}"))?;
    open_file(&file)?;
    Ok(format!("{title}已打开"))
}

fn wheel_scrolls_columns(delta_x: f32, delta_y: f32) -> bool {
    delta_x != 0.0 && delta_x.abs() > delta_y.abs()
}

fn route_directory_scroll(offset_after_scroll: f32, max_offset: f32) -> DirectoryScrollRoute {
    if max_offset <= 0.0 {
        return DirectoryScrollRoute {
            directory_offset: 0.0,
            column_delta: offset_after_scroll,
        };
    }

    let directory_offset = offset_after_scroll.clamp(-max_offset, 0.0);
    DirectoryScrollRoute {
        directory_offset,
        column_delta: offset_after_scroll - directory_offset,
    }
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

    if raw == "/" {
        return Ok(root_directory(current_dir));
    }

    #[cfg(target_os = "windows")]
    if raw.starts_with('/') && !raw.starts_with("//") {
        return Ok(root_directory(current_dir).join(raw.trim_start_matches('/')));
    }

    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(current_dir.join(path))
    }
}

fn preview_property(label: &str, value: String, cx: &App) -> AnyElement {
    v_flex()
        .w_full()
        .gap_0p5()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label.to_string()),
        )
        .child(div().text_sm().child(SelectableText::new(
            format!("preview-property-{label}"),
            value,
        )))
        .into_any_element()
}

fn preview_info_text(entry: &FileEntry) -> String {
    let mut lines = vec![
        format!("名称：{}", entry.name),
        format!("类型：{}", entry.type_label()),
        format!("大小：{}", entry.size_label()),
        format!("修改日期：{}", entry.modified_label()),
        format!("位置：{}", path_text(&entry.path)),
    ];
    if let Some(status) = entry.git_status {
        lines.push(format!("Git：{} ({})", status.label(), status.code()));
    }
    if let Some(status) = entry.svn_status {
        lines.push(format!("SVN：{} ({})", status.label(), status.code()));
    }
    lines.join("\n")
}

fn is_image_preview_entry(entry: &FileEntry) -> bool {
    matches!(
        entry.extension.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg"
    )
}

fn is_text_preview_entry(entry: &FileEntry) -> bool {
    if entry.size > 512 * 1024 {
        return false;
    }
    entry.extension.is_empty()
        || matches!(
            entry.extension.as_str(),
            "txt"
                | "md"
                | "log"
                | "json"
                | "toml"
                | "yaml"
                | "yml"
                | "xml"
                | "ini"
                | "rs"
                | "py"
                | "js"
                | "ts"
                | "tsx"
                | "jsx"
                | "html"
                | "css"
                | "c"
                | "cpp"
                | "h"
                | "cs"
                | "java"
                | "go"
                | "kt"
                | "sh"
                | "ps1"
                | "bat"
        )
}

fn read_text_preview(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|error| format!("无法读取预览：{error}"))?;
    let mut reader = std::io::BufReader::new(file).take(64 * 1024);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| format!("无法读取预览：{error}"))?;
    if bytes.contains(&0) {
        return Err("此文件不是文本文件".to_string());
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut excerpt = text.chars().take(6_000).collect::<String>();
    if text.chars().count() > 6_000 {
        excerpt.push('…');
    }
    if excerpt.trim().is_empty() {
        Ok("文件没有可显示的文本内容。".to_string())
    } else {
        Ok(excerpt)
    }
}
fn column_chain(root: &Path) -> Vec<PathBuf> {
    let mut chain = root.ancestors().map(Path::to_path_buf).collect::<Vec<_>>();
    chain.reverse();
    chain
}

fn common_column_prefix(panes: &[ColumnPane], chain: &[PathBuf]) -> usize {
    panes
        .iter()
        .zip(chain)
        .take_while(|(pane, path)| !pane.loading && same_path(&pane.path, path))
        .count()
}

fn load_column_pane(path: PathBuf, sort: SortSpec) -> ColumnPane {
    let (mut entries, skipped) = load_directory(&path, sort)
        .map(|snapshot| (snapshot.entries, snapshot.skipped))
        .unwrap_or_default();
    let snapshot = load_vcs_snapshot(&path);
    apply_statuses(&mut entries, &path, &snapshot);
    ColumnPane {
        path,
        entries,
        selected_path: None,
        loading: false,
        skipped,
        git_available: snapshot.git_available(),
        svn_available: snapshot.svn_available(),
    }
}

fn column_context_menu_target(pane: &ColumnPane, can_paste: bool) -> ContextMenuTarget {
    let title = pane
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| pane.path.display().to_string());
    ContextMenuTarget::directory(
        pane.path.clone(),
        title,
        ContextMenuCapabilities {
            loading: pane.loading,
            can_paste,
            git_available: pane.git_available,
            svn_available: pane.svn_available,
        },
    )
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

fn loaded_column_index(panes: &[ColumnPane], path: &Path) -> Option<usize> {
    panes
        .iter()
        .position(|pane| !pane.loading && same_path(&pane.path, path))
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
            .on_action(cx.listener(Self::on_context_menu_action))
            .on_action(cx.listener(Self::show_list_view))
            .on_action(cx.listener(Self::show_columns_view))
            .on_action(cx.listener(Self::toggle_path_bar))
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
            .on_action(cx.listener(Self::set_default_explorer))
            .on_action(cx.listener(Self::restore_default_explorer))
            .on_action(cx.listener(Self::create_start_menu_shortcut))
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
                        DropdownButton::new("default-explorer-menu")
                            .with_size(Size::Small)
                            .menu_accessibility_label("默认资源管理器设置")
                            .button(
                                Button::new("default-explorer-action")
                                    .ghost()
                                    .compact()
                                    .icon(IconName::FolderCog)
                                    .tooltip("设为默认资源管理器")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.set_default_explorer(&SetDefaultExplorer, window, cx);
                                    })),
                            )
                            .dropdown_menu(|menu, _, _| {
                                menu.menu("设为默认资源管理器…", Box::new(SetDefaultExplorer))
                                    .separator()
                                    .menu(
                                        "恢复之前的文件夹打开方式…",
                                        Box::new(RestoreDefaultExplorer),
                                    )
                                    .separator()
                                    .menu(
                                        "创建开始菜单快捷方式…",
                                        Box::new(CreateStartMenuShortcut),
                                    )
                            }),
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

#[cfg(test)]
mod path_bar_tests {
    use super::{
        ColumnPane, column_context_menu_target, common_column_prefix, loaded_column_index,
        path_after_recycle, path_after_rename,
    };
    use std::path::{Path, PathBuf};

    fn pane(path: &str, loading: bool) -> ColumnPane {
        ColumnPane {
            path: PathBuf::from(path),
            entries: Vec::new(),
            selected_path: None,
            loading,
            skipped: 0,
            git_available: false,
            svn_available: false,
        }
    }

    #[test]
    fn path_bar_reuses_only_loaded_columns() {
        let panes = [
            pane(r"C:\", false),
            pane(r"C:\Users", false),
            pane(r"C:\Users\fengqy", true),
        ];

        assert_eq!(loaded_column_index(&panes, Path::new(r"C:\Users")), Some(1));
        assert_eq!(
            loaded_column_index(&panes, Path::new(r"C:\Users\fengqy")),
            None
        );
        assert_eq!(loaded_column_index(&panes, Path::new(r"C:\Windows")), None);
    }

    #[test]
    fn column_update_keeps_only_loaded_common_prefix() {
        let panes = [
            pane(r"C:\", false),
            pane(r"C:\Users", false),
            pane(r"C:\Users\fengqy", true),
        ];
        let chain = [
            PathBuf::from(r"C:\"),
            PathBuf::from(r"C:\Users"),
            PathBuf::from(r"C:\Users\fengqy"),
            PathBuf::from(r"C:\Users\fengqy\Documents"),
        ];

        assert_eq!(common_column_prefix(&panes, &chain), 2);
    }

    #[test]
    fn column_context_menu_targets_that_directory() {
        let mut column = pane(r"C:\Users\fengqy\Documents", false);
        column.git_available = true;

        let target = column_context_menu_target(&column, true);

        assert_eq!(target.path, column.path);
        assert_eq!(target.working_directory, column.path);
        assert_eq!(target.title, "Documents");
        assert!(target.is_dir);
        assert!(target.git_available);
    }

    #[test]
    fn renaming_an_ancestor_rewrites_the_current_path() {
        assert_eq!(
            path_after_rename(
                Path::new(r"C:\repo\src\ui"),
                Path::new(r"C:\repo\src"),
                Path::new(r"C:\repo\source"),
            ),
            PathBuf::from(r"C:\repo\source\ui")
        );
        assert_eq!(
            path_after_rename(
                Path::new(r"C:\repo\docs"),
                Path::new(r"C:\repo\src"),
                Path::new(r"C:\repo\source"),
            ),
            PathBuf::from(r"C:\repo\docs")
        );
    }

    #[test]
    fn recycling_an_ancestor_moves_the_current_path_to_its_parent() {
        assert_eq!(
            path_after_recycle(Path::new(r"C:\repo\src\ui"), Path::new(r"C:\repo\src"),),
            PathBuf::from(r"C:\repo")
        );
        assert_eq!(
            path_after_recycle(Path::new(r"C:\repo\src"), Path::new(r"C:\repo\src\ui"),),
            PathBuf::from(r"C:\repo\src")
        );
    }
}

#[cfg(test)]
mod preview_tests {
    use super::{preview_info_text, read_text_preview};
    use crate::fs::{EntryKind, FileEntry, VcsStatus};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reads_text_preview_and_rejects_binary_content() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("kite-preview-{unique}"));
        fs::create_dir_all(&root).expect("create preview directory");
        let text_path = root.join("notes.txt");
        let binary_path = root.join("binary.txt");
        fs::write(&text_path, "first line\nsecond line\n").expect("write text preview");
        fs::write(&binary_path, b"text\0binary").expect("write binary preview");

        let preview = read_text_preview(&text_path).expect("read text preview");
        assert!(preview.contains("first line"));
        assert!(read_text_preview(&binary_path).is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn preview_copy_text_includes_properties_and_vcs() {
        let entry = FileEntry {
            path: PathBuf::from(r"C:\demo\notes.txt"),
            name: "notes.txt".to_string(),
            kind: EntryKind::File,
            size: 1_536,
            modified: None,
            extension: "txt".to_string(),
            git_status: Some(VcsStatus::Modified),
            svn_status: None,
        };

        let text = preview_info_text(&entry);

        assert!(text.contains("名称：notes.txt"));
        assert!(text.contains("位置：C:\\demo\\notes.txt"));
        assert!(text.contains("Git：已修改 (M)"));
    }
}
#[cfg(test)]
mod scroll_routing_tests {
    use super::{DirectoryScrollRoute, route_directory_scroll, wheel_scrolls_columns};

    #[test]
    fn horizontal_gesture_takes_priority_over_vertical_noise() {
        assert!(wheel_scrolls_columns(18.0, 3.0));
        assert!(wheel_scrolls_columns(8.0, 7.0));
        assert!(!wheel_scrolls_columns(3.0, 18.0));
        assert!(!wheel_scrolls_columns(0.0, 18.0));
    }

    #[test]
    fn directory_interior_consumes_vertical_wheel_input() {
        assert_eq!(
            route_directory_scroll(-420.0, 800.0),
            DirectoryScrollRoute {
                directory_offset: -420.0,
                column_delta: 0.0,
            }
        );
    }

    #[test]
    fn bottom_boundary_routes_only_overflow_to_columns() {
        assert_eq!(
            route_directory_scroll(-830.0, 800.0),
            DirectoryScrollRoute {
                directory_offset: -800.0,
                column_delta: -30.0,
            }
        );
    }

    #[test]
    fn top_boundary_routes_only_overflow_to_columns() {
        assert_eq!(
            route_directory_scroll(25.0, 800.0),
            DirectoryScrollRoute {
                directory_offset: 0.0,
                column_delta: 25.0,
            }
        );
    }

    #[test]
    fn non_scrollable_directory_passes_full_delta_to_columns() {
        assert_eq!(
            route_directory_scroll(-90.0, 0.0),
            DirectoryScrollRoute {
                directory_offset: 0.0,
                column_delta: -90.0,
            }
        );
    }
}
