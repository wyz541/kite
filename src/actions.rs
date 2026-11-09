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
        SelectAll,
        FocusAddress,
        FocusSearch,
        OpenGlobalSearch,
        SetGlobalSearchScopeCurrentFolder,
        SetGlobalSearchScopeFavorites,
        SetGlobalSearchScopeLocalDisks,
        SetGlobalSearchScopeNetworkVolumes,
        SetGlobalSearchKindAll,
        SetGlobalSearchKindFolders,
        SetGlobalSearchKindDocuments,
        SetGlobalSearchKindSourceCode,
        SetGlobalSearchKindMarkdown,
        SetGlobalSearchKindImages,
        SetGlobalSearchKindMedia,
        SetGlobalSearchTimeAny,
        SetGlobalSearchTimeLast24Hours,
        SetGlobalSearchTimeLast7Days,
        SetGlobalSearchTimeLast30Days,
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
    CopyTo,
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
    CompressZip,
    CompressTarGz,
    ExtractHere,
    ExtractTo,
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
pub enum ColumnPaneCommand {
    SortNameAscending,
    SortNameDescending,
    SortModifiedNewest,
    SortModifiedOldest,
    SortKind,
    SortSizeDescending,
    FilterAll,
    FilterFolders,
    FilterFiles,
    FilterMedia,
    FilterDocuments,
    GroupNone,
    GroupKind,
    GroupExtension,
    GroupModified,
    ResetView,
    ToggleDateColumn,
    ToggleKindColumn,
    ToggleSizeColumn,
    SetCompactColumnWidth,
    SetStandardColumnWidth,
    SetWideColumnWidth,
    ResetLayout,
}

#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = kite, no_json)]
pub struct ColumnPaneAction {
    pub path: PathBuf,
    pub command: ColumnPaneCommand,
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
