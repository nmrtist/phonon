#[derive(Debug, Clone)]
pub enum AppAction {
    CreateProject,
    OpenProject,
    OpenRecentProject(std::path::PathBuf),
    CloseProject,
    SaveProject,
    NewEmptyEntry,
    OpenFile,
    OpenPdbFetchDialog,
    FetchPdb,
    CancelPdbFetch,
    Save,
    SaveAs,
    Undo,
    Redo,
    EditStructure,
    ApplyStructureEdits,
    CancelStructureEdits,
    SelectAll,
    InvertSelection,
    ClearSelection,
    /// Replace the selection with every atom of a chemical category (protein,
    /// solvent, ligand, …).
    SelectCategory(crate::domain::AtomCategory),
    SelectAtom {
        atom_index: usize,
        toggle: bool,
    },
    /// Apply a per-atom drawing style to the current selection (or, when the
    /// selection is empty, to every atom as the new default).
    SetSelectionStyle(crate::frontend::state::AtomStyle),
    /// Remove per-atom style overrides for the current selection (or all atoms
    /// when the selection is empty), reverting them to the category defaults.
    ResetSelectionStyle,
    /// Set the project-level default style for a whole category (the "project
    /// display style"), overriding the software default for every atom of that
    /// category that has no per-atom override.
    SetCategoryStyle(
        crate::domain::AtomCategory,
        crate::frontend::state::AtomStyle,
    ),
    ActivateEntry(u64),
    DeleteEntry(u64),
    RenameEntry {
        entry_id: u64,
        new_name: String,
    },
    CreateGroup {
        name: String,
    },
    RenameGroup {
        group_id: String,
        new_name: String,
    },
    DeleteGroup(String),
    MoveEntryToGroup {
        entry_id: u64,
        group_id: String,
    },
    CreateTask(&'static str),
    RunTask(u64),
    OpenTaskPanel(u64),
    CloseTaskPanel(u64),
    ActivateTaskPanel(u64),
    PreviewFramework,
    BuildFramework,
    CancelFramework,
    PreviewNanosheet,
    BuildNanosheet,
    CancelNanosheet,
    SaveBuildingBlock,
    CancelBuildingBlock,
    StartOptimization,
    CancelOptimizationPrompt,
    ConfirmSupercell,
    CancelSupercellPrompt,
    ConfirmProteinPrep,
    CancelProteinPrepPrompt,
    ConfirmMdSystem,
    CancelMdSystemPrompt,
    PickMdTopologyOverride,
    /// Select a custom force field from the library by name (or `None` for
    /// built-in only) for the MD System Builder; loads and caches its text.
    SelectCustomForceField(Option<String>),
    /// Save the MD System Builder's draft custom force field to the library under
    /// its draft name, then select it.
    SaveCustomForceField,
    /// Delete the named custom force field from the library.
    DeleteCustomForceField(String),
    /// Open a file picker and load a `.itp` into the draft custom force field.
    ImportCustomForceFieldFile,
    StartMdRun,
    CancelMdRunPrompt,
    RefreshEngineRegistry,
    DetectEngineVersions,
    ApplyEngineOverride(crate::engines::registry::EngineId),
    ClearEngineOverride(crate::engines::registry::EngineId),
    BrowseEngineProgram(crate::engines::registry::EngineId),
    RunConsoleCommand(String),
}
