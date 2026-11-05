use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::prelude::StatefulInteractiveElement as _;
use gpui_kit::*;

use crate::actions::{ContextMenuAction, ContextMenuCommand, VcsMenuCommand};
use crate::archive;
use crate::platform::HostPlatform;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub(crate) use windows::{request_system_menu, show_properties};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ContextMenuPolicy {
    pub platform: HostPlatform,
    pub compact: bool,
    pub hide_unavailable: bool,
    system_menu_label: &'static str,
}

impl ContextMenuPolicy {
    pub(crate) const fn current() -> Self {
        Self::for_platform(crate::platform::host_platform())
    }

    pub(crate) const fn for_platform(platform: HostPlatform) -> Self {
        match platform {
            HostPlatform::Windows => Self {
                platform,
                compact: false,
                hide_unavailable: false,
                system_menu_label: "显示更多选项 (Show more options)",
            },
            HostPlatform::MacOS => Self {
                platform,
                compact: true,
                hide_unavailable: true,
                system_menu_label: "macOS 系统菜单…",
            },
            HostPlatform::Other => Self {
                platform,
                compact: true,
                hide_unavailable: true,
                system_menu_label: "系统菜单…",
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ContextMenuCapabilities {
    pub loading: bool,
    pub can_paste: bool,
    pub git_available: bool,
    pub svn_available: bool,
    pub supports_transfer_ops: bool,
    pub supports_cut: bool,
    pub supports_rename: bool,
    pub supports_delete: bool,
    pub supports_favorite: bool,
    pub is_shell_namespace: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ContextMenuTarget {
    pub path: PathBuf,
    pub working_directory: PathBuf,
    pub title: String,
    pub is_dir: bool,
    pub loading: bool,
    pub can_paste: bool,
    pub git_available: bool,
    pub svn_available: bool,
    pub supports_transfer_ops: bool,
    pub supports_cut: bool,
    pub supports_rename: bool,
    pub supports_delete: bool,
    pub supports_favorite: bool,
    pub is_shell_namespace: bool,
}

impl ContextMenuCapabilities {
    pub(crate) fn for_path(
        path: &std::path::Path,
        loading: bool,
        can_paste: bool,
        git_available: bool,
        svn_available: bool,
    ) -> Self {
        let is_shell_namespace = crate::shell_fs::is_shell_namespace_path(path);
        let (supports_cut, supports_rename, supports_delete) = if is_shell_namespace {
            let (can_rename, can_delete, can_move) = crate::shell_fs::item_capabilities(path);
            (can_move, can_rename, can_delete)
        } else {
            let can_change = path.parent().is_some();
            (can_change, can_change, can_change)
        };
        Self {
            loading,
            can_paste,
            git_available,
            svn_available,
            supports_transfer_ops: true,
            supports_cut,
            supports_rename,
            supports_delete,
            supports_favorite: !is_shell_namespace,
            is_shell_namespace,
        }
    }
}

impl ContextMenuTarget {
    pub(crate) fn directory(
        path: PathBuf,
        title: String,
        capabilities: ContextMenuCapabilities,
    ) -> Self {
        Self {
            working_directory: path.clone(),
            path,
            title,
            is_dir: true,
            loading: capabilities.loading,
            can_paste: capabilities.can_paste,
            git_available: capabilities.git_available,
            svn_available: capabilities.svn_available,
            supports_transfer_ops: capabilities.supports_transfer_ops,
            supports_cut: capabilities.supports_cut,
            supports_rename: capabilities.supports_rename,
            supports_delete: capabilities.supports_delete,
            supports_favorite: capabilities.supports_favorite,
            is_shell_namespace: capabilities.is_shell_namespace,
        }
    }

    pub(crate) fn entry(
        path: PathBuf,
        title: String,
        is_dir: bool,
        working_directory: PathBuf,
        capabilities: ContextMenuCapabilities,
    ) -> Self {
        Self {
            path,
            working_directory,
            title,
            is_dir,
            loading: capabilities.loading,
            can_paste: capabilities.can_paste,
            git_available: capabilities.git_available,
            svn_available: capabilities.svn_available,
            supports_transfer_ops: capabilities.supports_transfer_ops,
            supports_cut: capabilities.supports_cut,
            supports_rename: capabilities.supports_rename,
            supports_delete: capabilities.supports_delete,
            supports_favorite: capabilities.supports_favorite,
            is_shell_namespace: capabilities.is_shell_namespace,
        }
    }

    fn action(&self, command: ContextMenuCommand) -> ContextMenuAction {
        ContextMenuAction {
            target: self.path.clone(),
            working_directory: self.working_directory.clone(),
            command,
        }
    }
}

#[derive(Clone, Debug)]
enum ContextMenuNode {
    Separator,
    Item {
        label: &'static str,
        icon: IconName,
        command: ContextMenuCommand,
        enabled: bool,
    },
    Submenu {
        label: &'static str,
        icon: IconName,
        enabled: bool,
        entries: Vec<ContextMenuNode>,
    },
}

#[derive(Clone, Debug, Default)]
struct ContextMenuModel {
    entries: Vec<ContextMenuNode>,
}

impl ContextMenuModel {
    fn separator(&mut self) {
        self.entries.push(ContextMenuNode::Separator);
    }

    fn item(
        &mut self,
        label: &'static str,
        icon: IconName,
        command: ContextMenuCommand,
        enabled: bool,
    ) {
        self.entries.push(ContextMenuNode::Item {
            label,
            icon,
            command,
            enabled,
        });
    }

    fn submenu(
        &mut self,
        label: &'static str,
        icon: IconName,
        enabled: bool,
        entries: Vec<ContextMenuNode>,
    ) {
        self.entries.push(ContextMenuNode::Submenu {
            label,
            icon,
            enabled,
            entries,
        });
    }

    fn normalize(&mut self, policy: ContextMenuPolicy) {
        let mut normalized = Vec::with_capacity(self.entries.len());
        for entry in self.entries.drain(..) {
            if matches!(entry, ContextMenuNode::Separator)
                && (normalized.is_empty()
                    || matches!(normalized.last(), Some(ContextMenuNode::Separator)))
            {
                continue;
            }
            if policy.hide_unavailable {
                match &entry {
                    ContextMenuNode::Item { enabled: false, .. }
                    | ContextMenuNode::Submenu { enabled: false, .. } => continue,
                    _ => {}
                }
            }
            normalized.push(entry);
        }
        while matches!(normalized.last(), Some(ContextMenuNode::Separator)) {
            normalized.pop();
        }
        self.entries = normalized;
    }
}

trait ContextMenuProvider {
    fn append(
        &self,
        target: &ContextMenuTarget,
        policy: ContextMenuPolicy,
        model: &mut ContextMenuModel,
    );
}

struct CoreMenuProvider;
struct ArchiveMenuProvider;
struct VcsMenuProvider;
struct SystemMenuProvider;
struct RefreshMenuProvider;

impl ContextMenuProvider for CoreMenuProvider {
    fn append(
        &self,
        target: &ContextMenuTarget,
        policy: ContextMenuPolicy,
        model: &mut ContextMenuModel,
    ) {
        let disabled = target.loading;
        let open_label = if target.is_dir {
            "打开文件夹"
        } else {
            "打开文件"
        };
        model.item(
            open_label,
            IconName::FolderOpen,
            ContextMenuCommand::Open,
            !disabled,
        );
        model.separator();
        model.item(
            "复制",
            IconName::Copy,
            ContextMenuCommand::Copy,
            !disabled && target.supports_transfer_ops,
        );
        model.item(
            "拷贝到…",
            IconName::FolderInput,
            ContextMenuCommand::CopyTo,
            !disabled && target.supports_transfer_ops,
        );
        model.item(
            "复制完整路径",
            IconName::Link,
            ContextMenuCommand::CopyPath,
            !disabled,
        );
        model.item(
            "剪切",
            IconName::Scissors,
            ContextMenuCommand::Cut,
            !disabled && target.supports_transfer_ops && target.supports_cut,
        );
        model.item(
            "粘贴",
            IconName::ClipboardPaste,
            ContextMenuCommand::Paste,
            !disabled && target.can_paste && target.supports_transfer_ops,
        );
        if !policy.compact {
            model.separator();
        }
        model.item(
            "新建文件夹",
            IconName::FolderPlus,
            ContextMenuCommand::NewFolder,
            !disabled && target.supports_transfer_ops,
        );
        model.item(
            "新建文件",
            IconName::FileText,
            ContextMenuCommand::NewFile,
            !disabled && target.supports_transfer_ops,
        );
        model.item(
            "添加到收藏",
            IconName::StarPlus,
            ContextMenuCommand::AddFavorite,
            !disabled && target.supports_favorite,
        );
        model.item(
            "重命名",
            IconName::Pencil,
            ContextMenuCommand::Rename,
            !disabled && target.supports_rename,
        );
        model.item(
            if target.is_shell_namespace {
                "删除设备项目"
            } else {
                "移到回收站"
            },
            IconName::Trash,
            ContextMenuCommand::Recycle,
            !disabled && target.supports_delete,
        );
        if !policy.compact {
            model.separator();
        }
        model.item(
            "属性",
            IconName::Info,
            ContextMenuCommand::ShowProperties,
            !disabled,
        );
    }
}

impl ContextMenuProvider for ArchiveMenuProvider {
    fn append(
        &self,
        target: &ContextMenuTarget,
        _: ContextMenuPolicy,
        model: &mut ContextMenuModel,
    ) {
        let can_compress = !target.loading
            && target.supports_transfer_ops
            && !target.is_shell_namespace
            && target.path.parent().is_some();
        let can_extract = !target.loading
            && target.supports_transfer_ops
            && !target.is_shell_namespace
            && !target.is_dir
            && archive::is_supported_archive(&target.path);

        model.separator();
        model.submenu(
            "压缩",
            IconName::FileArchive,
            can_compress,
            vec![
                archive_menu_item("ZIP (.zip)", ContextMenuCommand::CompressZip, can_compress),
                archive_menu_item(
                    "TAR.GZ (.tar.gz)",
                    ContextMenuCommand::CompressTarGz,
                    can_compress,
                ),
            ],
        );
        model.submenu(
            "解压",
            IconName::FolderOpen,
            can_extract,
            vec![
                archive_menu_item(
                    "解压到当前文件夹",
                    ContextMenuCommand::ExtractHere,
                    can_extract,
                ),
                archive_menu_item("解压到…", ContextMenuCommand::ExtractTo, can_extract),
            ],
        );
    }
}

impl ContextMenuProvider for VcsMenuProvider {
    fn append(
        &self,
        target: &ContextMenuTarget,
        _: ContextMenuPolicy,
        model: &mut ContextMenuModel,
    ) {
        model.separator();
        model.submenu(
            "Git",
            IconName::GitBranch,
            !target.loading && target.git_available,
            git_menu_entries(!target.loading && target.git_available),
        );
        model.submenu(
            "SVN",
            IconName::Boxes,
            !target.loading && target.svn_available,
            svn_menu_entries(!target.loading && target.svn_available),
        );
    }
}

impl ContextMenuProvider for SystemMenuProvider {
    fn append(
        &self,
        target: &ContextMenuTarget,
        policy: ContextMenuPolicy,
        model: &mut ContextMenuModel,
    ) {
        model.separator();
        model.item(
            policy.system_menu_label,
            IconName::Ellipsis,
            ContextMenuCommand::SystemMenu,
            !target.loading,
        );
    }
}

impl ContextMenuProvider for RefreshMenuProvider {
    fn append(
        &self,
        _: &ContextMenuTarget,
        policy: ContextMenuPolicy,
        model: &mut ContextMenuModel,
    ) {
        if policy.compact {
            return;
        }
        model.item(
            "刷新",
            IconName::RotateCw,
            ContextMenuCommand::Refresh,
            true,
        );
    }
}

fn git_menu_entries(enabled: bool) -> Vec<ContextMenuNode> {
    vec![
        menu_item("添加", IconName::GitBranch, VcsMenuCommand::GitAdd, enabled),
        menu_item(
            "取消暂存",
            IconName::GitBranch,
            VcsMenuCommand::GitUnstage,
            enabled,
        ),
        menu_item(
            "提交…",
            IconName::GitBranch,
            VcsMenuCommand::GitCommit,
            enabled,
        ),
        ContextMenuNode::Separator,
        menu_item(
            "拉取",
            IconName::GitBranch,
            VcsMenuCommand::GitPull,
            enabled,
        ),
        menu_item(
            "推送",
            IconName::GitBranch,
            VcsMenuCommand::GitPush,
            enabled,
        ),
        menu_item(
            "获取",
            IconName::GitBranch,
            VcsMenuCommand::GitFetch,
            enabled,
        ),
        ContextMenuNode::Separator,
        menu_item(
            "显示差异",
            IconName::GitBranch,
            VcsMenuCommand::GitDiff,
            enabled,
        ),
        menu_item(
            "显示日志",
            IconName::GitBranch,
            VcsMenuCommand::GitLog,
            enabled,
        ),
        ContextMenuNode::Separator,
        menu_item(
            "还原…",
            IconName::GitBranch,
            VcsMenuCommand::GitRevert,
            enabled,
        ),
        menu_item(
            "清理工作区…",
            IconName::GitBranch,
            VcsMenuCommand::GitClean,
            enabled,
        ),
    ]
}

fn svn_menu_entries(enabled: bool) -> Vec<ContextMenuNode> {
    vec![
        menu_item("更新", IconName::Boxes, VcsMenuCommand::SvnUpdate, enabled),
        menu_item("添加", IconName::Boxes, VcsMenuCommand::SvnAdd, enabled),
        menu_item("提交…", IconName::Boxes, VcsMenuCommand::SvnCommit, enabled),
        ContextMenuNode::Separator,
        menu_item("锁定", IconName::Boxes, VcsMenuCommand::SvnLock, enabled),
        menu_item("解锁", IconName::Boxes, VcsMenuCommand::SvnUnlock, enabled),
        ContextMenuNode::Separator,
        menu_item(
            "显示差异",
            IconName::Boxes,
            VcsMenuCommand::SvnDiff,
            enabled,
        ),
        menu_item("显示日志", IconName::Boxes, VcsMenuCommand::SvnLog, enabled),
        ContextMenuNode::Separator,
        menu_item("还原…", IconName::Boxes, VcsMenuCommand::SvnRevert, enabled),
        menu_item(
            "清理工作副本",
            IconName::Boxes,
            VcsMenuCommand::SvnCleanup,
            enabled,
        ),
    ]
}

fn archive_menu_item(
    label: &'static str,
    command: ContextMenuCommand,
    enabled: bool,
) -> ContextMenuNode {
    ContextMenuNode::Item {
        label,
        icon: IconName::FileArchive,
        command,
        enabled,
    }
}

fn menu_item(
    label: &'static str,
    icon: IconName,
    command: VcsMenuCommand,
    enabled: bool,
) -> ContextMenuNode {
    ContextMenuNode::Item {
        label,
        icon,
        command: ContextMenuCommand::Vcs(command),
        enabled,
    }
}

fn build_model(target: &ContextMenuTarget, policy: ContextMenuPolicy) -> ContextMenuModel {
    let providers: [&dyn ContextMenuProvider; 5] = [
        &RefreshMenuProvider,
        &CoreMenuProvider,
        &ArchiveMenuProvider,
        &VcsMenuProvider,
        &SystemMenuProvider,
    ];
    let mut model = ContextMenuModel::default();
    for provider in providers {
        provider.append(target, policy, &mut model);
    }
    model.normalize(policy);
    model
}

fn append_nodes(
    mut menu: PopupMenu,
    nodes: &[ContextMenuNode],
    target: &ContextMenuTarget,
    action_context: &FocusHandle,
    window: &mut Window,
    cx: &mut App,
) -> PopupMenu {
    for node in nodes {
        match node {
            ContextMenuNode::Separator => menu = menu.separator(),
            ContextMenuNode::Item {
                label,
                icon,
                command,
                enabled,
            } => {
                let action = target.action(*command);
                let item = if matches!(command, ContextMenuCommand::SystemMenu) {
                    let hover_action = action.clone();
                    let hover_context = action_context.clone();
                    let hover_triggered = Rc::new(Cell::new(false));
                    let label = *label;
                    let icon = *icon;
                    PopupMenuItem::element(move |_, cx| {
                        let hover_context = hover_context.clone();
                        let hover_action = hover_action.clone();
                        let hover_triggered = hover_triggered.clone();
                        h_flex()
                            .id("context-menu-show-more-hover")
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .on_hover(move |hovered, window, cx| {
                                if !*hovered {
                                    hover_triggered.set(false);
                                    return;
                                }
                                if hover_triggered.replace(true) {
                                    return;
                                }
                                hover_context.focus(window, cx);
                                window.dispatch_action(Box::new(hover_action.clone()), cx);
                            })
                            .child(label)
                            .child(
                                Icon::new(IconName::ChevronRight)
                                    .xsmall()
                                    .text_color(cx.theme().muted_foreground),
                            )
                    })
                    .icon(icon)
                    .disabled(!enabled)
                    .action(Box::new(action))
                } else {
                    PopupMenuItem::new(*label)
                        .icon(*icon)
                        .disabled(!enabled)
                        .action(Box::new(action))
                };
                menu = menu.item(item);
            }
            ContextMenuNode::Submenu {
                label,
                icon,
                enabled,
                entries,
            } => {
                let entries = entries.clone();
                let target = target.clone();
                let action_context = action_context.clone();
                let submenu = PopupMenu::build(window, cx, move |menu, window, cx| {
                    append_nodes(menu, &entries, &target, &action_context, window, cx)
                });
                menu = menu.item(
                    PopupMenuItem::submenu(*label, submenu)
                        .icon(*icon)
                        .disabled(!enabled),
                );
            }
        }
    }
    menu
}

pub(crate) fn render_context_menu(
    menu: PopupMenu,
    target: &ContextMenuTarget,
    action_context: FocusHandle,
    window: &mut Window,
    cx: &mut App,
) -> PopupMenu {
    let policy = ContextMenuPolicy::current();
    let model = build_model(target, policy);
    let menu = menu.label(target.title.clone()).separator();
    append_nodes(menu, &model.entries, target, &action_context, window, cx)
}

#[cfg(test)]
mod tests {
    use super::{
        ContextMenuCapabilities, ContextMenuCommand, ContextMenuModel, ContextMenuNode,
        ContextMenuPolicy, ContextMenuTarget, VcsMenuCommand, build_model,
    };
    use crate::platform::HostPlatform;
    use std::path::PathBuf;

    fn target() -> ContextMenuTarget {
        ContextMenuTarget::directory(
            PathBuf::from(r"C:\repo\src"),
            "src".to_string(),
            ContextMenuCapabilities {
                loading: false,
                can_paste: true,
                git_available: true,
                svn_available: true,
                supports_transfer_ops: true,
                supports_cut: true,
                supports_rename: true,
                supports_delete: true,
                supports_favorite: true,
                is_shell_namespace: false,
            },
        )
    }

    fn archive_target() -> ContextMenuTarget {
        ContextMenuTarget::entry(
            PathBuf::from(r"C:\repo\report.zip"),
            "report.zip".to_string(),
            false,
            PathBuf::from(r"C:\repo"),
            ContextMenuCapabilities {
                loading: false,
                can_paste: true,
                git_available: false,
                svn_available: false,
                supports_transfer_ops: true,
                supports_cut: true,
                supports_rename: true,
                supports_delete: true,
                supports_favorite: true,
                is_shell_namespace: false,
            },
        )
    }

    fn windows_policy() -> ContextMenuPolicy {
        ContextMenuPolicy::for_platform(HostPlatform::Windows)
    }

    #[test]
    fn local_root_keeps_transfer_actions_and_disables_destructive_actions() {
        let capabilities = ContextMenuCapabilities::for_path(
            std::path::Path::new(r"C:\"),
            false,
            true,
            false,
            false,
        );

        assert!(capabilities.supports_transfer_ops);
        assert!(capabilities.supports_favorite);
        assert!(!capabilities.supports_cut);
        assert!(!capabilities.supports_rename);
        assert!(!capabilities.supports_delete);
    }

    fn submenu_entries(model: &ContextMenuModel, label: &str) -> Vec<ContextMenuNode> {
        model
            .entries
            .iter()
            .find_map(|entry| match entry {
                ContextMenuNode::Submenu {
                    label: item_label,
                    entries,
                    ..
                } if item_label == &label => Some(entries.clone()),
                _ => None,
            })
            .expect("submenu")
    }

    #[test]
    fn first_level_menu_keeps_git_svn_and_native_shell() {
        let model = build_model(&target(), windows_policy());
        let labels = model
            .entries
            .iter()
            .filter_map(|entry| match entry {
                ContextMenuNode::Item { label, .. } | ContextMenuNode::Submenu { label, .. } => {
                    Some(*label)
                }
                ContextMenuNode::Separator => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(labels.first(), Some(&"刷新"));
        assert!(labels.contains(&"Git"));
        assert!(labels.contains(&"SVN"));
        assert!(labels.contains(&"压缩"));
        assert!(labels.contains(&"解压"));
        assert!(labels.contains(&"新建文件夹"));
        assert!(labels.contains(&"新建文件"));
        assert!(labels.contains(&"添加到收藏"));
        assert!(labels.contains(&"拷贝到…"));
        assert!(labels.contains(&"复制完整路径"));
        assert!(labels.contains(&"属性"));
        assert!(model.entries.iter().any(|entry| matches!(
            entry,
            ContextMenuNode::Item {
                label: "显示更多选项 (Show more options)",
                command: ContextMenuCommand::SystemMenu,
                enabled: true,
                ..
            }
        )));
    }

    #[test]
    fn compression_menu_contains_zip_and_tar_gz() {
        let model = build_model(&target(), windows_policy());
        let commands = submenu_entries(&model, "压缩")
            .iter()
            .filter_map(|entry| match entry {
                ContextMenuNode::Item { command, .. } => Some(*command),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(commands.contains(&ContextMenuCommand::CompressZip));
        assert!(commands.contains(&ContextMenuCommand::CompressTarGz));
    }

    #[test]
    fn extraction_menu_is_only_enabled_for_supported_archives() {
        let archive_model = build_model(&archive_target(), windows_policy());
        let commands = submenu_entries(&archive_model, "解压")
            .iter()
            .filter_map(|entry| match entry {
                ContextMenuNode::Item {
                    command, enabled, ..
                } if *enabled => Some(*command),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(commands.contains(&ContextMenuCommand::ExtractHere));
        assert!(commands.contains(&ContextMenuCommand::ExtractTo));

        let text_model = build_model(
            &ContextMenuTarget::entry(
                PathBuf::from(r"C:\repo\notes.txt"),
                "notes.txt".to_string(),
                false,
                PathBuf::from(r"C:\repo"),
                ContextMenuCapabilities {
                    loading: false,
                    can_paste: true,
                    git_available: false,
                    svn_available: false,
                    supports_transfer_ops: true,
                    supports_cut: true,
                    supports_rename: true,
                    supports_delete: true,
                    supports_favorite: true,
                    is_shell_namespace: false,
                },
            ),
            windows_policy(),
        );
        assert!(text_model.entries.iter().any(|entry| matches!(
            entry,
            ContextMenuNode::Submenu {
                label: "解压",
                enabled: false,
                ..
            }
        )));
    }

    #[test]
    fn git_menu_contains_complete_action_set() {
        let model = build_model(&target(), windows_policy());
        let entries = submenu_entries(&model, "Git");
        let commands = entries
            .iter()
            .filter_map(|entry| match entry {
                ContextMenuNode::Item { command, .. } => Some(*command),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::GitAdd)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::GitUnstage)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::GitCommit)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::GitPull)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::GitPush)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::GitFetch)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::GitDiff)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::GitLog)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::GitRevert)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::GitClean)));
    }

    #[test]
    fn svn_menu_contains_complete_action_set() {
        let model = build_model(&target(), windows_policy());
        let entries = submenu_entries(&model, "SVN");
        let commands = entries
            .iter()
            .filter_map(|entry| match entry {
                ContextMenuNode::Item { command, .. } => Some(*command),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::SvnUpdate)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::SvnAdd)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::SvnCommit)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::SvnLock)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::SvnUnlock)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::SvnDiff)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::SvnLog)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::SvnRevert)));
        assert!(commands.contains(&ContextMenuCommand::Vcs(VcsMenuCommand::SvnCleanup)));
    }

    #[test]
    fn loading_disables_target_actions() {
        let mut target = target();
        target.loading = true;
        let model = build_model(&target, windows_policy());
        let native_shell_enabled = model.entries.iter().any(|entry| {
            matches!(
                entry,
                ContextMenuNode::Item {
                    label: "显示更多选项 (Show more options)",
                    enabled,
                    ..
                } if !enabled
            )
        });
        let vcs_disabled = model.entries.iter().all(|entry| match entry {
            ContextMenuNode::Submenu { label, enabled, .. } if matches!(*label, "Git" | "SVN") => {
                !enabled
            }
            _ => true,
        });
        let new_file_disabled = model.entries.iter().any(|entry| {
            matches!(
                entry,
                ContextMenuNode::Item {
                    command: ContextMenuCommand::NewFile,
                    enabled: false,
                    ..
                }
            )
        });

        assert!(native_shell_enabled);
        assert!(vcs_disabled);
        assert!(new_file_disabled);
    }

    #[test]
    fn macos_policy_hides_unavailable_vcs_and_uses_platform_label() {
        let mut target = target();
        target.git_available = false;
        target.svn_available = false;
        let model = build_model(
            &target,
            ContextMenuPolicy::for_platform(HostPlatform::MacOS),
        );
        let labels = model
            .entries
            .iter()
            .filter_map(|entry| match entry {
                ContextMenuNode::Item { label, .. } | ContextMenuNode::Submenu { label, .. } => {
                    Some(*label)
                }
                ContextMenuNode::Separator => None,
            })
            .collect::<Vec<_>>();

        assert!(!labels.contains(&"Git"));
        assert!(!labels.contains(&"SVN"));
        assert!(labels.contains(&"压缩"));
        assert!(!labels.contains(&"解压"));
        assert!(!labels.contains(&"刷新"));
        assert!(labels.contains(&"macOS 系统菜单…"));
    }

    #[test]
    fn entry_action_keeps_its_own_working_directory() {
        let target = ContextMenuTarget::entry(
            PathBuf::from(r"C:\repo\src\main.rs"),
            "main.rs".to_string(),
            false,
            PathBuf::from(r"C:\repo\src"),
            ContextMenuCapabilities {
                loading: false,
                can_paste: true,
                git_available: true,
                svn_available: false,
                supports_transfer_ops: true,
                supports_cut: true,
                supports_rename: true,
                supports_delete: true,
                supports_favorite: true,
                is_shell_namespace: false,
            },
        );
        let action = target.action(ContextMenuCommand::Paste);

        assert_eq!(action.target, PathBuf::from(r"C:\repo\src\main.rs"));
        assert_eq!(action.working_directory, PathBuf::from(r"C:\repo\src"));
        assert_eq!(action.command, ContextMenuCommand::Paste);
    }

    #[test]
    fn shell_target_allows_transfer_and_hides_unsafe_operations() {
        let target = ContextMenuTarget::entry(
            PathBuf::from(r"::{20D04FE0-3AEA-1069-A2D8-08002B30309D}\phone\photo.jpg"),
            "photo.jpg".to_string(),
            false,
            PathBuf::from(r"::{20D04FE0-3AEA-1069-A2D8-08002B30309D}\phone"),
            ContextMenuCapabilities {
                loading: false,
                can_paste: true,
                git_available: false,
                svn_available: false,
                supports_transfer_ops: true,
                supports_cut: false,
                supports_rename: false,
                supports_delete: false,
                supports_favorite: false,
                is_shell_namespace: true,
            },
        );
        let model = build_model(&target, windows_policy());
        let commands = model
            .entries
            .iter()
            .filter_map(|entry| match entry {
                ContextMenuNode::Item { command, .. } => Some(*command),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(commands.contains(&ContextMenuCommand::Open));
        assert!(commands.contains(&ContextMenuCommand::Copy));
        assert!(commands.contains(&ContextMenuCommand::CopyTo));
        assert!(commands.contains(&ContextMenuCommand::CopyPath));
        assert!(commands.contains(&ContextMenuCommand::Paste));
        assert!(commands.contains(&ContextMenuCommand::NewFolder));
        assert!(commands.contains(&ContextMenuCommand::NewFile));
        assert!(commands.contains(&ContextMenuCommand::ShowProperties));
        assert!(model.entries.iter().any(|entry| matches!(
            entry,
            ContextMenuNode::Item {
                label: "删除设备项目",
                command: ContextMenuCommand::Recycle,
                enabled: false,
                ..
            }
        )));
        for command in [
            ContextMenuCommand::AddFavorite,
            ContextMenuCommand::Cut,
            ContextMenuCommand::Rename,
            ContextMenuCommand::Recycle,
        ] {
            assert!(model.entries.iter().any(|entry| matches!(
                entry,
                ContextMenuNode::Item {
                    command: item_command,
                    enabled: false,
                    ..
                } if *item_command == command
            )));
        }
    }

    #[test]
    fn directory_action_targets_the_column_directory() {
        let target = ContextMenuTarget::directory(
            PathBuf::from(r"C:\repo\src\ui"),
            "ui".to_string(),
            ContextMenuCapabilities {
                loading: false,
                can_paste: true,
                git_available: true,
                svn_available: false,
                supports_transfer_ops: true,
                supports_cut: true,
                supports_rename: true,
                supports_delete: true,
                supports_favorite: true,
                is_shell_namespace: false,
            },
        );
        let folder_action = target.action(ContextMenuCommand::NewFolder);
        let file_action = target.action(ContextMenuCommand::NewFile);
        let favorite_action = target.action(ContextMenuCommand::AddFavorite);

        assert_eq!(folder_action.target, PathBuf::from(r"C:\repo\src\ui"));
        assert_eq!(folder_action.working_directory, folder_action.target);
        assert_eq!(file_action.target, PathBuf::from(r"C:\repo\src\ui"));
        assert_eq!(file_action.working_directory, file_action.target);
        assert_eq!(favorite_action.target, PathBuf::from(r"C:\repo\src\ui"));
        assert_eq!(favorite_action.working_directory, favorite_action.target);
    }
}
