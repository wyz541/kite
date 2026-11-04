use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::actions::*;
use crate::context_menu::{
    ContextMenuCapabilities, ContextMenuTarget, render_context_menu, request_system_menu,
    show_properties,
};
use crate::devices::{DeviceSnapshot, PortableDevice, StorageVolume, VolumeKind};
use crate::favorites::{self as favorites_store, FavoriteEntryDto};
use crate::file_clipboard::{self, ClipboardMode, FileClipboard};
use crate::fs::{
    DirectorySnapshot, FileEntry, SortKey, SortSpec, create_file, create_folder, format_size,
    load_directory, open_file, recycle_entry, rename_entry, search_directory, sort_entries,
    transfer_entry, validate_name, worse_status,
};
use crate::platform::{default_root_path, root_directory};
use crate::preview_window::PreviewWindow;
use crate::shell_fs;
use crate::table::{FileTableDelegate, icon_color, icon_for, vcs_icon_badge};
use crate::text_editor;
use crate::ui_preferences::{self, ColumnGroupPreference as ColumnGroup, ColumnViewRule};
use crate::vcs::{
    VcsOperation, VcsQuery, VcsTool, apply_statuses, load_vcs_snapshot, open_vcs_log_window,
    run_vcs_operation, run_vcs_query,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::{SelectableText, TextSelectionHandle};
use gpui_kit::component::button::{Button, ButtonVariants as _, DropdownButton};
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::scroll::{ScrollableMask, Scrollbar, ScrollbarHandle, ScrollbarMode};
use gpui_kit::component::table::{DataTable, TableEvent, TableState};
use gpui_kit::component::text::{TextView, TextViewState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, StyledExt as _, Theme, ThemeMode,
    h_flex, v_flex,
};
use gpui_kit::prelude::{FluentBuilder as _, StatefulInteractiveElement as _};
use gpui_kit::*;

const SIDEBAR_WIDTH: f32 = 218.;
const TOOLBAR_CONTROL_HEIGHT: f32 = 40.;
const COLUMN_WIDTH: f32 = 274.;
const DETAILED_COLUMN_MIN_WIDTH: f32 = 250.;
const DETAILED_COLUMN_MAX_WIDTH: f32 = 640.;
const DETAILED_COLUMN_NAME_OVERHEAD: f32 = 150.;
const DETAILED_DATE_WIDTH: f32 = 84.;
const DETAILED_KIND_WIDTH: f32 = 70.;
const DETAILED_SIZE_WIDTH: f32 = 64.;
const COLUMN_SEARCH_DEBOUNCE: Duration = Duration::from_millis(180);

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
struct SidebarItem {
    id: SharedString,
    label: SharedString,
    path: PathBuf,
    icon: IconName,
    detail: Option<SharedString>,
    active: bool,
    is_directory: bool,
    favorite: bool,
}

#[derive(Clone)]
struct SearchState {
    query: String,
    root: PathBuf,
}

#[derive(Clone)]
enum ModalKind {
    NewFolder(PathBuf),
    NewFile(PathBuf),
    Rename(PathBuf),

    GitCommit(PathBuf),
    SvnCommit(PathBuf),
}

struct ModalState {
    kind: ModalKind,
    input: Entity<InputState>,
    _subscription: Subscription,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ColumnFilter {
    #[default]
    All,
    Folders,
    Files,
    Media,
    Documents,
}

#[derive(Clone, Copy)]
struct ColumnViewRules {
    sort: SortSpec,
    filter: ColumnFilter,
    group: ColumnGroup,
}

#[derive(Clone, Copy)]
struct ColumnLayoutState {
    show_date_column: bool,
    show_kind_column: bool,
    show_size_column: bool,
    preferred_width: u16,
}

#[derive(Clone)]
enum ColumnDisplayRow {
    Group {
        label: SharedString,
        count: usize,
        collapsed: bool,
    },
    Entry(FileEntry),
}

#[derive(Clone)]
struct KiteFileDrag {
    paths: Vec<(PathBuf, bool)>,
    label: SharedString,
}

struct KiteFileDragPreview {
    label: SharedString,
    count: usize,
    position: Point<Pixels>,
}

impl Render for KiteFileDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = if self.count > 1 {
            format!("{} · {} 项", self.label, self.count)
        } else {
            self.label.to_string()
        };
        div()
            .pl(self.position.x - px(72.))
            .pt(self.position.y - px(18.))
            .child(
                h_flex()
                    .h(px(36.))
                    .max_w(px(180.))
                    .gap_2()
                    .px_3()
                    .items_center()
                    .rounded_md()
                    .bg(cx.theme().accent)
                    .text_sm()
                    .text_color(cx.theme().accent_foreground)
                    .shadow_md()
                    .child(Icon::new(IconName::Folder).size_4())
                    .child(div().min_w_0().truncate().child(label)),
            )
    }
}

#[derive(Clone)]
struct ColumnPane {
    path: PathBuf,
    directory_entries: Vec<FileEntry>,
    search_entries: Option<Vec<FileEntry>>,
    entries: Vec<FileEntry>,
    selected_path: Option<PathBuf>,
    selected_paths: Vec<PathBuf>,
    selection_anchor_path: Option<PathBuf>,
    sort: SortSpec,
    filter: ColumnFilter,
    group: ColumnGroup,
    collapsed_groups: BTreeSet<String>,
    search_query: Option<String>,
    search_truncated: bool,
    loading: bool,
    skipped: usize,
    git_available: bool,
    svn_available: bool,
}

impl ColumnPane {
    fn new(
        path: PathBuf,
        directory_entries: Vec<FileEntry>,
        skipped: usize,
        loading: bool,
        git_available: bool,
        svn_available: bool,
        rule: ColumnViewRule,
    ) -> Self {
        let mut pane = Self {
            path,
            directory_entries,
            search_entries: None,
            entries: Vec::new(),
            selected_path: None,
            selected_paths: Vec::new(),
            selection_anchor_path: None,
            sort: rule.sort,
            filter: ColumnFilter::All,
            group: rule.group,
            collapsed_groups: rule.collapsed_groups,
            search_query: None,
            search_truncated: false,
            loading,
            skipped,
            git_available,
            svn_available,
        };
        pane.refresh_view();
        pane
    }

    fn source_entries(&self) -> &[FileEntry] {
        self.search_entries
            .as_deref()
            .unwrap_or(self.directory_entries.as_slice())
    }

    fn refresh_view(&mut self) {
        let mut entries = self
            .source_entries()
            .iter()
            .filter(|entry| self.filter.matches(entry))
            .cloned()
            .collect::<Vec<_>>();
        sort_entries(&mut entries, self.sort);
        self.entries = entries;
        self.selected_paths.retain(|selected| {
            self.entries
                .iter()
                .any(|entry| same_path(&entry.path, selected))
        });
        if self.selection_anchor_path.as_ref().is_some_and(|selected| {
            !self
                .entries
                .iter()
                .any(|entry| same_path(&entry.path, selected))
        }) {
            self.selection_anchor_path = None;
        }
        if self.selected_path.as_ref().is_some_and(|selected| {
            !self
                .selected_paths
                .iter()
                .any(|path| same_path(path, selected))
        }) {
            self.selected_path = None;
        }
        if self.selected_path.is_none() {
            self.selected_path = self.selected_paths.first().cloned();
        }
    }

    fn is_selected(&self, path: &Path) -> bool {
        self.selected_paths
            .iter()
            .any(|selected| same_path(selected, path))
    }

    fn set_single_selection(&mut self, path: PathBuf) {
        self.selected_path = Some(path.clone());
        self.selected_paths = vec![path.clone()];
        self.selection_anchor_path = Some(path);
    }

    fn toggle_selection(&mut self, path: PathBuf) {
        if let Some(index) = self
            .selected_paths
            .iter()
            .position(|selected| same_path(selected, &path))
        {
            self.selected_paths.remove(index);
        } else {
            self.selected_paths.push(path.clone());
        }
        self.selected_path = self.selected_paths.last().cloned();
        self.selection_anchor_path = Some(path);
    }

    fn select_range_to(&mut self, path: PathBuf) {
        let visible_entries = self
            .display_rows()
            .into_iter()
            .filter_map(|row| match row {
                ColumnDisplayRow::Entry(entry) => Some(entry.path),
                ColumnDisplayRow::Group { .. } => None,
            })
            .collect::<Vec<_>>();
        let anchor = self
            .selection_anchor_path
            .as_ref()
            .and_then(|anchor| {
                visible_entries
                    .iter()
                    .position(|path| same_path(path, anchor))
            })
            .or_else(|| {
                self.selected_path.as_ref().and_then(|selected| {
                    visible_entries
                        .iter()
                        .position(|path| same_path(path, selected))
                })
            });
        let target = visible_entries
            .iter()
            .position(|candidate| same_path(candidate, &path));
        let (Some(anchor), Some(target)) = (anchor, target) else {
            self.set_single_selection(path);
            return;
        };
        let range = if anchor <= target {
            anchor..=target
        } else {
            target..=anchor
        };
        self.selected_paths = visible_entries[range].to_vec();
        self.selected_path = Some(path.clone());
        if self.selection_anchor_path.is_none() {
            self.selection_anchor_path = Some(path);
        }
    }

    fn clear_selection(&mut self) {
        self.selected_path = None;
        self.selected_paths.clear();
        self.selection_anchor_path = None;
    }

    fn select_all(&mut self) {
        self.selected_paths = self
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        self.selected_path = self.selected_paths.first().cloned();
        self.selection_anchor_path = self.selected_paths.first().cloned();
    }

    fn toggle_sort_order(&mut self) {
        self.sort.ascending = !self.sort.ascending;
        self.refresh_view();
    }

    fn toggle_sort_for_header(&mut self, key: SortKey) {
        let ascending = if self.sort.key == key {
            !self.sort.ascending
        } else {
            default_column_header_sort_ascending(key)
        };
        self.sort = SortSpec { key, ascending };
        self.refresh_view();
    }

    fn set_directory_entries(&mut self, entries: Vec<FileEntry>, skipped: usize) {
        self.directory_entries = entries;
        self.search_entries = None;
        self.search_query = None;
        self.search_truncated = false;
        self.skipped = skipped;
        self.loading = false;
        self.refresh_view();
    }

    fn set_search_results(&mut self, query: String, entries: Vec<FileEntry>, truncated: bool) {
        self.search_entries = Some(entries);
        self.search_query = Some(query);
        self.search_truncated = truncated;
        self.refresh_view();
    }

    fn clear_search_results(&mut self) {
        self.search_entries = None;
        self.search_query = None;
        self.search_truncated = false;
        self.refresh_view();
    }

    fn display_rows(&self) -> Vec<ColumnDisplayRow> {
        if self.group == ColumnGroup::None {
            return self
                .entries
                .iter()
                .cloned()
                .map(ColumnDisplayRow::Entry)
                .collect();
        }

        let now = SystemTime::now();
        let mut groups = std::collections::BTreeMap::<(u8, String), Vec<FileEntry>>::new();
        for entry in &self.entries {
            let (rank, label) = self.group_key(entry, now);
            groups.entry((rank, label)).or_default().push(entry.clone());
        }

        let mut rows = Vec::new();
        for ((_, label), mut entries) in groups {
            sort_entries(&mut entries, self.sort);
            let collapsed = self.collapsed_groups.contains(&label);
            rows.push(ColumnDisplayRow::Group {
                label: SharedString::from(label),
                count: entries.len(),
                collapsed,
            });
            if !collapsed {
                rows.extend(entries.into_iter().map(ColumnDisplayRow::Entry));
            }
        }
        rows
    }

    fn visible_row_count(&self) -> usize {
        self.display_rows().len()
    }

    fn entry_at_visible_row(&self, row: usize) -> Option<FileEntry> {
        self.display_rows().get(row).and_then(|row| match row {
            ColumnDisplayRow::Entry(entry) => Some(entry.clone()),
            ColumnDisplayRow::Group { .. } => None,
        })
    }

    fn view_rule(&self) -> ColumnViewRule {
        ColumnViewRule {
            sort: self.sort,
            group: self.group,
            collapsed_groups: self.collapsed_groups.clone(),
        }
    }

    fn toggle_group_collapse(&mut self, label: &str) {
        if !self.collapsed_groups.remove(label) {
            self.collapsed_groups.insert(label.to_string());
            self.prune_selection_to_visible_rows();
        }
    }

    fn prune_selection_to_visible_rows(&mut self) {
        let visible_paths = self
            .display_rows()
            .into_iter()
            .filter_map(|row| match row {
                ColumnDisplayRow::Entry(entry) => Some(entry.path),
                ColumnDisplayRow::Group { .. } => None,
            })
            .collect::<Vec<_>>();
        self.selected_paths.retain(|selected| {
            visible_paths
                .iter()
                .any(|visible| same_path(visible, selected))
        });
        if self.selected_path.as_ref().is_some_and(|selected| {
            !visible_paths
                .iter()
                .any(|visible| same_path(visible, selected))
        }) {
            self.selected_path = None;
        }
        if self.selection_anchor_path.as_ref().is_some_and(|selected| {
            !visible_paths
                .iter()
                .any(|visible| same_path(visible, selected))
        }) {
            self.selection_anchor_path = None;
        }
        if self.selected_path.is_none() {
            self.selected_path = self.selected_paths.first().cloned();
        }
    }

    fn group_key(&self, entry: &FileEntry, now: SystemTime) -> (u8, String) {
        match self.group {
            ColumnGroup::None => (0, String::new()),
            ColumnGroup::Kind => {
                if entry.is_dir() {
                    (0, "文件夹".to_string())
                } else {
                    (1, "文件".to_string())
                }
            }
            ColumnGroup::Extension => {
                if entry.is_dir() {
                    (0, "文件夹".to_string())
                } else if entry.extension.is_empty() {
                    (2, "无扩展名".to_string())
                } else {
                    (1, entry.extension.to_uppercase())
                }
            }
            ColumnGroup::Modified => {
                let Some(modified) = entry.modified else {
                    return (4, "未知日期".to_string());
                };
                let age = now.duration_since(modified).unwrap_or_default();
                if age < Duration::from_secs(24 * 60 * 60) {
                    (0, "今天".to_string())
                } else if age < Duration::from_secs(7 * 24 * 60 * 60) {
                    (1, "最近 7 天".to_string())
                } else if age < Duration::from_secs(30 * 24 * 60 * 60) {
                    (2, "最近 30 天".to_string())
                } else {
                    (3, "更早".to_string())
                }
            }
        }
    }
}

impl ColumnFilter {
    fn matches(self, entry: &FileEntry) -> bool {
        match self {
            Self::All => true,
            Self::Folders => entry.is_dir(),
            Self::Files => !entry.is_dir(),
            Self::Media => {
                !entry.is_dir()
                    && matches!(
                        entry.extension.as_str(),
                        "png"
                            | "jpg"
                            | "jpeg"
                            | "gif"
                            | "webp"
                            | "svg"
                            | "bmp"
                            | "ico"
                            | "mp3"
                            | "wav"
                            | "flac"
                            | "aac"
                            | "ogg"
                            | "mp4"
                            | "mkv"
                            | "mov"
                            | "avi"
                            | "webm"
                    )
            }
            Self::Documents => {
                !entry.is_dir()
                    && matches!(
                        entry.extension.as_str(),
                        "txt"
                            | "md"
                            | "log"
                            | "ini"
                            | "toml"
                            | "yaml"
                            | "yml"
                            | "json"
                            | "pdf"
                            | "doc"
                            | "docx"
                            | "xls"
                            | "xlsx"
                            | "ppt"
                            | "pptx"
                    )
            }
        }
    }
}

#[derive(Clone)]
struct ColumnListScroll {
    path: PathBuf,
    handle: UniformListScrollHandle,
}

#[derive(Clone)]
struct ColumnPreviewState {
    path: PathBuf,
    // Fixed-height rows make the scroll extent deterministic for large files.
    lines: Arc<Vec<SharedString>>,
    selection: Option<TextSelectionHandle>,
    markdown_view: Option<Entity<TextViewState>>,
    error: Option<SharedString>,
    loading: bool,
    scroll: UniformListScrollHandle,
    markdown_scroll: ScrollHandle,
}

enum PreviewScrollbar {
    Lines(UniformListScrollHandle),
    Markdown(ScrollHandle),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AppLaunchTarget {
    pub directory: PathBuf,
    pub file: Option<PathBuf>,
}

impl AppLaunchTarget {
    fn from_existing_path(path: PathBuf) -> Option<Self> {
        let path = absolute_path(path);
        let metadata = std::fs::metadata(&path).ok()?;
        if metadata.is_dir() {
            return Some(Self {
                directory: path,
                file: None,
            });
        }
        if metadata.is_file() {
            return Some(Self {
                directory: path.parent()?.to_path_buf(),
                file: Some(path),
            });
        }
        None
    }
}

pub struct KiteApp {
    current_dir: PathBuf,
    history: Vec<PathBuf>,
    history_index: usize,
    quick_links: Vec<QuickLink>,
    favorites: Vec<FavoriteEntryDto>,
    connected_devices: Vec<QuickLink>,
    drives: Vec<QuickLink>,
    portable_devices: Vec<PortableDevice>,
    address_input: Entity<InputState>,
    search_input: Entity<InputState>,
    column_search_input: Entity<InputState>,
    table: Entity<TableState<FileTableDelegate>>,
    focus_handle: FocusHandle,
    active_search: Option<SearchState>,
    search_truncated: bool,
    loading: bool,
    skipped: usize,
    error: Option<SharedString>,
    notice: Option<SharedString>,
    update_in_progress: bool,
    update_download_progress: Option<crate::updater::UpdateDownloadProgress>,
    modal: Option<ModalState>,
    clipboard: SharedClipboard,
    load_generation: u64,
    search_generation: u64,
    vcs_generation: u64,
    search_cancel: Option<Arc<AtomicBool>>,
    column_search_path: Option<PathBuf>,
    column_search_loading: bool,
    column_search_generation: u64,
    column_search_cancel: Option<Arc<AtomicBool>>,
    _load_task: Task<()>,
    _search_task: Task<()>,
    _column_search_task: Task<()>,
    _vcs_task: Task<()>,
    view_mode: ViewMode,
    show_path_bar: bool,
    address_editing: bool,
    search_visible: bool,
    show_preview: bool,
    show_column_details: bool,
    show_date_column: bool,
    show_kind_column: bool,
    show_size_column: bool,
    preferred_column_width: u16,
    column_panes: Vec<ColumnPane>,
    column_selected_path: Option<PathBuf>,
    column_scroll: ScrollHandle,
    column_vertical_scrolls: Vec<ColumnListScroll>,
    column_scroll_pending: bool,
    column_preview: Option<ColumnPreviewState>,
    pending_file_selection: Option<PathBuf>,
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
    pub(crate) fn launch_target_override() -> Option<AppLaunchTarget> {
        std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .and_then(AppLaunchTarget::from_existing_path)
    }

    pub(crate) fn launch_directory_override() -> Option<PathBuf> {
        Self::launch_target_override().map(|target| target.directory)
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
        Self::new_at_with_state(home, None, ViewMode::Columns, true, clipboard, window, cx)
    }

    pub(crate) fn new_launch_target(
        target: AppLaunchTarget,
        clipboard: SharedClipboard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_at_with_state(
            target.directory,
            target.file,
            ViewMode::Columns,
            true,
            clipboard,
            window,
            cx,
        )
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
        Self::new_at_with_state(
            path,
            None,
            view_mode,
            state.show_path_bar,
            clipboard,
            window,
            cx,
        )
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
        initial_file: Option<PathBuf>,
        view_mode: ViewMode,
        show_path_bar: bool,
        clipboard: SharedClipboard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let quick_links = build_quick_links();
        let favorites = favorites_store::load_favorites();
        let snapshot = crate::devices::snapshot();
        let (connected_devices, drives) = volume_quick_links(&snapshot.volumes);
        let portable_devices = snapshot.portable_devices;
        let address_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入文件夹或文件路径，按 Enter 打开")
                .clean_on_escape()
        });
        let search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("全局搜索当前文件夹")
                .clean_on_escape()
        });
        let column_search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("搜索此分栏")
                .clean_on_escape()
        });
        let ui_preferences = ui_preferences::load_ui_preferences();
        let table = cx.new(|cx| {
            TableState::new(
                FileTableDelegate::new(
                    Vec::new(),
                    ui_preferences.default_column_rule.sort,
                    home.clone(),
                ),
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
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let value = this.address_input.read(cx).value().to_string();
                    this.navigate_from_text(value, window, cx);
                }
                InputEvent::Focus => {
                    this.address_editing = true;
                    cx.notify();
                }
                InputEvent::Blur => {
                    this.address_editing = false;
                    cx.notify();
                }
                InputEvent::Change => {}
            },
        );
        let search_subscription = cx.subscribe_in(
            &search_input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let value = this.search_input.read(cx).value().to_string();
                    this.search_visible = true;
                    this.submit_search(value, window, cx);
                }
                InputEvent::Focus => {
                    this.search_visible = true;
                    cx.notify();
                }
                InputEvent::Blur => {
                    if this.active_search.is_none() && this.search_input.read(cx).value().is_empty()
                    {
                        this.search_visible = false;
                        cx.notify();
                    }
                }
                InputEvent::Change => {}
            },
        );
        let column_search_subscription = cx.subscribe_in(
            &column_search_input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let value = this.column_search_input.read(cx).value().to_string();
                    this.submit_column_search(value, window, cx);
                }
                InputEvent::Change => {
                    let value = this.column_search_input.read(cx).value().to_string();
                    if value.trim().is_empty() {
                        this.clear_column_search_results(window, cx);
                    } else {
                        this.schedule_column_search(value, window, cx);
                    }
                }
                _ => {}
            },
        );
        let table_subscription = cx.subscribe_in(&table, window, Self::on_table_event);

        let mut app = Self {
            current_dir: home.clone(),
            history: Vec::new(),
            history_index: 0,
            quick_links,
            favorites,
            connected_devices,
            drives,
            portable_devices,
            address_input,
            search_input,
            column_search_input,
            table,
            focus_handle: cx.focus_handle(),
            active_search: None,
            search_truncated: false,
            loading: false,
            skipped: 0,
            error: None,
            notice: None,
            update_in_progress: false,
            update_download_progress: None,
            modal: None,
            clipboard,
            load_generation: 0,
            search_generation: 0,
            vcs_generation: 0,
            search_cancel: None,
            column_search_path: None,
            column_search_loading: false,
            column_search_generation: 0,
            column_search_cancel: None,
            _load_task: Task::ready(()),
            _search_task: Task::ready(()),
            _column_search_task: Task::ready(()),
            _vcs_task: Task::ready(()),
            view_mode,
            show_path_bar,
            address_editing: false,
            search_visible: false,
            show_preview: true,
            show_column_details: ui_preferences.column_details_visible,
            show_date_column: ui_preferences.date_column_visible,
            show_kind_column: ui_preferences.kind_column_visible,
            show_size_column: ui_preferences.size_column_visible,
            preferred_column_width: ui_preferences.column_width,
            column_panes: Vec::new(),
            column_selected_path: None,
            column_scroll: ScrollHandle::new(),
            column_vertical_scrolls: Vec::new(),
            column_scroll_pending: false,
            column_preview: None,
            pending_file_selection: initial_file.clone(),
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
                column_search_subscription,
                table_subscription,
            ],
        };
        app.request_load_with_file_selection(
            home,
            NavigationIntent::Initial,
            initial_file,
            window,
            cx,
        );
        app.start_device_watcher(cx);
        app
    }

    pub(crate) fn current_dir(&self) -> &Path {
        &self.current_dir
    }

    pub(crate) fn preview_visible(&self) -> bool {
        self.show_preview
    }

    pub(crate) fn toggle_preview(&mut self, cx: &mut Context<Self>) {
        self.show_preview = !self.show_preview;
        if self.show_preview {
            if let Some(entry) = self.selected_last_column_file() {
                self.start_column_preview(entry, cx);
            }
        } else {
            self.clear_column_preview();
        }
        cx.notify();
    }

    fn toggle_column_sort_order(&mut self, index: usize, cx: &mut Context<Self>) {
        let (path, rule) = {
            let Some(pane) = self.column_panes.get_mut(index) else {
                return;
            };
            pane.toggle_sort_order();
            (pane.path.clone(), pane.view_rule())
        };
        if let Err(error) = ui_preferences::save_column_view_rule(&path, rule) {
            self.notice = Some(error.into());
        }
        self.clear_hidden_column_selection();
        self.column_scroll_pending = true;
        cx.notify();
    }

    fn sort_column_from_header(&mut self, index: usize, key: SortKey, cx: &mut Context<Self>) {
        let (path, rule) = {
            let Some(pane) = self.column_panes.get_mut(index) else {
                return;
            };
            pane.toggle_sort_for_header(key);
            (pane.path.clone(), pane.view_rule())
        };
        if let Err(error) = ui_preferences::save_column_view_rule(&path, rule) {
            self.notice = Some(error.into());
        }
        self.clear_hidden_column_selection();
        self.column_scroll_pending = true;
        cx.notify();
    }

    fn toggle_column_details(&mut self, cx: &mut Context<Self>) {
        self.show_column_details = !self.show_column_details;
        self.column_scroll_pending = true;
        if let Err(error) = self.save_layout_preferences() {
            self.notice = Some(error.into());
        }
        cx.notify();
    }

    fn save_layout_preferences(&self) -> Result<(), String> {
        let mut preferences = ui_preferences::load_ui_preferences();
        preferences.column_details_visible = self.show_column_details;
        preferences.date_column_visible = self.show_date_column;
        preferences.kind_column_visible = self.show_kind_column;
        preferences.size_column_visible = self.show_size_column;
        preferences.column_width = self.preferred_column_width;
        ui_preferences::save_ui_preferences(preferences)
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
            KeyBinding::new("secondary-a", SelectAll, Some("Kite")),
            KeyBinding::new("f2", RenameSelection, Some("Kite")),
            KeyBinding::new("delete", RecycleSelection, Some("Kite")),
            KeyBinding::new("enter", OpenSelection, Some("Kite")),
            KeyBinding::new("escape", ClearSearchOverlay, Some("Kite")),
            KeyBinding::new("secondary-s", SaveEditor, Some("KiteEditor")),
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
                MenuItem::action("检查更新…", CheckForUpdates),
                MenuItem::separator(),
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
        self.request_load_with_file_selection(path, intent, None, window, cx);
    }

    fn request_load_with_file_selection(
        &mut self,
        path: PathBuf,
        intent: NavigationIntent,
        pending_file_selection: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_column_search(window, cx);
        self.pending_file_selection = pending_file_selection;
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
        self.address_editing = false;
        self.search_visible = false;
        self.table.update(cx, |state, cx| {
            state.delegate_mut().set_loading(true);
            state.refresh(cx);
            cx.notify();
        });
        cx.notify();

        self._load_task = cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { load_directory_snapshot(&load_path, sort) })
                .await;

            let _ = this.update_in(cx, |this, window, cx| {
                if this.load_generation != generation {
                    return;
                }
                this.loading = false;
                let pending_file_selection = this.pending_file_selection.take();
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
                        this.address_editing = false;
                        this.search_visible = false;
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
                        if this.view_mode == ViewMode::Columns
                            && let Some(entries) = column_entries
                        {
                            this.update_columns_for_loaded_path(
                                this.current_dir.clone(),
                                entries,
                                column_skipped,
                                cx,
                            );
                        }
                        if let Some(path) = pending_file_selection.as_deref()
                            && this.select_loaded_file(path, cx)
                        {
                            this.open_path(path.to_path_buf(), window, cx);
                        }
                        this.refresh_vcs_statuses(this.current_dir.clone(), cx);
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

    fn navigate_to_location(
        &mut self,
        path: PathBuf,
        intent: NavigationIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = AppLaunchTarget::from_existing_path(path.clone()) else {
            self.navigate_to(path, intent, window, cx);
            return;
        };
        self.notice = None;
        self.request_load_with_file_selection(target.directory, intent, target.file, window, cx);
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
        self.close_column_search(window, cx);
        self.clear_column_preview();
        self.pending_file_selection = None;
        let Some((entries, skipped)) = self
            .column_panes
            .get(column)
            .map(|pane| (pane.directory_entries.clone(), pane.skipped))
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
        self.address_editing = false;
        self.search_visible = false;
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
            let child_path = self.column_panes[index + 1].path.clone();
            self.column_panes[index].set_single_selection(child_path);
        }
        if let Some(pane) = self.column_panes.last_mut() {
            pane.clear_selection();
        }
        self.column_selected_path = None;
        self.column_scroll_pending = true;
        self.path_bar_scroll_pending = true;
        cx.notify();
    }

    fn navigate_from_text(&mut self, value: String, window: &mut Window, cx: &mut Context<Self>) {
        match resolve_location_input(&value, &self.current_dir) {
            Ok(path) => self.navigate_to_location(path, NavigationIntent::Push, window, cx),
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
        let Some(parent) = navigation_parent(&self.current_dir) else {
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

    fn toggle_column_search(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .column_search_path
            .as_ref()
            .is_some_and(|active| same_path(active, &path))
        {
            self.close_column_search(window, cx);
            return;
        }

        self.cancel_column_search_task();
        self.column_search_path = Some(path);
        for pane in &mut self.column_panes {
            pane.clear_search_results();
        }
        self.column_search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn submit_column_search(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        self.start_column_search(query, Duration::ZERO, window, cx);
    }

    fn schedule_column_search(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_column_search(query, COLUMN_SEARCH_DEBOUNCE, window, cx);
    }

    fn start_column_search(
        &mut self,
        query: String,
        delay: Duration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = query.trim().to_string();
        if query.is_empty() {
            self.clear_column_search_results(window, cx);
            return;
        }
        let Some(root) = self.column_search_path.clone() else {
            return;
        };
        let sort = self
            .column_panes
            .iter()
            .find(|pane| same_path(&pane.path, &root))
            .map(|pane| pane.sort)
            .unwrap_or_default();

        self.cancel_column_search_task();
        let generation = self.column_search_generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.column_search_cancel = Some(cancel.clone());
        self.column_search_loading = true;
        self.error = None;
        cx.notify();

        self._column_search_task = cx.spawn_in(window, async move |this, cx| {
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let result = cx
                .background_spawn(
                    async move { search_directory(&root, &query, sort, cancel.as_ref()) },
                )
                .await;

            let _ = this.update_in(cx, |this, window, cx| {
                if this.column_search_generation != generation {
                    return;
                }
                this.column_search_cancel = None;
                this.column_search_loading = false;
                match result {
                    Ok(Some(snapshot)) => {
                        if let Some(pane) = this
                            .column_panes
                            .iter_mut()
                            .find(|pane| same_path(&pane.path, &snapshot.root))
                        {
                            pane.set_search_results(
                                snapshot.query,
                                snapshot.entries,
                                snapshot.truncated,
                            );
                        }
                        this.clear_hidden_column_selection();
                        this.column_scroll_pending = true;
                        this.error = None;
                        this.refresh_vcs_statuses(snapshot.root, cx);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        this.error = Some(error.into());
                    }
                }
                this.column_search_input.update(cx, |input, cx| {
                    input.focus(window, cx);
                });
                cx.notify();
            });
        });
    }

    fn cancel_column_search_task(&mut self) {
        if let Some(cancel) = self.column_search_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.column_search_loading = false;
        self.column_search_generation = self.column_search_generation.wrapping_add(1);
        self._column_search_task = Task::ready(());
    }

    fn clear_column_search_results(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_column_search_task();
        for pane in &mut self.column_panes {
            pane.clear_search_results();
        }
        self.column_search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        self.clear_hidden_column_selection();
        cx.notify();
    }

    fn close_column_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_column_search_results(window, cx);
        self.column_search_path = None;
        cx.notify();
    }

    fn clear_hidden_column_selection(&mut self) {
        let selection_is_visible = self.column_selected_path.as_ref().is_some_and(|selected| {
            self.column_panes
                .iter()
                .any(|pane| pane.is_selected(selected))
        });
        if !selection_is_visible {
            self.column_selected_path = None;
            self.clear_column_preview();
        }
    }

    fn on_column_pane_action(
        &mut self,
        action: &ColumnPaneAction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let layout_changed = match action.command {
            ColumnPaneCommand::ToggleDateColumn => {
                self.show_date_column = !self.show_date_column;
                self.show_column_details = true;
                true
            }
            ColumnPaneCommand::ToggleKindColumn => {
                self.show_kind_column = !self.show_kind_column;
                self.show_column_details = true;
                true
            }
            ColumnPaneCommand::ToggleSizeColumn => {
                self.show_size_column = !self.show_size_column;
                self.show_column_details = true;
                true
            }
            ColumnPaneCommand::SetCompactColumnWidth => {
                self.preferred_column_width = 250;
                true
            }
            ColumnPaneCommand::SetStandardColumnWidth => {
                self.preferred_column_width = 320;
                true
            }
            ColumnPaneCommand::SetWideColumnWidth => {
                self.preferred_column_width = 420;
                true
            }
            ColumnPaneCommand::ResetLayout => {
                self.show_column_details = true;
                self.show_date_column = true;
                self.show_kind_column = true;
                self.show_size_column = true;
                self.preferred_column_width = 320;
                true
            }
            _ => false,
        };
        if layout_changed {
            if let Err(error) = self.save_layout_preferences() {
                self.notice = Some(error.into());
            }
            self.column_scroll_pending = true;
            cx.notify();
            return;
        }

        let Some(index) = self
            .column_panes
            .iter()
            .position(|pane| same_path(&pane.path, &action.path))
        else {
            return;
        };
        let (path, rule) = {
            let Some(pane) = self.column_panes.get_mut(index) else {
                return;
            };

            match action.command {
                ColumnPaneCommand::SortNameAscending => {
                    pane.sort = SortSpec {
                        key: crate::fs::SortKey::Name,
                        ascending: true,
                    }
                }
                ColumnPaneCommand::SortNameDescending => {
                    pane.sort = SortSpec {
                        key: crate::fs::SortKey::Name,
                        ascending: false,
                    }
                }
                ColumnPaneCommand::SortModifiedNewest => {
                    pane.sort = SortSpec {
                        key: crate::fs::SortKey::Modified,
                        ascending: false,
                    }
                }
                ColumnPaneCommand::SortModifiedOldest => {
                    pane.sort = SortSpec {
                        key: crate::fs::SortKey::Modified,
                        ascending: true,
                    }
                }
                ColumnPaneCommand::SortKind => {
                    pane.sort = SortSpec {
                        key: crate::fs::SortKey::Kind,
                        ascending: true,
                    }
                }
                ColumnPaneCommand::SortSizeDescending => {
                    pane.sort = SortSpec {
                        key: crate::fs::SortKey::Size,
                        ascending: false,
                    }
                }
                ColumnPaneCommand::FilterAll => pane.filter = ColumnFilter::All,
                ColumnPaneCommand::FilterFolders => pane.filter = ColumnFilter::Folders,
                ColumnPaneCommand::FilterFiles => pane.filter = ColumnFilter::Files,
                ColumnPaneCommand::FilterMedia => pane.filter = ColumnFilter::Media,
                ColumnPaneCommand::FilterDocuments => pane.filter = ColumnFilter::Documents,
                ColumnPaneCommand::GroupNone => {
                    if pane.group != ColumnGroup::None {
                        pane.group = ColumnGroup::None;
                        pane.collapsed_groups.clear();
                    }
                }
                ColumnPaneCommand::GroupKind => {
                    if pane.group != ColumnGroup::Kind {
                        pane.group = ColumnGroup::Kind;
                        pane.collapsed_groups.clear();
                    }
                }
                ColumnPaneCommand::GroupExtension => {
                    if pane.group != ColumnGroup::Extension {
                        pane.group = ColumnGroup::Extension;
                        pane.collapsed_groups.clear();
                    }
                }
                ColumnPaneCommand::GroupModified => {
                    if pane.group != ColumnGroup::Modified {
                        pane.group = ColumnGroup::Modified;
                        pane.collapsed_groups.clear();
                    }
                }
                ColumnPaneCommand::ResetView => {
                    pane.sort = SortSpec::default();
                    pane.filter = ColumnFilter::All;
                    pane.group = ColumnGroup::None;
                    pane.collapsed_groups.clear();
                }
                ColumnPaneCommand::ToggleDateColumn
                | ColumnPaneCommand::ToggleKindColumn
                | ColumnPaneCommand::ToggleSizeColumn
                | ColumnPaneCommand::SetCompactColumnWidth
                | ColumnPaneCommand::SetStandardColumnWidth
                | ColumnPaneCommand::SetWideColumnWidth
                | ColumnPaneCommand::ResetLayout => return,
            }
            pane.refresh_view();
            (pane.path.clone(), pane.view_rule())
        };
        if let Err(error) = ui_preferences::save_column_view_rule(&path, rule) {
            self.notice = Some(error.into());
        }
        self.clear_hidden_column_selection();
        self.column_scroll_pending = true;
        cx.notify();
    }

    fn toggle_column_group(&mut self, column: usize, label: SharedString, cx: &mut Context<Self>) {
        let (path, rule) = {
            let Some(pane) = self.column_panes.get_mut(column) else {
                return;
            };
            pane.toggle_group_collapse(label.as_ref());
            (pane.path.clone(), pane.view_rule())
        };
        if let Err(error) = ui_preferences::save_column_view_rule(&path, rule) {
            self.notice = Some(error.into());
        }
        self.clear_hidden_column_selection();
        self.column_scroll_pending = true;
        cx.notify();
    }

    fn invalidate_vcs(&mut self) {
        self.vcs_generation = self.vcs_generation.wrapping_add(1);
        self._vcs_task = Task::ready(());
    }

    fn refresh_vcs_statuses(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        self.vcs_generation = self.vcs_generation.wrapping_add(1);
        if shell_fs::is_shell_namespace_path(&root) {
            self._vcs_task = Task::ready(());
            return;
        }
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
                    apply_statuses(&mut pane.directory_entries, &root, &snapshot);
                    if let Some(search_entries) = pane.search_entries.as_mut() {
                        apply_statuses(search_entries, &root, &snapshot);
                    }
                    pane.git_available = snapshot.git_available();
                    pane.svn_available = snapshot.svn_available();
                    pane.refresh_view();
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
        let preferences = ui_preferences::load_ui_preferences();
        let rules = chain
            .iter()
            .map(|path| preferences.column_view_rule(path))
            .collect::<Vec<_>>();
        self.column_panes = chain
            .iter()
            .zip(rules.iter())
            .map(|(path, rule)| {
                ColumnPane::new(
                    path.clone(),
                    Vec::new(),
                    0,
                    true,
                    false,
                    false,
                    rule.clone(),
                )
            })
            .collect();
        self.column_selected_path = None;
        self.column_scroll_pending = true;
        self.path_bar_scroll_pending = true;
        cx.notify();

        let load_paths = chain.into_iter().zip(rules).collect::<Vec<_>>();
        self._column_task = cx.spawn(async move |this, cx| {
            let panes = cx
                .background_spawn(async move {
                    let mut panes = Vec::with_capacity(load_paths.len());
                    for (path, rule) in load_paths {
                        panes.push(load_column_pane(path, rule));
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
                    let child_path = panes[index + 1].path.clone();
                    panes[index].set_single_selection(child_path);
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
        let preferences = ui_preferences::load_ui_preferences();

        for (index, path) in chain.iter().enumerate().skip(common) {
            let ready_entries = (index == final_index).then(|| entries.take().unwrap_or_default());
            panes.push(ColumnPane::new(
                path.clone(),
                ready_entries.unwrap_or_default(),
                if index == final_index { skipped } else { 0 },
                index != final_index,
                git_available,
                svn_available,
                preferences.column_view_rule(path),
            ));
        }
        if let Some(entries) = entries
            && let Some(last) = panes.get_mut(final_index)
        {
            last.set_directory_entries(entries, skipped);
        }

        self.column_panes = panes;
        for index in 0..self.column_panes.len().saturating_sub(1) {
            let child_path = self.column_panes[index + 1].path.clone();
            self.column_panes[index].set_single_selection(child_path);
        }
        if let Some(last) = self.column_panes.last_mut() {
            last.clear_selection();
        }
        self.column_selected_path = None;
        self.column_scroll_pending = true;
        self.path_bar_scroll_pending = true;

        let load_paths = (common..final_index)
            .map(|index| {
                let path = chain[index].clone();
                let rule = preferences.column_view_rule(&path);
                (index, path, rule)
            })
            .collect::<Vec<_>>();
        if load_paths.is_empty() {
            self._column_task = Task::ready(());
        } else {
            self._column_task = cx.spawn(async move |this, cx| {
                let loaded = cx
                    .background_spawn(async move {
                        load_paths
                            .into_iter()
                            .map(|(index, path, rule)| (index, load_column_pane(path, rule)))
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
                        let child_path = this.column_panes[index + 1].path.clone();
                        this.column_panes[index].set_single_selection(child_path);
                    }
                    cx.notify();
                });
            });
        }
        cx.notify();
    }

    fn show_list_view(&mut self, _: &ShowListView, window: &mut Window, cx: &mut Context<Self>) {
        self.close_column_search(window, cx);
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
    fn start_search(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        let query = query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.close_column_search(window, cx);
        if shell_fs::is_shell_namespace_path(&self.current_dir) {
            self.notice = Some("移动设备内搜索暂不支持".into());
            self.error = None;
            cx.notify();
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
        self.search_visible = false;
        self.request_load(
            self.current_dir.clone(),
            NavigationIntent::Refresh,
            window,
            cx,
        );
    }

    fn focus_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.address_editing = true;
        self.address_input.update(cx, |input, cx| {
            input.select_all(window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_visible = true;
        self.search_input.update(cx, |input, cx| {
            input.select_all(window, cx);
            input.focus(window, cx);
        });
        cx.notify();
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
        self.open_path(entry.path, window, cx);
    }

    fn open_preview_window(
        &mut self,
        entry: FileEntry,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor_target = text_editor::is_text_path(&entry.path)
            && entry.size <= text_editor::MAX_TEXT_FILE_BYTES;
        let title = if editor_target {
            format!("Kite 编辑器 — {}", entry.name)
        } else {
            format!("Kite 预览 — {}", entry.name)
        };
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(title.into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::centered(size(px(980.), px(760.)), cx)),
            window_min_size: Some(size(px(640.), px(480.))),
            ..Default::default()
        };
        match gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| PreviewWindow::new(entry, window, cx))
        }) {
            Ok(_) => {
                self.error = None;
            }
            Err(error) => {
                self.notice = None;
                self.error = Some(format!("无法打开预览窗口：{error}").into());
            }
        }
        cx.notify();
    }

    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let shell_path = shell_fs::is_shell_namespace_path(&path);
        if path.is_dir() || (shell_path && shell_fs::is_directory(&path)) {
            self.navigate_to(path, NavigationIntent::Push, window, cx);
            return;
        }
        self.error = None;
        self.notice = Some("正在打开文件…".into());
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    if shell_path {
                        shell_fs::open_item(&path)
                    } else {
                        open_file(&path)
                    }
                })
                .await;
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

    fn selected_entries(&self, cx: &App) -> Vec<FileEntry> {
        if self.view_mode == ViewMode::Columns && self.active_search.is_none() {
            let Some(selected) = self.column_selected_path.as_ref() else {
                return Vec::new();
            };
            let Some(pane) = self
                .column_panes
                .iter()
                .rev()
                .find(|pane| pane.is_selected(selected))
            else {
                return Vec::new();
            };
            return pane
                .entries
                .iter()
                .filter(|entry| pane.is_selected(&entry.path))
                .cloned()
                .collect();
        }

        self.table
            .read(cx)
            .selected_row()
            .and_then(|selected| self.table.read(cx).delegate().entry(selected).cloned())
            .into_iter()
            .collect()
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

        self.selected_entries(cx).into_iter().next()
    }

    fn open_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.selected_entry(cx) {
            self.open_entry(entry, window, cx);
        }
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        if self.view_mode == ViewMode::Columns && self.active_search.is_none() {
            let Some(last) = self.column_panes.len().checked_sub(1) else {
                return;
            };
            if let Some(pane) = self.column_panes.get_mut(last) {
                pane.select_all();
                self.column_selected_path = pane.selected_path.clone();
            }
            self.clear_column_preview();
            self.column_scroll_pending = true;
            cx.notify();
            return;
        }

        let has_entries = !self.table.read(cx).delegate().entries().is_empty();
        if has_entries {
            self.table
                .update(cx, |state, cx| state.set_selected_row(0, cx));
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
            ContextMenuCommand::CopyTo => self.copy_to_directory(action.target.clone(), window, cx),
            ContextMenuCommand::CopyPath => {
                cx.write_to_clipboard(ClipboardItem::new_string(path_text(&action.target)));
                self.error = None;
                self.notice = Some("已复制完整路径".into());
                cx.notify();
            }
            ContextMenuCommand::Cut => {
                self.set_clipboard_path(action.target.clone(), ClipboardMode::Cut, cx)
            }
            ContextMenuCommand::Paste => {
                let destination = paste_destination(&action.target, &action.working_directory);
                self.paste_clipboard_into(destination, window, cx)
            }
            ContextMenuCommand::NewFolder => {
                self.begin_new_folder_in(action.working_directory.clone(), window, cx)
            }
            ContextMenuCommand::NewFile => {
                self.begin_new_file_in(action.working_directory.clone(), window, cx)
            }
            ContextMenuCommand::AddFavorite => self.add_favorite(action.target.clone(), cx),
            ContextMenuCommand::RemoveFavorite => self.remove_favorite(action.target.clone(), cx),
            ContextMenuCommand::Rename => self.begin_rename_path(action.target.clone(), window, cx),
            ContextMenuCommand::Recycle => {
                self.confirm_recycle_path(action.target.clone(), window, cx)
            }
            ContextMenuCommand::ShowProperties => {
                match show_properties(window, &action.target) {
                    Ok(()) => {
                        self.error = None;
                        self.notice = Some("已打开属性".into());
                    }
                    Err(error) => {
                        self.notice = None;
                        self.error = Some(error.into());
                    }
                }
                cx.notify();
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
        let receiver = match request_system_menu(window, &path) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.notice = None;
                self.error = Some(error.into());
                cx.notify();
                return;
            }
        };
        self.notice = None;

        cx.spawn_in(window, async move |this, cx| {
            let result = receiver.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(true)) => this.refresh(window, cx),
                Ok(Ok(false)) => {}
                Ok(Err(error)) => {
                    this.notice = None;
                    this.error = Some(error.into());
                    cx.notify();
                }
                Err(error) => {
                    this.notice = None;
                    this.error = Some(format!("无法读取 Windows 原生菜单结果：{error}").into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn context_target_missing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = Some("项目已不存在，列表已刷新".into());
        self.refresh(window, cx);
    }

    fn check_for_updates(
        &mut self,
        _: &CheckForUpdates,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.update_in_progress {
            self.notice = Some("正在检查或下载 Kite 更新…".into());
            cx.notify();
            return;
        }
        self.update_in_progress = true;
        self.error = None;
        self.notice = Some("正在检查 Kite 更新…".into());
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async { crate::updater::check_for_update() })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.update_in_progress = false;
                match result {
                    Ok(update) => this.offer_update(update, window, cx),
                    Err(error) => {
                        this.notice = None;
                        this.error = Some(error.into());
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn offer_update(
        &mut self,
        update: crate::updater::UpdateInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !update.update_available {
            self.error = None;
            self.notice = Some(format!("Kite {} 已是最新版本", update.current_version).into());
            cx.notify();
            let message = "Kite 已是最新版本";
            let detail = format!(
                "当前版本：{}
最新版本：{}",
                update.current_version, update.latest_version
            );
            let answers = [PromptButton::ok("确定")];
            let answer = window.prompt(PromptLevel::Info, message, Some(&detail), &answers, cx);
            cx.spawn_in(window, async move |_, _| {
                let _ = answer.await;
            })
            .detach();
            return;
        }

        if let Err(error) = crate::updater::windows_update_target() {
            self.notice = None;
            self.error = Some(error.clone().into());
            cx.notify();
            let answers = [PromptButton::ok("确定")];
            let answer = window.prompt(
                PromptLevel::Warning,
                "无法自动更新 Kite",
                Some(&error),
                &answers,
                cx,
            );
            cx.spawn_in(window, async move |_, _| {
                let _ = answer.await;
            })
            .detach();
            return;
        }

        let message = format!("发现 Kite {}", update.latest_version);
        let notes = update_notes_summary(&update.release_notes);
        let detail = format!(
            "当前版本：{}

{notes}",
            update.current_version
        );
        let answers = [PromptButton::ok("下载并更新"), PromptButton::cancel("稍后")];
        let answer = window.prompt(PromptLevel::Info, &message, Some(&detail), &answers, cx);
        cx.spawn_in(window, async move |this, cx| {
            let Ok(choice) = answer.await else {
                return;
            };
            if choice != 0 {
                return;
            }
            let version = update.latest_version.clone();
            let initial_progress = crate::updater::UpdateDownloadProgress {
                downloaded_bytes: 0,
                total_bytes: update.asset_size,
                active: true,
            };
            let _ = this.update_in(cx, |this, _, cx| {
                this.update_in_progress = true;
                this.error = None;
                this.update_download_progress = Some(initial_progress);
                this.notice = Some(update_download_notice(&version, initial_progress).into());
                cx.notify();
            });

            let download_finished = Arc::new(AtomicBool::new(false));
            let observed_download_finished = download_finished.clone();
            let progress_version = update.latest_version.clone();
            let progress_entity = this.clone();
            let progress_task = cx.spawn(async move |cx| {
                loop {
                    let progress = crate::updater::download_progress();
                    if progress.active || progress.downloaded_bytes == 0 {
                        let _ = progress_entity.update(cx, |this, cx| {
                            this.update_download_progress = Some(progress);
                            this.notice =
                                Some(update_download_notice(&progress_version, progress).into());
                            cx.notify();
                        });
                    }
                    if observed_download_finished.load(Ordering::Relaxed) {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(120))
                        .await;
                }
            });

            let download_finished_for_task = download_finished.clone();
            let result = cx
                .background_spawn(async move {
                    let download_result = update.download();
                    download_finished_for_task.store(true, Ordering::Relaxed);
                    let downloaded = download_result?;
                    crate::updater::install_windows_update(
                        &downloaded,
                        &update.asset_sha256,
                        &update.latest_version,
                    )?;
                    Ok::<String, String>(update.latest_version)
                })
                .await;
            download_finished.store(true, Ordering::Relaxed);
            let _ = progress_task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.update_in_progress = false;
                this.update_download_progress = None;
                match result {
                    Ok(version) => {
                        this.error = None;
                        this.notice =
                            Some(format!("Kite {version} 已准备好，正在重启安装…").into());
                        cx.notify();
                        window.defer(cx, |_, cx| cx.quit());
                    }
                    Err(error) => {
                        this.notice = None;
                        this.error = Some(error.into());
                        cx.notify();
                    }
                }
            });
        })
        .detach();
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
        let can_paste = self.clipboard.borrow().is_some() || file_clipboard::has_files();
        self.table.update(cx, |state, cx| {
            state.delegate_mut().set_can_paste(can_paste);
            cx.notify();
        });
    }

    fn select_loaded_file(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        if self.view_mode == ViewMode::Columns && self.active_search.is_none() {
            let Some(column) = self.column_panes.len().checked_sub(1) else {
                return false;
            };
            let Some(row) = self.column_panes[column]
                .display_rows()
                .iter()
                .position(|row| {
                    matches!(
                        row,
                        ColumnDisplayRow::Entry(entry)
                            if !entry.is_dir() && same_path(&entry.path, path)
                    )
                })
            else {
                return false;
            };
            self.select_column_entry(column, row, cx);
            return true;
        }

        let Some(row) = self
            .table
            .read(cx)
            .delegate()
            .entries()
            .iter()
            .position(|entry| !entry.is_dir() && same_path(&entry.path, path))
        else {
            return false;
        };
        self.table
            .update(cx, |state, cx| state.set_selected_row(row, cx));
        true
    }

    fn select_column_entry(&mut self, column: usize, row: usize, cx: &mut Context<Self>) {
        self.select_column_entry_with_modifiers(column, row, "", cx);
    }

    fn select_column_entry_with_modifiers(
        &mut self,
        column: usize,
        row: usize,
        mode: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self
            .column_panes
            .get(column)
            .and_then(|pane| pane.entry_at_visible_row(row))
        else {
            return;
        };
        if let Some(pane) = self.column_panes.get_mut(column) {
            match mode {
                "toggle" => pane.toggle_selection(entry.path.clone()),
                "range" => pane.select_range_to(entry.path.clone()),
                _ => pane.set_single_selection(entry.path.clone()),
            }
        }
        let selected_count = self
            .column_panes
            .get(column)
            .map_or(0, |pane| pane.selected_paths.len());
        self.column_selected_path = self
            .column_panes
            .get(column)
            .and_then(|pane| pane.selected_path.clone());
        if selected_count != 1 {
            self.clear_column_preview();
            self.path_bar_scroll_pending = true;
            cx.notify();
            return;
        }
        let Some(entry) = self.column_panes.get(column).and_then(|pane| {
            pane.selected_path.as_ref().and_then(|selected| {
                pane.entries
                    .iter()
                    .find(|entry| same_path(&entry.path, selected))
                    .cloned()
            })
        }) else {
            return;
        };
        if !entry.is_dir() {
            self.column_panes
                .truncate(column_count_after_file_selection(
                    column,
                    self.column_panes.len(),
                ));
            if self.show_preview {
                self.start_column_preview(entry, cx);
            } else {
                self.clear_column_preview();
            }
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
        let log_preview = is_log_preview_entry(&entry);
        let markdown_view = (load_text && is_markdown_preview_entry(&entry)).then(|| {
            cx.new(|cx| {
                TextViewState::markdown("正在加载预览…", cx)
                    .selectable(true)
                    .scrollable(false)
            })
        });
        self._preview_task = Task::ready(());
        self.column_preview = Some(ColumnPreviewState {
            path: path.clone(),
            lines: Arc::new(Vec::new()),
            selection: None,
            markdown_view,
            error: None,
            loading: load_text,
            scroll: UniformListScrollHandle::new(),
            markdown_scroll: ScrollHandle::new(),
        });
        if !load_text {
            return;
        }

        self._preview_task = cx.spawn(async move |this, cx| {
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
                if this.preview_generation != generation {
                    return;
                }
                let markdown_view = this
                    .column_preview
                    .as_ref()
                    .and_then(|preview| preview.markdown_view.clone());
                if let Some(preview) = this.column_preview.as_mut() {
                    preview.loading = false;
                    match result {
                        Ok(text) => {
                            if let Some(markdown_view) = markdown_view {
                                markdown_view.update(cx, |view, cx| view.set_text(&text, cx));
                            } else {
                                preview.lines = Arc::new(preview_text_lines(&text));
                                preview.selection = Some(TextSelectionHandle::new(text, cx));
                            }
                        }
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
            .and_then(|pane| pane.entry_at_visible_row(row))
        else {
            return;
        };
        let modifiers = event.modifiers();
        let selection_mode = if modifiers.shift {
            "range"
        } else if modifiers.secondary() {
            "toggle"
        } else {
            ""
        };
        if !selection_mode.is_empty() {
            self.select_column_entry_with_modifiers(column, row, selection_mode, cx);
            return;
        }
        if entry.is_dir() {
            self.column_selected_path = None;
            if let Some(pane) = self.column_panes.get_mut(column) {
                pane.clear_selection();
            }
            self.navigate_to(entry.path, NavigationIntent::Push, window, cx);
            return;
        }
        self.select_column_entry(column, row, cx);
        if event.click_count() == 2 {
            self.open_entry(entry, window, cx);
        }
    }

    fn set_clipboard(&mut self, mode: ClipboardMode, cx: &mut Context<Self>) {
        let paths = self
            .selected_entries(cx)
            .into_iter()
            .map(|entry| entry.path)
            .collect::<Vec<_>>();
        self.set_clipboard_paths(paths, mode, cx);
    }

    fn set_clipboard_path(&mut self, path: PathBuf, mode: ClipboardMode, cx: &mut Context<Self>) {
        self.set_clipboard_paths(vec![path], mode, cx);
    }

    fn set_clipboard_paths(
        &mut self,
        paths: Vec<PathBuf>,
        mode: ClipboardMode,
        cx: &mut Context<Self>,
    ) {
        if paths.is_empty() {
            return;
        }
        if mode == ClipboardMode::Cut
            && paths.iter().any(|path| {
                shell_fs::is_shell_namespace_path(path) && !shell_fs::can_move_item(path)
            })
        {
            self.error = None;
            self.notice = Some("此设备项目不支持剪切".into());
            cx.notify();
            return;
        }
        let path_count = paths.len();
        if paths
            .iter()
            .any(|path| shell_fs::is_shell_namespace_path(path))
        {
            self.error = None;
            self.notice = Some("移动设备项目只在 Kite 内部剪贴板中保留".into());
            *self.clipboard.borrow_mut() = Some(FileClipboard {
                paths: paths.clone(),
                mode,
            });
            self.sync_clipboard_state(cx);
            cx.notify();
            return;
        }
        let system_error = file_clipboard::write_file_clipboard(&paths, mode).err();
        *self.clipboard.borrow_mut() = Some(FileClipboard { paths, mode });
        self.sync_clipboard_state(cx);
        self.error = system_error
            .map(|error| format!("系统文件剪贴板不可用，复制仅在 Kite 内有效：{error}").into());
        self.notice = Some(if mode == ClipboardMode::Cut {
            format!("已剪切 {path_count} 项").into()
        } else {
            format!("已复制 {path_count} 项").into()
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

    fn copy_to_directory(&mut self, source: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("拷贝到这里".into()),
        });
        self.error = None;
        self.notice = Some("请选择目标文件夹…".into());
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let selected = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    let _ = this.update_in(cx, |this, _, cx| {
                        this.notice = None;
                        this.error = Some(format!("无法打开目标文件夹选择器：{error}").into());
                        cx.notify();
                    });
                    return;
                }
                Err(_) => None,
            };
            let Some(destination_dir) = selected else {
                let _ = this.update_in(cx, |this, _, cx| {
                    this.notice = None;
                    cx.notify();
                });
                return;
            };

            let copy_source = source.clone();
            let copy_destination = destination_dir.clone();
            let uses_shell_operation = shell_fs::is_shell_namespace_path(&copy_source)
                || shell_fs::is_shell_namespace_path(&copy_destination);
            let result = cx
                .background_spawn(async move {
                    if uses_shell_operation {
                        shell_fs::transfer_items(
                            std::slice::from_ref(&copy_source),
                            &copy_destination,
                            false,
                        )
                        .map(|()| copy_destination.clone())
                    } else {
                        transfer_entry(&copy_source, &copy_destination, false)
                    }
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(copied) => {
                    this.error = None;
                    this.notice = Some(
                        format!(
                            "已拷贝到 {}",
                            copied.parent().unwrap_or(&destination_dir).display()
                        )
                        .into(),
                    );
                    if same_path(&this.current_dir, &destination_dir) {
                        this.refresh(window, cx);
                    } else {
                        cx.notify();
                    }
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

    fn drag_payload_for_entry(&self, column: usize, entry: &FileEntry) -> KiteFileDrag {
        let Some(pane) = self.column_panes.get(column) else {
            return KiteFileDrag {
                paths: vec![(entry.path.clone(), entry.is_dir())],
                label: entry.name.clone().into(),
            };
        };
        let use_selection = pane.selected_paths.len() > 1
            && pane
                .selected_paths
                .iter()
                .any(|selected| same_path(selected, &entry.path));
        let entries = if use_selection {
            pane.entries
                .iter()
                .filter(|candidate| pane.is_selected(&candidate.path))
                .collect::<Vec<_>>()
        } else {
            vec![entry]
        };
        let paths = entries
            .iter()
            .map(|candidate| (candidate.path.clone(), candidate.is_dir()))
            .collect::<Vec<_>>();
        let label = if entries.len() == 1 {
            entries[0].name.clone().into()
        } else {
            SharedString::from(format!("{} 个项目", entries.len()))
        };
        KiteFileDrag { paths, label }
    }

    fn transfer_dropped_paths(
        &mut self,
        sources: Vec<PathBuf>,
        destination_dir: PathBuf,
        move_source: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut sources = sources;
        sources.sort();
        sources.dedup();
        if sources.is_empty() {
            return;
        }
        if shell_fs::is_shell_namespace_path(&destination_dir)
            || sources
                .iter()
                .any(|source| shell_fs::is_shell_namespace_path(source))
        {
            self.error = None;
            self.notice = Some("移动设备项目拖放暂不支持".into());
            cx.notify();
            return;
        }

        self.error = None;
        self.notice = Some(if move_source {
            format!("正在移动 {} 项…", sources.len()).into()
        } else {
            format!("正在复制 {} 项…", sources.len()).into()
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let mut destinations = Vec::with_capacity(sources.len());
                    for source in &sources {
                        destinations.push(transfer_entry(source, &destination_dir, move_source)?);
                    }
                    Ok::<_, String>(destinations)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(destinations) => {
                    this.error = None;
                    this.notice = Some(if move_source {
                        format!("已移动 {} 项", destinations.len()).into()
                    } else {
                        format!("已复制 {} 项", destinations.len()).into()
                    });
                    this.refresh(window, cx);
                }
                Err(error) => {
                    this.notice = None;
                    this.error = Some(error.into());
                    this.refresh(window, cx);
                }
            });
        })
        .detach();
    }

    fn paste_clipboard_into(
        &mut self,
        destination_dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let internal_clipboard = self.clipboard.borrow().clone();
        let clipboard = if internal_clipboard.as_ref().is_some_and(|clipboard| {
            clipboard
                .paths
                .iter()
                .any(|path| shell_fs::is_shell_namespace_path(path))
        }) {
            internal_clipboard
        } else {
            file_clipboard::read_file_clipboard().or(internal_clipboard)
        };
        let Some(clipboard) = clipboard else {
            return;
        };
        let sources = clipboard.paths;
        let move_source = clipboard.mode == ClipboardMode::Cut;
        if sources.is_empty() {
            return;
        }
        let uses_shell_operation = sources
            .iter()
            .any(|source| shell_fs::is_shell_namespace_path(source))
            || shell_fs::is_shell_namespace_path(&destination_dir);
        let cleared_sources = sources.clone();
        self.error = None;
        self.notice = Some(if move_source {
            "正在移动项目…".into()
        } else {
            "正在复制项目…".into()
        });
        let operation_destination = destination_dir.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    if uses_shell_operation {
                        return match shell_fs::transfer_items(
                            &sources,
                            &operation_destination,
                            move_source,
                        ) {
                            Ok(()) => Ok(sources.len()),
                            Err(error) => Err((0usize, error)),
                        };
                    }
                    for (completed, source) in sources.iter().enumerate() {
                        if let Err(error) =
                            transfer_entry(source, &operation_destination, move_source)
                        {
                            return Err((completed, error));
                        }
                    }
                    Ok(sources.len())
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(item_count) => {
                    if move_source {
                        *this.clipboard.borrow_mut() = None;
                        file_clipboard::clear_if_matches(&cleared_sources);
                        this.sync_clipboard_state(cx);
                    }
                    this.notice = Some(if move_source {
                        format!("已移动 {item_count} 项").into()
                    } else {
                        format!("已粘贴 {item_count} 项").into()
                    });
                    this.refresh(window, cx);
                }
                Err((completed, error)) => {
                    if uses_shell_operation {
                        this.error = None;
                        this.notice = Some(format!("设备写入未完成：{error}").into());
                        if same_path(&this.current_dir, &destination_dir) {
                            this.refresh(window, cx);
                        } else {
                            cx.notify();
                        }
                    } else if completed > 0 {
                        if move_source {
                            let remaining = cleared_sources
                                .iter()
                                .skip(completed)
                                .cloned()
                                .collect::<Vec<_>>();
                            if remaining.is_empty() {
                                *this.clipboard.borrow_mut() = None;
                                file_clipboard::clear_if_matches(&cleared_sources);
                            } else {
                                let _ = file_clipboard::write_file_clipboard(
                                    &remaining,
                                    ClipboardMode::Cut,
                                );
                                *this.clipboard.borrow_mut() = Some(FileClipboard {
                                    paths: remaining,
                                    mode: ClipboardMode::Cut,
                                });
                            }
                            this.sync_clipboard_state(cx);
                        }
                        this.error = None;
                        this.notice =
                            Some(format!("已处理 {completed} 项，后续操作失败：{error}").into());
                        if same_path(&this.current_dir, &destination_dir) {
                            this.refresh(window, cx);
                        } else {
                            cx.notify();
                        }
                    } else {
                        this.notice = None;
                        this.error = Some(error.into());
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn add_favorite(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if shell_fs::is_shell_namespace_path(&path) {
            self.error = None;
            self.notice = Some("设备项目不能添加为收藏".into());
            cx.notify();
            return;
        }
        let name = favorite_name(&path);
        let is_directory = path.is_dir();
        match favorites_store::add_favorite(&path, &name, is_directory) {
            Ok(true) => {
                self.favorites = favorites_store::load_favorites();
                self.error = None;
                self.notice = Some("已添加到收藏".into());
            }
            Ok(false) => {
                self.error = None;
                self.notice = Some("已在收藏中".into());
            }
            Err(error) => {
                self.notice = None;
                self.error = Some(error.into());
            }
        }
        cx.notify();
    }

    fn toggle_current_favorite(&mut self, cx: &mut Context<Self>) {
        if self.current_dir_is_favorite() {
            self.remove_favorite(self.current_dir.clone(), cx);
        } else {
            self.add_favorite(self.current_dir.clone(), cx);
        }
    }

    fn current_dir_is_favorite(&self) -> bool {
        self.favorites
            .iter()
            .any(|favorite| same_path(Path::new(&favorite.path), &self.current_dir))
    }

    fn remove_favorite(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match favorites_store::remove_favorite(&path) {
            Ok(true) => {
                self.favorites = favorites_store::load_favorites();
                self.error = None;
                self.notice = Some("已从收藏移除".into());
            }
            Ok(false) => {
                self.error = None;
                self.notice = Some("项目不在收藏中".into());
            }
            Err(error) => {
                self.notice = None;
                self.error = Some(error.into());
            }
        }
        cx.notify();
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

    fn begin_new_file_in(
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
                .placeholder("文件名，例如 notes.txt")
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
            kind: ModalKind::NewFile(directory),
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
        let is_shell_path = shell_fs::is_shell_namespace_path(&path);
        if is_shell_path && !shell_fs::can_rename_item(&path) {
            self.error = None;
            self.notice = Some("此设备项目不支持重命名".into());
            cx.notify();
            return;
        }
        let name = if is_shell_path {
            shell_fs::display_name(&path)
        } else {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        };
        let Some(name) = name else {
            self.error = Some("无法重命名驱动器根目录".into());
            cx.notify();
            return;
        };
        if !is_shell_path && !path.exists() {
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
        if let Some(modal) = self.modal.take() {
            modal
                .input
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
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
            ModalKind::NewFolder(_) | ModalKind::NewFile(_) | ModalKind::Rename(_) => {
                validate_name(&value)
            }
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
                let uses_shell_operation = shell_fs::is_shell_namespace_path(&parent);
                self.notice = Some("正在新建文件夹…".into());
                cx.spawn_in(window, async move |this, cx| {
                    let result = cx
                        .background_spawn(async move {
                            if uses_shell_operation {
                                shell_fs::create_item_in(&parent, &name, true)
                            } else {
                                create_folder(&parent, &name).map(|_| ())
                            }
                        })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(_) => {
                            this.close_modal(window, cx);
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
            ModalKind::NewFile(parent) => {
                let name = value;
                let uses_shell_operation = shell_fs::is_shell_namespace_path(&parent);
                self.notice = Some("正在新建文件…".into());
                cx.spawn_in(window, async move |this, cx| {
                    let result = cx
                        .background_spawn(async move {
                            if uses_shell_operation {
                                shell_fs::create_item_in(&parent, &name, false)
                            } else {
                                create_file(&parent, &name).map(|_| ())
                            }
                        })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(_) => {
                            this.close_modal(window, cx);
                            this.notice = Some("文件已创建".into());
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
                let uses_shell_operation = shell_fs::is_shell_namespace_path(&path);
                self.notice = Some("正在重命名…".into());
                cx.spawn_in(window, async move |this, cx| {
                    let result = cx
                        .background_spawn(async move {
                            if uses_shell_operation {
                                shell_fs::rename_item(&rename_source, &name)
                            } else {
                                rename_entry(&rename_source, &name).map(|_| ())
                            }
                        })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(_) => {
                            this.close_modal(window, cx);
                            let destination =
                                path_after_rename(&this.current_dir, &path, &renamed_path);
                            this.navigate_to(destination, NavigationIntent::Refresh, window, cx);
                            this.notice = Some("名称已更新".into());
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
                self.close_modal(window, cx);
                self.run_vcs_task(VcsTool::Git, VcsOperation::Commit(value), path, window, cx);
            }
            ModalKind::SvnCommit(path) => {
                self.close_modal(window, cx);
                self.run_vcs_task(VcsTool::Svn, VcsOperation::Commit(value), path, window, cx);
            }
        }
    }

    fn confirm_recycle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = self
            .selected_entries(cx)
            .into_iter()
            .map(|entry| entry.path)
            .collect::<Vec<_>>();
        self.confirm_recycle_paths(paths, window, cx);
    }

    fn confirm_recycle_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.confirm_recycle_paths(vec![path], window, cx);
    }

    fn confirm_recycle_paths(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if paths.is_empty() {
            return;
        }
        let has_shell_path = paths
            .iter()
            .any(|path| shell_fs::is_shell_namespace_path(path));
        if paths
            .iter()
            .any(|path| shell_fs::is_shell_namespace_path(path) && !shell_fs::can_delete_item(path))
        {
            self.error = None;
            self.notice = Some("此设备项目不支持删除".into());
            cx.notify();
            return;
        }
        let missing_local_path = paths
            .iter()
            .any(|path| !shell_fs::is_shell_namespace_path(path) && !path.exists());
        if missing_local_path {
            self.context_target_missing(window, cx);
            return;
        }
        let destination = path_after_recycle_paths(&self.current_dir, &paths);
        let message = if paths.len() == 1 {
            let path = &paths[0];
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path_text(path));
            if has_shell_path {
                format!("删除“{name}”？设备项目不会进入回收站。")
            } else {
                format!("将“{name}”移到回收站？")
            }
        } else if has_shell_path {
            format!(
                "删除所选的 {} 个项目？设备项目不会进入回收站。",
                paths.len()
            )
        } else {
            format!("将所选的 {} 个项目移到回收站？", paths.len())
        };
        let detail = paths
            .iter()
            .take(5)
            .map(|path| path_text(path))
            .collect::<Vec<_>>()
            .join("\n");
        let action = if has_shell_path {
            "删除"
        } else {
            "移到回收站"
        };
        let answers = [PromptButton::ok(action), PromptButton::cancel("取消")];
        let answer = window.prompt(PromptLevel::Warning, &message, Some(&detail), &answers, cx);
        cx.spawn_in(window, async move |this, cx| {
            let Ok(choice) = answer.await else {
                return;
            };
            if choice != 0 {
                return;
            }
            let result = cx
                .background_spawn(async move {
                    for (completed, path) in paths.iter().enumerate() {
                        let result = if shell_fs::is_shell_namespace_path(path) {
                            shell_fs::delete_item(path)
                        } else {
                            recycle_entry(path)
                        };
                        if let Err(error) = result {
                            return Err((completed, error));
                        }
                    }
                    Ok(paths.len())
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(item_count) => {
                    this.navigate_to(destination, NavigationIntent::Refresh, window, cx);
                    this.notice = Some(if has_shell_path {
                        format!("已删除 {item_count} 项").into()
                    } else {
                        format!("已将 {item_count} 项移到回收站").into()
                    });
                }
                Err((completed, error)) => {
                    if has_shell_path || completed > 0 {
                        this.error = None;
                        this.navigate_to(destination, NavigationIntent::Refresh, window, cx);
                        this.notice = Some(if completed > 0 {
                            format!("已处理 {completed} 项，后续操作失败：{error}").into()
                        } else {
                            format!("设备删除未完成：{error}").into()
                        });
                    } else {
                        this.notice = None;
                        this.error = Some(error.into());
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn clear_search_or_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.address_editing {
            self.address_editing = false;
            self.address_input.update(cx, |input, cx| {
                input.set_value(path_text(&self.current_dir), window, cx);
            });
            self.focus_table(window, cx);
            cx.notify();
        } else if self.modal.is_some() {
            self.close_modal(window, cx);
        } else if self.column_search_path.is_some() {
            self.close_column_search(window, cx);
        } else if self.active_search.is_some() {
            self.clear_search(window, cx);
        } else if self.search_visible {
            self.search_visible = false;
            self.search_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
            self.focus_table(window, cx);
            cx.notify();
        }
    }

    fn render_toolbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let can_back = self.history_index > 0;
        let can_forward = self.history_index + 1 < self.history.len();
        let can_up = navigation_parent(&self.current_dir).is_some();
        let selected_count = self.selected_entries(cx).len();
        let has_selection = selected_count > 0;
        let can_modify_current = !self.current_dir.as_os_str().is_empty();
        let has_clipboard = self.clipboard.borrow().is_some() || file_clipboard::has_files();
        let current_favorite = self.current_dir_is_favorite();
        let loading = self.loading;
        let search_expanded = self.search_visible
            || self.active_search.is_some()
            || !self.search_input.read(cx).value().is_empty();
        let breadcrumbs =
            (!self.address_editing && self.show_path_bar).then(|| self.render_path_bar(cx));

        let navigation_group = h_flex()
            .h(px(TOOLBAR_CONTROL_HEIGHT))
            .gap_0p5()
            .px_1()
            .items_center()
            .rounded_md()
            .bg(cx.theme().background.opacity(0.42))
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
                Button::new("refresh")
                    .ghost()
                    .compact()
                    .icon(IconName::RotateCw)
                    .tooltip("刷新 (Ctrl+R)")
                    .on_click(cx.listener(Self::refresh_clicked)),
            );

        let navigation_slot = h_flex()
            .w(px(SIDEBAR_WIDTH))
            .h(px(TOOLBAR_CONTROL_HEIGHT))
            .flex_shrink_0()
            .items_center()
            .child(navigation_group);

        let actions = h_flex()
            .h(px(TOOLBAR_CONTROL_HEIGHT))
            .gap_0p5()
            .px_1()
            .items_center()
            .rounded_md()
            .bg(cx.theme().background.opacity(0.42))
            .child(
                Button::new("favorite-current-directory")
                    .ghost()
                    .compact()
                    .icon(if current_favorite {
                        IconName::StarMinus
                    } else {
                        IconName::StarPlus
                    })
                    .tooltip(if current_favorite {
                        "取消收藏当前文件夹"
                    } else {
                        "收藏当前文件夹"
                    })
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.toggle_current_favorite(cx)
                    })),
            )
            .child(
                Button::new("copy")
                    .ghost()
                    .compact()
                    .icon(IconName::Copy)
                    .tooltip(if selected_count > 1 {
                        "复制所选项目 (Ctrl+C)"
                    } else {
                        "复制 (Ctrl+C)"
                    })
                    .disabled(!has_selection)
                    .on_click(cx.listener(Self::copy_clicked)),
            )
            .child(
                Button::new("cut")
                    .ghost()
                    .compact()
                    .icon(IconName::Scissors)
                    .tooltip("剪切 (Ctrl+X)")
                    .disabled(!has_selection)
                    .on_click(cx.listener(Self::cut_clicked)),
            )
            .child(
                Button::new("paste")
                    .ghost()
                    .compact()
                    .icon(IconName::ClipboardPaste)
                    .tooltip("粘贴 (Ctrl+V)")
                    .disabled(!has_clipboard || !can_modify_current)
                    .on_click(cx.listener(Self::paste_clicked)),
            )
            .child(
                Button::new("new-folder")
                    .ghost()
                    .compact()
                    .icon(IconName::FolderPlus)
                    .tooltip("新建文件夹 (Ctrl+Shift+N)")
                    .disabled(loading || !can_modify_current)
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.begin_new_folder(window, cx)
                    })),
            )
            .when(!search_expanded, |this| {
                this.child(
                    Button::new("show-search")
                        .ghost()
                        .compact()
                        .icon(IconName::Search)
                        .tooltip("搜索当前文件夹 (Ctrl+F)")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.focus_search(window, cx)
                        })),
                )
            });

        let address_control = if let Some(breadcrumbs) = breadcrumbs {
            h_flex()
                .flex_1()
                .min_w(px(180.))
                .h(px(TOOLBAR_CONTROL_HEIGHT))
                .gap_1()
                .px_1()
                .items_center()
                .rounded_md()
                .bg(cx.theme().background.opacity(0.42))
                .child(
                    Icon::new(IconName::Folder)
                        .size_4()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(div().flex_1().min_w_0().child(breadcrumbs))
                .child(
                    Button::new("edit-address")
                        .ghost()
                        .compact()
                        .icon(IconName::Pencil)
                        .tooltip("编辑路径 (Ctrl+L)")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.focus_address(window, cx)
                        })),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w(px(180.))
                .child(
                    Input::new(&self.address_input)
                        .aria_label("地址")
                        .prefix(Icon::new(IconName::Folder).size_4()),
                )
                .into_any_element()
        };

        let search_input = search_expanded.then(|| {
            Input::new(&self.search_input)
                .aria_label("搜索")
                .prefix(Icon::new(IconName::Search).size_4())
                .cleanable(true)
                .w(px(220.))
                .into_any_element()
        });

        let toolbar = v_flex()
            .w_full()
            .flex_shrink_0()
            .gap_1()
            .py(px(6.))
            .rounded_lg()
            .bg(cx.theme().title_bar)
            .border_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .h(px(TOOLBAR_CONTROL_HEIGHT))
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(navigation_slot)
                    .child(address_control)
                    .when_some(search_input, |this, input| this.child(input))
                    .child(actions),
            );
        toolbar.into_any_element()
    }

    fn toggle_theme(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let mode = if cx.theme().mode.is_dark() {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        Theme::change(mode, Some(window), cx);
    }
    fn sidebar_item(&self, item: SidebarItem, cx: &mut Context<Self>) -> AnyElement {
        let accent = cx.theme().accent;
        let active_fg = cx.theme().accent_foreground;
        let fg = cx.theme().sidebar_foreground;
        let muted = cx.theme().muted_foreground;
        let row_id = item.id.clone();
        let navigation_path = item.path.clone();
        let is_directory = item.is_directory;
        let is_favorite = item.favorite;
        let context = self.focus_handle.clone();
        let open_action = ContextMenuAction {
            target: item.path.clone(),
            working_directory: item.path.clone(),
            command: ContextMenuCommand::Open,
        };
        let copy_path_action = ContextMenuAction {
            target: item.path.clone(),
            working_directory: item.path.clone(),
            command: ContextMenuCommand::CopyPath,
        };
        let properties_action = ContextMenuAction {
            target: item.path.clone(),
            working_directory: item.path.clone(),
            command: ContextMenuCommand::ShowProperties,
        };
        let remove_action = is_favorite.then(|| ContextMenuAction {
            target: item.path.clone(),
            working_directory: item.path.clone(),
            command: ContextMenuCommand::RemoveFavorite,
        });
        let row = div()
            .id(row_id)
            .h(px(30.))
            .w_full()
            .flex()
            .items_center()
            .gap_1()
            .px_1()
            .rounded_md()
            .cursor_pointer()
            .text_sm()
            .text_color(if item.active { active_fg } else { fg })
            .when(item.active, |this| this.bg(accent))
            .when(!item.active, |this| {
                this.hover(move |style| style.bg(muted.opacity(0.12)))
            })
            .child(Icon::new(item.icon).size_4().text_color(if item.active {
                active_fg
            } else {
                muted
            }))
            .child(div().flex_1().min_w_0().truncate().child(item.label))
            .when_some(item.detail, |this, detail| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(if item.active { active_fg } else { muted })
                        .child(detail),
                )
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if is_directory {
                    this.navigate_to(navigation_path.clone(), NavigationIntent::Push, window, cx);
                } else {
                    this.open_path(navigation_path.clone(), window, cx);
                }
            }))
            .context_menu(move |mut menu, window, cx| {
                context.focus(window, cx);
                menu = menu.menu_with_icon(
                    "打开",
                    IconName::FolderOpen,
                    Box::new(open_action.clone()),
                );
                menu = menu.menu_with_icon(
                    "复制完整路径",
                    IconName::Link,
                    Box::new(copy_path_action.clone()),
                );
                menu = menu.separator();
                menu = menu.menu_with_icon(
                    "属性…",
                    IconName::Info,
                    Box::new(properties_action.clone()),
                );
                if let Some(remove_action) = remove_action.as_ref() {
                    menu = menu.separator();
                    menu = menu.menu_with_icon(
                        "从收藏移除",
                        IconName::StarMinus,
                        Box::new(remove_action.clone()),
                    );
                }
                menu
            });

        v_flex().w_full().gap_0p5().child(row).into_any_element()
    }

    fn favorite_item(
        &self,
        favorite: FavoriteEntryDto,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = PathBuf::from(&favorite.path);
        self.sidebar_item(
            SidebarItem {
                id: SharedString::from(format!("favorite-{}", favorite.path)),
                label: favorite.name.into(),
                path,
                icon: if favorite.is_directory {
                    IconName::Folder
                } else {
                    IconName::File
                },
                detail: None,
                active,
                is_directory: favorite.is_directory,
                favorite: true,
            },
            cx,
        )
    }

    fn portable_device_item(&self, device: PortableDevice, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let hover_color = muted.opacity(0.12);
        let clicked = device.clone();
        let path = PathBuf::from(&device.shell_path);
        let open_action = ContextMenuAction {
            target: path.clone(),
            working_directory: path.clone(),
            command: ContextMenuCommand::Open,
        };
        let properties_action = ContextMenuAction {
            target: path,
            working_directory: PathBuf::from(&device.shell_path),
            command: ContextMenuCommand::ShowProperties,
        };
        let context = self.focus_handle.clone();
        let active = same_path(&self.current_dir, Path::new(&device.shell_path));
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
            .text_color(if active {
                cx.theme().accent_foreground
            } else {
                cx.theme().sidebar_foreground
            })
            .when(active, |this| this.bg(cx.theme().accent))
            .child(Icon::new(IconName::Usb).size_4().text_color(if active {
                cx.theme().accent_foreground
            } else {
                muted
            }))
            .child(div().flex_1().min_w_0().truncate().child(device.name))
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child("MTP"),
            )
            .child(Icon::new(IconName::ChevronRight).size_3())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_portable_device(clicked.clone(), window, cx);
            }))
            .context_menu(move |mut menu, window, cx| {
                context.focus(window, cx);
                menu = menu.menu_with_icon(
                    "打开",
                    IconName::FolderOpen,
                    Box::new(open_action.clone()),
                );
                menu = menu.separator();
                menu = menu.menu_with_icon(
                    "属性…",
                    IconName::Info,
                    Box::new(properties_action.clone()),
                );
                menu
            })
            .into_any_element()
    }

    fn open_portable_device(
        &mut self,
        device: PortableDevice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_to(
            PathBuf::from(device.shell_path),
            NavigationIntent::Push,
            window,
            cx,
        );
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
                SidebarItem {
                    id: SharedString::from(format!("quick-{}", link.path.display())),
                    label: link.label,
                    path: link.path,
                    icon: link.icon,
                    detail: link.detail,
                    active,
                    is_directory: true,
                    favorite: false,
                },
                cx,
            ));
        }
        content = content.child(
            div()
                .px_2()
                .pt_4()
                .pb_1()
                .text_xs()
                .font_medium()
                .text_color(section_color)
                .child("收藏"),
        );
        if self.favorites.is_empty() {
            content = content.child(
                div()
                    .px_2()
                    .pb_2()
                    .text_xs()
                    .text_color(section_color)
                    .child("点击工具栏星标或右键菜单添加"),
            );
        } else {
            for favorite in self.favorites.clone() {
                let active = favorite.is_directory
                    && same_path(Path::new(&favorite.path), &self.current_dir);
                content = content.child(self.favorite_item(favorite, active, cx));
            }
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
                    SidebarItem {
                        id: SharedString::from(format!("device-{}", device.path.display())),
                        label: device.label,
                        path: device.path,
                        icon: device.icon,
                        detail: device.detail,
                        active,
                        is_directory: true,
                        favorite: false,
                    },
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
                SidebarItem {
                    id: SharedString::from(format!("drive-{}", drive.path.display())),
                    label: drive.label,
                    path: drive.path,
                    icon: drive.icon,
                    detail: drive.detail,
                    active,
                    is_directory: true,
                    favorite: false,
                },
                cx,
            ));
        }

        let settings = DropdownButton::new("sidebar-settings")
            .with_size(Size::Small)
            .menu_accessibility_label("Kite 设置")
            .button(
                Button::new("sidebar-settings-action")
                    .ghost()
                    .compact()
                    .label("设置")
                    .tooltip("Kite 设置"),
            )
            .dropdown_menu(|menu, _, _| {
                menu.menu("检查更新…", Box::new(CheckForUpdates))
                    .separator()
                    .menu("设为默认资源管理器…", Box::new(SetDefaultExplorer))
                    .separator()
                    .menu(
                        "恢复之前的文件夹打开方式…",
                        Box::new(RestoreDefaultExplorer),
                    )
                    .separator()
                    .menu("创建开始菜单快捷方式…", Box::new(CreateStartMenuShortcut))
            });
        let theme_icon = if cx.theme().mode.is_dark() {
            IconName::Sun
        } else {
            IconName::Moon
        };
        let theme_tooltip = if cx.theme().mode.is_dark() {
            "切换到浅色主题"
        } else {
            "切换到深色主题"
        };

        v_flex()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex_shrink_0()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(cx.theme().sidebar)
            .border_1()
            .border_color(cx.theme().border)
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(content),
            )
            .child(
                h_flex()
                    .h(px(38.))
                    .w_full()
                    .px_1()
                    .gap_1()
                    .items_center()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(div().flex_1().child(settings))
                    .child(
                        Button::new("sidebar-theme")
                            .ghost()
                            .compact()
                            .icon(theme_icon)
                            .tooltip(theme_tooltip)
                            .on_click(cx.listener(Self::toggle_theme)),
                    ),
            )
            .into_any_element()
    }
    fn render_path_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let segments = path_segments(&self.current_dir, false);
        let scroll_handle = self.path_bar_scroll.clone();
        let mut bar = h_flex()
            .id("path-bar")
            .h(px(30.))
            .w_full()
            .gap_1()
            .items_center()
            .overflow_x_scroll()
            .track_scroll(&scroll_handle);
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
            .gap_2()
            .p_2()
            .overflow_hidden()
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
        if self.show_preview
            && let Some(entry) = self.selected_last_column_file()
        {
            strip = strip.child(self.render_column_preview(&entry, cx));
        }
        let horizontal_wheel_handle = scroll_handle.clone();
        div()
            .relative()
            .size_full()
            .child(strip.into_any_element())
            .child(
                ScrollableMask::new(Axis::Horizontal, &horizontal_wheel_handle)
                    .id("column-horizontal-wheel"),
            )
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
        let preview_more_items = preview_more_menu_items();
        let preview_more_target = entry.path.clone();
        let preview_more_working_directory = self.current_dir.clone();
        let preview_window_entry = entry.clone();
        let preview_more_supports_local_ops = !shell_fs::is_shell_namespace_path(&entry.path);
        let editor_preview = preview_more_supports_local_ops
            && text_editor::is_text_path(&entry.path)
            && entry.size <= text_editor::MAX_TEXT_FILE_BYTES;
        let preview = self
            .column_preview
            .as_ref()
            .filter(|preview| same_path(&preview.path, &entry.path))
            .cloned();
        let preview_area = if is_image_preview_entry(entry) {
            let fallback_entry = entry.clone();
            h_flex()
                .h(px(440.))
                .w_full()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .rounded_md()
                .bg(cx.theme().muted.opacity(0.25))
                .child(
                    img(entry.path.clone())
                        .max_w_full()
                        .max_h(px(520.))
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
            let foreground = cx.theme().foreground;
            let mut scrollbar: Option<PreviewScrollbar> = None;
            let mut content = v_flex().size_full().p_2();
            content = if let Some(preview) = preview.as_ref() {
                if preview.loading {
                    content.child(div().text_xs().text_color(muted).child("正在加载预览…"))
                } else if let Some(error) = preview.error.clone() {
                    content.child(div().text_xs().text_color(cx.theme().danger).child(
                        SelectableText::new(
                            format!("column-preview-error-{}", entry.path.display()),
                            error,
                        ),
                    ))
                } else if let Some(markdown_view) = preview.markdown_view.clone() {
                    let scroll = preview.markdown_scroll.clone();
                    let scroll_for_wheel = scroll.clone();
                    scrollbar = Some(PreviewScrollbar::Markdown(scroll.clone()));
                    content
                        .text_color(foreground)
                        .on_scroll_wheel(cx.listener(
                            move |_this, event: &ScrollWheelEvent, window, cx| {
                                let delta = event.delta.pixel_delta(window.line_height());
                                let delta_x = delta.x.as_f32();
                                let delta_y = delta.y.as_f32();
                                if delta_y == 0.0 {
                                    return;
                                }
                                if wheel_scrolls_columns(delta_x, delta_y) {
                                    let current = scroll_for_wheel.offset();
                                    let max_y = scroll_for_wheel.max_offset().y.as_f32().max(0.0);
                                    let applied_y =
                                        if delta.y.is_zero() { delta_x } else { delta_y };
                                    let restored_y =
                                        (current.y.as_f32() - applied_y).clamp(-max_y, 0.0);
                                    scroll_for_wheel.set_offset(point(current.x, px(restored_y)));
                                    cx.notify();
                                    return;
                                }
                                cx.stop_propagation();
                            },
                        ))
                        .child(
                            div()
                                .id("column-preview-markdown-scroll")
                                .size_full()
                                .overflow_y_scroll()
                                .track_scroll(&scroll)
                                .child(
                                    TextView::new(&markdown_view)
                                        .w_full()
                                        .text_sm()
                                        .text_color(foreground)
                                        .selectable(true)
                                        .scrollable(false),
                                ),
                        )
                } else if let Some(selection) = preview.selection.clone() {
                    let lines = preview.lines.clone();
                    let line_count = lines.len();
                    let scroll = preview.scroll.clone();
                    let scroll_for_wheel = scroll.clone();
                    scrollbar = Some(PreviewScrollbar::Lines(scroll.clone()));
                    let list_id = SharedString::from(format!(
                        "column-preview-lines-{}",
                        entry.path.display()
                    ));
                    content
                        .on_scroll_wheel(cx.listener(
                            move |_this, event: &ScrollWheelEvent, window, cx| {
                                let delta = event.delta.pixel_delta(window.line_height());
                                let delta_x = delta.x.as_f32();
                                let delta_y = delta.y.as_f32();
                                if delta_y == 0.0 {
                                    return;
                                }
                                if wheel_scrolls_columns(delta_x, delta_y) {
                                    let base = scroll_for_wheel.0.borrow().base_handle.clone();
                                    let current = base.offset();
                                    let max_y = base.max_offset().y.as_f32().max(0.0);
                                    let applied_y =
                                        if delta.y.is_zero() { delta_x } else { delta_y };
                                    let restored_y =
                                        (current.y.as_f32() - applied_y).clamp(-max_y, 0.0);
                                    base.set_offset(point(current.x, px(restored_y)));
                                    cx.notify();
                                    return;
                                }
                                cx.stop_propagation();
                            },
                        ))
                        .child(
                            uniform_list(
                                list_id,
                                line_count,
                                cx.processor(move |_this, range: Range<usize>, _window, _cx| {
                                    range
                                        .map(|row_ix| {
                                            let line =
                                                lines.get(row_ix).cloned().unwrap_or_default();
                                            h_flex()
                                                .id(("column-preview-line", row_ix))
                                                .h(px(20.))
                                                .w_full()
                                                .min_w_0()
                                                .items_center()
                                                .overflow_hidden()
                                                .px_1()
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(foreground)
                                                        .whitespace_nowrap()
                                                        .child(
                                                            SelectableText::with_handle(
                                                                (
                                                                    "column-preview-line-text",
                                                                    row_ix,
                                                                ),
                                                                selection.clone(),
                                                                line,
                                                            )
                                                            .document_order(row_ix as u64),
                                                        ),
                                                )
                                                .into_any_element()
                                        })
                                        .collect::<Vec<_>>()
                                }),
                            )
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .track_scroll(&scroll),
                        )
                } else {
                    content.child(div().text_xs().text_color(muted).child("无文本预览"))
                }
            } else {
                content.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("选择文件后加载预览"),
                )
            };

            let mut panel = v_flex()
                .id("column-preview-text")
                .h(px(440.))
                .w_full()
                .rounded_md()
                .bg(cx.theme().muted.opacity(0.25))
                .relative()
                .overflow_hidden()
                .child(content);
            if let Some(scroll) = scrollbar {
                let scrollbar = match scroll {
                    PreviewScrollbar::Lines(handle) => preview_scrollbar_element(&handle, cx),
                    PreviewScrollbar::Markdown(handle) => preview_scrollbar_element(&handle, cx),
                };
                panel = panel.child(div().absolute().inset_0().child(scrollbar));
            }
            panel.into_any_element()
        } else {
            v_flex()
                .h(px(440.))
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
            .w(px(316.))
            .h_full()
            .flex_shrink_0()
            .rounded_lg()
            .overflow_hidden()
            .bg(cx.theme().title_bar)
            .border_1()
            .border_color(border)
            .child(
                h_flex()
                    .h(px(34.))
                    .w_full()
                    .px_2()
                    .gap_1()
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
                        Button::new(format!("preview-enlarge-{}", entry.path.display()))
                            .ghost()
                            .compact()
                            .icon(if editor_preview {
                                IconName::FilePen
                            } else {
                                IconName::Maximize2
                            })
                            .tooltip(if editor_preview {
                                "在文本编辑器中打开"
                            } else {
                                "在窗口中预览"
                            })
                            .accessibility_label(if editor_preview {
                                "在文本编辑器中打开"
                            } else {
                                "在窗口中预览"
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.open_preview_window(preview_window_entry.clone(), window, cx);
                            })),
                    )
                    .child(
                        Clipboard::new(format!("copy-preview-info-{}", entry.path.display()))
                            .value(copy_text)
                            .tooltip("复制预览信息")
                            .accessibility_label("复制预览信息"),
                    )
                    .child(
                        Button::new(format!("preview-more-{}", entry.path.display()))
                            .ghost()
                            .compact()
                            .icon(IconName::Ellipsis)
                            .tooltip("更多预览选项")
                            .accessibility_label("更多预览选项")
                            .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
                                for (index, (label, icon, command)) in
                                    preview_more_items.iter().enumerate()
                                {
                                    if matches!(index, 3 | 4) {
                                        menu = menu.separator();
                                    }
                                    menu = menu.menu_with_icon_and_disabled(
                                        *label,
                                        *icon,
                                        Box::new(ContextMenuAction {
                                            target: preview_more_target.clone(),
                                            working_directory: preview_more_working_directory
                                                .clone(),
                                            command: *command,
                                        }),
                                        !preview_more_supports_local_ops
                                            && *command == ContextMenuCommand::Copy,
                                    );
                                }
                                menu
                            }),
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(pane) = self.column_panes.get(index) else {
            return div().into_any_element();
        };
        let path = pane.path.clone();
        let loading = pane.loading;
        let entry_count = pane.entries.len();
        let row_count = pane.visible_row_count();
        let rules = ColumnViewRules {
            sort: pane.sort,
            filter: pane.filter,
            group: pane.group,
        };
        let search_query = pane.search_query.clone();
        let search_truncated = pane.search_truncated;
        let vertical_scroll = self
            .column_vertical_scrolls
            .get(index)
            .map(|state| state.handle.clone())
            .unwrap_or_default();
        let search_open = self
            .column_search_path
            .as_ref()
            .is_some_and(|search_path| same_path(search_path, &path));
        let search_loading = search_open && self.column_search_loading;
        let active = same_path(&path, &self.current_dir);
        let details_visible =
            active && self.show_column_details && index + 1 == self.column_panes.len();
        let layout = ColumnLayoutState {
            show_date_column: self.show_date_column,
            show_kind_column: self.show_kind_column,
            show_size_column: self.show_size_column,
            preferred_width: self.preferred_column_width,
        };
        let column_width = if details_visible {
            detailed_column_width(
                &pane.entries,
                window,
                layout.show_date_column,
                layout.show_kind_column,
                layout.show_size_column,
                layout.preferred_width,
            )
        } else {
            COLUMN_WIDTH
        };
        let title = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let can_paste = self.clipboard.borrow().is_some() || file_clipboard::has_files();
        let column_menu_target = column_context_menu_target(pane, can_paste);
        let action_context = self.focus_handle.clone();
        let menu_path = path.clone();
        let search_path = path.clone();
        let close_search_path = path.clone();
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().accent;
        let empty_label = if search_query.is_some() {
            "无搜索结果"
        } else if rules.filter != ColumnFilter::All {
            "筛选后无项目"
        } else {
            "空文件夹"
        };
        let body = if loading {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(muted)
                .child("正在读取…")
                .into_any_element()
        } else if entry_count == 0 {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(muted)
                .child(empty_label)
                .into_any_element()
        } else {
            uniform_list(
                SharedString::from(format!("column-list-{index}-{}", path.display())),
                row_count,
                cx.processor(move |this, range, window, cx| {
                    this.render_column_rows(index, range, details_visible, layout, window, cx)
                }),
            )
            .flex_1()
            .min_h_0()
            .w_full()
            .track_scroll(&vertical_scroll)
            .into_any_element()
        };
        let mut header = h_flex()
            .h(px(36.))
            .w_full()
            .px_2()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(border)
            .when(active, |this| this.bg(accent.opacity(0.5)))
            .child(Icon::new(IconName::Folder).size_4().text_color(if active {
                accent
            } else {
                muted
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .font_medium()
                    .text_color(if active { cx.theme().foreground } else { muted })
                    .child(title),
            )
            .when(search_loading, |this| {
                this.child(div().text_xs().text_color(muted).child("搜索中…"))
            })
            .when(search_truncated, |this| {
                this.child(div().text_xs().text_color(muted).child("1000+"))
            })
            .child(
                div()
                    .px_1()
                    .py_0p5()
                    .rounded_md()
                    .bg(muted.opacity(0.14))
                    .text_xs()
                    .text_color(muted)
                    .child(entry_count.to_string()),
            )
            .child(
                Button::new(("column-sort-order", index))
                    .ghost()
                    .compact()
                    .icon(if rules.sort.ascending {
                        IconName::SortAscending
                    } else {
                        IconName::SortDescending
                    })
                    .tooltip(if rules.sort.ascending {
                        "当前升序，点击改为降序"
                    } else {
                        "当前降序，点击改为升序"
                    })
                    .accessibility_label(if rules.sort.ascending {
                        "切换为降序排序"
                    } else {
                        "切换为升序排序"
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.toggle_column_sort_order(index, cx)
                    })),
            );
        if active {
            header = header.child(
                Button::new(("toggle-column-details", index))
                    .ghost()
                    .compact()
                    .icon(IconName::Grid2x2)
                    .tooltip(if details_visible {
                        "隐藏详细信息"
                    } else {
                        "显示详细信息"
                    })
                    .accessibility_label(if details_visible {
                        "隐藏详细信息"
                    } else {
                        "显示详细信息"
                    })
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_column_details(cx)),
                    ),
            );
        }
        header = header
            .child(
                Button::new(("column-search", index))
                    .ghost()
                    .compact()
                    .icon(IconName::Search)
                    .tooltip("搜索此分栏")
                    .accessibility_label("搜索此分栏")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.toggle_column_search(search_path.clone(), window, cx)
                    })),
            )
            .child(
                Button::new(("column-more", index))
                    .ghost()
                    .compact()
                    .icon(IconName::Ellipsis)
                    .tooltip("分栏显示规则")
                    .accessibility_label("分栏显示规则")
                    .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| {
                        build_column_control_menu(menu, &menu_path, rules, layout, window, cx)
                    }),
            );

        let details_header = details_visible.then(|| {
            let sort_key = rules.sort.key;
            let sort_ascending = rules.sort.ascending;
            let mut header = h_flex()
                .h(px(24.))
                .w_full()
                .px_2()
                .gap_2()
                .items_center()
                .border_b_1()
                .border_color(border)
                .bg(muted.opacity(0.08))
                .text_xs()
                .font_medium()
                .text_color(muted)
                .child(
                    Button::new(("column-header-name", index))
                        .ghost()
                        .compact()
                        .h(px(22.))
                        .px_0()
                        .flex_1()
                        .min_w_0()
                        .toggled(sort_key == SortKey::Name)
                        .tooltip(column_header_tooltip(
                            "名称",
                            SortKey::Name,
                            sort_key,
                            sort_ascending,
                        ))
                        .accessibility_label(column_header_tooltip(
                            "名称",
                            SortKey::Name,
                            sort_key,
                            sort_ascending,
                        ))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.sort_column_from_header(index, SortKey::Name, cx);
                        }))
                        .child(
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .pl(px(24.))
                                .gap_1()
                                .when(sort_key == SortKey::Name, |this| {
                                    this.child(column_sort_header_icon(sort_ascending))
                                })
                                .child("名称"),
                        ),
                );
            if layout.show_date_column {
                header = header.child(
                    Button::new(("column-header-modified", index))
                        .ghost()
                        .compact()
                        .h(px(22.))
                        .px_0()
                        .w(px(DETAILED_DATE_WIDTH))
                        .toggled(sort_key == SortKey::Modified)
                        .tooltip(column_header_tooltip(
                            "日期",
                            SortKey::Modified,
                            sort_key,
                            sort_ascending,
                        ))
                        .accessibility_label(column_header_tooltip(
                            "日期",
                            SortKey::Modified,
                            sort_key,
                            sort_ascending,
                        ))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.sort_column_from_header(index, SortKey::Modified, cx);
                        }))
                        .child(
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .gap_1()
                                .when(sort_key == SortKey::Modified, |this| {
                                    this.child(column_sort_header_icon(sort_ascending))
                                })
                                .child("日期"),
                        ),
                );
            }
            if layout.show_kind_column {
                header = header.child(
                    Button::new(("column-header-kind", index))
                        .ghost()
                        .compact()
                        .h(px(22.))
                        .px_0()
                        .w(px(DETAILED_KIND_WIDTH))
                        .toggled(sort_key == SortKey::Kind)
                        .tooltip(column_header_tooltip(
                            "类型",
                            SortKey::Kind,
                            sort_key,
                            sort_ascending,
                        ))
                        .accessibility_label(column_header_tooltip(
                            "类型",
                            SortKey::Kind,
                            sort_key,
                            sort_ascending,
                        ))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.sort_column_from_header(index, SortKey::Kind, cx);
                        }))
                        .child(
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .gap_1()
                                .when(sort_key == SortKey::Kind, |this| {
                                    this.child(column_sort_header_icon(sort_ascending))
                                })
                                .child("类型"),
                        ),
                );
            }
            if layout.show_size_column {
                header = header.child(
                    Button::new(("column-header-size", index))
                        .ghost()
                        .compact()
                        .h(px(22.))
                        .px_0()
                        .w(px(DETAILED_SIZE_WIDTH))
                        .toggled(sort_key == SortKey::Size)
                        .tooltip(column_header_tooltip(
                            "大小",
                            SortKey::Size,
                            sort_key,
                            sort_ascending,
                        ))
                        .accessibility_label(column_header_tooltip(
                            "大小",
                            SortKey::Size,
                            sort_key,
                            sort_ascending,
                        ))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.sort_column_from_header(index, SortKey::Size, cx);
                        }))
                        .child(
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .justify_end()
                                .gap_1()
                                .when(sort_key == SortKey::Size, |this| {
                                    this.child(column_sort_header_icon(sort_ascending))
                                })
                                .child("大小"),
                        ),
                );
            }
            header
        });

        let search_bar = search_open.then(|| {
            h_flex()
                .h(px(34.))
                .w_full()
                .px_2()
                .gap_1()
                .items_center()
                .bg(cx.theme().status_bar)
                .border_b_1()
                .border_color(border)
                .child(
                    Input::new(&self.column_search_input)
                        .aria_label("搜索当前分栏")
                        .prefix(Icon::new(IconName::Search).size_3())
                        .cleanable(true)
                        .flex_1(),
                )
                .child(
                    Button::new(("column-close-search", index))
                        .ghost()
                        .compact()
                        .icon(IconName::X)
                        .tooltip("关闭分栏搜索 (Esc)")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.column_search_path.as_ref().is_some_and(|active_path| {
                                same_path(active_path, &close_search_path)
                            }) {
                                this.close_column_search(window, cx);
                            }
                        })),
                )
                .into_any_element()
        });

        let mut column = v_flex()
            .id(("column", index))
            .w(px(column_width))
            .h_full()
            .flex_shrink_0()
            .rounded_lg()
            .overflow_hidden()
            .bg(cx.theme().title_bar)
            .border_1()
            .border_color(border)
            .child(header);
        if let Some(details_header) = details_header {
            column = column.child(details_header);
        }
        if let Some(search_bar) = search_bar {
            column = column.child(search_bar);
        }
        let internal_destination = path.clone();
        let external_destination = path.clone();
        column
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .on_scroll_wheel(cx.listener(
                        move |this, event: &ScrollWheelEvent, window, cx| {
                            this.handle_column_scroll_wheel(index, event, window, cx)
                        },
                    ))
                    .child(body),
            )
            .drag_over::<KiteFileDrag>(move |style, _, _, _| style.bg(accent.opacity(0.14)))
            .on_drop(cx.listener(move |this, drag: &KiteFileDrag, window, cx| {
                let sources = drag
                    .paths
                    .iter()
                    .map(|(path, _)| path.clone())
                    .collect::<Vec<_>>();
                this.transfer_dropped_paths(
                    sources,
                    internal_destination.clone(),
                    !window.modifiers().control,
                    window,
                    cx,
                );
            }))
            .drag_over::<ExternalPaths>(move |style, _, _, _| style.bg(accent.opacity(0.10)))
            .on_drop(cx.listener(move |this, paths: &ExternalPaths, window, cx| {
                this.transfer_dropped_paths(
                    paths.paths().to_vec(),
                    external_destination.clone(),
                    false,
                    window,
                    cx,
                );
            }))
            .context_menu(move |menu, window, cx| {
                render_context_menu(
                    menu,
                    &column_menu_target,
                    action_context.clone(),
                    window,
                    cx,
                )
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

        if let Some(horizontal_delta) =
            horizontal_wheel_delta(event.modifiers.shift, delta_x, delta_y)
        {
            // The list maps one-axis gestures to its vertical offset. Restore it
            // before the column strip receives this horizontal gesture.
            let applied_y = if delta.y.is_zero() { delta_x } else { delta_y };
            let max_y = base.max_offset().y.as_f32().max(0.0);
            let restored_y = (current.y.as_f32() - applied_y).clamp(-max_y, 0.0);
            base.set_offset(point(current.x, px(restored_y)));
            self.scroll_columns_by(horizontal_delta, cx);
            cx.notify();
            cx.stop_propagation();
            return;
        }

        if delta.y.is_zero() {
            return;
        }

        let max_y = base.max_offset().y.as_f32().max(0.0);
        let clamped_y = directory_scroll_offset(current.y.as_f32(), max_y);
        base.set_offset(point(current.x, px(clamped_y)));
        // The list handles wheel input before this wrapper during bubbling.
        // Keep vertical input inside the directory at both scroll boundaries.
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
        details_visible: bool,
        layout: ColumnLayoutState,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(pane) = self.column_panes.get(column) else {
            return Vec::new();
        };
        let display_rows = pane.display_rows();
        let column_path = pane.path.clone();
        let loading = pane.loading;
        let can_paste = self.clipboard.borrow().is_some() || file_clipboard::has_files();
        let git_available = pane.git_available;
        let svn_available = pane.svn_available;
        let accent = cx.theme().accent;
        let accent_foreground = cx.theme().accent_foreground;
        let hover = cx.theme().muted;
        let muted = cx.theme().muted_foreground;
        let action_context = self.focus_handle.clone();
        let mut rows = Vec::new();
        for row_ix in range {
            let Some(display_row) = display_rows.get(row_ix) else {
                continue;
            };
            let entry = match display_row {
                ColumnDisplayRow::Group {
                    label,
                    count,
                    collapsed,
                } => {
                    let click_label = label.clone();
                    rows.push(
                        h_flex()
                            .id(("column-group", column * 100_000 + row_ix))
                            .h(px(30.))
                            .w_full()
                            .gap_1()
                            .px_2()
                            .items_center()
                            .text_xs()
                            .font_medium()
                            .text_color(muted)
                            .bg(cx.theme().status_bar)
                            .cursor_pointer()
                            .child(
                                Icon::new(IconName::ChevronRight)
                                    .size_3()
                                    .rotate(percentage(if *collapsed { 0. } else { 0.25 })),
                            )
                            .child(div().flex_1().truncate().child(label.clone()))
                            .child(div().text_xs().child(count.to_string()))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.toggle_column_group(column, click_label.clone(), cx)
                            }))
                            .into_any_element(),
                    );
                    continue;
                }
                ColumnDisplayRow::Entry(entry) => entry.clone(),
            };
            let selected = pane.is_selected(&entry.path);
            let metadata_color = if selected { accent_foreground } else { muted };
            let modified_label = column_modified_label(&entry);
            let kind_label = entry.type_label();
            let size_label = column_size_label(&entry);
            let row_column_path = column_path.clone();
            let menu_entry = entry.clone();
            let row_action_context = action_context.clone();
            let row_git_available = git_available || entry.git_status.is_some();
            let row_svn_available = svn_available || entry.svn_status.is_some();
            let icon_status = worse_status(entry.git_status, entry.svn_status);
            let icon_status_is_svn = match (entry.git_status, entry.svn_status) {
                (Some(git), Some(svn)) => svn.rank() > git.rank(),
                (None, Some(_)) => true,
                _ => false,
            };
            let mut row = h_flex()
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
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis_middle()
                        .child(entry.name.clone()),
                );
            if entry.is_dir() {
                row = row.child(
                    Icon::new(IconName::ChevronRight)
                        .size_3()
                        .text_color(metadata_color),
                );
            }
            if details_visible {
                if layout.show_date_column {
                    row = row.child(
                        div()
                            .w(px(DETAILED_DATE_WIDTH))
                            .truncate()
                            .text_xs()
                            .text_color(metadata_color)
                            .child(modified_label),
                    );
                }
                if layout.show_kind_column {
                    row = row.child(
                        div()
                            .w(px(DETAILED_KIND_WIDTH))
                            .truncate()
                            .text_xs()
                            .text_color(metadata_color)
                            .child(kind_label),
                    );
                }
                if layout.show_size_column {
                    row = row.child(
                        div()
                            .w(px(DETAILED_SIZE_WIDTH))
                            .truncate()
                            .text_right()
                            .text_xs()
                            .text_color(metadata_color)
                            .child(size_label),
                    );
                }
            }
            let drag_payload = self.drag_payload_for_entry(column, &entry);
            let mut row = row
                .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    this.activate_column_entry(column, row_ix, event, window, cx)
                }))
                .on_drag(drag_payload, |drag: &KiteFileDrag, position, _, cx| {
                    file_clipboard::trace_drag(&format!(
                        "GPUI drag preview created at ({:.1},{:.1}) label={} paths={:?}",
                        position.x.as_f32(),
                        position.y.as_f32(),
                        drag.label,
                        drag.paths
                    ));
                    cx.new(|_| KiteFileDragPreview {
                        label: drag.label.clone(),
                        count: drag.paths.len(),
                        position,
                    })
                })
                .external_drag_payload::<KiteFileDrag>(|drag, _, _| {
                    Some(ExternalDragPayload::Files(FileDragPaths::new(
                        drag.paths.clone(),
                    )))
                });
            if entry.is_dir() {
                let internal_destination = entry.path.clone();
                let external_destination = entry.path.clone();
                row = row
                    .drag_over::<KiteFileDrag>(move |style, _, _, _| style.bg(accent.opacity(0.28)))
                    .on_drop(cx.listener(move |this, drag: &KiteFileDrag, window, cx| {
                        let sources = drag
                            .paths
                            .iter()
                            .map(|(path, _)| path.clone())
                            .collect::<Vec<_>>();
                        this.transfer_dropped_paths(
                            sources,
                            internal_destination.clone(),
                            !window.modifiers().control,
                            window,
                            cx,
                        );
                    }))
                    .drag_over::<ExternalPaths>(move |style, _, _, _| {
                        style.bg(accent.opacity(0.20))
                    })
                    .on_drop(cx.listener(move |this, paths: &ExternalPaths, window, cx| {
                        this.transfer_dropped_paths(
                            paths.paths().to_vec(),
                            external_destination.clone(),
                            false,
                            window,
                            cx,
                        );
                    }));
            }
            let row = row
                // Right-click must not select the row. Selection loads the next
                // column for folders and starts preview loading for files.
                .context_menu(move |menu, window, cx| {
                    let capabilities = ContextMenuCapabilities::for_path(
                        &menu_entry.path,
                        loading,
                        can_paste,
                        row_git_available,
                        row_svn_available,
                    );
                    let target = ContextMenuTarget::entry(
                        menu_entry.path.clone(),
                        menu_entry.name.clone(),
                        menu_entry.is_dir(),
                        row_column_path.clone(),
                        capabilities,
                    );
                    render_context_menu(menu, &target, row_action_context.clone(), window, cx)
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
            ModalKind::NewFile(_) => ("新建文件", "输入文件名。按 Enter 创建空文件。", "创建"),
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

fn favorite_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
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
            detail: volume.capacity_label().map(SharedString::from),
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

fn update_download_notice(
    version: &str,
    progress: crate::updater::UpdateDownloadProgress,
) -> String {
    let downloaded = format_size(progress.downloaded_bytes);
    let Some(fraction) = progress.fraction() else {
        return format!("正在下载 Kite {version}… {downloaded}");
    };
    let percent = (fraction * 100.0).round() as u32;
    let total = format_size(progress.total_bytes);
    format!("正在下载 Kite {version}… {percent}% · {downloaded} / {total}")
}

fn update_notes_summary(notes: &str) -> String {
    let normalized = notes
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(
            "
",
        );
    if normalized.is_empty() {
        return "GitHub Release 未提供更新说明。".to_string();
    }
    const LIMIT: usize = 1800;
    let mut chars = normalized.chars();
    let summary = chars.by_ref().take(LIMIT).collect::<String>();
    if chars.next().is_some() {
        format!("{summary}\n…")
    } else {
        summary
    }
}

fn load_directory_snapshot(path: &Path, sort: SortSpec) -> Result<DirectorySnapshot, String> {
    if shell_fs::is_shell_namespace_path(path) {
        shell_fs::load_directory(path, sort)
    } else {
        load_directory(path, sort)
    }
}

fn navigation_parent(path: &Path) -> Option<PathBuf> {
    if shell_fs::is_shell_namespace_path(path) {
        shell_fs::parent_path(path)
    } else {
        path.parent().map(Path::to_path_buf)
    }
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}

fn absolute_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    std::env::current_dir()
        .map(|current_dir| current_dir.join(&path))
        .unwrap_or(path)
}

fn same_path(left: &Path, right: &Path) -> bool {
    crate::platform::paths_equal(left, right)
}

fn paste_destination(target: &Path, working_directory: &Path) -> PathBuf {
    let target_is_directory = if shell_fs::is_shell_namespace_path(target) {
        shell_fs::is_directory(target)
    } else {
        target.is_dir()
    };
    if target_is_directory {
        target.to_path_buf()
    } else {
        working_directory.to_path_buf()
    }
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

fn path_after_recycle_paths(current_dir: &Path, removed: &[PathBuf]) -> PathBuf {
    removed
        .iter()
        .fold(current_dir.to_path_buf(), |current, path| {
            path_after_recycle(&current, path)
        })
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

fn horizontal_wheel_delta(shift: bool, delta_x: f32, delta_y: f32) -> Option<f32> {
    if wheel_scrolls_columns(delta_x, delta_y) {
        Some(delta_x)
    } else if shift && delta_y != 0.0 {
        Some(delta_y)
    } else {
        None
    }
}

fn detailed_column_width(
    entries: &[FileEntry],
    window: &Window,
    show_date_column: bool,
    show_kind_column: bool,
    show_size_column: bool,
    preferred_width: u16,
) -> f32 {
    let text_style = window.text_style();
    let text_system = window.text_system();
    let font_id = text_system.resolve_font(&text_style.font());
    let font_size = px(14.);
    let max_name_width = entries
        .iter()
        .map(|entry| {
            entry
                .name
                .chars()
                .map(|character| {
                    text_system
                        .layout_width(font_id, font_size, character)
                        .as_f32()
                })
                .sum::<f32>()
        })
        .fold(0.0, f32::max);
    detailed_column_width_for_name_width(
        max_name_width,
        show_date_column,
        show_kind_column,
        show_size_column,
        preferred_width,
    )
}

fn detailed_column_width_for_name_width(
    name_width: f32,
    show_date_column: bool,
    show_kind_column: bool,
    show_size_column: bool,
    preferred_width: u16,
) -> f32 {
    let visible_widths = [
        (show_date_column, DETAILED_DATE_WIDTH),
        (show_kind_column, DETAILED_KIND_WIDTH),
        (show_size_column, DETAILED_SIZE_WIDTH),
    ];
    let visible_count = visible_widths
        .iter()
        .filter(|(visible, _)| *visible)
        .count();
    let details_width = visible_widths
        .iter()
        .filter(|(visible, _)| *visible)
        .map(|(_, width)| *width)
        .sum::<f32>();
    let gaps = visible_count.saturating_sub(1) as f32 * 8.0;
    (DETAILED_COLUMN_NAME_OVERHEAD + details_width + gaps + name_width.ceil())
        .max(f32::from(preferred_width))
        .clamp(DETAILED_COLUMN_MIN_WIDTH, DETAILED_COLUMN_MAX_WIDTH)
}

fn default_column_header_sort_ascending(key: SortKey) -> bool {
    !matches!(key, SortKey::Modified | SortKey::Size)
}

fn column_header_tooltip(
    title: &str,
    key: SortKey,
    active_key: SortKey,
    ascending: bool,
) -> SharedString {
    if active_key == key {
        if ascending {
            format!("当前按{title}升序，点击改为降序")
        } else {
            format!("当前按{title}降序，点击改为升序")
        }
    } else {
        format!("按{title}排序")
    }
    .into()
}

fn column_sort_header_icon(ascending: bool) -> Icon {
    Icon::new(if ascending {
        IconName::SortAscending
    } else {
        IconName::SortDescending
    })
    .size_3()
}

fn column_modified_label(entry: &FileEntry) -> String {
    entry
        .modified_label()
        .split_whitespace()
        .next()
        .unwrap_or("未知")
        .to_string()
}

fn column_size_label(entry: &FileEntry) -> String {
    if entry.is_dir() {
        "—".to_string()
    } else {
        entry.size_label()
    }
}

fn directory_scroll_offset(offset_after_scroll: f32, max_offset: f32) -> f32 {
    let max_offset = max_offset.max(0.0);
    offset_after_scroll.clamp(-max_offset, 0.0)
}

fn resolve_location_input(value: &str, current_dir: &Path) -> Result<PathBuf, String> {
    let raw = value.trim().trim_matches('"');
    if raw.is_empty() {
        return Err("请输入路径".to_string());
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
    if shell_fs::is_shell_namespace_path(&path) {
        return Ok(path);
    }
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

fn preview_scrollbar_element<H>(handle: &H, cx: &App) -> AnyElement
where
    H: ScrollbarHandle + Clone + 'static,
{
    Scrollbar::vertical(handle)
        .id("column-preview-text-scrollbar")
        .mode(ScrollbarMode::Always)
        .styles(|styles| {
            let track = cx.theme().muted.opacity(0.55);
            let thumb = cx.theme().muted_foreground.opacity(0.75);
            let thumb_hover = cx.theme().muted_foreground.opacity(0.95);
            styles
                .track(|style| style.bg(track).width(px(10.)))
                .thumb(|style| style.bg(thumb).width(px(6.)).inset(px(2.)).radius(px(99.)))
                .thumb_hover(|style| style.bg(thumb_hover))
        })
        .viewport_from_layout()
        .into_any_element()
}

fn preview_more_menu_items() -> [(&'static str, IconName, ContextMenuCommand); 5] {
    [
        ("打开文件", IconName::FolderOpen, ContextMenuCommand::Open),
        ("复制", IconName::Copy, ContextMenuCommand::Copy),
        ("复制完整路径", IconName::Link, ContextMenuCommand::CopyPath),
        (
            "文件属性…",
            IconName::Info,
            ContextMenuCommand::ShowProperties,
        ),
        (
            "显示原生菜单…",
            IconName::Ellipsis,
            ContextMenuCommand::SystemMenu,
        ),
    ]
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

fn preview_text_lines(text: &str) -> Vec<SharedString> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).into())
        .collect()
}

pub(crate) fn is_image_preview_entry(entry: &FileEntry) -> bool {
    matches!(
        entry.extension.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg"
    )
}

pub(crate) fn is_log_preview_entry(entry: &FileEntry) -> bool {
    entry.extension.eq_ignore_ascii_case("log")
}

pub(crate) fn is_markdown_preview_entry(entry: &FileEntry) -> bool {
    matches!(
        entry.extension.to_ascii_lowercase().as_str(),
        "txt" | "md" | "markdown"
    )
}

pub(crate) fn is_text_preview_entry(entry: &FileEntry) -> bool {
    if entry.size > 512 * 1024 && !is_log_preview_entry(entry) {
        return false;
    }
    entry.extension.is_empty()
        || is_log_preview_entry(entry)
        || matches!(
            entry.extension.as_str(),
            "txt"
                | "md"
                | "markdown"
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

pub(crate) fn read_text_preview(path: &Path) -> Result<String, String> {
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

pub(crate) fn read_log_preview(path: &Path) -> Result<String, String> {
    const READ_LIMIT: u64 = 64 * 1024;
    const CHAR_LIMIT: usize = 6_000;

    let mut file =
        std::fs::File::open(path).map_err(|error| format!("无法读取日志预览：{error}"))?;
    let length = file
        .metadata()
        .map_err(|error| format!("无法读取日志信息：{error}"))?
        .len();
    let truncated = length > READ_LIMIT;
    if truncated {
        file.seek(SeekFrom::Start(length - READ_LIMIT))
            .map_err(|error| format!("无法定位日志末尾：{error}"))?;
    }

    let mut bytes = Vec::with_capacity(READ_LIMIT as usize);
    std::io::BufReader::new(file)
        .take(READ_LIMIT)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("无法读取日志预览：{error}"))?;
    if bytes.contains(&0) {
        return Err("此日志包含二进制内容，无法显示文本预览".to_string());
    }

    let text = String::from_utf8_lossy(&bytes);
    let text = if truncated {
        text.split_once('\n')
            .map_or_else(|| text.as_ref(), |(_, rest)| rest)
    } else {
        text.as_ref()
    };
    let chars = text.chars().collect::<Vec<_>>();
    let tail_start = chars.len().saturating_sub(CHAR_LIMIT);
    let excerpt = chars[tail_start..].iter().collect::<String>();
    if excerpt.trim().is_empty() {
        return Ok("日志没有可显示的文本内容。".to_string());
    }

    if truncated || tail_start > 0 {
        Ok(format!("日志末尾预览：\n{excerpt}"))
    } else {
        Ok(excerpt)
    }
}

fn column_chain(root: &Path) -> Vec<PathBuf> {
    if shell_fs::is_shell_namespace_path(root) {
        return vec![root.to_path_buf()];
    }
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

fn load_column_pane(path: PathBuf, rule: ColumnViewRule) -> ColumnPane {
    let (mut entries, skipped) = load_directory_snapshot(&path, rule.sort)
        .map(|snapshot| (snapshot.entries, snapshot.skipped))
        .unwrap_or_default();
    if shell_fs::is_shell_namespace_path(&path) {
        return ColumnPane::new(path, entries, skipped, false, false, false, rule);
    }
    let snapshot = load_vcs_snapshot(&path);
    apply_statuses(&mut entries, &path, &snapshot);
    ColumnPane::new(
        path,
        entries,
        skipped,
        false,
        snapshot.git_available(),
        snapshot.svn_available(),
        rule,
    )
}

fn column_pane_action(path: &Path, command: ColumnPaneCommand) -> Box<dyn Action> {
    Box::new(ColumnPaneAction {
        path: path.to_path_buf(),
        command,
    })
}

fn build_column_control_menu(
    menu: PopupMenu,
    path: &Path,
    rules: ColumnViewRules,
    layout: ColumnLayoutState,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let sort_path = path.to_path_buf();
    let filter_path = path.to_path_buf();
    let group_path = path.to_path_buf();
    let layout_path = path.to_path_buf();
    let reset_path = path.to_path_buf();
    let is_default = rules.sort == SortSpec::default()
        && rules.filter == ColumnFilter::All
        && rules.group == ColumnGroup::None;

    menu.min_w(px(250.))
        .submenu_with_icon(
            Some(Icon::new(IconName::ArrowUpDown).size_4()),
            "排序",
            window,
            cx,
            move |menu, _, _| {
                let action_path = sort_path.clone();
                menu.item(
                    PopupMenuItem::new("名称：升序")
                        .checked(rules.sort.key == SortKey::Name && rules.sort.ascending)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::SortNameAscending,
                        )),
                )
                .item(
                    PopupMenuItem::new("名称：降序")
                        .checked(rules.sort.key == SortKey::Name && !rules.sort.ascending)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::SortNameDescending,
                        )),
                )
                .separator()
                .item(
                    PopupMenuItem::new("修改日期：最新优先")
                        .checked(rules.sort.key == SortKey::Modified && !rules.sort.ascending)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::SortModifiedNewest,
                        )),
                )
                .item(
                    PopupMenuItem::new("修改日期：最旧优先")
                        .checked(rules.sort.key == SortKey::Modified && rules.sort.ascending)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::SortModifiedOldest,
                        )),
                )
                .item(
                    PopupMenuItem::new("类型")
                        .checked(rules.sort.key == SortKey::Kind)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::SortKind,
                        )),
                )
                .item(
                    PopupMenuItem::new("大小：从大到小")
                        .checked(rules.sort.key == SortKey::Size && !rules.sort.ascending)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::SortSizeDescending,
                        )),
                )
            },
        )
        .submenu_with_icon(
            Some(Icon::new(IconName::ListFilter).size_4()),
            "筛选",
            window,
            cx,
            move |menu, _, _| {
                let action_path = filter_path.clone();
                menu.item(
                    PopupMenuItem::new("全部项目")
                        .checked(rules.filter == ColumnFilter::All)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::FilterAll,
                        )),
                )
                .item(
                    PopupMenuItem::new("仅文件夹")
                        .checked(rules.filter == ColumnFilter::Folders)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::FilterFolders,
                        )),
                )
                .item(
                    PopupMenuItem::new("仅文件")
                        .checked(rules.filter == ColumnFilter::Files)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::FilterFiles,
                        )),
                )
                .item(
                    PopupMenuItem::new("媒体文件")
                        .checked(rules.filter == ColumnFilter::Media)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::FilterMedia,
                        )),
                )
                .item(
                    PopupMenuItem::new("文档文件")
                        .checked(rules.filter == ColumnFilter::Documents)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::FilterDocuments,
                        )),
                )
            },
        )
        .submenu_with_icon(
            Some(Icon::new(IconName::Group).size_4()),
            "分组规则",
            window,
            cx,
            move |menu, _, _| {
                let action_path = group_path.clone();
                menu.item(
                    PopupMenuItem::new("不分组")
                        .checked(rules.group == ColumnGroup::None)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::GroupNone,
                        )),
                )
                .item(
                    PopupMenuItem::new("按类型")
                        .checked(rules.group == ColumnGroup::Kind)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::GroupKind,
                        )),
                )
                .item(
                    PopupMenuItem::new("按扩展名")
                        .checked(rules.group == ColumnGroup::Extension)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::GroupExtension,
                        )),
                )
                .item(
                    PopupMenuItem::new("按修改日期")
                        .checked(rules.group == ColumnGroup::Modified)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::GroupModified,
                        )),
                )
            },
        )
        .submenu_with_icon(
            Some(Icon::new(IconName::Grid2x2).size_4()),
            "列与布局",
            window,
            cx,
            move |menu, _, _| {
                let action_path = layout_path.clone();
                menu.item(
                    PopupMenuItem::new("显示修改日期列")
                        .checked(layout.show_date_column)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::ToggleDateColumn,
                        )),
                )
                .item(
                    PopupMenuItem::new("显示类型列")
                        .checked(layout.show_kind_column)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::ToggleKindColumn,
                        )),
                )
                .item(
                    PopupMenuItem::new("显示大小列")
                        .checked(layout.show_size_column)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::ToggleSizeColumn,
                        )),
                )
                .separator()
                .item(
                    PopupMenuItem::new("紧凑宽度")
                        .checked(layout.preferred_width == 250)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::SetCompactColumnWidth,
                        )),
                )
                .item(
                    PopupMenuItem::new("标准宽度")
                        .checked(layout.preferred_width == 320)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::SetStandardColumnWidth,
                        )),
                )
                .item(
                    PopupMenuItem::new("宽列")
                        .checked(layout.preferred_width == 420)
                        .action(column_pane_action(
                            &action_path,
                            ColumnPaneCommand::SetWideColumnWidth,
                        )),
                )
                .separator()
                .item(
                    PopupMenuItem::new("重置列与布局").action(column_pane_action(
                        &action_path,
                        ColumnPaneCommand::ResetLayout,
                    )),
                )
            },
        )
        .separator()
        .menu_with_disabled(
            "重置分栏视图",
            column_pane_action(&reset_path, ColumnPaneCommand::ResetView),
            is_default,
        )
}

fn column_context_menu_target(pane: &ColumnPane, can_paste: bool) -> ContextMenuTarget {
    let title = if shell_fs::is_shell_namespace_path(&pane.path) {
        shell_fs::display_name(&pane.path)
    } else {
        None
    }
    .or_else(|| {
        pane.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    })
    .unwrap_or_else(|| pane.path.display().to_string());
    ContextMenuTarget::directory(
        pane.path.clone(),
        title,
        ContextMenuCapabilities::for_path(
            &pane.path,
            pane.loading,
            can_paste,
            pane.git_available,
            pane.svn_available,
        ),
    )
}

fn path_segments(path: &Path, final_is_file: bool) -> Vec<PathSegment> {
    if shell_fs::is_shell_namespace_path(path) {
        return vec![PathSegment {
            label: shell_fs::display_name(path)
                .unwrap_or_else(|| path.display().to_string())
                .into(),
            path: path.to_path_buf(),
            clickable: false,
        }];
    }
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

fn column_count_after_file_selection(column: usize, current_count: usize) -> usize {
    (column + 1).min(current_count)
}

impl Focusable for KiteApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<TabRequest> for KiteApp {}

impl Render for KiteApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_entries = self.selected_entries(cx);
        let selected_count = selected_entries.len();
        let selected = selected_entries.first().cloned();
        let selected_exists = selected.is_some();
        let column_mode = self.view_mode == ViewMode::Columns && self.active_search.is_none();
        let item_count = if column_mode {
            self.column_panes
                .last()
                .map(|pane| pane.entries.len())
                .unwrap_or(0)
        } else {
            self.table.read(cx).delegate().entries().len()
        };
        let selected_size = if selected_count == 1 {
            selected
                .as_ref()
                .map(FileEntry::size_label)
                .unwrap_or_default()
        } else if selected_entries.iter().any(|entry| !entry.is_dir()) {
            format_size(
                selected_entries
                    .iter()
                    .filter(|entry| !entry.is_dir())
                    .map(|entry| entry.size)
                    .sum(),
            )
        } else {
            String::new()
        };
        let error = self.error.clone();
        let notice = self.notice.clone();
        let update_download_fraction = self
            .update_download_progress
            .and_then(|progress| progress.fraction())
            .map(|fraction| fraction as f32);
        let loading = self.loading;
        let active_search = self.active_search.clone();
        let search_truncated = self.search_truncated;
        let skipped = self.skipped;
        if self.column_scroll_pending {
            self.column_scroll_pending = false;
            let handle = self.column_scroll.clone();
            window.defer(cx, move |_, _| {
                handle.set_offset(point(-handle.max_offset().x, handle.offset().y));
            });
        }
        if self.show_path_bar && !self.address_editing && self.path_bar_scroll_pending {
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
                format!("已选择 {selected_count} 项")
            } else {
                format!("已选择 {selected_count} 项 · {selected_size}")
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
            let menu_target = self
                .table
                .read(cx)
                .delegate()
                .directory_context_menu_target();
            let action_context = self.focus_handle.clone();
            div()
                .size_full()
                .context_menu(move |menu, window, cx| {
                    render_context_menu(menu, &menu_target, action_context.clone(), window, cx)
                })
                .child(
                    DataTable::new(&self.table)
                        .with_size(Size::Small)
                        .bordered(false)
                        .stripe(false)
                        .scroll_bounce(false)
                        .scrollbar_visible(false, true),
                )
                .into_any_element()
        };
        v_flex()
            .id("kite-root")
            .key_context("Kite")
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .gap_2()
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
            .on_action(cx.listener(Self::on_column_pane_action))
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
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| this.select_all(cx)))
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
            .on_action(cx.listener(Self::check_for_updates))
            .on_action(cx.listener(Self::set_default_explorer))
            .on_action(cx.listener(Self::restore_default_explorer))
            .on_action(cx.listener(Self::create_start_menu_shortcut))
            .on_drag_move::<KiteFileDrag>(|event, window, cx| {
                let position = event.event.position;
                file_clipboard::trace_drag(&format!(
                    "GPUI drag move at ({:.1},{:.1})",
                    position.x.as_f32(),
                    position.y.as_f32()
                ));
                let viewport = window.viewport_size();
                let outside = position.x < px(0.)
                    || position.y < px(0.)
                    || position.x > viewport.width
                    || position.y > viewport.height;
                if !outside {
                    return;
                }
                let paths = event
                    .drag(cx)
                    .paths
                    .iter()
                    .map(|(path, _)| path.clone())
                    .collect::<Vec<_>>();
                file_clipboard::trace_drag(&format!(
                    "GPUI drag left window at ({:.1},{:.1}) paths={:?}",
                    position.x.as_f32(),
                    position.y.as_f32(),
                    paths
                ));
                // SHDoDragDrop enters a nested Windows message loop. Post a native
                // window message so OLE starts after this GPUI input event returns.
                if let Err(error) = file_clipboard::request_file_drag(window, &paths) {
                    file_clipboard::trace_drag(&format!("failed to queue native drag: {error}"));
                    eprintln!("failed to queue native file drag: {error}");
                    return;
                }
                cx.stop_active_drag(window);
            })
            .child(self.render_toolbar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .items_stretch()
                    .gap_2()
                    .child(sidebar)
                    .child(
                        v_flex()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .min_h_0()
                            .rounded_lg()
                            .overflow_hidden()
                            .bg(cx.theme().title_bar)
                            .border_1()
                            .border_color(cx.theme().border)
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
            .child(
                h_flex()
                    .h(px(30.))
                    .w_full()
                    .flex_shrink_0()
                    .px_3()
                    .gap_3()
                    .items_center()
                    .justify_between()
                    .rounded_lg()
                    .bg(cx.theme().status_bar)
                    .border_1()
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
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .when_some(update_download_fraction, |this, fraction| {
                                this.child(
                                    div()
                                        .w(px(64.))
                                        .h(px(4.))
                                        .rounded_md()
                                        .overflow_hidden()
                                        .bg(cx.theme().border)
                                        .child(
                                            div()
                                                .h_full()
                                                .w(px((64. * fraction).max(2.)))
                                                .rounded_md()
                                                .bg(cx.theme().primary),
                                        ),
                                )
                            })
                            .child(div().max_w(px(500.)).truncate().child(footer_right)),
                    ),
            )
            .child(self.render_modal(cx))
    }
}

#[cfg(test)]
mod clipboard_destination_tests {
    use super::paste_destination;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn file_target_pastes_into_the_working_directory() {
        let destination =
            paste_destination(Path::new(r"C:\repo\src\main.rs"), Path::new(r"C:\repo\src"));
        assert_eq!(destination, PathBuf::from(r"C:\repo\src"));
    }

    #[test]
    fn directory_target_pastes_inside_the_directory() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "kite-paste-destination-{}-{unique}",
            std::process::id()
        ));
        let target = root.join("src");
        std::fs::create_dir_all(&target).expect("create target directory");

        let destination = paste_destination(&target, &root);
        assert_eq!(destination, target);
        std::fs::remove_dir_all(root).expect("remove test directory");
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
            paths: vec![PathBuf::from(r"C:\example.txt")],
            mode: ClipboardMode::Copy,
        });

        assert!(destination.borrow().is_some());
        *destination.borrow_mut() = None;
        assert!(source.borrow().is_none());
    }
}

#[cfg(test)]
mod update_ui_tests {
    use super::{update_download_notice, update_notes_summary};
    use crate::updater::UpdateDownloadProgress;

    #[test]
    fn empty_release_notes_have_a_fallback() {
        assert!(update_notes_summary(" \n ").contains("未提供更新说明"));
    }

    #[test]
    fn long_release_notes_are_bounded() {
        let notes = "a".repeat(2_000);
        let summary = update_notes_summary(&notes);

        assert!(summary.ends_with('…'));
        assert!(summary.chars().count() < 1_810);
    }

    #[test]
    fn download_notice_includes_percent_and_size() {
        let notice = update_download_notice(
            "0.6.34",
            UpdateDownloadProgress {
                downloaded_bytes: 1024 * 1024,
                total_bytes: 2 * 1024 * 1024,
                active: true,
            },
        );

        assert!(notice.contains("50%"));
        assert!(notice.contains("1.0 MB / 2.0 MB"));
    }

    #[test]
    fn download_notice_handles_unknown_total_size() {
        let notice = update_download_notice(
            "0.6.34",
            UpdateDownloadProgress {
                downloaded_bytes: 1024,
                total_bytes: 0,
                active: true,
            },
        );

        assert!(!notice.contains('%'));
        assert!(notice.contains("1.0 KB"));
    }
}

#[cfg(test)]
mod launch_target_tests {
    use super::{AppLaunchTarget, resolve_location_input};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!("kite-launch-{label}-{unique}"))
    }

    #[test]
    fn file_path_uses_the_containing_directory_and_keeps_the_file() {
        let root = temporary_directory("file");
        fs::create_dir_all(&root).expect("create test directory");
        let file = root.join("app_config 工作架构与运行原理.md");
        fs::write(&file, "notes").expect("write test file");

        let resolved = resolve_location_input(&format!("\"{}\"", file.display()), &root)
            .expect("resolve quoted file path");
        let target = AppLaunchTarget::from_existing_path(resolved).expect("classify file path");

        assert_eq!(target.directory, root);
        assert_eq!(target.file.as_deref(), Some(file.as_path()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn directory_path_stays_a_directory_target() {
        let root = temporary_directory("directory");
        fs::create_dir_all(&root).expect("create test directory");

        let target = AppLaunchTarget::from_existing_path(root.clone()).expect("classify directory");

        assert_eq!(target.directory, root);
        assert!(target.file.is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_path_is_not_classified_as_a_file() {
        let root = temporary_directory("missing");
        let missing = root.join("missing.md");

        assert_eq!(AppLaunchTarget::from_existing_path(missing), None);
    }
}

#[cfg(test)]
mod path_bar_tests {
    use super::{
        ColumnDisplayRow, ColumnFilter, ColumnPane, column_chain, column_context_menu_target,
        column_count_after_file_selection, common_column_prefix, loaded_column_index,
        path_after_recycle, path_after_recycle_paths, path_after_rename,
    };
    use crate::fs::{EntryKind, FileEntry, SortKey, SortSpec};
    use crate::ui_preferences::{ColumnGroupPreference as ColumnGroup, ColumnViewRule};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, UNIX_EPOCH};

    fn pane(path: &str, loading: bool) -> ColumnPane {
        ColumnPane::new(
            PathBuf::from(path),
            Vec::new(),
            0,
            loading,
            false,
            false,
            ColumnViewRule::default(),
        )
    }

    fn entry(path: &str, kind: EntryKind) -> FileEntry {
        let path = PathBuf::from(path);
        FileEntry {
            extension: path
                .extension()
                .map(|value| value.to_string_lossy().to_lowercase())
                .unwrap_or_default(),
            name: path
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path,
            kind,
            size: 0,
            modified: None,
            git_status: None,
            svn_status: None,
        }
    }

    #[test]
    fn column_filter_and_grouping_change_visible_rows() {
        let root = PathBuf::from(r"C:\repo");
        let mut column = ColumnPane::new(
            root.clone(),
            vec![
                entry(r"C:\repo\src", EntryKind::Directory),
                entry(r"C:\repo\notes.txt", EntryKind::File),
                entry(r"C:\repo\photo.png", EntryKind::File),
            ],
            0,
            false,
            false,
            false,
            ColumnViewRule::default(),
        );

        column.filter = ColumnFilter::Documents;
        column.refresh_view();
        assert_eq!(column.entries.len(), 1);
        assert_eq!(column.entries[0].name, "notes.txt");

        column.filter = ColumnFilter::All;
        column.group = ColumnGroup::Extension;
        column.refresh_view();
        let rows = column.display_rows();
        let groups = rows
            .iter()
            .filter_map(|row| match row {
                ColumnDisplayRow::Group { label, .. } => Some(label.to_string()),
                ColumnDisplayRow::Entry(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(groups, vec!["文件夹", "PNG", "TXT"]);

        column.toggle_group_collapse("PNG");
        let collapsed_rows = column.display_rows();
        assert!(collapsed_rows.iter().any(|row| matches!(
            row,
            ColumnDisplayRow::Group { label, count, collapsed }
                if label.as_ref() == "PNG" && *count == 1 && *collapsed
        )));
        assert!(!collapsed_rows.iter().any(|row| matches!(
            row,
            ColumnDisplayRow::Entry(entry) if entry.name == "photo.png"
        )));
    }

    #[test]
    fn column_sort_button_toggles_current_sort_direction() {
        let root = PathBuf::from(r"C:\repo");
        let mut column = ColumnPane::new(
            root,
            vec![
                entry(r"C:\repo\zeta.txt", EntryKind::File),
                entry(r"C:\repo\alpha.txt", EntryKind::File),
            ],
            0,
            false,
            false,
            false,
            ColumnViewRule {
                sort: SortSpec {
                    key: SortKey::Name,
                    ascending: true,
                },
                ..ColumnViewRule::default()
            },
        );
        assert_eq!(column.entries[0].name, "alpha.txt");
        assert_eq!(column.entries[1].name, "zeta.txt");

        column.toggle_sort_order();

        assert_eq!(
            column.sort,
            SortSpec {
                key: SortKey::Name,
                ascending: false,
            }
        );
        assert_eq!(column.entries[0].name, "zeta.txt");
        assert_eq!(column.entries[1].name, "alpha.txt");
    }

    #[test]
    fn column_header_selects_field_and_toggles_direction() {
        let root = PathBuf::from(r"C:\repo");
        let mut older = entry(r"C:\repo\older.txt", EntryKind::File);
        older.modified = Some(UNIX_EPOCH + Duration::from_secs(100));
        let mut newer = entry(r"C:\repo\newer.txt", EntryKind::File);
        newer.modified = Some(UNIX_EPOCH + Duration::from_secs(200));
        let mut column = ColumnPane::new(
            root,
            vec![older, newer],
            0,
            false,
            false,
            false,
            ColumnViewRule::default(),
        );

        column.toggle_sort_for_header(SortKey::Modified);
        assert_eq!(
            column.sort,
            SortSpec {
                key: SortKey::Modified,
                ascending: false,
            }
        );
        assert_eq!(column.entries[0].name, "newer.txt");

        column.toggle_sort_for_header(SortKey::Modified);
        assert_eq!(
            column.sort,
            SortSpec {
                key: SortKey::Modified,
                ascending: true,
            }
        );
        assert_eq!(column.entries[0].name, "older.txt");

        column.toggle_sort_for_header(SortKey::Size);
        assert_eq!(column.sort.key, SortKey::Size);
        assert!(!column.sort.ascending);
    }

    #[test]
    fn column_search_results_restore_the_directory_listing() {
        let root = PathBuf::from(r"C:\repo");
        let mut column = ColumnPane::new(
            root.clone(),
            vec![entry(r"C:\repo\notes.txt", EntryKind::File)],
            0,
            false,
            false,
            false,
            ColumnViewRule::default(),
        );

        column.set_search_results(
            "target".to_string(),
            vec![entry(r"C:\repo\nested\target.md", EntryKind::File)],
            true,
        );
        assert_eq!(column.search_query.as_deref(), Some("target"));
        assert!(column.search_truncated);
        assert_eq!(column.entries[0].name, "target.md");

        column.clear_search_results();
        assert!(column.search_query.is_none());
        assert!(!column.search_truncated);
        assert_eq!(column.entries[0].name, "notes.txt");
    }

    #[test]
    fn column_selection_supports_toggle_and_visible_range() {
        let root = PathBuf::from(r"C:\repo");
        let mut column = ColumnPane::new(
            root,
            vec![
                entry(r"C:\repo\alpha.txt", EntryKind::File),
                entry(r"C:\repo\bravo.txt", EntryKind::File),
                entry(r"C:\repo\charlie.txt", EntryKind::File),
            ],
            0,
            false,
            false,
            false,
            ColumnViewRule::default(),
        );

        column.set_single_selection(PathBuf::from(r"C:\repo\alpha.txt"));
        column.toggle_selection(PathBuf::from(r"C:\repo\charlie.txt"));
        assert_eq!(column.selected_paths.len(), 2);
        assert!(column.is_selected(Path::new(r"C:\repo\charlie.txt")));
        assert_eq!(
            column.selected_path,
            Some(PathBuf::from(r"C:\repo\charlie.txt"))
        );

        column.toggle_selection(PathBuf::from(r"C:\repo\charlie.txt"));
        assert_eq!(
            column.selected_path,
            Some(PathBuf::from(r"C:\repo\alpha.txt"))
        );

        column.selection_anchor_path = Some(PathBuf::from(r"C:\repo\alpha.txt"));
        column.select_range_to(PathBuf::from(r"C:\repo\bravo.txt"));
        assert_eq!(column.selected_paths.len(), 2);
        assert!(column.is_selected(Path::new(r"C:\repo\alpha.txt")));
        assert!(column.is_selected(Path::new(r"C:\repo\bravo.txt")));
        assert!(!column.is_selected(Path::new(r"C:\repo\charlie.txt")));
    }

    #[test]
    fn batch_recycle_uses_the_nearest_removed_ancestor() {
        assert_eq!(
            path_after_recycle_paths(
                Path::new(r"C:\repo\src\ui"),
                &[PathBuf::from(r"C:\repo\src")],
            ),
            PathBuf::from(r"C:\repo")
        );
    }

    #[test]
    fn shell_namespace_path_uses_one_column() {
        let path = PathBuf::from(r"::{20D04FE0-3AEA-1069-A2D8-08002B30309D}\phone\photo.jpg");
        assert_eq!(column_chain(&path), vec![path]);
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
    fn file_selection_removes_deeper_columns() {
        assert_eq!(column_count_after_file_selection(0, 3), 1);
        assert_eq!(column_count_after_file_selection(1, 3), 2);
        assert_eq!(column_count_after_file_selection(2, 3), 3);
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
    use super::{
        is_markdown_preview_entry, is_text_preview_entry, preview_info_text,
        preview_more_menu_items, preview_text_lines, read_log_preview, read_text_preview,
    };
    use crate::actions::ContextMenuCommand;
    use crate::fs::{EntryKind, FileEntry, VcsStatus};
    use gpui_kit::SharedString;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn preview_more_menu_contains_file_actions() {
        let commands = preview_more_menu_items()
            .iter()
            .map(|(_, _, command)| *command)
            .collect::<Vec<_>>();
        assert_eq!(
            commands,
            vec![
                ContextMenuCommand::Open,
                ContextMenuCommand::Copy,
                ContextMenuCommand::CopyPath,
                ContextMenuCommand::ShowProperties,
                ContextMenuCommand::SystemMenu,
            ]
        );
    }

    #[test]
    fn txt_and_markdown_entries_use_markdown_preview() {
        let make_entry = |extension: &str| FileEntry {
            path: PathBuf::from(format!(
                r"C:\demo
otes.{extension}"
            )),
            name: format!("notes.{extension}"),
            kind: EntryKind::File,
            size: 128,
            modified: None,
            extension: extension.to_string(),
            git_status: None,
            svn_status: None,
        };

        assert!(is_markdown_preview_entry(&make_entry("txt")));
        assert!(is_markdown_preview_entry(&make_entry("TXT")));
        assert!(is_markdown_preview_entry(&make_entry("md")));
        assert!(is_markdown_preview_entry(&make_entry("markdown")));
        assert!(!is_markdown_preview_entry(&make_entry("log")));
        assert!(is_text_preview_entry(&make_entry("markdown")));
    }

    #[test]
    fn splits_preview_text_into_stable_lines() {
        assert!(preview_text_lines("").is_empty());
        let expected: Vec<SharedString> = vec!["first".into(), "second".into(), "".into()];
        assert_eq!(preview_text_lines("first\r\nsecond\n"), expected);
    }

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

    #[test]
    fn large_log_preview_reads_only_the_tail() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("kite-log-preview-{unique}"));
        fs::create_dir_all(&root).expect("create log preview directory");
        let path = root.join("application.log");
        let mut content = String::from("oldest-entry\n");
        content.push_str(&"a".repeat(128 * 1024));
        content.push_str("\nnewest-entry\n");
        fs::write(&path, content).expect("write log preview");

        let preview = read_log_preview(&path).expect("read log preview");

        assert!(preview.starts_with("日志末尾预览："));
        assert!(preview.contains("newest-entry"));
        assert!(!preview.contains("oldest-entry"));
        assert!(preview.chars().count() < 7_000);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn log_files_ignore_the_regular_text_size_limit() {
        let make_entry = |extension: &str| FileEntry {
            path: PathBuf::from(format!(r"C:\demo\application.{extension}")),
            name: format!("application.{extension}"),
            kind: EntryKind::File,
            size: 2 * 1024 * 1024,
            modified: None,
            extension: extension.to_string(),
            git_status: None,
            svn_status: None,
        };

        assert!(is_text_preview_entry(&make_entry("log")));
        assert!(is_text_preview_entry(&make_entry("LOG")));
        assert!(!is_text_preview_entry(&make_entry("txt")));
    }
}
#[cfg(test)]
mod column_details_tests {
    use super::{
        DETAILED_COLUMN_MAX_WIDTH, DETAILED_COLUMN_MIN_WIDTH, column_modified_label,
        column_size_label, detailed_column_width_for_name_width,
    };
    use crate::fs::{EntryKind, FileEntry};
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    fn entry(is_dir: bool, size: u64) -> FileEntry {
        FileEntry {
            path: PathBuf::from(r"C:\demo\example.mp4"),
            name: "example.mp4".to_string(),
            kind: if is_dir {
                EntryKind::Directory
            } else {
                EntryKind::File
            },
            size,
            modified: Some(UNIX_EPOCH + Duration::from_secs(1_800_000_000)),
            extension: "mp4".to_string(),
            git_status: None,
            svn_status: None,
        }
    }

    #[test]
    fn detailed_column_width_is_clamped() {
        assert_eq!(
            detailed_column_width_for_name_width(0.0, false, false, false, 250),
            DETAILED_COLUMN_MIN_WIDTH
        );
        assert_eq!(
            detailed_column_width_for_name_width(10_000.0, true, true, true, 320),
            DETAILED_COLUMN_MAX_WIDTH
        );
    }

    #[test]
    fn hidden_detail_columns_reduce_the_required_width() {
        let all_columns = detailed_column_width_for_name_width(80.0, true, true, true, 250);
        let date_only = detailed_column_width_for_name_width(80.0, true, false, false, 250);

        assert!(date_only < all_columns);
    }

    #[test]
    fn detailed_size_uses_adaptive_units() {
        assert_eq!(column_size_label(&entry(false, 211)), "211 B");
        assert_eq!(column_size_label(&entry(false, 1536)), "1.5 KB");
        assert_eq!(column_size_label(&entry(false, 157_286_400)), "150.0 MB");
        assert_eq!(column_size_label(&entry(true, 157_286_400)), "—");
    }

    #[test]
    fn detailed_date_omits_time() {
        let label = column_modified_label(&entry(false, 0));
        assert!(!label.contains(':'));
    }
}

#[cfg(test)]
mod scroll_routing_tests {
    use super::{directory_scroll_offset, horizontal_wheel_delta, wheel_scrolls_columns};

    #[test]
    fn horizontal_gesture_takes_priority_over_vertical_noise() {
        assert!(wheel_scrolls_columns(18.0, 3.0));
        assert!(wheel_scrolls_columns(8.0, 7.0));
        assert!(!wheel_scrolls_columns(3.0, 18.0));
        assert!(!wheel_scrolls_columns(0.0, 18.0));
    }

    #[test]
    fn shift_vertical_wheel_scrolls_columns() {
        assert_eq!(horizontal_wheel_delta(true, 0.0, -90.0), Some(-90.0));
        assert_eq!(horizontal_wheel_delta(true, 0.0, 90.0), Some(90.0));
        assert_eq!(horizontal_wheel_delta(false, 0.0, -90.0), None);
    }

    #[test]
    fn horizontal_wheel_does_not_require_shift() {
        assert_eq!(horizontal_wheel_delta(false, 18.0, 3.0), Some(18.0));
    }

    #[test]
    fn directory_interior_preserves_vertical_offset() {
        assert_eq!(directory_scroll_offset(-420.0, 800.0), -420.0);
    }

    #[test]
    fn bottom_boundary_stops_vertical_offset() {
        assert_eq!(directory_scroll_offset(-830.0, 800.0), -800.0);
    }

    #[test]
    fn top_boundary_stops_vertical_offset() {
        assert_eq!(directory_scroll_offset(25.0, 800.0), 0.0);
    }

    #[test]
    fn non_scrollable_directory_stays_at_start() {
        assert_eq!(directory_scroll_offset(-90.0, 0.0), 0.0);
    }
}
