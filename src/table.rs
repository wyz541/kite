use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::table::{Column, ColumnSort, TableDelegate, TableState};
use gpui_kit::component::{ActiveTheme as _, Icon, StyledExt as _, h_flex, v_flex};
use gpui_kit::*;

use crate::actions::*;

use crate::fs::{FileEntry, SortKey, SortSpec, VcsStatus, sort_entries};
use crate::vcs::{VcsSnapshot, apply_statuses};
use gpui_kit::assets::IconName;

pub struct FileTableDelegate {
    entries: Vec<FileEntry>,
    sort: SortSpec,
    loading: bool,
    can_paste: bool,
    git_available: bool,
    svn_available: bool,
    active_query: Option<String>,
}

impl FileTableDelegate {
    pub fn new(entries: Vec<FileEntry>, sort: SortSpec) -> Self {
        Self {
            entries,
            sort,
            loading: false,
            can_paste: false,
            git_available: false,
            svn_available: false,
            active_query: None,
        }
    }

    pub fn entries(&self) -> &[FileEntry] {
        &self.entries
    }

    pub fn entry(&self, index: usize) -> Option<&FileEntry> {
        self.entries.get(index)
    }

    pub fn replace_entries(&mut self, entries: Vec<FileEntry>, query: Option<String>) {
        self.entries = entries;
        self.active_query = query;
        self.loading = false;
        self.git_available = false;
        self.svn_available = false;
        sort_entries(&mut self.entries, self.sort);
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    pub fn set_can_paste(&mut self, can_paste: bool) {
        self.can_paste = can_paste;
    }

    pub fn apply_vcs_statuses(&mut self, view_root: &std::path::Path, snapshot: &VcsSnapshot) {
        self.git_available = snapshot.git_available();
        self.svn_available = snapshot.svn_available();
        apply_statuses(&mut self.entries, view_root, snapshot);
        sort_entries(&mut self.entries, self.sort);
    }

    pub fn clear_entries(&mut self) {
        self.entries.clear();
        self.active_query = None;
    }

    pub fn sort_spec(&self) -> SortSpec {
        self.sort
    }

    fn set_sort(&mut self, key: SortKey, ascending: bool) {
        self.sort = SortSpec { key, ascending };
        sort_entries(&mut self.entries, self.sort);
    }
}

impl TableDelegate for FileTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        5
    }

    fn rows_count(&self, _: &App) -> usize {
        self.entries.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        match col_ix {
            0 => Column::new("name", "名称")
                .width(px(420.))
                .min_width(px(220.))
                .fixed_left()
                .sortable(),
            1 => Column::new("vcs", "版本控制")
                .width(px(130.))
                .min_width(px(100.))
                .sortable(),
            2 => Column::new("modified", "修改日期")
                .width(px(165.))
                .min_width(px(130.))
                .sortable(),
            3 => Column::new("kind", "类型")
                .width(px(145.))
                .min_width(px(110.))
                .sortable(),
            4 => Column::new("size", "大小")
                .width(px(100.))
                .min_width(px(80.))
                .text_right()
                .sortable(),
            _ => Column::new("unknown", ""),
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let key = match col_ix {
            0 => SortKey::Name,
            1 => SortKey::Vcs,
            2 => SortKey::Modified,
            3 => SortKey::Kind,
            4 => SortKey::Size,
            _ => return,
        };
        match sort {
            ColumnSort::Ascending => self.set_sort(key, true),
            ColumnSort::Descending => self.set_sort(key, false),
            ColumnSort::Default => self.set_sort(SortKey::Name, true),
        }
        cx.notify();
    }

    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        let Some(entry) = self.entries.get(row_ix) else {
            return menu;
        };
        let title = entry.name.clone();
        let open_label = if entry.is_dir() {
            "打开文件夹"
        } else {
            "打开文件"
        };
        let disabled = self.loading;
        let can_paste = self.can_paste && !disabled;

        let mut menu = menu
            .label(title)
            .separator()
            .menu_with_icon_and_disabled(
                open_label,
                IconName::FolderOpen,
                Box::new(RowContextAction {
                    row: row_ix,
                    command: RowContextCommand::Open,
                }),
                disabled,
            )
            .separator()
            .menu_with_icon_and_disabled(
                "复制",
                IconName::Copy,
                Box::new(RowContextAction {
                    row: row_ix,
                    command: RowContextCommand::Copy,
                }),
                disabled,
            )
            .menu_with_icon_and_disabled(
                "剪切",
                IconName::Scissors,
                Box::new(RowContextAction {
                    row: row_ix,
                    command: RowContextCommand::Cut,
                }),
                disabled,
            )
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
                disabled,
            )
            .menu_with_icon_and_disabled(
                "重命名",
                IconName::Pencil,
                Box::new(RowContextAction {
                    row: row_ix,
                    command: RowContextCommand::Rename,
                }),
                disabled,
            )
            .menu_with_icon_and_disabled(
                "移到回收站",
                IconName::Trash,
                Box::new(RowContextAction {
                    row: row_ix,
                    command: RowContextCommand::Recycle,
                }),
                disabled,
            );

        if self.git_available {
            menu = menu
                .separator()
                .menu_with_icon_and_disabled(
                    "Git 添加",
                    IconName::GitBranch,
                    Box::new(RowContextAction {
                        row: row_ix,
                        command: RowContextCommand::Vcs(VcsMenuCommand::GitAdd),
                    }),
                    disabled,
                )
                .menu_with_icon_and_disabled(
                    "Git 提交…",
                    IconName::GitBranch,
                    Box::new(RowContextAction {
                        row: row_ix,
                        command: RowContextCommand::Vcs(VcsMenuCommand::GitCommit),
                    }),
                    disabled,
                )
                .menu_with_icon_and_disabled(
                    "Git 还原…",
                    IconName::GitBranch,
                    Box::new(RowContextAction {
                        row: row_ix,
                        command: RowContextCommand::Vcs(VcsMenuCommand::GitRevert),
                    }),
                    disabled,
                );
        }

        if self.svn_available {
            menu = menu
                .separator()
                .menu_with_icon_and_disabled(
                    "SVN 更新",
                    IconName::Boxes,
                    Box::new(RowContextAction {
                        row: row_ix,
                        command: RowContextCommand::Vcs(VcsMenuCommand::SvnUpdate),
                    }),
                    disabled,
                )
                .menu_with_icon_and_disabled(
                    "SVN 添加",
                    IconName::Boxes,
                    Box::new(RowContextAction {
                        row: row_ix,
                        command: RowContextCommand::Vcs(VcsMenuCommand::SvnAdd),
                    }),
                    disabled,
                )
                .menu_with_icon_and_disabled(
                    "SVN 提交…",
                    IconName::Boxes,
                    Box::new(RowContextAction {
                        row: row_ix,
                        command: RowContextCommand::Vcs(VcsMenuCommand::SvnCommit),
                    }),
                    disabled,
                )
                .menu_with_icon_and_disabled(
                    "SVN 还原…",
                    IconName::Boxes,
                    Box::new(RowContextAction {
                        row: row_ix,
                        command: RowContextCommand::Vcs(VcsMenuCommand::SvnRevert),
                    }),
                    disabled,
                );
        }

        menu.separator().menu_with_icon_and_disabled(
            "刷新",
            IconName::RotateCw,
            Box::new(RefreshFolder),
            false,
        )
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(entry) = self.entries.get(row_ix) else {
            return div();
        };

        let base = div()
            .w_full()
            .h_full()
            .min_w_0()
            .flex()
            .items_center()
            .overflow_hidden();

        match col_ix {
            0 => base.child(
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .child(
                        Icon::new(icon_for(entry))
                            .size_4()
                            .text_color(icon_color(entry, cx)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .child(entry.name.clone()),
                    ),
            ),
            1 => {
                let mut badges = h_flex().h_full().gap_1().items_center();
                if let Some(status) = entry.git_status {
                    badges = badges.child(vcs_badge(IconName::GitBranch, status));
                }
                if let Some(status) = entry.svn_status {
                    badges = badges.child(vcs_badge(IconName::Boxes, status));
                }
                base.child(badges)
            }
            2 => base.child(
                div()
                    .truncate()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(entry.modified_label()),
            ),
            3 => base.child(
                div()
                    .truncate()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(entry.type_label()),
            ),
            4 => base.child(
                div()
                    .w_full()
                    .truncate()
                    .text_sm()
                    .text_right()
                    .text_color(cx.theme().muted_foreground)
                    .child(entry.size_label()),
            ),
            _ => base,
        }
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize, _: &App) -> String {
        let Some(entry) = self.entries.get(row_ix) else {
            return String::new();
        };
        match col_ix {
            0 => entry.name.clone(),
            1 => vcs_cell_text(entry),
            2 => entry.modified_label(),
            3 => entry.type_label(),
            4 => entry.size_label(),
            _ => String::new(),
        }
    }

    fn loading(&self, _: &App) -> bool {
        self.loading
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let (icon, title, detail) = if let Some(query) = &self.active_query {
            (
                IconName::Search,
                format!("没有匹配“{query}”的项目"),
                "尝试更短的关键词，或按 Esc 返回目录。",
            )
        } else {
            (
                IconName::Folder,
                "此文件夹为空".to_string(),
                "按 Ctrl+Shift+N 可新建文件夹。",
            )
        };

        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .text_center()
            .child(
                Icon::new(icon)
                    .size_10()
                    .text_color(cx.theme().muted_foreground.opacity(0.55)),
            )
            .child(div().text_sm().font_medium().child(title))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(detail),
            )
    }
}

pub(crate) fn icon_for(entry: &FileEntry) -> IconName {
    if entry.is_dir() {
        return IconName::Folder;
    }
    match entry.extension.as_str() {
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

pub(crate) fn icon_color(entry: &FileEntry, cx: &App) -> Hsla {
    if entry.is_dir() {
        return rgb(0xf3b64b).into();
    }
    match entry.extension.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" => rgb(0x5ea7ff).into(),
        "mp3" | "wav" | "flac" | "aac" | "ogg" => rgb(0xe58cff).into(),
        "mp4" | "mkv" | "mov" | "avi" | "webm" => rgb(0xff7f8b).into(),
        "zip" | "7z" | "rar" | "tar" | "gz" => rgb(0xf1a85b).into(),
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "html" | "css" | "c" | "cpp" | "h" | "cs"
        | "java" | "go" | "kt" => rgb(0x65c88a).into(),
        "txt" | "md" | "log" | "ini" | "toml" | "yaml" | "yml" | "json" | "pdf" | "doc"
        | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => rgb(0x9aa7b8).into(),
        _ => cx.theme().muted_foreground,
    }
}

pub(crate) fn vcs_badge(icon: IconName, status: VcsStatus) -> AnyElement {
    let color = vcs_status_color(status);
    h_flex()
        .h(px(19.))
        .items_center()
        .gap_0p5()
        .px_1()
        .rounded_sm()
        .bg(color.opacity(0.14))
        .child(Icon::new(icon).size_3().text_color(color))
        .child(
            div()
                .text_xs()
                .font_medium()
                .text_color(color)
                .child(status.code()),
        )
        .into_any_element()
}

fn vcs_status_color(status: VcsStatus) -> Hsla {
    match status {
        VcsStatus::Modified | VcsStatus::PropertyModified => rgb(0xe5a84b).into(),
        VcsStatus::Added | VcsStatus::Renamed => rgb(0x55b978).into(),
        VcsStatus::Deleted | VcsStatus::Conflicted | VcsStatus::Missing => rgb(0xef6b73).into(),
        VcsStatus::Untracked => rgb(0x4cc9c0).into(),
        VcsStatus::Ignored | VcsStatus::External => rgb(0x8a96a8).into(),
    }
}

fn vcs_cell_text(entry: &FileEntry) -> String {
    let mut values = Vec::new();
    if let Some(status) = entry.git_status {
        values.push(format!("Git {}", status.label()));
    }
    if let Some(status) = entry.svn_status {
        values.push(format!("SVN {}", status.label()));
    }
    values.join("; ")
}
