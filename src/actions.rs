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
        GoToFolder,
        ShowListView,
        ShowColumnsView,
        TogglePathBar,
        NewTab,
        CloseTab,
        NextTab,
        PreviousTab,
    ]
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowContextCommand {
    Open,
    Copy,
    Cut,
    Rename,
    Recycle,
}

#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = kite, no_json)]
pub struct RowContextAction {
    pub row: usize,
    pub command: RowContextCommand,
}
