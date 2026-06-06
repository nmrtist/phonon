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
    let pal = crate::frontend::theme::palette(ui);
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
                            .color(core_button_text_color(&pal, false)),
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
                            if active { pal.accent } else { pal.hairline },
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
        .color(pal.text_tertiary),
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
    // A periodic framework (nanosheet or other celled, bonded, non-biopolymer
    // structure) is routed to the bond-derived topology path instead of pdb2gmx.
    // Coverage depends on the selected custom force field, so snapshot the custom
    // atom types (from the cached `.itp` text) before borrowing the prompt mutably.
    let is_framework = molecular_dynamics::is_framework_shape(state.structure());
    let custom_types = state
        .ui
        .pending_md_system
        .as_ref()
        .and_then(|p| p.custom_force_field_text.as_deref())
        .map(crate::engines::gromacs::custom_ff::custom_types)
        .unwrap_or_default();
    let framework_flexible_ok =
        is_framework && molecular_dynamics::supports_flexible(state.structure());
    let framework_coverage = if is_framework {
        molecular_dynamics::framework_coverage(state.structure())
    } else {
        None
    };
    // Elements covered only by the user's force field, and any still uncovered.
    let user_provided_elements = if is_framework {
        molecular_dynamics::user_provided_elements(state.structure(), &custom_types)
    } else {
        Vec::new()
    };
    let unparameterized_elements = if is_framework {
        molecular_dynamics::unparameterized_elements(state.structure(), &custom_types)
    } else {
        Vec::new()
    };
    // Saved custom force fields available to pick, and the structure's own crystal
    // cell parameters for the cell editor's "reset" — both snapshotted up front.
    let available_force_fields = if is_framework {
        crate::backend::force_fields::list_force_fields()
    } else {
        Vec::new()
    };
    let framework_crystal_cell = if is_framework {
        state
            .structure()
            .cell
            .as_ref()
            .map(|c| [c.a, c.b, c.c, c.alpha, c.beta, c.gamma])
    } else {
        None
    };
    // Snapshot the previews out before borrowing the prompt mutably. The box
    // preview is cheap; the solvation preview is cached and only recomputed when
    // its inputs change (see `md_solvation_preview`).
    let preview = state.ui.pending_md_system.as_ref().map(|prompt| {
        crate::workflows::molecular_dynamics::preview(state.structure(), &prompt.config())
    });
    let solvation_preview = md_solvation_preview(state);

    if let Some(prompt) = &mut state.ui.pending_md_system {
        // The bond-derived material path (which keeps the crystal cell as the box)
        // is taken only for a framework built with GROMACS; the built-in geometry
        // path re-boxes like any other structure.
        let framework_build =
            is_framework && prompt.engine == crate::frontend::state::MdBuildEngine::Gromacs;
        ui.label(format!("{atom_count} atoms"));
        if framework_build {
            ui.label(
                "Periodic framework: the crystal cell becomes the simulation box, keeping its \
                 shape. Adjust the lattice below — typically only the out-of-plane axis, to open \
                 a vacuum gap or a solvent column.",
            );
        } else {
            if has_cell {
                ui.colored_label(
                    egui::Color32::from_rgb(0xd0, 0x90, 0x30),
                    "This structure already has a cell; building will replace it.",
                );
            }
            ui.label("Wrap the molecule in a periodic box, then optionally solvate.");
        }
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
            MdBuildEngine::Gromacs if is_framework => {
                use crate::workflows::molecular_dynamics::{Coverage, FrameworkMode};
                let amber = egui::Color32::from_rgb(0xd0, 0x90, 0x30);
                ui.label(
                    "Periodic framework: the topology is generated from the structure's bonds.",
                );
                if !framework_flexible_ok {
                    // No bonded parameters for this chemistry; rigid is the only option.
                    prompt.framework_mode = FrameworkMode::Rigid;
                }
                ui.horizontal(|ui| {
                    ui.label("Model:");
                    ui.radio_value(
                        &mut prompt.framework_mode,
                        FrameworkMode::Rigid,
                        "Rigid (frozen)",
                    );
                    ui.add_enabled_ui(framework_flexible_ok, |ui| {
                        ui.radio_value(
                            &mut prompt.framework_mode,
                            FrameworkMode::Flexible,
                            "Flexible (bonded)",
                        );
                    });
                });
                match prompt.framework_mode {
                    FrameworkMode::Rigid => {
                        ui.label("The sheet is frozen; only the surrounding system moves.");
                    }
                    FrameworkMode::Flexible => {
                        ui.label("The sheet flexes via bonds, angles and dihedrals.");
                    }
                }
                if !framework_flexible_ok {
                    ui.label(
                        "Flexible modeling needs carbon-family bonded parameters; only rigid is available for this material.",
                    );
                }
                // Standard biomolecular force fields don't parameterize these
                // materials, so Phonon supplies its own: validated OPLS-AA for
                // carbon, generic UFF otherwise. The flag grades those parameters.
                match framework_coverage {
                    Some(Coverage::Good) => {
                        ui.label(
                            "Parameters: OPLS-AA aromatic carbon (validated for carbon \
                             nanostructures).",
                        );
                    }
                    Some(Coverage::Approximate) => {
                        ui.colored_label(
                            amber,
                            "Parameters: generic UFF — no validated force field exists for this \
                             material, so results are approximate.",
                        );
                    }
                    Some(Coverage::Poor) => {
                        ui.colored_label(
                            amber,
                            "Parameters: generic UFF on transition-metal chemistry it was not \
                             designed for — treat results as qualitative.",
                        );
                    }
                    None => {}
                }

                // ---- Custom force field --------------------------------------
                // Cover elements the built-in tables lack, or override built-in
                // atom types, with a user-supplied GROMACS parameter block.
                ui.separator();
                ui.strong("Custom force field");
                ui.horizontal(|ui| {
                    ui.label("Use:");
                    let current = prompt.custom_force_field.clone();
                    egui::ComboBox::from_id_salt("md_custom_ff")
                        .selected_text(
                            current
                                .clone()
                                .unwrap_or_else(|| "(built-in only)".to_string()),
                        )
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(current.is_none(), "(built-in only)")
                                .clicked()
                            {
                                actions.push(AppAction::SelectCustomForceField(None));
                            }
                            for name in &available_force_fields {
                                let selected = current.as_deref() == Some(name.as_str());
                                if ui.selectable_label(selected, name).clicked() {
                                    actions.push(AppAction::SelectCustomForceField(Some(
                                        name.clone(),
                                    )));
                                }
                            }
                        });
                });
                if !unparameterized_elements.is_empty() {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        format!(
                            "No parameters for: {}. Add a custom force field that defines an atom \
                             type named after each element.",
                            unparameterized_elements.join(", ")
                        ),
                    );
                }
                if !user_provided_elements.is_empty() {
                    ui.colored_label(
                        amber,
                        format!(
                            "Using your custom force field for: {}.",
                            user_provided_elements.join(", ")
                        ),
                    );
                }
                ui.collapsing("Add / import a force field", |ui| {
                    ui.label(
                        "Paste a GROMACS [ atomtypes ] block (and optional [ bondtypes ] …). Name \
                         each atom type after its element symbol (e.g. Pt), or after a built-in \
                         type (CJ, HJ, Mo, …) to override it. Omit [ defaults ].",
                    );
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut prompt.custom_ff_draft_name);
                    });
                    ui.add(
                        egui::TextEdit::multiline(&mut prompt.custom_ff_draft)
                            .code_editor()
                            .desired_rows(5)
                            .desired_width(f32::INFINITY)
                            .hint_text("[ atomtypes ]\nPt  78  195.08  0.0  A  0.2754  0.33"),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Import file…").clicked() {
                            actions.push(AppAction::ImportCustomForceFieldFile);
                        }
                        if ui.button("Save to library").clicked() {
                            actions.push(AppAction::SaveCustomForceField);
                        }
                    });
                    if let Some(name) = prompt.custom_force_field.clone()
                        && ui.button(format!("Delete \"{name}\"")).clicked()
                    {
                        actions.push(AppAction::DeleteCustomForceField(name));
                    }
                });
            }
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

        // ---- Box / simulation cell ---------------------------------------
        if let (true, Some(cell)) = (framework_build, prompt.framework_cell.as_mut()) {
            // The crystal cell is the box; expose its lattice parameters directly
            // so the shape (e.g. hexagonal) is preserved and editable, rather than
            // forcing one of the generic cuboid box shapes.
            ui.strong("Simulation cell");
            ui.label(format!("Shape: {}", cell_shape_label(cell)));
            egui::Grid::new("md_framework_cell")
                .num_columns(2)
                .show(ui, |ui| {
                    for (label, idx) in [("a (A):", 0), ("b (A):", 1), ("c (A):", 2)] {
                        ui.label(label);
                        md_length_value(ui, &mut cell[idx]);
                        ui.end_row();
                    }
                    for (label, idx) in
                        [("alpha (deg):", 3), ("beta (deg):", 4), ("gamma (deg):", 5)]
                    {
                        ui.label(label);
                        ui.add(
                            egui::DragValue::new(&mut cell[idx])
                                .range(1.0..=179.0)
                                .speed(0.1)
                                .fixed_decimals(1),
                        );
                        ui.end_row();
                    }
                });
            ui.label(
                "Taken from the crystal cell. The in-plane lattice (a, b, gamma) tiles the sheet \
                 across the boundary — usually leave it; widen c for vacuum or solvent.",
            );
            if let Some(crystal) = framework_crystal_cell
                && ui.button("Reset c to crystal cell").clicked()
            {
                cell[2] = crystal[2];
            }
        } else {
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
                    ui.label(
                        "Default 10 A (= 1.0 nm) keeps the box above GROMACS' 1.0 nm cutoffs.",
                    );
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
        }

        // ---- Force field -------------------------------------------------
        // Only the GROMACS pdb2gmx path assigns a bundled force field; the
        // built-in build is geometry only, and a framework uses its own
        // bond-derived parameters — both ignore this selection.
        if prompt.engine == MdBuildEngine::Gromacs && !is_framework {
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
        if let (true, Some(cell)) = (framework_build, prompt.framework_cell) {
            // The framework box is the (edited) crystal cell; report it directly
            // and check it clears the nonbonded cutoff's minimum image — the most
            // common framework build failure — instead of a generic padded box.
            let [a, b, c, _, _, _] = cell;
            ui.label(format!(
                "Box: {a:.1} x {b:.1} x {c:.1} A ({})",
                cell_shape_label(&cell)
            ));
            let unit_cell = crate::domain::UnitCell::from_parameters(
                cell[0], cell[1], cell[2], cell[3], cell[4], cell[5],
            );
            if let Err(error) = crate::workflows::molecular_dynamics::ensure_periodic_cutoff_fits(
                &unit_cell,
                crate::workflows::molecular_dynamics::DEFAULT_CUTOFF_NM,
            ) {
                ui.colored_label(egui::Color32::LIGHT_RED, error.to_string());
            }
            if prompt.solvate {
                // The real water count comes from gmx solvate at build time; the
                // geometric estimate is computed against a padded box, so it does
                // not apply here.
                ui.label("Solvent water count is determined during the build.");
            }
        } else {
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

/// A coarse lattice-system label for the framework cell editor readout, so the
/// user can confirm the box matches their material. `cell` is `[a, b, c, α, β,
/// γ]` (lengths in A, angles in degrees).
fn cell_shape_label(cell: &[f32; 6]) -> &'static str {
    let [a, b, c, alpha, beta, gamma] = *cell;
    let ang = |x: f32, target: f32| (x - target).abs() < 0.5;
    let len = |x: f32, y: f32| (x - y).abs() < 0.01 * x.max(y).max(1.0);
    if ang(alpha, 90.0) && ang(beta, 90.0) && (ang(gamma, 120.0) || ang(gamma, 60.0)) {
        "hexagonal"
    } else if ang(alpha, 90.0) && ang(beta, 90.0) && ang(gamma, 90.0) {
        if len(a, b) && len(b, c) {
            "cubic"
        } else if len(a, b) {
            "tetragonal"
        } else {
            "orthorhombic"
        }
    } else {
        "triclinic"
    }
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
        use crate::frontend::state::MdEngineChoice;

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

        // Run the recommendation once (pure; reads only the effective context).
        let recommendation = prompt
            .effective()
            .map(|eff| crate::workflows::molecular_dynamics::run::recommend(&eff));

        // --- Inherited system + system-type overrides ----------------------
        if let Some(context) = prompt.context.clone() {
            ui.label(RichText::new("Inherited system").strong());
            ui.label(
                RichText::new(format!(
                    "Force field: {} ({}){}",
                    context.force_field_token,
                    context.force_field_family.label(),
                    context
                        .water_token
                        .as_deref()
                        .map(|water| format!(" · water {water}"))
                        .unwrap_or_default(),
                ))
                .small()
                .color(egui::Color32::GRAY),
            );

            // Override toggles edit the separate per-run overrides via actions and
            // NEVER write back into the persisted detection context; each shows
            // whether the value is auto-detected or user-set.
            if let Some(eff) = prompt.effective() {
                use crate::frontend::state::MdSystemAxis;
                use crate::workflows::molecular_dynamics::ValueSource;
                let axes = [
                    (MdSystemAxis::Membrane, "Membrane", eff.membrane()),
                    (MdSystemAxis::Ligand, "Ligand", eff.ligand()),
                    (MdSystemAxis::Nucleic, "Nucleic acid", eff.nucleic()),
                ];
                for (axis, label, (value, source)) in axes {
                    ui.horizontal(|ui| {
                        let mut checked = value;
                        if ui.checkbox(&mut checked, label).changed() {
                            actions.push(AppAction::SetMdRunOverride(axis, Some(checked)));
                        }
                        match source {
                            ValueSource::Detected => {
                                ui.label(
                                    RichText::new("auto-detected")
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );
                            }
                            ValueSource::Overridden => {
                                ui.label(
                                    RichText::new("you set")
                                        .small()
                                        .color(egui::Color32::LIGHT_BLUE),
                                );
                                if ui.small_button("auto").clicked() {
                                    actions.push(AppAction::SetMdRunOverride(axis, None));
                                }
                            }
                        }
                    });
                }
            }

            if let Some(rec) = &recommendation {
                for note in &rec.notes {
                    ui.label(
                        RichText::new(format!("• {} → {}", note.reason, note.intent))
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                }
                for warning in &rec.warnings {
                    ui.label(
                        RichText::new(format!("⚠ {warning}"))
                            .small()
                            .color(egui::Color32::YELLOW),
                    );
                }
            }
        } else {
            ui.label(
                RichText::new("No build context found; using generic defaults.")
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }

        ui.separator();

        // --- Preset --------------------------------------------------------
        {
            use crate::workflows::molecular_dynamics::PresetId;
            let recommended = recommendation.as_ref().map(|rec| rec.preset);
            ui.horizontal(|ui| {
                ui.label("Preset:");
                egui::ComboBox::from_id_salt("md_run_preset")
                    .selected_text(prompt.preset.title())
                    .show_ui(ui, |ui| {
                        for preset in PresetId::all() {
                            let applies =
                                prompt.effective().is_none_or(|eff| preset.applies_to(&eff));
                            let star = if recommended == Some(*preset) {
                                " ★"
                            } else {
                                ""
                            };
                            let na = if applies { "" } else { " (n/a)" };
                            if ui
                                .selectable_label(
                                    prompt.preset == *preset,
                                    format!("{}{star}{na}", preset.title()),
                                )
                                .clicked()
                            {
                                actions.push(AppAction::SetMdRunPreset(*preset));
                            }
                        }
                    });
            });
            ui.label(
                RichText::new(prompt.preset.description())
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }

        ui.separator();

        // --- Basic parameters ----------------------------------------------
        {
            use crate::workflows::molecular_dynamics::ProductionLength;
            egui::Grid::new("md_run_basic_params")
                .num_columns(3)
                .show(ui, |ui| {
                    ui.label("Temperature (K):");
                    let mut temperature = prompt.params.temperature_k;
                    if ui
                        .add(
                            egui::DragValue::new(&mut temperature)
                                .range(1.0..=2_000.0_f32)
                                .speed(1.0),
                        )
                        .changed()
                    {
                        actions.push(AppAction::SetMdRunTemperature(temperature));
                    }
                    if ui.small_button("310 K").clicked() {
                        actions.push(AppAction::SetMdRunTemperature(310.0));
                    }
                    ui.end_row();

                    ui.label("Production:");
                    egui::ComboBox::from_id_salt("md_run_production")
                        .selected_text(prompt.params.production.label())
                        .show_ui(ui, |ui| {
                            for length in ProductionLength::all() {
                                if ui
                                    .selectable_label(
                                        prompt.params.production == *length,
                                        length.label(),
                                    )
                                    .clicked()
                                {
                                    actions.push(AppAction::SetMdRunProduction(*length));
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Timestep (ps):");
                    let mut timestep = prompt.params.timestep_ps;
                    if ui
                        .add(
                            egui::DragValue::new(&mut timestep)
                                .range(0.0005..=0.005_f32)
                                .speed(0.0005)
                                .fixed_decimals(4),
                        )
                        .changed()
                    {
                        actions.push(AppAction::SetMdRunTimestep(timestep));
                    }
                    ui.end_row();
                });

            let mut save = prompt.save_trajectory;
            if ui
                .checkbox(&mut save, "Save trajectory (play back each stage)")
                .changed()
            {
                actions.push(AppAction::SetMdRunSaveTrajectory(save));
            }
        }

        ui.separator();

        // --- Stage sequence (add / remove / reorder / edit) ----------------
        {
            use crate::workflows::molecular_dynamics::StageKind;
            ui.label(RichText::new("Stages").strong());
            ui.label(
                RichText::new("Preset-filled defaults; click a stage to edit its parameters.")
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.horizontal_wrapped(|ui| {
                let adds = [
                    ("+ EM", StageKind::Minimize),
                    ("+ NVT", StageKind::NvtEquilibrate),
                    ("+ NPT", StageKind::NptEquilibrate),
                    ("+ Production", StageKind::Produce),
                    ("+ Anneal", StageKind::Anneal),
                    ("+ Extend", StageKind::Extend),
                ];
                for (label, kind) in adds {
                    if ui.button(label).clicked() {
                        actions.push(AppAction::AddMdRunStage(kind));
                    }
                }
            });

            let total = prompt.stages.len();
            let family = prompt.force_field_family();
            let expanded = prompt.expanded_stage;
            for (index, stage) in prompt.stages.iter().enumerate() {
                ui.add_space(4.0);
                render_md_stage_card(
                    ui,
                    index,
                    total,
                    stage,
                    expanded == Some(index),
                    family,
                    actions,
                );
            }
            if prompt.stages.is_empty() {
                ui.add_space(4.0);
                ui.label("No stages yet. Add one above or pick a preset.");
            }
        }

        // --- Validation ----------------------------------------------------
        if let Some(eff) = prompt.effective() {
            use crate::workflows::molecular_dynamics::run::IssueSeverity;
            let issues = crate::workflows::molecular_dynamics::run::validate(&prompt.stages, &eff);
            if !issues.is_empty() {
                ui.separator();
                for issue in &issues {
                    let (color, prefix) = match issue.severity {
                        IssueSeverity::Error => (egui::Color32::LIGHT_RED, "error"),
                        IssueSeverity::Warning => (egui::Color32::YELLOW, "warning"),
                    };
                    let stage = issue
                        .stage
                        .as_deref()
                        .map(|name| format!("[{name}] "))
                        .unwrap_or_default();
                    ui.label(
                        RichText::new(format!("{prefix}: {stage}{}", issue.message))
                            .small()
                            .color(color),
                    );
                }
            }
        }

        // --- Assembled .mdp preview ----------------------------------------
        // A read-only render of exactly what the run will hand the engine, so the
        // user sees how their inline/detail edits resolve. Computed through the
        // same realization path the launch uses.
        if !prompt.stages.is_empty() {
            ui.separator();
            egui::CollapsingHeader::new("Assembled .mdp preview")
                .id_salt("md_run_mdp_preview")
                .show(ui, |ui| {
                    let specs = crate::engines::gromacs::stage_specs_from_md_stages(
                        &prompt.stages,
                        prompt.force_field_family(),
                        None,
                    );
                    for spec in &specs {
                        ui.label(
                            RichText::new(format!("; {}.mdp", spec.stage_name))
                                .small()
                                .strong()
                                .monospace(),
                        );
                        let mdp = crate::engines::gromacs::input::render_mdp(&spec.settings);
                        let mut text = mdp.as_str();
                        ui.add(
                            egui::TextEdit::multiline(&mut text)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .interactive(false)
                                .code_editor(),
                        );
                        ui.add_space(4.0);
                    }
                });
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

/// One Run MD stage as a uniform card: the same component for every stage kind,
/// showing a header (reorder / remove / expand), an always-visible inline Basic
/// row (temperature / pressure / length), and — when expanded — the detail view
/// of the finer, tier-classified parameters and raw passthrough. Inapplicable
/// fields are simply hidden (a minimize card and an NPT card differ in fields, not
/// in shape). All edits flow out as [`AppAction`]s; this renders, never mutates.
#[allow(clippy::too_many_arguments)]
fn render_md_stage_card(
    ui: &mut egui::Ui,
    index: usize,
    total: usize,
    stage: &crate::workflows::molecular_dynamics::MdStage,
    expanded: bool,
    family: crate::workflows::molecular_dynamics::ForceFieldFamily,
    actions: &mut Vec<AppAction>,
) {
    use crate::frontend::state::MdStageEdit;

    Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        // Header: position + name + kind, then reorder / remove / expand.
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}. {}", index + 1, stage.name)).strong());
            ui.label(
                RichText::new(stage.kind.label())
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let caret = if expanded {
                    egui_phosphor::regular::CARET_UP
                } else {
                    egui_phosphor::regular::CARET_DOWN
                };
                if ui
                    .button(format!("{caret}  Details"))
                    .on_hover_text("Edit this stage's parameters")
                    .clicked()
                {
                    actions.push(AppAction::ToggleMdRunStageExpanded(index));
                }
                if ui
                    .add(egui::Button::new(egui_phosphor::regular::TRASH).frame(false))
                    .clicked()
                {
                    actions.push(AppAction::RemoveMdRunStage(index));
                }
                if ui
                    .add_enabled(
                        index + 1 < total,
                        egui::Button::new(egui_phosphor::regular::ARROW_DOWN),
                    )
                    .clicked()
                {
                    actions.push(AppAction::MoveMdRunStage { index, up: false });
                }
                if ui
                    .add_enabled(
                        index > 0,
                        egui::Button::new(egui_phosphor::regular::ARROW_UP),
                    )
                    .clicked()
                {
                    actions.push(AppAction::MoveMdRunStage { index, up: true });
                }
            });
        });

        // Inline Basic row: temperature (dynamics), pressure (coupled), length.
        ui.horizontal_wrapped(|ui| {
            if stage.kind.is_dynamics() {
                ui.label("T (K)");
                let mut t = stage.temperature_k;
                if ui
                    .add(
                        egui::DragValue::new(&mut t)
                            .range(1.0..=2_000.0_f32)
                            .speed(1.0),
                    )
                    .changed()
                {
                    actions.push(AppAction::EditMdRunStage {
                        index,
                        edit: MdStageEdit::Temperature(t),
                    });
                }
            }
            if let Some(pressure) = stage.pressure {
                ui.separator();
                ui.label("P (bar)");
                let mut p = pressure.ref_bar;
                if ui
                    .add(
                        egui::DragValue::new(&mut p)
                            .range(0.0..=1_000.0_f32)
                            .speed(0.1)
                            .fixed_decimals(1),
                    )
                    .changed()
                {
                    actions.push(AppAction::EditMdRunStage {
                        index,
                        edit: MdStageEdit::PressureBar(p),
                    });
                }
            }
            ui.separator();
            ui.label("Length");
            if let Some(length) = length_editor(ui, index, stage) {
                actions.push(AppAction::EditMdRunStage {
                    index,
                    edit: MdStageEdit::Length(length),
                });
            }
        });

        // A one-line summary keeps collapsed cards informative and uniform.
        let mut summary = format!("{} steps", stage.steps());
        if stage.restraint.is_restrained() {
            summary.push_str(" · restrained");
        }
        if stage.pressure.is_some() {
            summary.push_str(" · NPT");
        }
        ui.label(RichText::new(summary).small().color(egui::Color32::GRAY));

        if expanded {
            ui.separator();
            stage_detail_view(ui, index, stage, family, actions);
        }
    });
}

/// Inline length editor: a value plus a unit (steps / ps / ns). Returns the new
/// [`StageLength`] when the user changes either, else `None`.
fn length_editor(
    ui: &mut egui::Ui,
    index: usize,
    stage: &crate::workflows::molecular_dynamics::MdStage,
) -> Option<crate::workflows::molecular_dynamics::StageLength> {
    use crate::workflows::molecular_dynamics::StageLength;

    #[derive(PartialEq, Clone, Copy)]
    enum Unit {
        Steps,
        Ps,
        Ns,
    }
    // Decompose the current length into a display value and unit.
    let (mut amount, mut unit) = match stage.length {
        StageLength::Steps(n) => (n as f64, Unit::Steps),
        StageLength::Picoseconds(ps) if ps >= 1_000.0 => (ps / 1_000.0, Unit::Ns),
        StageLength::Picoseconds(ps) => (ps, Unit::Ps),
    };
    let compose = |amount: f64, unit: Unit| match unit {
        Unit::Steps => StageLength::Steps(amount.max(0.0) as u64),
        Unit::Ps => StageLength::Picoseconds(amount.max(0.0)),
        Unit::Ns => StageLength::Picoseconds((amount * 1_000.0).max(0.0)),
    };

    let mut changed = None;
    let value_changed = ui
        .add(
            egui::DragValue::new(&mut amount)
                .speed(1.0)
                .range(0.0..=1e12),
        )
        .changed();
    if value_changed {
        changed = Some(compose(amount, unit));
    }
    let before = unit;
    egui::ComboBox::from_id_salt(("md_stage_len_unit", index))
        .selected_text(match unit {
            Unit::Steps => "steps",
            Unit::Ps => "ps",
            Unit::Ns => "ns",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut unit, Unit::Steps, "steps");
            ui.selectable_value(&mut unit, Unit::Ps, "ps");
            ui.selectable_value(&mut unit, Unit::Ns, "ns");
        });
    if unit != before {
        // Switching unit re-expresses the same amount in the new unit.
        changed = Some(compose(amount, unit));
    }
    changed
}

/// The expanded detail view: structural fields that have no inline slot, then the
/// finer parameters split into Standard (shown) and Advanced (collapsed) tiers
/// driven by the [`ParamId`] descriptor table, then per-stage raw passthrough.
fn stage_detail_view(
    ui: &mut egui::Ui,
    index: usize,
    stage: &crate::workflows::molecular_dynamics::MdStage,
    family: crate::workflows::molecular_dynamics::ForceFieldFamily,
    actions: &mut Vec<AppAction>,
) {
    use crate::frontend::state::MdStageEdit;
    use crate::workflows::molecular_dynamics::run::{
        BarostatKind, CouplingGroups, MdParameters, ParamTier,
    };

    egui::Grid::new(("md_stage_detail", index))
        .num_columns(3)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            // Timestep (dynamics only).
            if stage.kind.is_dynamics() {
                ui.label("Timestep (ps)");
                let mut dt = stage.timestep_ps;
                if ui
                    .add(
                        egui::DragValue::new(&mut dt)
                            .range(0.0005..=0.005_f32)
                            .speed(0.0005)
                            .fixed_decimals(4),
                    )
                    .changed()
                {
                    actions.push(AppAction::EditMdRunStage {
                        index,
                        edit: MdStageEdit::Timestep(dt),
                    });
                }
                ui.label("");
                ui.end_row();

                // Coupling groups.
                ui.label("Coupling groups");
                let mut groups = stage.coupling_groups;
                egui::ComboBox::from_id_salt(("md_stage_groups", index))
                    .selected_text(coupling_groups_label(groups))
                    .show_ui(ui, |ui| {
                        for option in [
                            CouplingGroups::WholeSystem,
                            CouplingGroups::SoluteSolvent,
                            CouplingGroups::SoluteLipidSolvent,
                            CouplingGroups::NucleicSolvent,
                        ] {
                            ui.selectable_value(&mut groups, option, coupling_groups_label(option));
                        }
                    });
                if groups != stage.coupling_groups {
                    actions.push(AppAction::EditMdRunStage {
                        index,
                        edit: MdStageEdit::CouplingGroups(groups),
                    });
                }
                ui.label("");
                ui.end_row();
            }

            // Barostat (pressure-coupled stages only).
            if let Some(pressure) = stage.pressure {
                ui.label("Barostat");
                let mut barostat = pressure.barostat;
                egui::ComboBox::from_id_salt(("md_stage_barostat", index))
                    .selected_text(barostat_label(barostat))
                    .show_ui(ui, |ui| {
                        for option in [
                            BarostatKind::StochasticCellRescale,
                            BarostatKind::ParrinelloRahman,
                            BarostatKind::Berendsen,
                        ] {
                            ui.selectable_value(&mut barostat, option, barostat_label(option));
                        }
                    });
                if barostat != pressure.barostat {
                    actions.push(AppAction::EditMdRunStage {
                        index,
                        edit: MdStageEdit::Barostat(barostat),
                    });
                }
                ui.label("");
                ui.end_row();

                ui.label("Barostat τ (ps)");
                let mut tau = pressure.tau_ps;
                if ui
                    .add(
                        egui::DragValue::new(&mut tau)
                            .range(0.5..=20.0_f32)
                            .speed(0.1)
                            .fixed_decimals(1),
                    )
                    .changed()
                {
                    actions.push(AppAction::EditMdRunStage {
                        index,
                        edit: MdStageEdit::BarostatTau(tau),
                    });
                }
                ui.label("");
                ui.end_row();
            }

            // Restraint force constant (restrained stages only).
            if let Some(fc) = stage.restraint.force_constant() {
                ui.label("Restraint k (kJ/mol/nm²)");
                let mut value = fc;
                if ui
                    .add(
                        egui::DragValue::new(&mut value)
                            .range(0.0..=100_000.0_f32)
                            .speed(50.0),
                    )
                    .changed()
                {
                    actions.push(AppAction::EditMdRunStage {
                        index,
                        edit: MdStageEdit::RestraintForceConstant(value),
                    });
                }
                ui.label("");
                ui.end_row();
            }

            // Annealing ramp (annealing stages only).
            if let Some(spec) = &stage.anneal {
                let start = spec
                    .points
                    .first()
                    .copied()
                    .unwrap_or((0.0, stage.temperature_k));
                let end = spec.points.last().copied().unwrap_or(start);
                let (mut start_k, mut end_k, mut duration_ps) = (start.1, end.1, end.0);
                let mut emit = false;
                ui.label("Anneal start (K)");
                emit |= ui
                    .add(
                        egui::DragValue::new(&mut start_k)
                            .range(1.0..=2_000.0_f32)
                            .speed(1.0),
                    )
                    .changed();
                ui.label("");
                ui.end_row();
                ui.label("Anneal end (K)");
                emit |= ui
                    .add(
                        egui::DragValue::new(&mut end_k)
                            .range(1.0..=2_000.0_f32)
                            .speed(1.0),
                    )
                    .changed();
                ui.label("");
                ui.end_row();
                ui.label("Anneal duration (ps)");
                emit |= ui
                    .add(
                        egui::DragValue::new(&mut duration_ps)
                            .range(1.0..=1e6)
                            .speed(10.0),
                    )
                    .changed();
                ui.label("");
                ui.end_row();
                if emit {
                    actions.push(AppAction::EditMdRunStage {
                        index,
                        edit: MdStageEdit::Anneal {
                            start_k,
                            end_k,
                            duration_ps,
                        },
                    });
                }
            }

            // Standard-tier finer parameters (the descriptor table is the source
            // of truth for the inline-vs-detail split: Basic params live inline
            // above, Standard shows here, Advanced collapses below).
            for pid in MdParameters::tiers() {
                if pid.tier() == ParamTier::Standard {
                    param_row(ui, index, stage, *pid, family, actions);
                }
            }
        });

    // Advanced-tier parameters, collapsed by default.
    egui::CollapsingHeader::new("Advanced parameters")
        .id_salt(("md_stage_adv", index))
        .show(ui, |ui| {
            egui::Grid::new(("md_stage_adv_grid", index))
                .num_columns(3)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    for pid in MdParameters::tiers() {
                        if pid.tier() == ParamTier::Advanced {
                            param_row(ui, index, stage, *pid, family, actions);
                        }
                    }
                });
        });

    // Per-stage raw passthrough — identical semantics to `md run --raw`.
    egui::CollapsingHeader::new("Raw .mdp passthrough")
        .id_salt(("md_stage_raw", index))
        .show(ui, |ui| {
            for (line, (key, value)) in stage.raw_passthrough.iter().enumerate() {
                ui.horizontal(|ui| {
                    let mut k = key.clone();
                    let mut v = value.clone();
                    let key_changed = ui
                        .add(
                            egui::TextEdit::singleline(&mut k)
                                .hint_text("key")
                                .desired_width(120.0),
                        )
                        .changed();
                    ui.label("=");
                    let value_changed = ui
                        .add(
                            egui::TextEdit::singleline(&mut v)
                                .hint_text("value")
                                .desired_width(120.0),
                        )
                        .changed();
                    if key_changed || value_changed {
                        actions.push(AppAction::EditMdRunStage {
                            index,
                            edit: MdStageEdit::SetRawLine {
                                line,
                                key: k,
                                value: v,
                            },
                        });
                    }
                    if ui
                        .add(egui::Button::new(egui_phosphor::regular::TRASH).frame(false))
                        .clicked()
                    {
                        actions.push(AppAction::EditMdRunStage {
                            index,
                            edit: MdStageEdit::RemoveRawLine(line),
                        });
                    }
                });
            }
            if ui.button("+ Add line").clicked() {
                actions.push(AppAction::EditMdRunStage {
                    index,
                    edit: MdStageEdit::AddRawLine,
                });
            }
            ui.label(
                RichText::new(
                    "Verbatim key = value lines, merged last; may override any directive.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
        });
}

fn coupling_groups_label(
    groups: crate::workflows::molecular_dynamics::run::CouplingGroups,
) -> &'static str {
    use crate::workflows::molecular_dynamics::run::CouplingGroups;
    match groups {
        CouplingGroups::WholeSystem => "Whole system",
        CouplingGroups::SoluteSolvent => "Solute / solvent",
        CouplingGroups::SoluteLipidSolvent => "Solute / lipid / solvent",
        CouplingGroups::NucleicSolvent => "Nucleic / solvent",
    }
}

fn barostat_label(
    barostat: crate::workflows::molecular_dynamics::run::BarostatKind,
) -> &'static str {
    use crate::workflows::molecular_dynamics::run::BarostatKind;
    match barostat {
        BarostatKind::StochasticCellRescale => "Stochastic cell rescale",
        BarostatKind::ParrinelloRahman => "Parrinello–Rahman",
        BarostatKind::Berendsen => "Berendsen (equilibration)",
    }
}

/// Render one tiered parameter row (label · control · default-state) for a
/// [`ParamId`], emitting the matching [`MdStageEdit`] when the user changes it.
/// `None` parameters show a "set" affordance; set ones show a "default" revert.
fn param_row(
    ui: &mut egui::Ui,
    index: usize,
    stage: &crate::workflows::molecular_dynamics::MdStage,
    pid: crate::workflows::molecular_dynamics::run::ParamId,
    family: crate::workflows::molecular_dynamics::ForceFieldFamily,
    actions: &mut Vec<AppAction>,
) {
    use crate::frontend::state::MdStageEdit;
    use crate::workflows::molecular_dynamics::run::{
        ConstraintScope, ParamId, ThermostatKind, family_nonbonded_intent,
    };

    let params = &stage.params;
    let (rc, rv) = family_nonbonded_intent(family);
    let mut push = |edit: MdStageEdit| {
        actions.push(AppAction::EditMdRunStage { index, edit });
    };

    match pid {
        ParamId::CoulombCutoff => {
            if let Some(new) = opt_f32_row(
                ui,
                index,
                pid,
                params.coulomb_cutoff_nm,
                rc,
                0.5..=2.0,
                0.05,
                2,
            ) {
                push(MdStageEdit::CoulombCutoff(new));
            }
        }
        ParamId::VdwCutoff => {
            if let Some(new) =
                opt_f32_row(ui, index, pid, params.vdw_cutoff_nm, rv, 0.5..=2.0, 0.05, 2)
            {
                push(MdStageEdit::VdwCutoff(new));
            }
        }
        ParamId::Thermostat => {
            ui.label(pid.label());
            let mut selected = params.thermostat;
            let label = match selected {
                None => "default",
                Some(ThermostatKind::StochasticVelocityRescale) => "V-rescale",
                Some(ThermostatKind::NoseHoover) => "Nosé–Hoover",
                Some(ThermostatKind::Berendsen) => "Berendsen",
            };
            egui::ComboBox::from_id_salt(("md_param_thermostat", index))
                .selected_text(label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut selected, None, "default");
                    ui.selectable_value(
                        &mut selected,
                        Some(ThermostatKind::StochasticVelocityRescale),
                        "V-rescale",
                    );
                    ui.selectable_value(
                        &mut selected,
                        Some(ThermostatKind::NoseHoover),
                        "Nosé–Hoover",
                    );
                    ui.selectable_value(
                        &mut selected,
                        Some(ThermostatKind::Berendsen),
                        "Berendsen",
                    );
                });
            if selected != params.thermostat {
                push(MdStageEdit::Thermostat(selected));
            }
            ui.label("");
            ui.end_row();
        }
        ParamId::ThermostatTau => {
            if let Some(new) = opt_f32_row(
                ui,
                index,
                pid,
                params.thermostat_tau_ps,
                0.1,
                0.05..=10.0,
                0.05,
                2,
            ) {
                push(MdStageEdit::ThermostatTau(new));
            }
        }
        ParamId::Constraints => {
            ui.label(pid.label());
            let mut selected = params.constraints;
            let label = match selected {
                None => "default",
                Some(ConstraintScope::None) => "None (flexible)",
                Some(ConstraintScope::HBonds) => "H-bonds",
                Some(ConstraintScope::AllBonds) => "All bonds",
            };
            egui::ComboBox::from_id_salt(("md_param_constraints", index))
                .selected_text(label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut selected, None, "default");
                    ui.selectable_value(
                        &mut selected,
                        Some(ConstraintScope::None),
                        "None (flexible)",
                    );
                    ui.selectable_value(&mut selected, Some(ConstraintScope::HBonds), "H-bonds");
                    ui.selectable_value(
                        &mut selected,
                        Some(ConstraintScope::AllBonds),
                        "All bonds",
                    );
                });
            if selected != params.constraints {
                push(MdStageEdit::Constraints(selected));
            }
            ui.label("");
            ui.end_row();
        }
        ParamId::PmeSpacing => {
            if let Some(new) = opt_f32_row(
                ui,
                index,
                pid,
                params.pme_spacing_nm,
                0.12,
                0.05..=0.3,
                0.01,
                3,
            ) {
                push(MdStageEdit::PmeSpacing(new));
            }
        }
        ParamId::PmeOrder => {
            if let Some(new) = opt_u32_row(ui, index, pid, params.pme_order, 4, 4..=10) {
                push(MdStageEdit::PmeOrder(new));
            }
        }
        ParamId::ConstraintOrder => {
            if let Some(new) = opt_u32_row(ui, index, pid, params.constraint_order, 4, 1..=12) {
                push(MdStageEdit::ConstraintOrder(new));
            }
        }
        ParamId::ConstraintIterations => {
            if let Some(new) = opt_u32_row(ui, index, pid, params.constraint_iterations, 1, 1..=10)
            {
                push(MdStageEdit::ConstraintIterations(new));
            }
        }
        ParamId::DispersionCorrection => {
            if let Some(new) = opt_bool_row(ui, index, pid, params.dispersion_correction) {
                push(MdStageEdit::DispersionCorrection(new));
            }
        }
        ParamId::RemoveComMotion => {
            if let Some(new) = opt_bool_row(ui, index, pid, params.remove_com_motion) {
                push(MdStageEdit::RemoveComMotion(new));
            }
        }
        ParamId::NeighborListSteps => {
            if let Some(new) = opt_u32_row(ui, index, pid, params.neighbor_list_steps, 10, 1..=200)
            {
                push(MdStageEdit::NeighborListSteps(new));
            }
        }
        ParamId::RandomSeed => {
            ui.label(pid.label());
            match params.random_seed {
                Some(mut seed) => {
                    if ui
                        .add(
                            egui::DragValue::new(&mut seed)
                                .range(-1..=i64::MAX)
                                .speed(1.0),
                        )
                        .changed()
                    {
                        push(MdStageEdit::RandomSeed(Some(seed)));
                    }
                    if ui.small_button("default").clicked() {
                        push(MdStageEdit::RandomSeed(None));
                    }
                }
                None => {
                    if ui.button("set").clicked() {
                        push(MdStageEdit::RandomSeed(Some(-1)));
                    }
                    ui.label(RichText::new("default").small().color(egui::Color32::GRAY));
                }
            }
            ui.end_row();
        }
    }
}

/// A label · DragValue · revert row for an `Option<f32>` parameter. Returns the
/// new value (`Some(Some(v))` to set, `Some(None)` to revert) when it changes.
#[allow(clippy::too_many_arguments)]
fn opt_f32_row(
    ui: &mut egui::Ui,
    index: usize,
    pid: crate::workflows::molecular_dynamics::run::ParamId,
    value: Option<f32>,
    default: f32,
    range: std::ops::RangeInclusive<f32>,
    speed: f32,
    decimals: usize,
) -> Option<Option<f32>> {
    let _ = index;
    ui.label(pid.label());
    let mut result = None;
    match value {
        Some(mut v) => {
            if ui
                .add(
                    egui::DragValue::new(&mut v)
                        .range(range)
                        .speed(speed as f64)
                        .fixed_decimals(decimals),
                )
                .changed()
            {
                result = Some(Some(v));
            }
            if ui.small_button("default").clicked() {
                result = Some(None);
            }
        }
        None => {
            if ui.button("set").clicked() {
                result = Some(Some(default));
            }
            ui.label(RichText::new("default").small().color(egui::Color32::GRAY));
        }
    }
    ui.end_row();
    result
}

/// As [`opt_f32_row`] for an `Option<u32>` parameter.
fn opt_u32_row(
    ui: &mut egui::Ui,
    index: usize,
    pid: crate::workflows::molecular_dynamics::run::ParamId,
    value: Option<u32>,
    default: u32,
    range: std::ops::RangeInclusive<u32>,
) -> Option<Option<u32>> {
    let _ = index;
    ui.label(pid.label());
    let mut result = None;
    match value {
        Some(mut v) => {
            if ui
                .add(egui::DragValue::new(&mut v).range(range).speed(1.0))
                .changed()
            {
                result = Some(Some(v));
            }
            if ui.small_button("default").clicked() {
                result = Some(None);
            }
        }
        None => {
            if ui.button("set").clicked() {
                result = Some(Some(default));
            }
            ui.label(RichText::new("default").small().color(egui::Color32::GRAY));
        }
    }
    ui.end_row();
    result
}

/// As [`opt_f32_row`] for an `Option<bool>` parameter (default / yes / no combo).
fn opt_bool_row(
    ui: &mut egui::Ui,
    index: usize,
    pid: crate::workflows::molecular_dynamics::run::ParamId,
    value: Option<bool>,
) -> Option<Option<bool>> {
    ui.label(pid.label());
    let mut selected = value;
    let label = match selected {
        None => "default",
        Some(true) => "yes",
        Some(false) => "no",
    };
    egui::ComboBox::from_id_salt(("md_param_bool", pid.label(), index))
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut selected, None, "default");
            ui.selectable_value(&mut selected, Some(true), "yes");
            ui.selectable_value(&mut selected, Some(false), "no");
        });
    let result = (selected != value).then_some(selected);
    ui.label("");
    ui.end_row();
    result
}
