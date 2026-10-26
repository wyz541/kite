use std::path::{Path, PathBuf};

use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::*;

use crate::actions::{ContextMenuAction, ContextMenuCommand, VcsMenuCommand};
use crate::platform::HostPlatform;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub(crate) use windows::show_system_menu;

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
                system_menu_label: "Windows 原生菜单…",
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

#[derive(Clone, Debug)]
pub(crate) struct ContextMenuTarget {
    pub path: PathBuf,
    pub title: String,
    pub is_dir: bool,
    pub loading: bool,
    pub can_paste: bool,
    pub git_available: bool,
    pub svn_available: bool,
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
        model.item("复制", IconName::Copy, ContextMenuCommand::Copy, !disabled);
        model.item(
            "剪切",
            IconName::Scissors,
            ContextMenuCommand::Cut,
            !disabled,
        );
        model.item(
            "粘贴",
            IconName::ClipboardPaste,
            ContextMenuCommand::Paste,
            !disabled && target.can_paste,
        );
        if !policy.compact {
            model.separator();
        }
        model.item(
            "新建文件夹",
            IconName::FolderPlus,
            ContextMenuCommand::NewFolder,
            !disabled,
        );
        model.item(
            "重命名",
            IconName::Pencil,
            ContextMenuCommand::Rename,
            !disabled,
        );
        model.item(
            "移到回收站",
            IconName::Trash,
            ContextMenuCommand::Recycle,
            !disabled,
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
        model.separator();
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
    let providers: [&dyn ContextMenuProvider; 4] = [
        &CoreMenuProvider,
        &VcsMenuProvider,
        &SystemMenuProvider,
        &RefreshMenuProvider,
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
    target: &Path,
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
                menu = menu.menu_with_icon_and_disabled(
                    *label,
                    *icon,
                    Box::new(ContextMenuAction {
                        target: target.to_path_buf(),
                        command: *command,
                    }),
                    !enabled,
                );
            }
            ContextMenuNode::Submenu {
                label,
                icon,
                enabled,
                entries,
            } => {
                let entries = entries.clone();
                let target = target.to_path_buf();
                let submenu = PopupMenu::build(window, cx, move |menu, window, cx| {
                    append_nodes(menu, &entries, &target, window, cx)
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
    window: &mut Window,
    cx: &mut App,
) -> PopupMenu {
    let policy = ContextMenuPolicy::current();
    let model = build_model(target, policy);
    let menu = menu.label(target.title.clone()).separator();
    append_nodes(menu, &model.entries, &target.path, window, cx)
}

#[cfg(test)]
mod tests {
    use super::{
        ContextMenuCommand, ContextMenuModel, ContextMenuNode, ContextMenuPolicy,
        ContextMenuTarget, VcsMenuCommand, build_model,
    };
    use crate::platform::HostPlatform;
    use std::path::PathBuf;

    fn target() -> ContextMenuTarget {
        ContextMenuTarget {
            path: PathBuf::from(r"C:\repo\src"),
            title: "src".to_string(),
            is_dir: true,
            loading: false,
            can_paste: true,
            git_available: true,
            svn_available: true,
        }
    }

    fn windows_policy() -> ContextMenuPolicy {
        ContextMenuPolicy::for_platform(HostPlatform::Windows)
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

        assert!(labels.contains(&"Git"));
        assert!(labels.contains(&"SVN"));
        assert!(labels.contains(&"Windows 原生菜单…"));
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
                    label: "Windows 原生菜单…",
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

        assert!(native_shell_enabled);
        assert!(vcs_disabled);
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
        assert!(labels.contains(&"macOS 系统菜单…"));
    }
}
