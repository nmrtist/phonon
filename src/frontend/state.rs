use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    backend::{
        config::{AppConfig, RecentProject},
        entries::EntryStore,
        history::{EditSnapshot, History},
        project::WorkspaceSession,
        storage::ProjectSnapshot,
        tasks::TaskManager,
    },
    domain::Structure,
    frontend::{
        AtomSelection, BuildingBlockEditor, CommandConsoleState, NanosheetBuilderPanel,
        ReticularBuilderPanel, StructureEditor, ViewCamera, ViewportVisualState,
        jobs::JobManager,
        viewport::ViewportCache,
        viewport_defaults::{apply_entry_render_defaults, apply_solvent_render_default},
    },
    io::structure_io,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryView {
    EntryList,
    Tasks,
    Settings,
}

impl PrimaryView {
    pub fn all() -> &'static [Self] {
        &[Self::EntryList, Self::Tasks, Self::Settings]
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::EntryList => egui_phosphor::regular::LIST,
            Self::Tasks => egui_phosphor::regular::ROCKET_LAUNCH,
            Self::Settings => egui_phosphor::regular::GEAR,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::EntryList => "Entry List",
            Self::Tasks => "Tasks",
            Self::Settings => "Settings",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelTab {
    Output,
    Console,
    TaskMonitor,
}

impl PanelTab {
    pub fn all() -> &'static [Self] {
        &[Self::Console, Self::TaskMonitor, Self::Output]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Console => "Console",
            Self::TaskMonitor => "Task Monitor",
            Self::Output => "Output",
        }
    }
}

/// An item in the sidebar list that can be selected: either an entry or a group header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionItem {
    Entry(u64),
    Group(String),
}

#[derive(Debug, Clone, Default)]
pub struct EntryListState {
    pub search_query: String,
    pub selected_entry_ids: std::collections::BTreeSet<u64>,
    pub selected_group_ids: std::collections::BTreeSet<String>,
    pub selection_anchor: Option<SelectionItem>,
    pub collapsed_group_ids: std::collections::BTreeSet<String>,
    pub renaming_entry_id: Option<u64>,
    pub rename_buffer: String,
    pub creating_group: bool,
    pub new_group_name: String,
    pub renaming_group_id: Option<String>,
    pub rename_group_buffer: String,
    /// Set once focus is handed to the group rename editor, so it is requested
    /// only on the first frame of a rename.
    pub rename_group_focus_requested: bool,
}

#[derive(Debug, Clone)]
pub struct LayoutState {
    pub active_primary_view: PrimaryView,
    pub active_panel_tab: PanelTab,
    pub show_primary_sidebar: bool,
    pub show_secondary_sidebar: bool,
    pub show_panel: bool,
    pub primary_sidebar_width: f32,
    pub secondary_sidebar_width: f32,
    pub panel_height: f32,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            active_primary_view: PrimaryView::EntryList,
            active_panel_tab: PanelTab::Console,
            show_primary_sidebar: true,
            show_secondary_sidebar: false,
            show_panel: true,
            primary_sidebar_width: 240.0,
            secondary_sidebar_width: 320.0,
            panel_height: 180.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateOptimizationScope {
    AllAtoms,
    SelectedAtoms,
}

/// Per-atom drawing style, applied to a selection of atoms. Mirrors the common
/// representation types in ChimeraX / PyMOL / VMD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AtomStyle {
    /// Polymer-backbone ribbon. Only standard amino-acid residues actually
    /// render as cartoon; other atoms styled this way are not drawn.
    Cartoon,
    /// Not drawn at all.
    Hidden,
    /// A small flat disc per atom (PyMOL `nonbonded` / dots). Cheapest; ideal
    /// for bulk solvent and ions.
    Point,
    /// Bonds as thin lines only; atoms carry no marker (PyMOL `lines` /
    /// ChimeraX `wire`). Ideal for bulk solvent — pure lines, no dots.
    Wireframe,
    /// Bonds as cylinders, no atom spheres (VMD Licorice / PyMOL `sticks`).
    Stick,
    /// Cylinders plus small atom spheres.
    #[default]
    BallAndStick,
    /// Full van der Waals spheres, no bonds (VMD VDW / PyMOL `spheres` / CPK).
    Sphere,
}

impl AtomStyle {
    pub fn all() -> &'static [Self] {
        &[
            Self::Cartoon,
            Self::BallAndStick,
            Self::Stick,
            Self::Wireframe,
            Self::Sphere,
            Self::Point,
            Self::Hidden,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cartoon => "Cartoon",
            Self::Hidden => "Hidden",
            Self::Point => "Dots",
            Self::Wireframe => "Wireframe",
            Self::Stick => "Stick",
            Self::BallAndStick => "Ball-and-stick",
            Self::Sphere => "Sphere (VdW)",
        }
    }

    /// Stable string token for persistence and the console.
    pub fn token(self) -> &'static str {
        match self {
            Self::Cartoon => "cartoon",
            Self::Hidden => "hidden",
            Self::Point => "dots",
            Self::Wireframe => "wireframe",
            Self::Stick => "stick",
            Self::BallAndStick => "ball-stick",
            Self::Sphere => "sphere",
        }
    }

    pub fn from_token(token: &str) -> Option<Self> {
        Some(match token {
            "cartoon" => Self::Cartoon,
            "hidden" | "hide" => Self::Hidden,
            "dots" | "point" | "points" => Self::Point,
            "wireframe" | "line" | "lines" => Self::Wireframe,
            "stick" | "licorice" => Self::Stick,
            "ball-stick" | "ball_and_stick" => Self::BallAndStick,
            "sphere" | "spheres" | "vdw" => Self::Sphere,
            _ => return None,
        })
    }

    /// Whether atoms in this style draw a tessellated sphere, and at what
    /// fraction of the element's display radius. `None` means the atom is drawn
    /// as a flat point disc, via the cartoon path, or not at all.
    pub fn sphere_radius_scale(self) -> Option<f32> {
        match self {
            Self::Sphere => Some(1.0),
            Self::BallAndStick => Some(0.78),
            // A small joint so isolated atoms (lone ions / water O) stay visible.
            Self::Stick => Some(0.30),
            // Point is a flat disc; Wireframe draws only its line bonds (no atom
            // marker); Cartoon/Hidden draw no atom here.
            Self::Wireframe | Self::Point | Self::Cartoon | Self::Hidden => None,
        }
    }

    /// Whether visible atoms in this style are drawn as a flat point disc. Only
    /// `Point` (Dots) draws a disc; `Wireframe` shows bonds as lines with no
    /// per-atom marker.
    pub fn draws_point(self) -> bool {
        matches!(self, Self::Point)
    }

    /// True for styles whose per-atom geometry is heavy enough that very large
    /// selections must be downgraded to points to stay within the GPU buffer.
    pub fn is_heavy(self) -> bool {
        self.sphere_radius_scale().is_some()
    }

    /// Whether bonds touching an atom of this style are drawn as solid
    /// cylinders.
    pub fn draws_stick_bonds(self) -> bool {
        matches!(self, Self::Stick | Self::BallAndStick)
    }

    /// Whether bonds touching an atom of this style are drawn as thin lines.
    pub fn draws_line_bonds(self) -> bool {
        matches!(self, Self::Wireframe)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OptimizationPrompt {
    pub cell: crate::engines::forcefield::CellOptimizationOptions,
    pub coordinate_scope: CoordinateOptimizationScope,
    pub allow_cell_optimization: bool,
}

impl OptimizationPrompt {
    pub fn new(allow_cell_optimization: bool, selection: &AtomSelection) -> Self {
        Self {
            cell: if allow_cell_optimization {
                crate::engines::forcefield::CellOptimizationOptions::lengths_only()
            } else {
                crate::engines::forcefield::CellOptimizationOptions::default()
            },
            coordinate_scope: if selection.is_empty() {
                CoordinateOptimizationScope::AllAtoms
            } else {
                CoordinateOptimizationScope::SelectedAtoms
            },
            allow_cell_optimization,
        }
    }

    pub fn options(
        &self,
        selection: &AtomSelection,
    ) -> crate::engines::forcefield::OptimizationOptions {
        crate::engines::forcefield::OptimizationOptions {
            atoms: match self.coordinate_scope {
                CoordinateOptimizationScope::AllAtoms => {
                    crate::engines::forcefield::AtomOptimizationScope::All
                }
                CoordinateOptimizationScope::SelectedAtoms => {
                    crate::engines::forcefield::AtomOptimizationScope::Selected(
                        selection.ordered_indices(),
                    )
                }
            },
            cell: if self.allow_cell_optimization {
                self.cell
            } else {
                crate::engines::forcefield::CellOptimizationOptions::default()
            },
            ..crate::engines::forcefield::OptimizationOptions::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SupercellPrompt {
    pub repeats: [u32; 3],
}

/// User-editable configuration for the Protein Preparation task. This round
/// exposes only hydrogen completion; the other fields are placeholders for
/// future steps (protonation states, terminus patching, missing-atom repair)
/// and are not yet wired.
#[derive(Debug, Clone, Copy)]
pub struct ProteinPrepPrompt {
    /// Add missing hydrogens with chemistry heuristics.
    pub add_hydrogens: bool,
}

impl Default for ProteinPrepPrompt {
    fn default() -> Self {
        Self {
            add_hydrogens: true,
        }
    }
}

/// Which sizing strategy the MD system panel is currently editing. Both sets of
/// values are retained so toggling between modes does not lose the user's input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MdSystemSizingMode {
    #[default]
    Padding,
    Absolute,
}

/// User-editable configuration for the MD system builder. Padding and absolute
/// edge lengths are both held (per-axis, in angstroms); `mode` selects which
/// drives the build, and `shape` selects the lattice geometry.
///
/// The solvation fields mirror [`SolvationOptions`](crate::workflows::molecular_dynamics::SolvationOptions)
/// so the System Builder can box, solvate, and ionize in one step.
/// When `solvate` is false the box is built empty and the remaining fields are ignored.
#[derive(Debug, Clone)]
pub struct MdSystemPrompt {
    /// Human-readable run name; becomes the run directory's name. Seeded with a
    /// suggested `{kind}-N` when the panel opens, but freely editable.
    pub run_name: String,
    /// Which engine assembles the system. GROMACS (the default) produces a
    /// force-field topology a run reuses; the built-in path is geometry only.
    pub engine: MdBuildEngine,
    /// For a periodic framework (nanosheet) built with GROMACS, whether the
    /// sheet is modeled rigidly (frozen) or flexibly (bonded). Ignored for
    /// non-framework structures.
    pub framework_mode: crate::workflows::molecular_dynamics::FrameworkMode,
    /// For a periodic framework (nanosheet), the simulation cell's lattice
    /// parameters `[a, b, c, α, β, γ]` (lengths in A, angles in degrees), seeded
    /// from the input crystal cell when the panel opens and freely editable. The
    /// build uses this cell verbatim, preserving its shape (e.g. hexagonal), so
    /// the box matches the material rather than a generic cuboid. `None` until
    /// seeded / for non-framework structures.
    pub framework_cell: Option<[f32; 6]>,
    /// Name of the custom force field (from the reusable library) merged into a
    /// framework build, or `None` for built-in parameters only. Used to cover
    /// elements the built-in tables lack, or to override built-in types.
    pub custom_force_field: Option<String>,
    /// Cached `.itp` text of the selected `custom_force_field`, loaded when the
    /// selection changes so the panel and build don't re-read it each frame.
    pub custom_force_field_text: Option<String>,
    /// Draft name and `.itp` text for composing/importing a new custom force
    /// field before saving it to the library.
    pub custom_ff_draft_name: String,
    pub custom_ff_draft: String,
    pub mode: MdSystemSizingMode,
    pub padding_angstrom: [f32; 3],
    pub absolute_angstrom: [f32; 3],
    pub shape: crate::workflows::molecular_dynamics::BoxShape,
    /// Fill the box with explicit water and ions after building it.
    pub solvate: bool,
    pub water: crate::workflows::molecular_dynamics::WaterModel,
    pub force_field: String,
    /// Add the minimum ions needed to make the system net-neutral.
    pub neutralize: bool,
    /// Add a background salt bath at `salt_concentration_molar`.
    pub add_salt: bool,
    pub salt_concentration_molar: f32,
    pub positive_ion: String,
    pub negative_ion: String,
}

impl Default for MdSystemPrompt {
    fn default() -> Self {
        // Seed the solvation fields from the engine-neutral defaults so the GUI
        // and the `md solvate` console command start from the same place.
        let solv = crate::workflows::molecular_dynamics::SolvationOptions::default();
        Self {
            run_name: String::new(),
            engine: MdBuildEngine::default(),
            framework_mode: crate::workflows::molecular_dynamics::FrameworkMode::Rigid,
            framework_cell: None,
            custom_force_field: None,
            custom_force_field_text: None,
            custom_ff_draft_name: String::new(),
            custom_ff_draft: String::new(),
            mode: MdSystemSizingMode::Padding,
            padding_angstrom: [crate::workflows::molecular_dynamics::DEFAULT_PADDING_ANGSTROM; 3],
            absolute_angstrom: [30.0; 3],
            shape: crate::workflows::molecular_dynamics::BoxShape::default(),
            solvate: false,
            water: solv.water,
            force_field: crate::workflows::molecular_dynamics::DEFAULT_FORCE_FIELD.to_string(),
            neutralize: solv.neutralize,
            add_salt: false,
            salt_concentration_molar: 0.15,
            positive_ion: solv.positive_ion,
            negative_ion: solv.negative_ion,
        }
    }
}

impl MdSystemPrompt {
    pub fn config(&self) -> crate::workflows::molecular_dynamics::MdSystemConfig {
        use crate::workflows::molecular_dynamics::{BoxSizing, MdSystemConfig};
        let sizing = match self.mode {
            MdSystemSizingMode::Padding => BoxSizing::Padding {
                padding_angstrom: self.padding_angstrom,
            },
            MdSystemSizingMode::Absolute => BoxSizing::Absolute {
                edges_angstrom: self.absolute_angstrom,
            },
        };
        MdSystemConfig {
            sizing,
            shape: self.shape,
        }
    }

    /// The solvation request this prompt describes, or `None` when solvation is
    /// disabled. Folds the `add_salt` toggle and concentration into the engine's
    /// `Option<f32>` concentration field.
    pub fn solvation_options(
        &self,
    ) -> Option<crate::workflows::molecular_dynamics::SolvationOptions> {
        if !self.solvate {
            return None;
        }
        Some(crate::workflows::molecular_dynamics::SolvationOptions {
            water: self.water,
            positive_ion: self.positive_ion.clone(),
            negative_ion: self.negative_ion.clone(),
            neutralize: self.neutralize,
            concentration_molar: self.add_salt.then_some(self.salt_concentration_molar),
        })
    }
}

/// Which engine the MD System Builder uses to assemble the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MdBuildEngine {
    /// Run GROMACS' pdb2gmx → editconf → solvate → genion pipeline. Assigns a
    /// force field and writes a `topol.top` an MD run reuses directly.
    #[default]
    Gromacs,
    /// Built-in geometry-only build: periodic box plus solvation coordinates,
    /// with no force field or topology. A run still needs a topology supplied
    /// separately.
    BuiltIn,
}

impl MdBuildEngine {
    pub fn all() -> &'static [Self] {
        &[Self::Gromacs, Self::BuiltIn]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Gromacs => "GROMACS",
            Self::BuiltIn => "Built-in",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MdEngineChoice {
    #[default]
    Gromacs,
}

impl MdEngineChoice {
    pub fn all() -> &'static [Self] {
        &[Self::Gromacs]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Gromacs => "GROMACS",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MdRunStepPreset {
    EnergyMinimization,
    Nvt,
    Npt,
    Production,
    Custom,
}

impl MdRunStepPreset {
    pub fn all() -> &'static [Self] {
        &[
            Self::EnergyMinimization,
            Self::Nvt,
            Self::Npt,
            Self::Production,
            Self::Custom,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::EnergyMinimization => "EM",
            Self::Nvt => "NVT",
            Self::Npt => "NPT",
            Self::Production => "MD",
            Self::Custom => "Custom",
        }
    }

    pub fn default_stage_name(self) -> &'static str {
        match self {
            Self::EnergyMinimization => "em",
            Self::Nvt => "nvt",
            Self::Npt => "npt",
            Self::Production => "md",
            Self::Custom => "step",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MdRunStepPrompt {
    pub preset: MdRunStepPreset,
    pub stage_name: String,
    pub settings: crate::engines::gromacs::MdpSettings,
}

impl MdRunStepPrompt {
    pub fn from_preset(preset: MdRunStepPreset, temperature_k: f32, timestep_ps: f32) -> Self {
        let mut settings = match preset {
            MdRunStepPreset::EnergyMinimization => {
                crate::engines::gromacs::MdpSettings::energy_minimization()
            }
            MdRunStepPreset::Nvt => crate::engines::gromacs::MdpSettings::nvt(temperature_k),
            MdRunStepPreset::Npt => crate::engines::gromacs::MdpSettings::npt(temperature_k),
            MdRunStepPreset::Production => {
                crate::engines::gromacs::MdpSettings::production(500_000, temperature_k)
            }
            MdRunStepPreset::Custom => crate::engines::gromacs::MdpSettings::default(),
        };
        if !settings.integrator.is_minimization() {
            settings.timestep_ps = timestep_ps;
        }
        Self {
            preset,
            stage_name: preset.default_stage_name().to_string(),
            settings,
        }
    }

    pub fn reapply_preset(
        &mut self,
        preset: MdRunStepPreset,
        temperature_k: f32,
        timestep_ps: f32,
    ) {
        let stage_name = self.stage_name.clone();
        *self = Self::from_preset(preset, temperature_k, timestep_ps);
        self.stage_name = stage_name;
    }
}

#[derive(Debug, Clone)]
pub struct MdRunPrompt {
    /// Human-readable run name; becomes the run directory's name. Seeded with a
    /// suggested `{kind}-N` when the panel opens, but freely editable.
    pub run_name: String,
    pub engine: MdEngineChoice,
    pub steps: Vec<MdRunStepPrompt>,
    pub topology_override_path: Option<PathBuf>,
    pub show_advanced: bool,
}

impl Default for MdRunPrompt {
    fn default() -> Self {
        Self {
            run_name: String::new(),
            engine: MdEngineChoice::Gromacs,
            steps: Vec::new(),
            topology_override_path: None,
            show_advanced: false,
        }
    }
}

impl MdRunPrompt {
    pub fn reference_temperature(&self) -> f32 {
        self.steps
            .iter()
            .find_map(|step| {
                step.settings
                    .temperature_coupling
                    .as_ref()
                    .and_then(|tc| tc.ref_t.first().copied())
                    .or_else(|| {
                        step.settings
                            .velocity_generation
                            .as_ref()
                            .map(|velocity| velocity.gen_temp)
                    })
            })
            .unwrap_or(300.0)
    }

    pub fn reference_timestep(&self) -> f32 {
        self.steps
            .iter()
            .find_map(|step| {
                (!step.settings.integrator.is_minimization() && step.settings.timestep_ps > 0.0)
                    .then_some(step.settings.timestep_ps)
            })
            .unwrap_or(0.002)
    }

    pub fn add_step(&mut self, preset: MdRunStepPreset) {
        let temperature_k = self.reference_temperature();
        let timestep_ps = self.reference_timestep();
        self.steps.push(MdRunStepPrompt::from_preset(
            preset,
            temperature_k,
            timestep_ps,
        ));
    }

    pub fn add_relax_template(&mut self) {
        self.add_step(MdRunStepPreset::EnergyMinimization);
        self.add_step(MdRunStepPreset::Nvt);
        self.add_step(MdRunStepPreset::Npt);
    }
}

/// Editable draft for one engine's launch override in the Settings panel.
/// `command_prefix` is held as a single whitespace-separated line for easy
/// editing (e.g. `wsl.exe -e`); it is split on apply.
#[derive(Debug, Clone, Default)]
pub struct EngineDraft {
    pub command_prefix: String,
    pub program: String,
}

impl EngineDraft {
    pub fn from_launch(launch: &crate::engines::registry::EngineLaunch) -> Self {
        Self {
            command_prefix: launch.command_prefix.join(" "),
            program: launch.program.clone(),
        }
    }

    /// Build an [`EngineLaunch`] from the draft, or `None` if no program is
    /// set (which the dispatcher treats as "clear this override").
    pub fn to_launch(&self) -> Option<crate::engines::registry::EngineLaunch> {
        let program = self.program.trim();
        if program.is_empty() {
            return None;
        }
        Some(crate::engines::registry::EngineLaunch {
            command_prefix: self
                .command_prefix
                .split_whitespace()
                .map(str::to_string)
                .collect(),
            program: program.to_string(),
        })
    }
}

/// State backing the Settings primary view. The engine registry is probed
/// lazily (probing spawns `--version` subprocesses) and cached here.
#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    pub engine_registry: Option<crate::engines::registry::EngineRegistry>,
    pub engine_drafts: BTreeMap<String, EngineDraft>,
    /// When engine `--version` strings were last detected. Detection is slow
    /// (a WSL launch cold-starts the VM), so it runs only on explicit user
    /// request; the panel shows this so the displayed versions can be judged.
    pub engine_versions_checked_at: Option<std::time::SystemTime>,
    /// Free-text filter for the settings panel sections.
    pub search_query: String,
}

pub struct UiState {
    pub layout: LayoutState,
    pub entry_list: EntryListState,
    pub settings: SettingsState,
    pub camera: ViewCamera,
    pub viewport_cache: ViewportCache,
    /// Set once at startup when the GPU molecule renderer initializes
    /// successfully; gates the GPU rendering path in the viewport.
    pub gpu_ready: bool,
    pub hovered_atom: Option<usize>,
    pub selection: AtomSelection,
    pub viewport: ViewportVisualState,
    pub project_viewport: ViewportVisualState,
    pub entry_viewports: BTreeMap<u64, ViewportVisualState>,
    pub scripted_viewport_size: [u32; 2],
    pub console: CommandConsoleState,
    pub editor: Option<StructureEditor>,
    pub reticular_builder: Option<ReticularBuilderPanel>,
    pub nanosheet_builder: Option<NanosheetBuilderPanel>,
    pub block_editor: Option<BuildingBlockEditor>,
    pub pending_optimization: Option<OptimizationPrompt>,
    pub pending_supercell: Option<SupercellPrompt>,
    pub pending_protein_prep: Option<ProteinPrepPrompt>,
    pub pending_md_system: Option<MdSystemPrompt>,
    pub pending_md_run: Option<MdRunPrompt>,
    pub pending_pdb_fetch: Option<String>,
    /// Cached solvation count preview for the System Builder panel. Recomputed
    /// (which opens the force-field DB and grid-fills the box) only when
    /// `md_solvation_preview_key` changes, so the panel stays responsive.
    pub md_solvation_preview:
        Option<Result<crate::workflows::molecular_dynamics::SolvationEstimate, String>>,
    pub md_solvation_preview_key: u64,
    /// Active trajectory playback (loaded from an MD-output entry's run
    /// directory), or `None` when nothing is playing.
    pub trajectory: Option<crate::frontend::trajectory::TrajectoryPlayback>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            layout: LayoutState::default(),
            entry_list: EntryListState::default(),
            settings: SettingsState::default(),
            camera: ViewCamera::default(),
            viewport_cache: ViewportCache::default(),
            gpu_ready: false,
            hovered_atom: None,
            selection: AtomSelection::default(),
            viewport: ViewportVisualState::default(),
            project_viewport: ViewportVisualState::default(),
            entry_viewports: BTreeMap::new(),
            scripted_viewport_size: [1180, 760],
            console: CommandConsoleState::default(),
            editor: None,
            reticular_builder: None,
            nanosheet_builder: None,
            block_editor: None,
            pending_optimization: None,
            pending_supercell: None,
            pending_protein_prep: None,
            pending_md_system: None,
            pending_md_run: None,
            pending_pdb_fetch: None,
            md_solvation_preview: None,
            md_solvation_preview_key: 0,
            trajectory: None,
        }
    }
}

pub struct AppState {
    pub workspace: WorkspaceSession,
    pub config: AppConfig,
    pub recent_projects: Vec<RecentProject>,
    pub entries: EntryStore,
    pub history: History,
    pub tasks: TaskManager,
    pub jobs: JobManager,
    pub ui: UiState,
    pub message: String,
    pub output_log: Vec<String>,
    pub active_task_run: Option<u64>,
    pub edit_origin: Option<EditSnapshot>,
    pub builder_origin: Option<EditSnapshot>,
    pub optimization_origin: Option<EditSnapshot>,
    workspace_structure: Structure,
    workspace_save_path: PathBuf,
    last_logged_message: String,
    /// egui time (seconds) at which a coalesced autosave should flush, or `None`
    /// when no project change is pending. Set by the dispatcher after a
    /// persist-worthy action and drained on the UI thread once the debounce
    /// window elapses, so rapid interactions don't each pay a full project save.
    autosave_deadline: Option<f64>,
}

impl AppState {
    pub fn new(
        structure: Structure,
        source_path: Option<PathBuf>,
        workspace: WorkspaceSession,
        config: AppConfig,
        recent_projects: Vec<RecentProject>,
        project_snapshot: Option<ProjectSnapshot>,
    ) -> Self {
        let save_path =
            structure_io::default_structure_save_path(&structure, source_path.as_deref());
        let has_initial_entry = source_path.is_some()
            || !structure.atoms.is_empty()
            || !structure.bonds.is_empty()
            || structure.cell.is_some()
            || {
                let trimmed_title = structure.title.trim();
                !trimmed_title.is_empty() && trimmed_title != "Untitled"
            };
        let message = "Ready to open or edit a structure".to_string();
        let entries = if let Some(snapshot) = project_snapshot.as_ref() {
            snapshot.entries.clone()
        } else if has_initial_entry {
            EntryStore::with_initial(structure.clone(), source_path, save_path.clone())
        } else {
            EntryStore::new_empty()
        };
        let tasks = project_snapshot
            .as_ref()
            .map(|snapshot| snapshot.tasks.clone())
            .unwrap_or_default();
        let mut state = Self {
            workspace,
            config,
            recent_projects,
            entries,
            history: History::default(),
            tasks,
            jobs: JobManager::default(),
            ui: UiState::default(),
            message: message.clone(),
            output_log: vec![message.clone()],
            active_task_run: None,
            edit_origin: None,
            builder_origin: None,
            optimization_origin: None,
            workspace_structure: structure,
            workspace_save_path: save_path,
            last_logged_message: message,
            autosave_deadline: None,
        };
        if let Some(snapshot) = project_snapshot.as_ref() {
            state.ui.project_viewport = snapshot.view.viewport.clone();
            state.ui.viewport = state.ui.project_viewport.clone();
            state.ui.entry_viewports = snapshot.view.entry_viewports.clone();
            if let Some(entry_id) = state.entries.active_entry_id() {
                state
                    .ui
                    .entry_viewports
                    .entry(entry_id)
                    .or_insert_with(|| state.ui.project_viewport.clone());
            }
            state.history = snapshot.history.clone();
            state
                .history
                .set_active_entry(state.entries.active_entry_id());
        }
        state.load_viewport_for_active_entry();
        state.ui.entry_list.selected_entry_ids.clear();
        if let Some(id) = state.entries.active_entry_id() {
            state.ui.entry_list.selected_entry_ids.insert(id);
        }
        state
            .history
            .set_active_entry(state.entries.active_entry_id());
        state
    }

    pub fn scratch(config: AppConfig, recent_projects: Vec<RecentProject>) -> Self {
        Self::new(
            Structure::empty(),
            None,
            WorkspaceSession::Scratch,
            config,
            recent_projects,
            None,
        )
    }

    pub fn has_active_entry(&self) -> bool {
        self.entries.active_entry().is_some()
    }

    pub fn structure(&self) -> &Structure {
        self.entries
            .active_entry()
            .map(|entry| &entry.structure)
            .unwrap_or(&self.workspace_structure)
    }

    pub fn structure_mut(&mut self) -> &mut Structure {
        if let Some(entry) = self.entries.active_entry_mut() {
            &mut entry.structure
        } else {
            &mut self.workspace_structure
        }
    }

    pub fn mark_structure_changed(&mut self) {
        self.entries.bump_active_revision();
        self.ui.hovered_atom = None;
        self.ui.viewport_cache.clear();
        let atom_count = self.structure().atoms.len();
        self.ui.viewport.retain_atom_styles(atom_count);
    }

    pub fn runs_dir(&self) -> std::path::PathBuf {
        self.workspace
            .project()
            .map(|project| project.root.join("runs"))
            .unwrap_or_else(|| std::env::temp_dir().join("phonon").join("runs"))
    }

    pub fn apply_render_defaults_for_active_entry(&mut self) {
        let structure = self.structure().clone();
        apply_entry_render_defaults(&mut self.ui.viewport, &structure);
    }

    pub fn save_viewport_for_active_entry(&mut self) {
        let Some(entry_id) = self.entries.active_entry_id() else {
            return;
        };
        self.ui
            .entry_viewports
            .insert(entry_id, self.ui.viewport.clone());
    }

    pub fn load_viewport_for_active_entry(&mut self) {
        let Some(entry_id) = self.entries.active_entry_id() else {
            self.ui.viewport = ViewportVisualState::default();
            return;
        };
        if let Some(viewport) = self.ui.entry_viewports.get(&entry_id).cloned() {
            self.ui.viewport = viewport;
            // Migrate entries saved before the bulk-solvent wireframe default: if
            // no per-atom style was ever stored for this entry, apply the default
            // now. A non-empty map means the user (or a newer build) already
            // configured atoms, so we leave their choices untouched.
            if self.ui.viewport.atom_styles.is_empty() {
                let structure = self.structure().clone();
                apply_solvent_render_default(&mut self.ui.viewport, &structure);
            }
        } else {
            self.ui.viewport = self.ui.project_viewport.clone();
            self.apply_render_defaults_for_active_entry();
        }
        // Category styles are project-level: every entry shows the project's
        // current category defaults, regardless of what was stored per entry.
        self.ui.viewport.category_styles = self.ui.project_viewport.category_styles.clone();
    }

    pub fn project_view_settings(&self) -> crate::backend::storage::ProjectViewSettings {
        let mut entry_viewports = self.ui.entry_viewports.clone();
        if let Some(entry_id) = self.entries.active_entry_id() {
            entry_viewports.insert(entry_id, self.ui.viewport.clone());
        }
        crate::backend::storage::ProjectViewSettings {
            viewport: self.ui.project_viewport.clone(),
            entry_viewports,
        }
    }

    pub fn save_path(&self) -> &PathBuf {
        self.entries
            .active_entry()
            .map(|entry| &entry.save_path)
            .unwrap_or(&self.workspace_save_path)
    }

    pub fn set_source_path(&mut self, source_path: Option<PathBuf>) {
        if let Some(entry) = self.entries.active_entry_mut() {
            entry.source_path = source_path;
        }
    }

    pub fn set_save_path(&mut self, save_path: PathBuf) {
        if let Some(entry) = self.entries.active_entry_mut() {
            entry.save_path = save_path;
        } else {
            self.workspace_save_path = save_path;
        }
    }

    pub fn current_entry_label(&self) -> String {
        self.entries
            .active_entry()
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| self.workspace.label())
    }

    pub fn workspace_label(&self) -> String {
        self.workspace.label()
    }

    /// Directory where downloaded structures (e.g. fetched PDB files) are kept.
    /// Anchored at the project root when a project is open, otherwise relative
    /// to the current working directory.
    pub fn structures_dir(&self) -> std::path::PathBuf {
        let subdir = crate::io::pdb_fetch::DOWNLOAD_SUBDIR;
        match self.workspace.project() {
            Some(project) => project.root.join(subdir),
            None => std::path::PathBuf::from(subdir),
        }
    }

    /// A cheap hash of the persisted entry/group state — entry set, per-entry
    /// revision (bumped on every edit), names, and grouping. Deliberately
    /// excludes transient/view state (active tab, selection, camera, render
    /// styles): the autosave policy only saves when entries are added, removed,
    /// or edited, leaving view-only changes to be persisted at exit. Touches no
    /// geometry, so it is fast even for entry-heavy projects.
    pub fn entries_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.entries.records.len().hash(&mut hasher);
        for record in &self.entries.records {
            record.id.hash(&mut hasher);
            record.revision.hash(&mut hasher);
            record.name.hash(&mut hasher);
            record.group_id.hash(&mut hasher);
            // Provenance (e.g. an entry becoming an MD-run output) is persisted,
            // so a change to it must trigger an autosave too.
            record.origin.kind_token().hash(&mut hasher);
            record.origin.trajectory().hash(&mut hasher);
        }
        for group in &self.entries.groups {
            group.id.hash(&mut hasher);
            group.name.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Schedule a coalesced autosave to flush `delay_seconds` after `now_seconds`
    /// (both egui clock seconds). Repeated calls push the deadline back so a burst
    /// of actions collapses into a single save once the user pauses.
    pub fn request_autosave(&mut self, now_seconds: f64, delay_seconds: f64) {
        self.autosave_deadline = Some(now_seconds + delay_seconds);
    }

    pub fn autosave_deadline(&self) -> Option<f64> {
        self.autosave_deadline
    }

    pub fn clear_autosave_deadline(&mut self) {
        self.autosave_deadline = None;
    }

    pub fn project_snapshot(&self) -> Option<ProjectSnapshot> {
        let project = self.workspace.project()?;
        Some(ProjectSnapshot {
            name: project.name.clone(),
            entries: self.entries.clone(),
            tasks: self.tasks.clone(),
            view: self.project_view_settings(),
            history: self.history.clone(),
        })
    }

    /// Materialize an entry's geometry if it was lazily left unloaded when the
    /// project was opened. No-op for already-loaded entries and scratch sessions.
    pub fn ensure_entry_loaded(&mut self, entry_id: u64) {
        let Some(project) = self.workspace.project().cloned() else {
            return;
        };
        let Some(entry) = self.entries.entry(entry_id) else {
            return;
        };
        if entry.loaded {
            return;
        }
        let compound_id = entry.compound_id.unwrap_or(entry.id as i64);
        match crate::backend::storage::load_structure_for_compound(
            &project.compounds_db,
            compound_id,
        ) {
            Ok(structure) => {
                if let Some(entry) = self.entries.entry_mut(entry_id) {
                    entry.structure = structure;
                    entry.loaded = true;
                }
            }
            Err(error) => self.set_message(format!("Failed to load entry #{entry_id}: {error}")),
        }
    }

    pub fn capture_edit_snapshot(&self) -> EditSnapshot {
        let entry = self
            .entries
            .active_entry()
            .expect("active entry must exist");
        EditSnapshot {
            structure: entry.structure.clone(),
            source_path: entry.source_path.clone(),
            save_path: entry.save_path.clone(),
            selection: self.ui.selection.clone(),
        }
    }

    pub fn restore_edit_snapshot(&mut self, snapshot: EditSnapshot) {
        self.cancel_transient_jobs();
        self.ui.pending_optimization = None;
        self.ui.pending_supercell = None;
        self.ui.pending_md_system = None;
        self.ui.pending_md_run = None;
        self.ui.editor = None;
        self.ui.reticular_builder = None;
        self.ui.nanosheet_builder = None;
        self.ui.block_editor = None;
        self.edit_origin = None;
        self.builder_origin = None;
        self.optimization_origin = None;
        self.ui.hovered_atom = None;

        if let Some(entry) = self.entries.active_entry_mut() {
            entry.structure = snapshot.structure;
            entry.source_path = snapshot.source_path;
            entry.save_path = snapshot.save_path;
        }
        self.mark_structure_changed();
        self.ui.selection = snapshot.selection;
        self.ui.selection.retain_valid(self.structure().atoms.len());
    }

    /// Forget every entry's undo/redo history (e.g. when closing a project).
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.reset_edit_origins();
    }

    fn reset_edit_origins(&mut self) {
        self.edit_origin = None;
        self.builder_origin = None;
        self.optimization_origin = None;
    }

    /// Point the (per-entry) undo/redo history at the currently active entry
    /// without discarding any entry's stacks. Each entry keeps its own history,
    /// so switching between entries — or reopening a project — preserves undo.
    pub fn sync_history_active_entry(&mut self) {
        let active = self.entries.active_entry_id();
        self.history.set_active_entry(active);
        self.reset_edit_origins();
    }

    pub fn history_navigation_enabled(&self) -> bool {
        self.ui.editor.is_none()
            && self.ui.reticular_builder.is_none()
            && self.ui.nanosheet_builder.is_none()
            && self.ui.block_editor.is_none()
            && self.ui.pending_optimization.is_none()
            && self.ui.pending_md_system.is_none()
            && self.ui.pending_md_run.is_none()
            && !self.jobs.optimization_running()
            && !self.jobs.engine_running()
    }

    pub fn can_undo(&self) -> bool {
        self.history_navigation_enabled() && self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history_navigation_enabled() && self.history.can_redo()
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.record_message_change();
    }

    pub fn record_message_change(&mut self) {
        if self.message == self.last_logged_message {
            return;
        }
        self.output_log.push(self.message.clone());
        self.last_logged_message = self.message.clone();
        if self.output_log.len() > 400 {
            let excess = self.output_log.len() - 400;
            self.output_log.drain(0..excess);
        }
    }

    pub fn cancel_transient_jobs(&mut self) {
        self.jobs.cancel_optimization();
        self.jobs.cancel_engine();
    }

    pub fn reset_layout_keep_view(&mut self) {
        let active_view = self.ui.layout.active_primary_view;
        self.ui.layout = LayoutState::default();
        self.ui.layout.active_primary_view = active_view;
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;

    #[test]
    fn empty_startup_does_not_create_initial_entry() {
        let state = AppState::scratch(Default::default(), Vec::new());

        assert!(!state.has_active_entry());
        assert_eq!(state.entries.records.len(), 0);
        assert_eq!(state.entries.tabs.len(), 0);
        assert_eq!(state.current_entry_label(), "Scratch");
    }
}
