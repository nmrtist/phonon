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
    DeleteEntries(Vec<u64>),
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
    DeleteGroupWithEntries(String),
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
    StartQmCalculation,
    CancelQmPrompt,
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
    /// Select the Run MD preset; rebuilds the stage sequence for the system.
    SetMdRunPreset(crate::workflows::molecular_dynamics::PresetId),
    /// Set a system-type override (membrane/ligand/nucleic) for the run. Edits
    /// the separate per-run overrides, never the persisted detection context, and
    /// rebuilds the stages. `None` reverts that axis to "trust detection".
    SetMdRunOverride(crate::frontend::state::MdSystemAxis, Option<bool>),
    /// Set the run-level target temperature (K), applied to every stage.
    SetMdRunTemperature(f32),
    /// Set the production-length quick pick, applied to the production stage(s).
    SetMdRunProduction(crate::workflows::molecular_dynamics::ProductionLength),
    /// Set the run-level MD timestep (ps), applied to every dynamics stage.
    SetMdRunTimestep(f32),
    /// Toggle whether dynamics stages write a playable trajectory.
    SetMdRunSaveTrajectory(bool),
    /// Append a stage of the given kind to the run's sequence.
    AddMdRunStage(crate::workflows::molecular_dynamics::StageKind),
    /// Remove the stage at the given index from the run's sequence.
    RemoveMdRunStage(usize),
    /// Move the stage at the given index up (`true`) or down (`false`).
    MoveMdRunStage {
        index: usize,
        up: bool,
    },
    /// Apply one inline/detail edit to the stage at `index`. The detail-view
    /// widgets emit these; the dispatcher applies them in place so preset-filled
    /// defaults stay the starting point and only the touched field changes.
    EditMdRunStage {
        index: usize,
        edit: crate::frontend::state::MdStageEdit,
    },
    /// Open or close the detail view of the stage at the given index.
    ToggleMdRunStageExpanded(usize),
    RefreshEngineRegistry,
    DetectEngineVersions,
    ApplyEngineOverride(crate::engines::registry::EngineId),
    ClearEngineOverride(crate::engines::registry::EngineId),
    BrowseEngineProgram(crate::engines::registry::EngineId),
    RunConsoleCommand(String),
    /// Set the light/dark appearance preference and persist it.
    SetThemeMode(crate::backend::config::ThemeMode),
    /// Decode an MD trajectory for the given entry (from its run directory) in
    /// the background and begin playback once it is ready. The optional path
    /// selects a specific stage's trajectory (project-root-relative, as stored);
    /// `None` plays the entry's default (production) trajectory.
    LoadTrajectory(u64, Option<std::path::PathBuf>),
    /// Toggle play/pause of the active trajectory.
    ToggleTrajectoryPlay,
    /// Jump the active trajectory to a specific frame (pauses playback).
    SetTrajectoryFrame(usize),
    /// Close trajectory playback, returning the viewport to the static entry.
    StopTrajectory,
    /// Resize a sidebar by a signed delta (drag direction already applied).
    ResizeSidebar(crate::frontend::state::Side, f32),
    /// Reset a sidebar to its default width.
    ResetSidebar(crate::frontend::state::Side),
    /// Resize the bottom panel by a signed delta.
    ResizePanel(f32),
    /// Reset the bottom panel to its default height.
    ResetPanel,
}
