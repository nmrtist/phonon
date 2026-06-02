use eframe::egui::{self, Align, Button, Frame, Layout, Margin, RichText, ScrollArea, Stroke};

use crate::{
    backend::tasks::TaskPanelKind,
    frontend::{
        actions::AppAction,
        state::{AppState, CoordinateOptimizationScope, PanelTab},
    },
};

use super::{core_button_text_color, with_core_button_style};

pub(super) fn render_secondary_sidebar(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
) {
    ScrollArea::vertical()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            render_secondary_sidebar_content(state, ui, actions);
        });
}

fn render_secondary_sidebar_content(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
) {
    let panels = state
        .tasks
        .panels
        .iter()
        .map(|panel| panel.task_run_id)
        .collect::<Vec<_>>();

    // Single header row: task tabs (or a hint) on the left, the hide-sidebar
    // button pinned to the right so it never costs a dedicated row.
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if with_core_button_style(ui, false, |ui| {
                ui.add_sized(
                    [28.0, 28.0],
                    Button::new(
                        RichText::new(egui_phosphor::regular::CARET_RIGHT)
                            .color(core_button_text_color(false)),
                    ),
                )
            })
            .on_hover_text("Hide sidebar")
            .clicked()
            {
                state.ui.layout.show_secondary_sidebar = false;
            }

            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                if panels.is_empty() {
                    ui.label("Double-click a task to open.");
                    return;
                }
                for task_run_id in &panels {
                    let Some(task) = state.tasks.task_run(*task_run_id) else {
                        continue;
                    };
                    let title = task.title.clone();
                    let active = state.tasks.active_panel == Some(*task_run_id);
                    Frame::group(ui.style())
                        .stroke(Stroke::new(
                            1.0,
                            if active {
                                egui::Color32::from_rgb(66, 113, 181)
                            } else {
                                egui::Color32::from_rgb(198, 205, 214)
                            },
                        ))
                        .inner_margin(Margin::symmetric(6, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui.selectable_label(active, title).clicked() {
                                    actions.push(AppAction::ActivateTaskPanel(*task_run_id));
                                }
                                if ui
                                    .add(egui::Button::new(egui_phosphor::regular::X).frame(false))
                                    .on_hover_text("Close task panel")
                                    .clicked()
                                {
                                    actions.push(AppAction::CloseTaskPanel(*task_run_id));
                                }
                            });
                        });
                }
            });
        });
    });
    ui.separator();

    if panels.is_empty() {
        return;
    }

    let Some(active_task_run_id) = state.tasks.active_panel else {
        ui.label("Select a task tab to continue.");
        return;
    };
    let Some(task) = state.tasks.task_run(active_task_run_id).cloned() else {
        ui.label("Task panel is unavailable.");
        return;
    };

    ui.label(RichText::new(task.title).strong());
    ui.label(
        RichText::new(format!(
            "{} / {} / {}",
            task.theme, task.method, task.application
        ))
        .small()
        .color(egui::Color32::GRAY),
    );
    ui.separator();

    match task.panel {
        TaskPanelKind::ReticularBuilder => render_framework_task_panel(state, ui, actions),
        TaskPanelKind::NanosheetBuilder => render_nanosheet_task_panel(state, ui, actions),
        TaskPanelKind::BuildingBlockEditor => render_building_block_task_panel(state, ui, actions),
        TaskPanelKind::OptimizationPrompt => render_optimization_task_panel(state, ui, actions),
        TaskPanelKind::SupercellPrompt => render_supercell_task_panel(state, ui, actions),
        TaskPanelKind::ProteinPrepPrompt => render_protein_prep_task_panel(state, ui, actions),
        TaskPanelKind::MdSystemPrompt => render_md_system_task_panel(state, ui, actions),
        TaskPanelKind::MdRunPrompt => render_md_run_task_panel(state, ui, actions),
        TaskPanelKind::None => {
            ui.label("This task runs directly and does not need a panel.");
            if ui
                .button(format!("{}  Close", egui_phosphor::regular::X))
                .clicked()
            {
                actions.push(AppAction::CloseTaskPanel(active_task_run_id));
            }
        }
    }
}

fn render_framework_task_panel(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
) {
    if let Some(panel) = &mut state.ui.reticular_builder {
        panel.ui(ui);
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button(format!("{}  Preview", egui_phosphor::regular::EYE))
                .clicked()
            {
                actions.push(AppAction::PreviewFramework);
            }
            if ui
                .button(format!("{}  Build", egui_phosphor::regular::HAMMER))
                .clicked()
            {
                actions.push(AppAction::BuildFramework);
            }
            if ui
                .button(format!("{}  Cancel", egui_phosphor::regular::X))
                .clicked()
            {
                actions.push(AppAction::CancelFramework);
            }
        });
    } else {
        ui.label("Task panel is not active.");
    }
}

fn render_nanosheet_task_panel(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
) {
    if let Some(panel) = &mut state.ui.nanosheet_builder {
        panel.ui(ui);
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button(format!("{}  Preview", egui_phosphor::regular::EYE))
                .clicked()
            {
                actions.push(AppAction::PreviewNanosheet);
            }
            if ui
                .button(format!("{}  Build", egui_phosphor::regular::HAMMER))
                .clicked()
            {
                actions.push(AppAction::BuildNanosheet);
            }
            if ui
                .button(format!("{}  Cancel", egui_phosphor::regular::X))
                .clicked()
            {
                actions.push(AppAction::CancelNanosheet);
            }
        });
    } else {
        ui.label("Task panel is not active.");
    }
}

fn render_building_block_task_panel(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
) {
    let Some(structure) = state.entries.active_entry().map(|entry| &entry.structure) else {
        ui.label("Open an entry to edit a building block.");
        return;
    };
    if let Some(editor) = &mut state.ui.block_editor {
        editor.ui(ui, structure);
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button(format!("{}  Save", egui_phosphor::regular::FLOPPY_DISK))
                .clicked()
            {
                actions.push(AppAction::SaveBuildingBlock);
            }
            if ui
                .button(format!("{}  Cancel", egui_phosphor::regular::X))
                .clicked()
            {
                actions.push(AppAction::CancelBuildingBlock);
            }
        });
    } else {
        ui.label("Task panel is not active.");
    }
}

fn render_optimization_task_panel(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
) {
    let has_selection = !state.ui.selection.is_empty();
    if let Some(prompt) = &mut state.ui.pending_optimization {
        ui.label("Atomic coordinates:");
        ui.radio_value(
            &mut prompt.coordinate_scope,
            CoordinateOptimizationScope::AllAtoms,
            "Optimize all atoms",
        );
        ui.add_enabled_ui(has_selection, |ui| {
            ui.radio_value(
                &mut prompt.coordinate_scope,
                CoordinateOptimizationScope::SelectedAtoms,
                format!("Optimize selected atoms ({})", state.ui.selection.len()),
            );
        });
        if !has_selection {
            ui.label("No atoms selected. Use the viewport or Selection panel to pick atoms.");
            prompt.coordinate_scope = CoordinateOptimizationScope::AllAtoms;
        }

        if prompt.allow_cell_optimization {
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .button(format!(
                        "{}  Select all",
                        egui_phosphor::regular::CHECK_SQUARE
                    ))
                    .clicked()
                {
                    prompt.cell = crate::engines::forcefield::CellOptimizationOptions::all();
                }
                if ui
                    .button(format!("{}  Clear", egui_phosphor::regular::SQUARE))
                    .clicked()
                {
                    prompt.cell = crate::engines::forcefield::CellOptimizationOptions::default();
                }
            });
            egui::Grid::new("sidebar_cell_optimization_options")
                .num_columns(3)
                .show(ui, |ui| {
                    ui.checkbox(&mut prompt.cell.a, "a");
                    ui.checkbox(&mut prompt.cell.b, "b");
                    ui.checkbox(&mut prompt.cell.c, "c");
                    ui.end_row();
                    ui.checkbox(&mut prompt.cell.alpha, "alpha");
                    ui.checkbox(&mut prompt.cell.beta, "beta");
                    ui.checkbox(&mut prompt.cell.gamma, "gamma");
                });
        }

        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button(format!("{}  Start", egui_phosphor::regular::PLAY))
                .clicked()
            {
                actions.push(AppAction::StartOptimization);
            }
            if ui
                .button(format!("{}  Cancel", egui_phosphor::regular::X))
                .clicked()
            {
                actions.push(AppAction::CancelOptimizationPrompt);
            }
        });
    } else if state.jobs.optimization_running() {
        ui.label("Optimization is running.");
        if ui
            .button(format!("{}  Show Output", egui_phosphor::regular::TERMINAL))
            .clicked()
        {
            state.ui.layout.show_panel = true;
            state.ui.layout.active_panel_tab = PanelTab::Output;
        }
    } else {
        ui.label("Optimization configuration is unavailable.");
    }
}

fn render_supercell_task_panel(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
) {
    let cell_metrics = state
        .structure()
        .cell
        .as_ref()
        .map(|cell| (cell.a, cell.b, cell.c))
        .unwrap_or((0.0, 0.0, 0.0));
    let atom_count = state.structure().atoms.len();
    let bond_count = state.structure().bonds.len();
    if let Some(prompt) = &mut state.ui.pending_supercell {
        ui.label(format!(
            "Current cell: {:.2} x {:.2} x {:.2} A",
            cell_metrics.0, cell_metrics.1, cell_metrics.2,
        ));
        ui.label(format!("{atom_count} atoms, {bond_count} bonds"));
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Repeats:");
            supercell_repeat_value(ui, &mut prompt.repeats[0]);
            ui.label("x");
            supercell_repeat_value(ui, &mut prompt.repeats[1]);
            ui.label("x");
            supercell_repeat_value(ui, &mut prompt.repeats[2]);
        });

        let total_atoms =
            atom_count * (prompt.repeats[0] * prompt.repeats[1] * prompt.repeats[2]) as usize;
        ui.label(format!(
            "Result: {}x{}x{} supercell, {} atoms",
            prompt.repeats[0], prompt.repeats[1], prompt.repeats[2], total_atoms,
        ));

        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button(format!("{}  Expand", egui_phosphor::regular::ARROWS_OUT))
                .clicked()
            {
                actions.push(AppAction::ConfirmSupercell);
            }
            if ui
                .button(format!("{}  Cancel", egui_phosphor::regular::X))
                .clicked()
            {
                actions.push(AppAction::CancelSupercellPrompt);
            }
        });
    } else {
        ui.label("Supercell panel is unavailable.");
    }
}

fn render_protein_prep_task_panel(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
) {
    let atom_count = state.structure().atoms.len();
    if let Some(prompt) = &mut state.ui.pending_protein_prep {
        ui.label(format!("{atom_count} atoms"));
        ui.label(
            "Prepare a biomolecule for simulation. The prepared structure is added as a new entry.",
        );
        ui.separator();

        ui.strong("Cleanup");
        ui.checkbox(&mut prompt.add_hydrogens, "Add missing hydrogens");

        ui.separator();
        ui.label(
            egui::RichText::new(
                "Coming soon: protonation states, terminus patching, and missing-atom repair.",
            )
            .small()
            .color(egui::Color32::GRAY),
        );

        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button(format!("{}  Prepare", egui_phosphor::regular::SPARKLE))
                .clicked()
            {
                actions.push(AppAction::ConfirmProteinPrep);
            }
            if ui
                .button(format!("{}  Cancel", egui_phosphor::regular::X))
                .clicked()
            {
                actions.push(AppAction::CancelProteinPrepPrompt);
            }
        });
    } else {
        ui.label("Protein preparation panel is unavailable.");
    }
}

/// Cations and anions offered in the System Builder. Restricted to monovalent
/// ions, matching the solvation logic's monovalent neutralization arithmetic.
/// Selectable ions as `(token, label)`: `token` is the GROMACS residue name
/// genion expects (`-pname`/`-nname`), `label` is the conventional chemical
/// form shown to the user.
const MD_POSITIVE_IONS: &[(&str, &str)] = &[("NA", "Na+"), ("K", "K+")];
const MD_NEGATIVE_IONS: &[(&str, &str)] = &[("CL", "Cl-")];

/// The display label for an ion token, falling back to the token itself.
fn ion_label(token: &str) -> &str {
    MD_POSITIVE_IONS
        .iter()
        .chain(MD_NEGATIVE_IONS)
        .find(|(value, _)| *value == token)
        .map(|(_, label)| *label)
        .unwrap_or(token)
}

fn render_md_system_task_panel(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
) {
    use crate::{
        frontend::state::MdSystemSizingMode,
        workflows::molecular_dynamics::{self, BoxShape as MdBoxShape, WaterModel},
    };

    let atom_count = state.structure().atoms.len();
    let has_cell = state.structure().cell.is_some();
    // Snapshot the previews out before borrowing the prompt mutably. The box
    // preview is cheap; the solvation preview is cached and only recomputed when
    // its inputs change (see `md_solvation_preview`).
    let preview = state.ui.pending_md_system.as_ref().map(|prompt| {
        crate::workflows::molecular_dynamics::preview(state.structure(), &prompt.config())
    });
    let solvation_preview = md_solvation_preview(state);

    if let Some(prompt) = &mut state.ui.pending_md_system {
        ui.label(format!("{atom_count} atoms"));
        if has_cell {
            ui.colored_label(
                egui::Color32::from_rgb(0xd0, 0x90, 0x30),
                "This structure already has a cell; building will replace it.",
            );
        }
        ui.label("Wrap the molecule in a periodic box, then optionally solvate.");
        ui.separator();

        // ---- Run name ----------------------------------------------------
        run_name_field(ui, &mut prompt.run_name);
        ui.separator();

        // ---- Build engine ------------------------------------------------
        use crate::frontend::state::MdBuildEngine;
        ui.strong("Build engine");
        ui.horizontal(|ui| {
            ui.label("Engine:");
            egui::ComboBox::from_id_salt("md_build_engine")
                .selected_text(prompt.engine.label())
                .show_ui(ui, |ui| {
                    for engine in MdBuildEngine::all() {
                        ui.selectable_value(&mut prompt.engine, *engine, engine.label());
                    }
                });
        });
        match prompt.engine {
            MdBuildEngine::Gromacs => {
                ui.label(
                    "GROMACS assigns the force field and writes a topology the MD run reuses.",
                );
            }
            MdBuildEngine::BuiltIn => {
                ui.colored_label(
                    egui::Color32::from_rgb(0xd0, 0x90, 0x30),
                    "Geometry only: no topology is produced, so the MD run needs a custom topology.",
                );
            }
        }

        // ---- Box ---------------------------------------------------------
        ui.strong("Box");
        ui.label("Box shape:");
        ui.horizontal(|ui| {
            for shape in MdBoxShape::selectable() {
                ui.radio_value(&mut prompt.shape, *shape, shape.label());
            }
        });

        ui.label("Sizing:");
        ui.horizontal(|ui| {
            ui.radio_value(&mut prompt.mode, MdSystemSizingMode::Padding, "Padding");
            ui.radio_value(&mut prompt.mode, MdSystemSizingMode::Absolute, "Absolute");
        });

        match prompt.mode {
            MdSystemSizingMode::Padding => {
                egui::Grid::new("md_system_padding")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Padding X (A):");
                        md_length_value(ui, &mut prompt.padding_angstrom[0]);
                        ui.end_row();
                        ui.label("Padding Y (A):");
                        md_length_value(ui, &mut prompt.padding_angstrom[1]);
                        ui.end_row();
                        ui.label("Padding Z (A):");
                        md_length_value(ui, &mut prompt.padding_angstrom[2]);
                        ui.end_row();
                    });
                ui.label("Default 10 A (= 1.0 nm) keeps the box above GROMACS' 1.0 nm cutoffs.");
            }
            MdSystemSizingMode::Absolute => {
                egui::Grid::new("md_system_absolute")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Edge a (A):");
                        md_length_value(ui, &mut prompt.absolute_angstrom[0]);
                        ui.end_row();
                        ui.label("Edge b (A):");
                        md_length_value(ui, &mut prompt.absolute_angstrom[1]);
                        ui.end_row();
                        ui.label("Edge c (A):");
                        md_length_value(ui, &mut prompt.absolute_angstrom[2]);
                        ui.end_row();
                    });
            }
        }

        if prompt.shape == MdBoxShape::Cubic {
            ui.label("Cubic: the largest edge is applied to all three axes.");
        }

        // ---- Force field -------------------------------------------------
        // Only GROMACS assigns a force field; the built-in build is geometry
        // only and ignores this selection.
        if prompt.engine == MdBuildEngine::Gromacs {
            ui.separator();
            ui.strong("Force field");
            ui.horizontal(|ui| {
                ui.label("Force field:");
                egui::ComboBox::from_id_salt("md_force_field")
                    .selected_text(molecular_dynamics::force_field_title(&prompt.force_field))
                    .show_ui(ui, |ui| {
                        for entry in molecular_dynamics::FORCE_FIELDS {
                            ui.selectable_value(
                                &mut prompt.force_field,
                                entry.token.to_string(),
                                entry.title,
                            );
                        }
                    });
            });
        }

        // ---- Solvent -----------------------------------------------------
        ui.separator();
        ui.strong("Solvent");
        ui.checkbox(&mut prompt.solvate, "Solvate system (add explicit water)");
        if prompt.solvate {
            ui.horizontal(|ui| {
                ui.label("Water model:");
                egui::ComboBox::from_id_salt("md_water_model")
                    .selected_text(prompt.water.label())
                    .show_ui(ui, |ui| {
                        for model in WaterModel::all() {
                            ui.selectable_value(&mut prompt.water, *model, model.label());
                        }
                    });
            });

            // ---- Ions ----------------------------------------------------
            ui.separator();
            ui.strong("Ions");
            ui.checkbox(&mut prompt.neutralize, "Neutralize net charge");
            ui.checkbox(&mut prompt.add_salt, "Add salt bath");
            if prompt.add_salt {
                ui.horizontal(|ui| {
                    ui.label("Concentration (mol/L):");
                    ui.add(
                        egui::DragValue::new(&mut prompt.salt_concentration_molar)
                            .range(0.0..=5.0)
                            .speed(0.01)
                            .fixed_decimals(2),
                    );
                });
            }
            if prompt.neutralize || prompt.add_salt {
                ui.horizontal(|ui| {
                    ui.label("Cation:");
                    md_ion_combo("md_cation", &mut prompt.positive_ion, MD_POSITIVE_IONS, ui);
                    ui.label("Anion:");
                    md_ion_combo("md_anion", &mut prompt.negative_ion, MD_NEGATIVE_IONS, ui);
                });
            }
        }

        // ---- Preview -----------------------------------------------------
        ui.separator();
        match preview.flatten() {
            Some(preview) => {
                let [a, b, c] = preview.edges_angstrom;
                ui.label(format!("Resulting box: {a:.1} x {b:.1} x {c:.1} A"));
                if !preview.fits {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        "Box is smaller than the molecule; build will fail.",
                    );
                }
            }
            None => {
                ui.colored_label(egui::Color32::LIGHT_RED, "No atoms to box.");
            }
        }
        if prompt.solvate {
            match &solvation_preview {
                Some(Ok(est)) => {
                    ui.label(format!(
                        "~ {} waters, +{} {}, +{} {}",
                        est.water,
                        est.cations,
                        ion_label(&prompt.positive_ion),
                        est.anions,
                        ion_label(&prompt.negative_ion)
                    ));
                }
                Some(Err(error)) => {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        format!("Solvation preview unavailable: {error}"),
                    );
                }
                None => {}
            }
        }

        // ---- Actions -----------------------------------------------------
        ui.separator();
        ui.horizontal(|ui| {
            let build_label = if prompt.solvate {
                "Build & Solvate"
            } else {
                "Build"
            };
            if ui
                .button(format!("{}  {build_label}", egui_phosphor::regular::CUBE))
                .clicked()
            {
                actions.push(AppAction::ConfirmMdSystem);
            }
            if ui
                .button(format!("{}  Cancel", egui_phosphor::regular::X))
                .clicked()
            {
                actions.push(AppAction::CancelMdSystemPrompt);
            }
        });
    } else {
        ui.label("MD system panel is unavailable.");
    }
}

/// A combo box for choosing an ion name from a fixed list.
fn md_ion_combo(id: &str, value: &mut String, options: &[(&str, &str)], ui: &mut egui::Ui) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(ion_label(value))
        .show_ui(ui, |ui| {
            for (token, label) in options {
                ui.selectable_value(value, (*token).to_string(), *label);
            }
        });
}

/// Compute — or reuse the cached — solvation count preview for the System
/// Builder panel. Returns `None` when there is no prompt or solvation is off.
/// Recomputing grid-fills the box, so the result is cached and only refreshed
/// when the inputs that affect it change.
fn md_solvation_preview(
    state: &mut AppState,
) -> Option<Result<crate::workflows::molecular_dynamics::SolvationEstimate, String>> {
    let (config, options) = {
        let prompt = state.ui.pending_md_system.as_ref()?;
        let options = prompt.solvation_options()?;
        (prompt.config(), options)
    };
    let key = md_solvation_estimate_key(state.structure(), &config, &options);
    if state.ui.md_solvation_preview_key == key
        && let Some(cached) = &state.ui.md_solvation_preview
    {
        return Some(cached.clone());
    }
    let result = md_compute_solvation_estimate(state.structure(), &config, &options);
    state.ui.md_solvation_preview = Some(result.clone());
    state.ui.md_solvation_preview_key = key;
    Some(result)
}

fn md_compute_solvation_estimate(
    solute: &crate::domain::Structure,
    config: &crate::workflows::molecular_dynamics::MdSystemConfig,
    options: &crate::workflows::molecular_dynamics::SolvationOptions,
) -> Result<crate::workflows::molecular_dynamics::SolvationEstimate, String> {
    use crate::workflows::molecular_dynamics::{build_md_system, estimate};
    // Preview against the box the build would actually produce.
    let (boxed, _) = build_md_system(solute, config).map_err(|e| e.to_string())?;
    estimate(&boxed, options).map_err(|e| e.to_string())
}

fn md_solvation_estimate_key(
    solute: &crate::domain::Structure,
    config: &crate::workflows::molecular_dynamics::MdSystemConfig,
    options: &crate::workflows::molecular_dynamics::SolvationOptions,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    solute.atoms.len().hash(&mut hasher);
    if let Some(edges) = crate::workflows::molecular_dynamics::preview_edges(solute, config) {
        for edge in edges {
            edge.to_bits().hash(&mut hasher);
        }
    }
    options.water.db_token().hash(&mut hasher);
    options.positive_ion.hash(&mut hasher);
    options.negative_ion.hash(&mut hasher);
    options.neutralize.hash(&mut hasher);
    options
        .concentration_molar
        .map(f32::to_bits)
        .hash(&mut hasher);
    hasher.finish()
}

fn md_length_value(ui: &mut egui::Ui, value: &mut f32) {
    ui.add(
        egui::DragValue::new(value)
            .range(0.0..=10_000.0)
            .speed(0.1)
            .fixed_decimals(1),
    );
}

fn supercell_repeat_value(ui: &mut egui::Ui, value: &mut u32) {
    ui.add_sized(
        [52.0, 20.0],
        egui::DragValue::new(value).range(1..=10).speed(0.1),
    );
}

/// Render the editable run-name field shared by directory-creating task panels.
/// This name is purely human-facing and becomes the run directory's name; the
/// task's durable identity is a separate UUID, so renaming is always safe.
fn run_name_field(ui: &mut egui::Ui, run_name: &mut String) {
    ui.horizontal(|ui| {
        ui.label("Run name:");
        ui.add(
            egui::TextEdit::singleline(run_name)
                .hint_text("auto")
                .desired_width(200.0),
        );
    });
}

fn render_md_run_task_panel(state: &mut AppState, ui: &mut egui::Ui, actions: &mut Vec<AppAction>) {
    if let Some(prompt) = &mut state.ui.pending_md_run {
        use crate::frontend::state::{MdEngineChoice, MdRunStepPreset};

        run_name_field(ui, &mut prompt.run_name);
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("MD engine:");
            egui::ComboBox::from_id_salt("md_run_engine")
                .selected_text(prompt.engine.label())
                .show_ui(ui, |ui| {
                    for engine in MdEngineChoice::all() {
                        ui.selectable_value(&mut prompt.engine, *engine, engine.label());
                    }
                });
        });

        ui.separator();
        ui.label("Steps:");
        ui.horizontal_wrapped(|ui| {
            if ui.button("+ Relax").clicked() {
                prompt.add_relax_template();
            }
            if ui.button("+ EM").clicked() {
                prompt.add_step(MdRunStepPreset::EnergyMinimization);
            }
            if ui.button("+ NVT").clicked() {
                prompt.add_step(MdRunStepPreset::Nvt);
            }
            if ui.button("+ NPT").clicked() {
                prompt.add_step(MdRunStepPreset::Npt);
            }
            if ui.button("+ MD").clicked() {
                prompt.add_step(MdRunStepPreset::Production);
            }
            if ui.button("+ Custom").clicked() {
                prompt.add_step(MdRunStepPreset::Custom);
            }
        });

        let reference_temperature = prompt.reference_temperature();
        let reference_timestep = prompt.reference_timestep();
        let mut remove_index = None;
        let mut move_up_index = None;
        let mut move_down_index = None;

        let total_steps = prompt.steps.len();
        for (index, step) in prompt.steps.iter_mut().enumerate() {
            ui.add_space(6.0);
            Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Step {}", index + 1)).strong());
                    ui.label(
                        RichText::new(step.preset.label())
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                    if ui
                        .add_enabled(
                            index > 0,
                            egui::Button::new(egui_phosphor::regular::ARROW_UP),
                        )
                        .clicked()
                    {
                        move_up_index = Some(index);
                    }
                    if ui
                        .add_enabled(
                            index + 1 < total_steps,
                            egui::Button::new(egui_phosphor::regular::ARROW_DOWN),
                        )
                        .clicked()
                    {
                        move_down_index = Some(index);
                    }
                    if ui
                        .add(egui::Button::new(egui_phosphor::regular::TRASH).frame(false))
                        .clicked()
                    {
                        remove_index = Some(index);
                    }
                });

                let previous_preset = step.preset;
                ui.horizontal(|ui| {
                    ui.label("Preset:");
                    egui::ComboBox::from_id_salt(("md_run_step_preset", index))
                        .selected_text(step.preset.label())
                        .show_ui(ui, |ui| {
                            for preset in MdRunStepPreset::all() {
                                ui.selectable_value(&mut step.preset, *preset, preset.label());
                            }
                        });
                });
                if step.preset != previous_preset && step.preset != MdRunStepPreset::Custom {
                    step.reapply_preset(step.preset, reference_temperature, reference_timestep);
                }

                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut step.stage_name);
                });

                egui::Grid::new(format!("md_run_step_{index}"))
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("nsteps:");
                        ui.add(
                            egui::DragValue::new(&mut step.settings.nsteps)
                                .range(1..=1_000_000_000u64)
                                .speed(50.0),
                        );
                        ui.end_row();

                        if step.settings.integrator.is_minimization() {
                            ui.label("emtol (kJ/mol/nm):");
                            ui.add(
                                egui::DragValue::new(&mut step.settings.emtol)
                                    .range(0.1..=1.0e6_f32)
                                    .speed(10.0),
                            );
                            ui.end_row();
                            ui.label("emstep (nm):");
                            ui.add(
                                egui::DragValue::new(&mut step.settings.emstep)
                                    .range(0.0001..=1.0_f32)
                                    .speed(0.001),
                            );
                            ui.end_row();
                        } else {
                            ui.label("dt (ps):");
                            ui.add(
                                egui::DragValue::new(&mut step.settings.timestep_ps)
                                    .range(0.0001..=0.1_f32)
                                    .speed(0.0005)
                                    .fixed_decimals(4),
                            );
                            ui.end_row();
                            ui.label("continuation:");
                            ui.checkbox(&mut step.settings.continuation, "");
                            ui.end_row();
                        }

                        ui.label("Coulomb cutoff (nm):");
                        ui.add(
                            egui::DragValue::new(&mut step.settings.coulomb_cutoff_nm)
                                .range(0.1..=5.0_f32)
                                .speed(0.05),
                        );
                        ui.end_row();
                        ui.label("VdW cutoff (nm):");
                        ui.add(
                            egui::DragValue::new(&mut step.settings.vdw_cutoff_nm)
                                .range(0.1..=5.0_f32)
                                .speed(0.05),
                        );
                        ui.end_row();
                    });

                if !step.settings.integrator.is_minimization() {
                    let mut temperature_coupling = step.settings.temperature_coupling.is_some();
                    let mut pressure_coupling = step.settings.pressure_coupling.is_some();
                    let mut velocity_generation = step.settings.velocity_generation.is_some();
                    let mut temperature_k = step
                        .settings
                        .temperature_coupling
                        .as_ref()
                        .and_then(|tc| tc.ref_t.first().copied())
                        .or_else(|| {
                            step.settings
                                .velocity_generation
                                .as_ref()
                                .map(|velocity| velocity.gen_temp)
                        })
                        .unwrap_or(reference_temperature);

                    ui.separator();
                    egui::Grid::new(format!("md_run_step_md_controls_{index}"))
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("target T (K):");
                            ui.add(
                                egui::DragValue::new(&mut temperature_k)
                                    .range(1.0..=5_000.0_f32)
                                    .speed(1.0),
                            );
                            ui.end_row();
                            ui.label("thermostat:");
                            ui.checkbox(&mut temperature_coupling, "");
                            ui.end_row();
                            ui.label("barostat:");
                            ui.checkbox(&mut pressure_coupling, "");
                            ui.end_row();
                            ui.label("generate velocities:");
                            ui.checkbox(&mut velocity_generation, "");
                            ui.end_row();
                        });

                    if temperature_coupling {
                        let tc = step.settings.temperature_coupling.get_or_insert_with(|| {
                            crate::engines::gromacs::input::TemperatureCoupling::whole_system(
                                temperature_k,
                            )
                        });
                        for value in &mut tc.ref_t {
                            *value = temperature_k;
                        }
                    } else {
                        step.settings.temperature_coupling = None;
                    }

                    if pressure_coupling {
                        step.settings.pressure_coupling.get_or_insert_with(|| {
                            crate::engines::gromacs::input::PressureCoupling::isotropic()
                        });
                    } else {
                        step.settings.pressure_coupling = None;
                    }

                    if velocity_generation {
                        let velocity = step.settings.velocity_generation.get_or_insert(
                            crate::engines::gromacs::input::VelocityGen {
                                gen_temp: temperature_k,
                                gen_seed: -1,
                            },
                        );
                        velocity.gen_temp = temperature_k;
                    } else {
                        step.settings.velocity_generation = None;
                    }
                }
            });
        }

        if let Some(index) = move_up_index {
            prompt.steps.swap(index - 1, index);
        }
        if let Some(index) = move_down_index {
            prompt.steps.swap(index, index + 1);
        }
        if let Some(index) = remove_index {
            prompt.steps.remove(index);
        }
        if prompt.steps.is_empty() {
            ui.add_space(6.0);
            ui.label("No steps yet. Add a template or a custom step.");
        }

        ui.separator();
        ui.checkbox(&mut prompt.show_advanced, "Advanced");
        if prompt.show_advanced {
            ui.label("Topology override (.top/.itp):");
            ui.horizontal(|ui| {
                let label = prompt
                    .topology_override_path
                    .as_ref()
                    .map(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("(unnamed)")
                            .to_string()
                    })
                    .unwrap_or_else(|| "Auto-detect / generated".to_string());
                ui.label(label);
                if ui
                    .button(format!("{}  Browse", egui_phosphor::regular::FOLDER))
                    .clicked()
                {
                    actions.push(AppAction::PickMdTopologyOverride);
                }
                if ui
                    .add_enabled(
                        prompt.topology_override_path.is_some(),
                        egui::Button::new(format!("{}  Clear", egui_phosphor::regular::X)),
                    )
                    .clicked()
                {
                    prompt.topology_override_path = None;
                }
            });
            ui.label(
                RichText::new(
                    "Without an override, Phonon reuses the captured MD topology or tries to generate one from the active structure.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
        }

        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button(format!("{}  Run", egui_phosphor::regular::PLAY))
                .clicked()
            {
                actions.push(AppAction::StartMdRun);
            }
            if ui
                .button(format!("{}  Cancel", egui_phosphor::regular::X))
                .clicked()
            {
                actions.push(AppAction::CancelMdRunPrompt);
            }
        });
    } else if state.jobs.engine_running() {
        ui.label("MD job is running.");
        if let Some(stage) = state
            .jobs
            .engine
            .as_ref()
            .and_then(|engine| engine.latest_stage.as_ref())
        {
            ui.label(RichText::new(stage).small());
        }
        if ui
            .button(format!("{}  Show Output", egui_phosphor::regular::TERMINAL))
            .clicked()
        {
            state.ui.layout.show_panel = true;
            state.ui.layout.active_panel_tab = PanelTab::Output;
        }
    } else {
        ui.label("MD configuration is unavailable.");
    }
}
