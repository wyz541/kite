use std::path::PathBuf;

use gpui_kit::*;

actions!(
    kite,
    [
        NavigateBack,
        NavigateForward,
        NavigateUp,
        RefreshFolder,
        OpenSelection,
        CopySelection,
        CutSelection,
        PasteSelection,
        FocusAddress,
        FocusSearch,
        CreateFolder,
        RenameSelection,
        RecycleSelection,
        ClearSearchOverlay,
        ShowListView,
        ShowColumnsView,
        TogglePathBar,
        NewTab,
        CloseTab,
        NextTab,
        PreviousTab,
        SetDefaultExplorer,
        RestoreDefaultExplorer,
        CreateStartMenuShortcut,
        CheckForUpdates,
        SaveEditor,
    ]
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextMenuCommand {
    Open,
    Copy,
    CopyPath,
    Cut,
    Paste,
    NewFolder,
    NewFile,
    AddFavorite,
    RemoveFavorite,
    Rename,
    Recycle,
    ShowProperties,
    Vcs(VcsMenuCommand),
    SystemMenu,
    Refresh,
}

#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = kite, no_json)]
pub struct ContextMenuAction {
    pub target: PathBuf,
    pub working_directory: PathBuf,
    pub command: ContextMenuCommand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VcsMenuCommand {
    GitAdd,
    GitUnstage,
    GitCommit,
    GitPull,
    GitPush,
    GitFetch,
    GitDiff,
    GitLog,
    GitRevert,
    GitClean,
    SvnUpdate,
    SvnAdd,
    SvnCommit,
    SvnLock,
    SvnUnlock,
    SvnDiff,
    SvnLog,
    SvnRevert,
    SvnCleanup,
}
