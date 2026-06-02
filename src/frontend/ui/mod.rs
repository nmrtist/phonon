use eframe::egui::{
    self, Align, Button, CursorIcon, Frame, Id, Layout, Margin, Order, Rect, RichText, ScrollArea,
    Sense, Stroke, Ui, Vec2,
};
use egui::viewport::{ResizeDirection, ViewportCommand};

use crate::{
    backend::tasks::task_controllers,
    engines::registry::{EngineId, EngineLaunch},
    frontend::{
        CartoonSectionStyle, LightPreset, SurfaceStyle,
        actions::AppAction,
        services::entry_details,
        state::{AppState, AtomStyle, EngineDraft, PrimaryView},
    },
};

mod bottom_panel;
mod secondary_sidebar;
mod workspace;

use bottom_panel::render_status_bar;
use secondary_sidebar::render_secondary_sidebar;
use workspace::render_workspace;
pub fn show_workbench(state: &mut AppState, ui: &mut egui::Ui, actions: &mut Vec<AppAction>) {
    let ctx = ui.ctx().clone();
    render_window_resize_handles(&ctx);

    egui::Panel::top("title_bar")
        .exact_size(32.0)
        .frame(
            Frame::default()
                .fill(egui::Color32::from_rgb(246, 248, 251))
                .inner_margin(Margin::symmetric(8, 3)),
        )
        .show_inside(ui, |ui| render_title_bar(state, ui, actions));

    egui::Panel::bottom("status_bar")
        .exact_size(24.0)
        .frame(
            Frame::default()
                .fill(egui::Color32::from_rgb(229, 236, 244))
                .inner_margin(Margin::symmetric(10, 3)),
        )
        .show_inside(ui, |ui| render_status_bar(state, ui));

    egui::Panel::left("activity_bar")
        .exact_size(52.0)
        .frame(
            Frame::default()
                .fill(egui::Color32::from_rgb(240, 243, 247))
                .inner_margin(Margin::symmetric(6, 10)),
        )
        .show_inside(ui, |ui| render_activity_bar(state, ui));

    if state.ui.layout.show_primary_sidebar {
        egui::Panel::left("primary_sidebar")
            .default_size(state.ui.layout.primary_sidebar_width)
            .min_size(220.0)
            .resizable(true)
            .frame(
                Frame::default()
                    .fill(egui::Color32::from_rgb(252, 252, 253))
                    .inner_margin(Margin::symmetric(10, 10)),
            )
            .show_inside(ui, |ui| {
                state.ui.layout.primary_sidebar_width = ui.available_width();
                render_primary_sidebar(state, ui, actions);
            });
    }

    if state.ui.layout.show_secondary_sidebar {
        egui::Panel::right("secondary_sidebar")
            .default_size(state.ui.layout.secondary_sidebar_width)
            .min_size(240.0)
            .resizable(true)
            .frame(
                Frame::default()
                    .fill(egui::Color32::from_rgb(252, 252, 253))
                    .inner_margin(Margin::symmetric(10, 10)),
            )
            .show_inside(ui, |ui| {
                state.ui.layout.secondary_sidebar_width = ui.available_width();
                render_secondary_sidebar(state, ui, actions);
            });
    }

    egui::CentralPanel::default()
        .frame(
            Frame::default()
                .fill(egui::Color32::from_rgb(245, 247, 249))
                .inner_margin(Margin::same(0)),
        )
        .show_inside(ui, |ui| render_workspace(state, ui, actions));

    render_structure_editor_window(state, actions, &ctx);
    render_pdb_fetch_window(state, actions, &ctx);
}

const WINDOW_RESIZE_HANDLE_THICKNESS: f32 = 6.0;
const WINDOW_RESIZE_CORNER_SIZE: f32 = 18.0;

fn render_window_resize_handles(ctx: &egui::Context) {
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    if maximized {
        return;
    }

    let viewport_rect = ctx.viewport_rect();
    let handle = WINDOW_RESIZE_HANDLE_THICKNESS;
    let corner = WINDOW_RESIZE_CORNER_SIZE;

    for spec in [
        ResizeHandleSpec::new(
            "north_west",
            Rect::from_min_size(viewport_rect.min, egui::vec2(corner, corner)),
            ResizeDirection::NorthWest,
            CursorIcon::ResizeNorthWest,
        ),
        ResizeHandleSpec::new(
            "north_east",
            Rect::from_min_size(
                egui::pos2(viewport_rect.right() - corner, viewport_rect.top()),
                egui::vec2(corner, corner),
            ),
            ResizeDirection::NorthEast,
            CursorIcon::ResizeNorthEast,
        ),
        ResizeHandleSpec::new(
            "south_west",
            Rect::from_min_size(
                egui::pos2(viewport_rect.left(), viewport_rect.bottom() - corner),
                egui::vec2(corner, corner),
            ),
            ResizeDirection::SouthWest,
            CursorIcon::ResizeSouthWest,
        ),
        ResizeHandleSpec::new(
            "south_east",
            Rect::from_min_size(
                egui::pos2(
                    viewport_rect.right() - corner,
                    viewport_rect.bottom() - corner,
                ),
                egui::vec2(corner, corner),
            ),
            ResizeDirection::SouthEast,
            CursorIcon::ResizeSouthEast,
        ),
        ResizeHandleSpec::new(
            "north",
            Rect::from_min_max(
                egui::pos2(viewport_rect.left() + corner, viewport_rect.top()),
                egui::pos2(viewport_rect.right() - corner, viewport_rect.top() + handle),
            ),
            ResizeDirection::North,
            CursorIcon::ResizeNorth,
        ),
        ResizeHandleSpec::new(
            "south",
            Rect::from_min_max(
                egui::pos2(
                    viewport_rect.left() + corner,
                    viewport_rect.bottom() - handle,
                ),
                egui::pos2(viewport_rect.right() - corner, viewport_rect.bottom()),
            ),
            ResizeDirection::South,
            CursorIcon::ResizeSouth,
        ),
        ResizeHandleSpec::new(
            "west",
            Rect::from_min_max(
                egui::pos2(viewport_rect.left(), viewport_rect.top() + corner),
                egui::pos2(
                    viewport_rect.left() + handle,
                    viewport_rect.bottom() - corner,
                ),
            ),
            ResizeDirection::West,
            CursorIcon::ResizeWest,
        ),
        ResizeHandleSpec::new(
            "east",
            Rect::from_min_max(
                egui::pos2(viewport_rect.right() - handle, viewport_rect.top() + corner),
                egui::pos2(viewport_rect.right(), viewport_rect.bottom() - corner),
            ),
            ResizeDirection::East,
            CursorIcon::ResizeEast,
        ),
    ] {
        render_resize_handle(ctx, spec);
    }
}

#[derive(Clone, Copy)]
struct ResizeHandleSpec {
    id: &'static str,
    rect: Rect,
    direction: ResizeDirection,
    cursor_icon: CursorIcon,
}

impl ResizeHandleSpec {
    const fn new(
        id: &'static str,
        rect: Rect,
        direction: ResizeDirection,
        cursor_icon: CursorIcon,
    ) -> Self {
        Self {
            id,
            rect,
            direction,
            cursor_icon,
        }
    }
}

fn render_resize_handle(ctx: &egui::Context, spec: ResizeHandleSpec) {
    egui::Area::new(Id::new(spec.id))
        .order(Order::Foreground)
        .fixed_pos(spec.rect.min)
        .interactable(true)
        .show(ctx, |ui| {
            let (_, response) = ui.allocate_exact_size(spec.rect.size(), Sense::click_and_drag());
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(spec.cursor_icon);
            }
            if response.drag_started() {
                ui.ctx()
                    .send_viewport_cmd(ViewportCommand::BeginResize(spec.direction));
            }
        });
}

const CORE_BUTTON_CORNER_RADIUS: u8 = 4;
const CORE_BUTTON_HOVER_ALPHA: u8 = 26;
const CORE_BUTTON_SELECTED_ALPHA: u8 = 44;
const CORE_BUTTON_SELECTED_HOVER_ALPHA: u8 = 58;

fn render_title_bar(state: &mut AppState, ui: &mut egui::Ui, actions: &mut Vec<AppAction>) {
    let ctx = ui.ctx().clone();
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    let show_inline_menus = !cfg!(target_os = "macos");
    let has_active_entry = state.has_active_entry();
    let title_color = egui::Color32::from_rgb(32, 37, 43);
    let muted_text = egui::Color32::from_rgb(92, 100, 112);
    let centered_title = state.workspace_label();
    let title_bar_rect = ui.max_rect();

    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.label(
            RichText::new("Phonon")
                .strong()
                .size(14.0)
                .color(title_color),
        );

        if show_inline_menus {
            with_core_button_style(ui, false, |ui| {
                ui.menu_button(RichText::new("File").color(title_color), |ui| {
                    if ui.button("Create a new project...").clicked() {
                        actions.push(AppAction::CreateProject);
                        ui.close();
                    }
                    if ui.button("Open Project...").clicked() {
                        actions.push(AppAction::OpenProject);
                        ui.close();
                    }
                    if ui.button("Save Project").clicked() {
                        actions.push(AppAction::SaveProject);
                        ui.close();
                    }
                    if ui
                        .add_enabled(state.workspace.is_project(), Button::new("Close Project"))
                        .clicked()
                    {
                        actions.push(AppAction::CloseProject);
                        ui.close();
                    }
                    if !state.recent_projects.is_empty() {
                        ui.separator();
                        ui.menu_button("Recent Projects", |ui| {
                            for project in state.recent_projects.clone() {
                                if ui
                                    .button(format!("{}\n{}", project.name, project.path.display()))
                                    .clicked()
                                {
                                    actions.push(AppAction::OpenRecentProject(project.path));
                                    ui.close();
                                }
                            }
                        });
                    }
                    ui.separator();
                    if ui.button("New Empty Entry").clicked() {
                        actions.push(AppAction::NewEmptyEntry);
                        ui.close();
                    }
                    if ui.button("Open File...").clicked() {
                        actions.push(AppAction::OpenFile);
                        ui.close();
                    }
                    if ui.button("Fetch from PDB ID...").clicked() {
                        actions.push(AppAction::OpenPdbFetchDialog);
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(has_active_entry, Button::new("Save"))
                        .clicked()
                    {
                        actions.push(AppAction::Save);
                        ui.close();
                    }
                    if ui
                        .add_enabled(has_active_entry, Button::new("Save As..."))
                        .clicked()
                    {
                        actions.push(AppAction::SaveAs);
                        ui.close();
                    }
                });
            });

            with_core_button_style(ui, false, |ui| {
                ui.menu_button(RichText::new("Edit").color(title_color), |ui| {
                    if ui
                        .add_enabled(state.can_undo(), Button::new("Undo\tCtrl+Z"))
                        .clicked()
                    {
                        actions.push(AppAction::Undo);
                        ui.close();
                    }
                    if ui
                        .add_enabled(state.can_redo(), Button::new("Redo\tCtrl+Y / Ctrl+Shift+Z"))
                        .clicked()
                    {
                        actions.push(AppAction::Redo);
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(has_active_entry, Button::new("Edit Structure..."))
                        .clicked()
                    {
                        actions.push(AppAction::EditStructure);
                        ui.close();
                    }
                });
            });

            with_core_button_style(ui, false, |ui| {
                ui.menu_button(RichText::new("Selection").color(title_color), |ui| {
                    if ui.button("Select All").clicked() {
                        actions.push(AppAction::SelectAll);
                        ui.close();
                    }
                    if ui.button("Invert Selection").clicked() {
                        actions.push(AppAction::InvertSelection);
                        ui.close();
                    }
                    if ui.button("Clear Selection").clicked() {
                        actions.push(AppAction::ClearSelection);
                        ui.close();
                    }
                    ui.separator();
                    ui.label(
                        RichText::new("Select by type")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                    for category in crate::domain::AtomCategory::selectable() {
                        if ui.button(category.label()).clicked() {
                            actions.push(AppAction::SelectCategory(*category));
                            ui.close();
                        }
                    }
                });
            });

            with_core_button_style(ui, false, |ui| {
                ui.menu_button(RichText::new("View").color(title_color), |ui| {
                    ui.checkbox(
                        &mut state.ui.layout.show_primary_sidebar,
                        "Primary Side Bar",
                    );
                    ui.checkbox(
                        &mut state.ui.layout.show_secondary_sidebar,
                        "Secondary Side Bar",
                    );
                    ui.checkbox(&mut state.ui.layout.show_panel, "Panel");
                    ui.checkbox(&mut state.ui.viewport.show_atom_labels, "Show Atom Labels");
                    ui.separator();
                    let selection_len = state.ui.selection.len();
                    ui.label(if selection_len == 0 {
                        "Style (all atoms)".to_string()
                    } else {
                        format!("Style ({selection_len} selected)")
                    });
                    for style in AtomStyle::all() {
                        if ui.button(style.label()).clicked() {
                            actions.push(AppAction::SetSelectionStyle(*style));
                            ui.close();
                        }
                    }
                });
            });
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if !cfg!(target_os = "macos") {
                render_window_controls(ui, maximized);
            }
        });
    });

    let left_reserved_width = if show_inline_menus { 300.0 } else { 96.0 };
    let right_reserved_width = if cfg!(target_os = "macos") {
        0.0
    } else {
        120.0
    };
    let center_width = (title_bar_rect.width() - left_reserved_width - right_reserved_width - 16.0)
        .clamp(96.0, 320.0);
    let center_drag_rect = Rect::from_center_size(
        title_bar_rect.center(),
        egui::vec2(center_width, title_bar_rect.height() - 6.0),
    );
    let drag_strip_top = title_bar_rect.top() + 2.0;
    let drag_strip_bottom = title_bar_rect.bottom() - 2.0;
    let left_drag_rect = Rect::from_min_max(
        egui::pos2(left_reserved_width, drag_strip_top),
        egui::pos2(
            center_drag_rect.left().max(left_reserved_width),
            drag_strip_bottom,
        ),
    );
    let right_drag_rect = Rect::from_min_max(
        egui::pos2(
            center_drag_rect
                .right()
                .min(title_bar_rect.right() - right_reserved_width),
            drag_strip_top,
        ),
        egui::pos2(
            title_bar_rect.right() - right_reserved_width,
            drag_strip_bottom,
        ),
    );

    let center_drag_response = ui.interact(
        center_drag_rect,
        Id::new("title_bar_drag_area_center"),
        Sense::click_and_drag(),
    );
    let left_drag_response = ui.interact(
        left_drag_rect,
        Id::new("title_bar_drag_area_left"),
        Sense::click_and_drag(),
    );
    let right_drag_response = ui.interact(
        right_drag_rect,
        Id::new("title_bar_drag_area_right"),
        Sense::click_and_drag(),
    );
    ui.painter().text(
        center_drag_rect.center(),
        egui::Align2::CENTER_CENTER,
        centered_title,
        egui::FontId::proportional(14.0),
        muted_text,
    );
    if center_drag_response.drag_started()
        || left_drag_response.drag_started()
        || right_drag_response.drag_started()
    {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if (center_drag_response.double_clicked()
        || left_drag_response.double_clicked()
        || right_drag_response.double_clicked())
        && !cfg!(target_os = "macos")
    {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    }

    fn render_window_controls(ui: &mut Ui, maximized: bool) {
        let ctx = ui.ctx().clone();
        for (icon, command, hover_fill) in [
            (
                egui_phosphor::regular::X,
                ViewportCommand::Close,
                egui::Color32::from_rgb(232, 84, 82),
            ),
            (
                if maximized {
                    egui_phosphor::regular::CORNERS_IN
                } else {
                    egui_phosphor::regular::CORNERS_OUT
                },
                ViewportCommand::Maximized(!maximized),
                egui::Color32::from_rgb(228, 232, 238),
            ),
            (
                egui_phosphor::regular::MINUS,
                ViewportCommand::Minimized(true),
                egui::Color32::from_rgb(228, 232, 238),
            ),
        ] {
            let response = window_control_button(ui, icon, hover_fill);
            if response.clicked() {
                ctx.send_viewport_cmd(command);
            }
        }
    }
}

fn render_activity_bar(state: &mut AppState, ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        for view in PrimaryView::all() {
            let selected = state.ui.layout.active_primary_view == *view;
            let response = with_core_button_style(ui, selected, |ui| {
                ui.add_sized(
                    [36.0, 36.0],
                    Button::new(
                        RichText::new(view.icon())
                            .strong()
                            .color(core_button_text_color(selected)),
                    )
                    .selected(selected),
                )
            })
            .on_hover_text(view.label());
            if response.clicked() {
                if selected && state.ui.layout.show_primary_sidebar {
                    state.ui.layout.show_primary_sidebar = false;
                } else {
                    state.ui.layout.active_primary_view = *view;
                    state.ui.layout.show_primary_sidebar = true;
                }
            }
        }
    });
}

fn render_primary_sidebar(state: &mut AppState, ui: &mut egui::Ui, actions: &mut Vec<AppAction>) {
    ui.horizontal(|ui| {
        ui.heading(state.ui.layout.active_primary_view.label());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if with_core_button_style(ui, false, |ui| {
                ui.add_sized(
                    [28.0, 28.0],
                    Button::new(
                        RichText::new(egui_phosphor::regular::CARET_LEFT)
                            .color(core_button_text_color(false)),
                    ),
                )
            })
            .on_hover_text("Hide sidebar")
            .clicked()
            {
                state.ui.layout.show_primary_sidebar = false;
            }
        });
    });
    ui.separator();

    match state.ui.layout.active_primary_view {
        PrimaryView::EntryList => render_entry_list(state, ui, actions),
        PrimaryView::Tasks => render_tasks_view(state, ui, actions),
        PrimaryView::Settings => viewport_visual_settings_view(state, ui, actions),
    }
}

fn render_entry_list(state: &mut AppState, ui: &mut egui::Ui, actions: &mut Vec<AppAction>) {
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if with_core_button_style(ui, false, |ui| {
                ui.add_sized(
                    [24.0, 24.0],
                    Button::new(
                        RichText::new(egui_phosphor::regular::FILE_PLUS)
                            .size(13.0)
                            .color(core_button_text_color(false)),
                    )
                    .frame(false),
                )
            })
            .on_hover_text("New Entry")
            .clicked()
            {
                actions.push(AppAction::NewEmptyEntry);
            }
            if with_core_button_style(ui, false, |ui| {
                ui.add_sized(
                    [24.0, 24.0],
                    Button::new(
                        RichText::new(egui_phosphor::regular::FOLDER_PLUS)
                            .size(13.0)
                            .color(core_button_text_color(false)),
                    )
                    .frame(false),
                )
            })
            .on_hover_text("New Group")
            .clicked()
            {
                state.ui.entry_list.creating_group = !state.ui.entry_list.creating_group;
            }
            if with_core_button_style(ui, false, |ui| {
                ui.add_sized(
                    [24.0, 24.0],
                    Button::new(
                        RichText::new(egui_phosphor::regular::ARROWS_IN_SIMPLE)
                            .size(13.0)
                            .color(core_button_text_color(false)),
                    )
                    .frame(false),
                )
            })
            .on_hover_text("Collapse All")
            .clicked()
            {
                for group in &state.entries.groups {
                    state
                        .ui
                        .entry_list
                        .collapsed_group_ids
                        .insert(group.id.clone());
                }
            }
            ui.add_sized(
                [ui.available_width(), 24.0],
                egui::TextEdit::singleline(&mut state.ui.entry_list.search_query)
                    .hint_text("Search"),
            );
        });
    });

    if state.ui.entry_list.creating_group {
        ui.horizontal(|ui| {
            let response = ui.text_edit_singleline(&mut state.ui.entry_list.new_group_name);
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                actions.push(AppAction::CreateGroup {
                    name: state.ui.entry_list.new_group_name.clone(),
                });
            }
            if ui.button("Create").clicked() {
                actions.push(AppAction::CreateGroup {
                    name: state.ui.entry_list.new_group_name.clone(),
                });
            }
        });
    }

    ui.separator();

    let search = state.ui.entry_list.search_query.to_lowercase();
    let groups = state.entries.groups.clone();
    let all_group_choices = groups
        .iter()
        .map(|group| (group.id.clone(), group.name.clone()))
        .collect::<Vec<_>>();
    let ungrouped_entries = state
        .entries
        .records
        .iter()
        .filter(|entry| entry.group_id.is_empty())
        .filter(|entry| {
            search.is_empty()
                || entry.name.to_lowercase().contains(&search)
                || entry.id.to_string().contains(&search)
        })
        .map(|entry| (entry.id, entry.name.clone(), entry.group_id.clone()))
        .collect::<Vec<_>>();
    let detail_lines = state
        .entries
        .active_entry()
        .map(|entry| entry_details(&entry.structure, entry.source_path.as_deref()))
        .unwrap_or_default();

    if !detail_lines.is_empty() {
        let detail_height = 28.0 + 20.0 * detail_lines.len() as f32;
        egui::Panel::bottom("entry_list_details")
            .exact_size(detail_height)
            .frame(Frame::default().inner_margin(Margin::same(0)))
            .show_inside(ui, |ui| {
                ui.separator();
                ui.label(RichText::new("Details").strong());
                for line in &detail_lines {
                    ui.label(line);
                }
            });
    }

    ScrollArea::vertical()
        .max_height(ui.available_height().max(120.0))
        .show(ui, |ui| {
            for group in &groups {
                let group_id = group.id.clone();
                let entries = state
                    .entries
                    .records
                    .iter()
                    .filter(|entry| entry.group_id == group_id)
                    .filter(|entry| {
                        search.is_empty()
                            || entry.name.to_lowercase().contains(&search)
                            || entry.id.to_string().contains(&search)
                    })
                    .map(|entry| (entry.id, entry.name.clone(), entry.group_id.clone()))
                    .collect::<Vec<_>>();
                if entries.is_empty() && search.is_empty() {
                    let collapsed = state.ui.entry_list.collapsed_group_ids.contains(&group.id);
                    let folder_icon = if collapsed {
                        egui_phosphor::regular::FOLDER
                    } else {
                        egui_phosphor::regular::FOLDER_OPEN
                    };
                    let marker = if collapsed {
                        egui_phosphor::regular::CARET_RIGHT
                    } else {
                        egui_phosphor::regular::CARET_DOWN
                    };

                    let group_header_response = ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 0.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.label(
                                RichText::new(marker)
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(92, 100, 112)),
                            );
                            ui.label(
                                RichText::new(folder_icon)
                                    .size(14.0)
                                    .color(egui::Color32::from_rgb(92, 100, 112)),
                            );
                            ui.label(RichText::new(&group.name).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add_enabled(
                                        true,
                                        egui::Button::new(
                                            RichText::new(
                                                egui_phosphor::regular::PENCIL_SIMPLE_LINE,
                                            )
                                            .size(11.0),
                                        )
                                        .frame(false),
                                    )
                                    .clicked()
                                {
                                    state.ui.entry_list.renaming_group_id = Some(group.id.clone());
                                    state.ui.entry_list.rename_group_buffer = group.name.clone();
                                }
                                if ui
                                    .add_enabled(
                                        true,
                                        egui::Button::new(
                                            RichText::new(egui_phosphor::regular::TRASH).size(11.0),
                                        )
                                        .frame(false),
                                    )
                                    .clicked()
                                {
                                    actions.push(AppAction::DeleteGroup(group.id.clone()));
                                }
                            });
                            ui.response()
                        },
                    );

                    let header_interact = ui.interact(
                        group_header_response.response.rect,
                        Id::new(format!("group_header_{}", group.id)),
                        Sense::click(),
                    );
                    if header_interact.clicked()
                        && !state
                            .ui
                            .entry_list
                            .collapsed_group_ids
                            .insert(group.id.clone())
                    {
                        state.ui.entry_list.collapsed_group_ids.remove(&group.id);
                    }

                    if state.ui.entry_list.renaming_group_id.as_deref() == Some(group.id.as_str()) {
                        let response =
                            ui.text_edit_singleline(&mut state.ui.entry_list.rename_group_buffer);
                        if response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        {
                            actions.push(AppAction::RenameGroup {
                                group_id: group.id.clone(),
                                new_name: state.ui.entry_list.rename_group_buffer.clone(),
                            });
                        }
                    }

                    if !collapsed {
                        for (entry_id, name, entry_group_id) in &entries {
                            render_entry_list_item(
                                state,
                                ui,
                                actions,
                                *entry_id,
                                name,
                                entry_group_id,
                                &all_group_choices,
                            );
                        }
                    }
                    ui.add_space(2.0);
                    continue;
                }
                if entries.is_empty() {
                    continue;
                }

                let collapsed = state.ui.entry_list.collapsed_group_ids.contains(&group.id);
                let folder_icon = if collapsed {
                    egui_phosphor::regular::FOLDER
                } else {
                    egui_phosphor::regular::FOLDER_OPEN
                };
                let marker = if collapsed {
                    egui_phosphor::regular::CARET_RIGHT
                } else {
                    egui_phosphor::regular::CARET_DOWN
                };

                let group_header_response = ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 0.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.label(
                            RichText::new(marker)
                                .size(11.0)
                                .color(egui::Color32::from_rgb(92, 100, 112)),
                        );
                        ui.label(
                            RichText::new(folder_icon)
                                .size(14.0)
                                .color(egui::Color32::from_rgb(92, 100, 112)),
                        );
                        ui.label(RichText::new(&group.name).strong());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    true,
                                    egui::Button::new(
                                        RichText::new(egui_phosphor::regular::PENCIL_SIMPLE_LINE)
                                            .size(11.0),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                state.ui.entry_list.renaming_group_id = Some(group.id.clone());
                                state.ui.entry_list.rename_group_buffer = group.name.clone();
                            }
                            if ui
                                .add_enabled(
                                    true,
                                    egui::Button::new(
                                        RichText::new(egui_phosphor::regular::TRASH).size(11.0),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                actions.push(AppAction::DeleteGroup(group.id.clone()));
                            }
                        });
                        ui.response()
                    },
                );

                let header_interact = ui.interact(
                    group_header_response.response.rect,
                    Id::new(format!("group_header_{}", group.id)),
                    Sense::click(),
                );
                if header_interact.clicked()
                    && !state
                        .ui
                        .entry_list
                        .collapsed_group_ids
                        .insert(group.id.clone())
                {
                    state.ui.entry_list.collapsed_group_ids.remove(&group.id);
                }

                if state.ui.entry_list.renaming_group_id.as_deref() == Some(group.id.as_str()) {
                    let response =
                        ui.text_edit_singleline(&mut state.ui.entry_list.rename_group_buffer);
                    if response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        actions.push(AppAction::RenameGroup {
                            group_id: group.id.clone(),
                            new_name: state.ui.entry_list.rename_group_buffer.clone(),
                        });
                    }
                }

                if !collapsed {
                    for (entry_id, name, entry_group_id) in &entries {
                        render_entry_list_item(
                            state,
                            ui,
                            actions,
                            *entry_id,
                            name,
                            entry_group_id,
                            &all_group_choices,
                        );
                    }
                }
                ui.add_space(2.0);
            }

            if !ungrouped_entries.is_empty() && !groups.is_empty() {
                ui.separator();
            }

            for (entry_id, name, group_id) in &ungrouped_entries {
                render_entry_list_item(
                    state,
                    ui,
                    actions,
                    *entry_id,
                    name,
                    group_id,
                    &all_group_choices,
                );
            }
        });
}

fn render_entry_list_item(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
    entry_id: u64,
    name: &str,
    group_id: &str,
    all_group_choices: &[(String, String)],
) {
    let active = state.ui.entry_list.selected_entry_id == Some(entry_id);
    let renaming = state.ui.entry_list.renaming_entry_id == Some(entry_id);

    if renaming {
        let response = ui.add_sized(
            [ui.available_width(), 20.0],
            egui::TextEdit::singleline(&mut state.ui.entry_list.rename_buffer),
        );
        if response.lost_focus() {
            actions.push(AppAction::RenameEntry {
                entry_id,
                new_name: state.ui.entry_list.rename_buffer.clone(),
            });
            state.ui.entry_list.renaming_entry_id = None;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            state.ui.entry_list.renaming_entry_id = None;
        }
    } else {
        let full_width = ui.available_width();
        let (rect, response) =
            ui.allocate_at_least(egui::vec2(full_width, 20.0), Sense::click_and_drag());

        let hovered = response.hovered();
        let bg_fill = if active {
            egui::Color32::from_rgba_unmultiplied(54, 97, 164, 40)
        } else if hovered {
            egui::Color32::from_rgba_unmultiplied(70, 78, 88, 18)
        } else {
            egui::Color32::TRANSPARENT
        };
        let text_color = if active {
            egui::Color32::from_rgb(32, 37, 43)
        } else {
            egui::Color32::from_rgb(70, 78, 88)
        };

        ui.painter().rect_filled(rect, 0.0, bg_fill);
        let text_rect = rect.shrink2(egui::vec2(6.0, 0.0));
        ui.painter().text(
            text_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            name,
            egui::FontId::proportional(13.0),
            text_color,
        );

        if response.clicked() {
            if state.ui.entry_list.selected_entry_id == Some(entry_id) {
                state.ui.entry_list.selected_entry_id = None;
            } else {
                state.ui.entry_list.selected_entry_id = Some(entry_id);
                actions.push(AppAction::ActivateEntry(entry_id));
            }
        }
        if response.double_clicked() {
            state.ui.entry_list.renaming_entry_id = Some(entry_id);
            state.ui.entry_list.rename_buffer = name.to_string();
        }
        response.context_menu(|ui| {
            if ui.button("Rename").clicked() {
                state.ui.entry_list.renaming_entry_id = Some(entry_id);
                state.ui.entry_list.rename_buffer = name.to_string();
                ui.close();
            }
            if !group_id.is_empty() && ui.button("Remove from group").clicked() {
                actions.push(AppAction::MoveEntryToGroup {
                    entry_id,
                    group_id: String::new(),
                });
                ui.close();
            }
            if !all_group_choices.is_empty() {
                ui.separator();
                ui.label("Move to group");
                for (target_group_id, target_group_name) in all_group_choices {
                    if target_group_id == group_id {
                        continue;
                    }
                    if ui.button(target_group_name).clicked() {
                        actions.push(AppAction::MoveEntryToGroup {
                            entry_id,
                            group_id: target_group_id.clone(),
                        });
                        ui.close();
                    }
                }
            }
            ui.separator();
            if ui.button("Delete Entry").clicked() {
                actions.push(AppAction::DeleteEntry(entry_id));
                ui.close();
            }
        });
    }
    ui.add_space(2.0);
}

/// Maps a controller's fine-grained `theme` into the broader, user-facing
/// category shown as a collapsible group in the task list.
fn task_category(theme: &str) -> &'static str {
    match theme {
        "Reticular Design" | "2D Materials" => "Structure Builder",
        "Geometry" => "Optimization",
        "Molecular Dynamics" => "Molecular Dynamics",
        // "Structure Editing" and "Crystal Editing" both fold into editing.
        _ => "Structure Editing",
    }
}

/// Display order of the task categories.
const TASK_CATEGORIES: &[&str] = &[
    "Structure Builder",
    "Structure Editing",
    "Optimization",
    "Molecular Dynamics",
];

fn render_tasks_view(state: &mut AppState, ui: &mut egui::Ui, actions: &mut Vec<AppAction>) {
    ui.horizontal(|ui| {
        ui.label("Search");
        ui.text_edit_singleline(&mut state.tasks.task_list.search_query);
    });
    ui.separator();
    ui.add_space(4.0);

    let search = state.tasks.task_list.search_query.to_lowercase();
    ScrollArea::vertical().show(ui, |ui| {
        for category in TASK_CATEGORIES {
            let controllers = task_controllers()
                .iter()
                .copied()
                .filter(|controller| task_category(controller.theme) == *category)
                .filter(|controller| {
                    search.is_empty()
                        || controller.title.to_lowercase().contains(&search)
                        || controller.short_title.to_lowercase().contains(&search)
                        || controller.theme.to_lowercase().contains(&search)
                        || controller.method.to_lowercase().contains(&search)
                        || controller.application.to_lowercase().contains(&search)
                })
                .collect::<Vec<_>>();
            if controllers.is_empty() {
                continue;
            }

            // A search keeps every matching group expanded so results stay visible.
            let collapsed =
                search.is_empty() && state.tasks.task_list.collapsed_themes.contains(*category);
            let marker = if collapsed {
                egui_phosphor::regular::CARET_RIGHT
            } else {
                egui_phosphor::regular::CARET_DOWN
            };

            let header = ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 0.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.label(
                        RichText::new(marker)
                            .size(11.0)
                            .color(egui::Color32::from_rgb(92, 100, 112)),
                    );
                    ui.label(RichText::new(*category).strong());
                    ui.response()
                },
            );
            let header_interact = ui.interact(
                header.response.rect,
                Id::new(format!("task_category_{category}")),
                Sense::click(),
            );
            if header_interact.clicked()
                && !state
                    .tasks
                    .task_list
                    .collapsed_themes
                    .insert((*category).to_string())
            {
                state.tasks.task_list.collapsed_themes.remove(*category);
            }

            if !collapsed {
                ui.add_space(2.0);
                for controller in controllers {
                    let response = Frame::default()
                        .fill(egui::Color32::from_rgb(248, 250, 252))
                        .stroke(Stroke::NONE)
                        .inner_margin(Margin::symmetric(10, 7))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(controller.short_title));
                        })
                        .response
                        .interact(Sense::click())
                        .on_hover_text(controller.description);
                    if response.hovered() {
                        ui.painter().rect_filled(
                            response.rect,
                            6.0,
                            egui::Color32::from_rgba_unmultiplied(66, 113, 181, 18),
                        );
                        ui.painter().rect_stroke(
                            response.rect,
                            6.0,
                            Stroke::new(
                                1.0,
                                egui::Color32::from_rgba_unmultiplied(66, 113, 181, 72),
                            ),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if response.clicked() {
                        actions.push(AppAction::CreateTask(controller.id));
                    }
                    ui.add_space(4.0);
                }
            }
            ui.add_space(8.0);
        }
    });
}

/// Owned snapshot of one engine capability, decoupled from the registry
/// borrow so we can freely mutate the per-engine drafts while rendering.
struct EngineRowView {
    id: EngineId,
    name: &'static str,
    description: &'static str,
    built_in: bool,
    available: bool,
    version: Option<String>,
    launch: Option<EngineLaunch>,
}

/// Render a `SystemTime` as a coarse relative age ("just now", "5m ago").
/// Avoids a date-formatting dependency; granularity is fine for "how stale is
/// this detection".
fn humanize_since(time: std::time::SystemTime) -> String {
    let Ok(elapsed) = time.elapsed() else {
        return "moments ago".to_string();
    };
    let secs = elapsed.as_secs();
    if secs < 5 {
        "just now".to_string()
    } else if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

fn render_engine_settings(state: &mut AppState, ui: &mut egui::Ui, actions: &mut Vec<AppAction>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Engines").strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .button("Re-detect")
                .on_hover_text("Run each engine's --version (can be slow for WSL)")
                .clicked()
            {
                actions.push(AppAction::DetectEngineVersions);
            }
        });
    });

    // Availability is resolved lazily and cheaply (no subprocess). Version
    // strings are NOT probed here — that spawns `--version` per engine and a
    // WSL launch cold-starts the VM, which made first open slow. Versions are
    // detected only on explicit "Re-detect" / "Apply & Detect".
    let Some(registry) = state.ui.settings.engine_registry.as_ref() else {
        actions.push(AppAction::RefreshEngineRegistry);
        return;
    };

    let versions_caption = match state.ui.settings.engine_versions_checked_at {
        Some(checked_at) => format!("Versions last checked {}", humanize_since(checked_at)),
        None => "Versions not checked yet — click Re-detect".to_string(),
    };
    ui.label(
        RichText::new(versions_caption)
            .small()
            .color(egui::Color32::GRAY),
    );

    let rows: Vec<EngineRowView> = registry
        .capabilities()
        .iter()
        .map(|cap| EngineRowView {
            id: cap.id,
            name: cap.name,
            description: cap.description,
            built_in: cap.built_in,
            available: cap.available(),
            version: cap.version.clone(),
            launch: cap.launch.clone(),
        })
        .collect();

    for row in rows {
        let badge = if row.available {
            RichText::new(format!(
                "{}  {}",
                egui_phosphor::regular::CHECK_CIRCLE,
                row.name
            ))
            .color(egui::Color32::from_rgb(40, 140, 70))
        } else {
            RichText::new(format!(
                "{}  {}",
                egui_phosphor::regular::X_CIRCLE,
                row.name
            ))
            .color(egui::Color32::from_rgb(170, 70, 70))
        };
        ui.label(badge.strong());
        if let Some(version) = &row.version {
            ui.label(RichText::new(format!("version {version}")).small());
        }
        ui.label(
            RichText::new(row.description)
                .small()
                .color(egui::Color32::GRAY),
        );

        if row.built_in {
            ui.add_space(6.0);
            continue;
        }

        // Seed the editable draft once, preferring an explicit override, then
        // the auto-detected launch, then empty.
        let key = row.id.as_str().to_string();
        if !state.ui.settings.engine_drafts.contains_key(&key) {
            let seed = state
                .config
                .engine_overrides
                .get(&key)
                .map(EngineDraft::from_launch)
                .or_else(|| row.launch.as_ref().map(EngineDraft::from_launch))
                .unwrap_or_default();
            state.ui.settings.engine_drafts.insert(key.clone(), seed);
        }
        let draft = state
            .ui
            .settings
            .engine_drafts
            .get_mut(&key)
            .expect("draft seeded above");

        ui.horizontal(|ui| {
            ui.label("Command prefix:");
            ui.text_edit_singleline(&mut draft.command_prefix);
        });
        ui.label(
            RichText::new("e.g. `wsl.exe -e` to run inside WSL; leave blank for a native install")
                .small()
                .color(egui::Color32::GRAY),
        );
        ui.horizontal(|ui| {
            ui.label("Program:");
            ui.text_edit_singleline(&mut draft.program);
            if ui.button("Browse").clicked() {
                actions.push(AppAction::BrowseEngineProgram(row.id));
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Apply & Detect").clicked() {
                actions.push(AppAction::ApplyEngineOverride(row.id));
            }
            if ui.button("Clear").clicked() {
                actions.push(AppAction::ClearEngineOverride(row.id));
            }
        });
        ui.add_space(8.0);
    }
}

/// A collapsing settings section that is filtered by the search query and
/// forced open whenever a search is active so matches stay visible.
fn settings_section(
    ui: &mut egui::Ui,
    title: &str,
    search: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    if !search.is_empty() && !title.to_lowercase().contains(search) {
        return;
    }
    let mut header = egui::CollapsingHeader::new(RichText::new(title).strong()).default_open(true);
    if !search.is_empty() {
        header = header.open(Some(true));
    }
    header.show(ui, add_contents);
}

fn viewport_visual_settings_view(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
) {
    ui.horizontal(|ui| {
        ui.label("Search");
        ui.text_edit_singleline(&mut state.ui.settings.search_query);
    });
    ui.separator();

    let search = state.ui.settings.search_query.to_lowercase();

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            settings_section(ui, "Engines", &search, |ui| {
                render_engine_settings(state, ui, actions);
            });

            settings_section(ui, "Workbench visibility", &search, |ui| {
                ui.checkbox(
                    &mut state.ui.layout.show_primary_sidebar,
                    "Show primary side bar",
                );
                ui.checkbox(
                    &mut state.ui.layout.show_secondary_sidebar,
                    "Show secondary side bar",
                );
                ui.checkbox(&mut state.ui.layout.show_panel, "Show panel");
                ui.checkbox(&mut state.ui.viewport.show_atom_labels, "Show atom labels");
            });

            settings_section(ui, "Representation", &search, |ui| {
                let selection_len = state.ui.selection.len();
                ui.label(if selection_len == 0 {
                    "Click a style to apply it to all atoms:".to_string()
                } else {
                    format!("Click a style to apply it to {selection_len} selected atom(s):")
                });
                ui.horizontal_wrapped(|ui| {
                    for style in AtomStyle::all() {
                        if ui.button(style.label()).clicked() {
                            actions.push(AppAction::SetSelectionStyle(*style));
                        }
                    }
                });
                if ui.button("Reset to default").clicked() {
                    actions.push(AppAction::ResetSelectionStyle);
                }
                ui.label(
                    RichText::new(
                        "Tip: use Selection ▸ Select by type to pick all protein / solvent / … atoms, then apply a style.",
                    )
                    .small()
                    .color(egui::Color32::GRAY),
                );

                ui.separator();
                ui.label(
                    RichText::new("Project default style by category")
                        .small()
                        .color(egui::Color32::GRAY),
                );
                egui::Grid::new("project_category_styles")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for category in crate::domain::AtomCategory::all() {
                            ui.label(category.label());
                            let current = state.ui.viewport.category_style(*category);
                            egui::ComboBox::from_id_salt(("category_style", category.token()))
                                .selected_text(current.label())
                                .show_ui(ui, |ui| {
                                    for style in AtomStyle::all() {
                                        if ui
                                            .selectable_label(*style == current, style.label())
                                            .clicked()
                                        {
                                            actions.push(AppAction::SetCategoryStyle(
                                                *category, *style,
                                            ));
                                        }
                                    }
                                });
                            ui.end_row();
                        }
                    });
            });

            settings_section(ui, "Rendering", &search, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Background");
                    ui.color_edit_button_srgba(&mut state.ui.viewport.background_color);
                });
                ui.checkbox(&mut state.ui.viewport.show_cell, "Show unit cell");
                // Solvent visibility is just its category style; this toggle is a
                // convenience shortcut for the common "hide the water" action.
                let mut hide_solvent = state
                    .ui
                    .viewport
                    .category_style(crate::domain::AtomCategory::Solvent)
                    == AtomStyle::Hidden;
                if ui.checkbox(&mut hide_solvent, "Hide solvent").changed() {
                    let style = if hide_solvent {
                        AtomStyle::Hidden
                    } else {
                        crate::frontend::viewport::software_default_style(
                            crate::domain::AtomCategory::Solvent,
                        )
                    };
                    actions.push(AppAction::SetCategoryStyle(
                        crate::domain::AtomCategory::Solvent,
                        style,
                    ));
                }
                egui::ComboBox::from_label("Light")
                    .selected_text(state.ui.viewport.lighting.preset.label())
                    .show_ui(ui, |ui| {
                        for preset in LightPreset::all() {
                            ui.selectable_value(
                                &mut state.ui.viewport.lighting.preset,
                                *preset,
                                preset.label(),
                            );
                        }
                    });
                ui.checkbox(&mut state.ui.viewport.lighting.silhouettes, "Silhouettes");
                ui.add(
                    egui::Slider::new(&mut state.ui.viewport.lighting.silhouette_width, 0.0..=6.0)
                        .text("Silhouette width"),
                );
            });

            settings_section(ui, "Cartoon", &search, |ui| {
                cartoon_section_controls(ui, "Helix", &mut state.ui.viewport.cartoon.helix);
                cartoon_section_controls(ui, "Sheet", &mut state.ui.viewport.cartoon.sheet);
                cartoon_section_controls(ui, "Coil", &mut state.ui.viewport.cartoon.coil);
                ui.add(
                    egui::Slider::new(&mut state.ui.viewport.cartoon.smoothing, 1..=32)
                        .text("Smoothing"),
                );
                ui.add(
                    egui::Slider::new(&mut state.ui.viewport.cartoon.profile_segments, 6..=48)
                        .text("Profile"),
                );
            });

            settings_section(ui, "Surface and Ions", &search, |ui| {
                egui::ComboBox::from_label("Surface style")
                    .selected_text(state.ui.viewport.surface.style.label())
                    .show_ui(ui, |ui| {
                        for style in SurfaceStyle::all() {
                            ui.selectable_value(
                                &mut state.ui.viewport.surface.style,
                                *style,
                                style.label(),
                            );
                        }
                    });
                ui.add(
                    egui::Slider::new(&mut state.ui.viewport.surface.transparency, 0.0..=1.0)
                        .text("Surface transparency"),
                );
                let mut show_ions = state.ui.viewport.ions.show_within.is_some();
                if ui.checkbox(&mut show_ions, "Show nearby ions").changed() {
                    state.ui.viewport.ions.show_within = show_ions.then_some(3.5);
                }
                if let Some(distance) = &mut state.ui.viewport.ions.show_within {
                    ui.add(egui::Slider::new(distance, 0.0..=10.0).text("Ion distance"));
                }
                ui.horizontal(|ui| {
                    ui.label("Ion color");
                    let color = state
                        .ui
                        .viewport
                        .ions
                        .color
                        .get_or_insert(egui::Color32::from_rgb(255, 226, 79));
                    ui.color_edit_button_srgba(color);
                });
            });

            let chains = state
                .structure()
                .biopolymer
                .as_ref()
                .map(|biopolymer| {
                    biopolymer
                        .chains
                        .iter()
                        .map(|chain| chain.id)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            settings_section(ui, "Chains", &search, |ui| {
                if chains.is_empty() {
                    ui.label("No biomolecular chains in the active entry.");
                }
                for chain_id in chains {
                    ui.horizontal(|ui| {
                        let mut surface = state.ui.viewport.surface.chains.contains(&chain_id);
                        if ui
                            .checkbox(&mut surface, format!("Surface {chain_id}"))
                            .changed()
                        {
                            if surface {
                                state.ui.viewport.surface.chains.insert(chain_id);
                            } else {
                                state.ui.viewport.surface.chains.remove(&chain_id);
                            }
                        }
                        let color = state
                            .ui
                            .viewport
                            .chain_colors
                            .entry(chain_id)
                            .or_insert(egui::Color32::from_rgb(120, 150, 210));
                        ui.label(format!("Color {chain_id}"));
                        ui.color_edit_button_srgba(color);
                    });
                }
            });

            settings_section(ui, "Layout", &search, |ui| {
                if ui.button("Reset Workbench Layout").clicked() {
                    state.reset_layout_keep_view();
                }
            });

            settings_section(ui, "Statistics", &search, |ui| {
                ui.label(format!("Entries: {}", state.entries.records.len()));
                ui.label(format!("Open tabs: {}", state.entries.tabs.len()));
                ui.label(format!("Tasks: {}", state.tasks.tasks.len()));
            });
        });
}

fn cartoon_section_controls(ui: &mut egui::Ui, label: &str, section: &mut CartoonSectionStyle) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(
            egui::DragValue::new(&mut section.width)
                .range(0.05..=10.0)
                .speed(0.05),
        );
        ui.add(
            egui::DragValue::new(&mut section.thickness)
                .range(0.05..=10.0)
                .speed(0.05),
        );
    });
}

fn render_structure_editor_window(
    state: &mut AppState,
    actions: &mut Vec<AppAction>,
    ctx: &egui::Context,
) {
    let Some(editor) = &mut state.ui.editor else {
        return;
    };

    let mut apply = false;
    let mut cancel = false;
    let mut preview_update = None;
    egui::Window::new("Edit Structure")
        .default_size([760.0, 420.0])
        .max_height(520.0)
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(if editor.draft.cell.is_some() {
                        "Periodic structure: atom coordinates are fractional."
                    } else {
                        "Non-periodic structure: atom coordinates are Cartesian."
                    });
                    ui.separator();
                    if editor.ui(ui) {
                        preview_update = Some(editor.draft.clone());
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .button(format!("{}  Apply", egui_phosphor::regular::CHECK))
                    .clicked()
                {
                    apply = true;
                }
                if ui
                    .button(format!("{}  Cancel", egui_phosphor::regular::X))
                    .clicked()
                {
                    cancel = true;
                }
            });
        });

    if let Some(draft) = preview_update {
        *state.structure_mut() = draft;
        state.mark_structure_changed();
        state.set_message("Editing preview updated".to_string());
    }

    if apply {
        actions.push(AppAction::ApplyStructureEdits);
    } else if cancel {
        actions.push(AppAction::CancelStructureEdits);
    }
}

fn render_pdb_fetch_window(
    state: &mut AppState,
    actions: &mut Vec<AppAction>,
    ctx: &egui::Context,
) {
    let Some(id) = &mut state.ui.pending_pdb_fetch else {
        return;
    };

    let mut fetch = false;
    let mut cancel = false;
    let mut open = true;
    egui::Window::new("Fetch from PDB ID")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("Enter a 4-character PDB id.");
            ui.label("The structure is downloaded from rcsb.org into the structures/ folder.");
            ui.add_space(6.0);
            let response = ui.add(egui::TextEdit::singleline(id).desired_width(120.0));
            // Focus the field when the dialog first appears without stealing
            // focus on later frames.
            if ui.memory(|memory| memory.focused().is_none()) {
                response.request_focus();
            }
            let submitted =
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            let can_fetch = !id.trim().is_empty();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        can_fetch,
                        Button::new(format!(
                            "{}  Fetch",
                            egui_phosphor::regular::DOWNLOAD_SIMPLE
                        )),
                    )
                    .clicked()
                {
                    fetch = true;
                }
                if ui
                    .button(format!("{}  Cancel", egui_phosphor::regular::X))
                    .clicked()
                {
                    cancel = true;
                }
            });
            if submitted && can_fetch {
                fetch = true;
            }
        });

    if fetch {
        actions.push(AppAction::FetchPdb);
    } else if cancel || !open {
        actions.push(AppAction::CancelPdbFetch);
    }
}

fn window_control_button(
    ui: &mut Ui,
    icon: &'static str,
    hover_fill: egui::Color32,
) -> egui::Response {
    let is_close = icon == egui_phosphor::regular::X;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(36.0, 24.0), Sense::click());
    let fill = if response.hovered() {
        hover_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    let text_color = if is_close && response.hovered() {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgb(70, 78, 88)
    };

    ui.painter()
        .rect_filled(rect, f32::from(CORE_BUTTON_CORNER_RADIUS), fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(14.0),
        text_color,
    );
    response
}

fn with_core_button_style<R>(
    ui: &mut Ui,
    selected: bool,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    ui.scope(|ui| {
        configure_core_button_visuals(ui, selected);
        add_contents(ui)
    })
    .inner
}

fn configure_core_button_visuals(ui: &mut Ui, selected: bool) {
    let inactive_fill = core_button_fill(selected, false);
    let hovered_fill = core_button_fill(selected, true);
    let selected_fill = core_button_fill(true, false);
    let selected_hover_fill = core_button_fill(true, true);
    let inactive_text = core_button_text_color(selected);
    let selected_text = core_button_text_color(true);
    let visuals = &mut ui.style_mut().visuals.widgets;

    visuals.inactive.weak_bg_fill = inactive_fill;
    visuals.inactive.bg_fill = inactive_fill;
    visuals.inactive.bg_stroke = Stroke::NONE;
    visuals.inactive.fg_stroke.color = inactive_text;

    visuals.hovered.weak_bg_fill = hovered_fill;
    visuals.hovered.bg_fill = hovered_fill;
    visuals.hovered.bg_stroke = Stroke::NONE;
    visuals.hovered.fg_stroke.color = selected_text;

    visuals.active.weak_bg_fill = selected_hover_fill;
    visuals.active.bg_fill = selected_hover_fill;
    visuals.active.bg_stroke = Stroke::NONE;
    visuals.active.fg_stroke.color = selected_text;

    visuals.open.weak_bg_fill = selected_fill;
    visuals.open.bg_fill = selected_fill;
    visuals.open.bg_stroke = Stroke::NONE;
    visuals.open.fg_stroke.color = selected_text;
}

fn core_button_fill(selected: bool, hovered: bool) -> egui::Color32 {
    match (selected, hovered) {
        (false, false) => egui::Color32::TRANSPARENT,
        (false, true) => egui::Color32::from_rgba_unmultiplied(70, 78, 88, CORE_BUTTON_HOVER_ALPHA),
        (true, false) => {
            egui::Color32::from_rgba_unmultiplied(70, 78, 88, CORE_BUTTON_SELECTED_ALPHA)
        }
        (true, true) => {
            egui::Color32::from_rgba_unmultiplied(70, 78, 88, CORE_BUTTON_SELECTED_HOVER_ALPHA)
        }
    }
}

fn core_button_text_color(selected: bool) -> egui::Color32 {
    if selected {
        egui::Color32::from_rgb(32, 37, 43)
    } else {
        egui::Color32::from_rgb(70, 78, 88)
    }
}
