use eframe::egui::{self, Button, Frame, Margin, RichText, ScrollArea, Sense, Stroke, Ui};

use crate::frontend::{
    ViewportDrawArgs,
    actions::AppAction,
    draw_viewport,
    state::AppState,
    viewport::{HOVER_FRAME, STRUCTURE_INTERACTION_FRAME},
};

use super::bottom_panel::render_bottom_panel;
pub(super) fn render_workspace(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
) {
    if state.ui.layout.show_panel {
        // Fixed height + driven by our own `panel_height`: egui 0.34's resizable
        // `Panel` persists the framed content rect as the next frame's size, and
        // with fill content that feeds back into a few-pixel-per-frame growth
        // that runs away on continuous repaints. `exact_size` pins the height so
        // it can't drift; the divider is resized by a custom handle (see
        // `show_workbench`) that writes `panel_height`, matching the sidebars.
        egui::Panel::bottom("bottom_panel")
            .exact_size(state.ui.layout.panel_height)
            .frame(
                Frame::default()
                    .fill(crate::frontend::theme::palette(ui).bottom_panel)
                    .inner_margin(Margin::symmetric(10, 8)),
            )
            .show_inside(ui, |ui| render_bottom_panel(state, ui, actions));
    }

    egui::CentralPanel::default()
        .frame(Frame::default().fill(crate::frontend::theme::palette(ui).central))
        .show_inside(ui, |ui| {
            if let Some(entry) = state.entries.active_entry() {
                let structure_id = entry.id;
                let structure_revision = entry.revision;
                let structure = &entry.structure;
                let ui_state = &mut state.ui;
                let viewport_interaction = draw_viewport(
                    ui,
                    ViewportDrawArgs {
                        structure,
                        structure_id,
                        structure_revision,
                        camera: &mut ui_state.camera,
                        selection: &ui_state.selection,
                        visual_state: &ui_state.viewport,
                        previous_hovered_atom: ui_state.hovered_atom,
                        cache: &mut ui_state.viewport_cache,
                        gpu_ready: ui_state.gpu_ready,
                        empty_state_hint: None,
                    },
                );
                if viewport_interaction.hover_changed {
                    ui_state.hovered_atom = viewport_interaction.hovered_atom;
                }
                if viewport_interaction.camera_changed || viewport_interaction.active_drag {
                    ui.ctx().request_repaint_after(STRUCTURE_INTERACTION_FRAME);
                } else if viewport_interaction.hover_changed {
                    ui.ctx().request_repaint_after(HOVER_FRAME);
                }

                let mut assigned_atom = None;
                if let Some(index) = viewport_interaction.clicked_atom {
                    let toggle = ui.input(|input| input.modifiers.command || input.modifiers.ctrl);
                    actions.push(AppAction::SelectAtom {
                        atom_index: index,
                        toggle,
                    });
                    if let Some(editor) = &mut ui_state.block_editor
                        && editor.apply_picked_atom(index)
                    {
                        assigned_atom = Some(index);
                    }
                }
                if let Some(index) = assigned_atom {
                    state.set_message(format!("Assigned atom {}", index + 1));
                }
            } else if !state.workspace.is_project() && state.entries.tabs.is_empty() {
                render_scratch_workspace(state, ui, actions);
            } else {
                let empty_structure = crate::domain::Structure::empty();
                let ui_state = &mut state.ui;
                let _ = draw_viewport(
                    ui,
                    ViewportDrawArgs {
                        structure: &empty_structure,
                        structure_id: 0,
                        structure_revision: 0,
                        camera: &mut ui_state.camera,
                        selection: &ui_state.selection,
                        visual_state: &ui_state.viewport,
                        previous_hovered_atom: ui_state.hovered_atom,
                        cache: &mut ui_state.viewport_cache,
                        gpu_ready: ui_state.gpu_ready,
                        empty_state_hint: state.entries.tabs.is_empty().then_some(
                            "Open a structure from File > Open, or drag and drop one here.",
                        ),
                    },
                );
            }
        });
}

fn render_scratch_workspace(state: &mut AppState, ui: &mut egui::Ui, actions: &mut Vec<AppAction>) {
    let content_width = ui.available_width().min(420.0);
    let recent_projects = state.recent_projects.clone();
    let pal = crate::frontend::theme::palette(ui);

    ui.vertical_centered(|ui| {
        ui.add_space(42.0);
        ui.set_width(content_width);
        ui.heading("Scratch temporary workspace");
        ui.add_space(4.0);
        ui.label(
            RichText::new("This workspace is not stored after Phonon closes.")
                .color(pal.text_muted),
        );
        ui.add_space(24.0);

        render_scratch_action_button(
            ui,
            egui_phosphor::regular::FOLDER_OPEN,
            "Open Project",
            AppAction::OpenProject,
            actions,
        );
        ui.add_space(8.0);
        render_scratch_action_button(
            ui,
            egui_phosphor::regular::FOLDER_PLUS,
            "Create a new project",
            AppAction::CreateProject,
            actions,
        );
        ui.add_space(8.0);
        render_scratch_action_button(
            ui,
            egui_phosphor::regular::FILE_PLUS,
            "Open file",
            AppAction::OpenFile,
            actions,
        );

        ui.add_space(34.0);
        ui.label(RichText::new("Recent Projects").strong());
        ui.add_space(10.0);

        if recent_projects.is_empty() {
            ui.label(RichText::new("No recent projects.").color(pal.text_tertiary));
            return;
        }

        ScrollArea::vertical()
            .max_height(ui.available_height().max(120.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(content_width);
                for project in recent_projects {
                    let response = Frame::default()
                        .fill(pal.item_fill)
                        .stroke(Stroke::new(1.0, pal.hairline))
                        .inner_margin(Margin::symmetric(12, 9))
                        .show(ui, |ui| {
                            ui.set_width(content_width - 24.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(egui_phosphor::regular::FOLDER_OPEN)
                                        .color(pal.accent),
                                );
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&project.name).strong());
                                    ui.label(
                                        RichText::new(project.path.display().to_string())
                                            .small()
                                            .color(pal.text_tertiary),
                                    );
                                });
                            });
                        })
                        .response
                        .interact(Sense::click());
                    if response.clicked() {
                        actions.push(AppAction::OpenRecentProject(project.path));
                    }
                    ui.add_space(6.0);
                }
            });
    });
}

fn render_scratch_action_button(
    ui: &mut Ui,
    icon: &'static str,
    label: &'static str,
    action: AppAction,
    actions: &mut Vec<AppAction>,
) {
    let width = ui.available_width();
    let response = ui
        .scope(|ui| {
            let pal = crate::frontend::theme::palette(ui);
            let visuals = &mut ui.style_mut().visuals.widgets;
            visuals.inactive.weak_bg_fill = pal.item_fill;
            visuals.inactive.bg_fill = pal.item_fill;
            visuals.inactive.bg_stroke = Stroke::new(1.0, pal.hairline);
            visuals.hovered.weak_bg_fill = pal.item_fill_hover;
            visuals.hovered.bg_fill = pal.item_fill_hover;
            visuals.hovered.bg_stroke = Stroke::new(1.0, pal.hairline);
            ui.add_sized(
                [width, 44.0],
                Button::new(
                    RichText::new(format!("{icon}  {label}"))
                        .size(14.0)
                        .color(pal.text_primary),
                ),
            )
        })
        .inner;
    if response.clicked() {
        actions.push(action);
    }
}
