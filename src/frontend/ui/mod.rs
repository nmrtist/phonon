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
        state::{AppState, AtomStyle, EngineDraft, PrimaryView, SelectionItem},
    },
};

mod bottom_panel;
mod secondary_sidebar;
mod workspace;

use bottom_panel::render_status_bar;
use secondary_sidebar::render_secondary_sidebar;
use workspace::render_workspace;
/// Corner radius of the *borderless* main window, in logical points.
///
/// Window-chrome model (the reason for the `cfg`s throughout this file):
/// Windows/Linux run a borderless, transparent window and draw their own chrome
/// — resize handles, rounded title/status-bar corners, and a hairline border.
/// macOS uses the native window frame, which owns resize, the squircle corners,
/// the border, and the shadow, so the app-drawn chrome is skipped there.
#[cfg(not(target_os = "macos"))]
const WINDOW_CORNER_RADIUS: u8 = 10;

pub fn show_workbench(state: &mut AppState, ui: &mut egui::Ui, actions: &mut Vec<AppAction>) {
    let ctx = ui.ctx().clone();
    let pal = crate::frontend::theme::palette(ui);

    render_window_resize_handles(&ctx);

    #[cfg(target_os = "macos")]
    let (top_corners, bottom_corners) = (egui::CornerRadius::ZERO, egui::CornerRadius::ZERO);
    #[cfg(not(target_os = "macos"))]
    let (top_corners, bottom_corners) = (
        egui::CornerRadius {
            nw: WINDOW_CORNER_RADIUS,
            ne: WINDOW_CORNER_RADIUS,
            sw: 0,
            se: 0,
        },
        egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: WINDOW_CORNER_RADIUS,
            se: WINDOW_CORNER_RADIUS,
        },
    );

    egui::Panel::top("title_bar")
        .exact_size(32.0)
        .frame(
            Frame::default()
                .fill(pal.title_bar)
                .corner_radius(top_corners)
                .inner_margin(Margin::symmetric(8, 3)),
        )
        .show_inside(ui, |ui| render_title_bar(state, ui, actions));

    egui::Panel::bottom("status_bar")
        .exact_size(24.0)
        .frame(
            Frame::default()
                .fill(pal.status_bar)
                .corner_radius(bottom_corners)
                .inner_margin(Margin::symmetric(10, 3)),
        )
        .show_inside(ui, |ui| render_status_bar(state, ui));

    egui::Panel::left("activity_bar")
        .exact_size(52.0)
        .resizable(false)
        .frame(
            Frame::default()
                .fill(pal.activity_bar)
                .inner_margin(Margin::symmetric(6, 10)),
        )
        .show_inside(ui, |ui| render_activity_bar(state, ui));

    // Sidebars are fixed-width panels driven by our own proximity-revealed
    // resize dividers (wired after the central panel; see `render_resize_divider`).
    // egui's native resize is off (`resizable(false)`) so it never paints the
    // harsh full-height hover line, and `show_separator_line(false)` hands the
    // at-rest hairline to our overlay too. `exact_size` also dodges egui's
    // resizable-panel growth bug (the same reason the bottom panel uses it).
    // Transient per-frame panel widths used to place the resize divider, the
    // central column, and the bottom panel flush with each sidebar's edge. The
    // sidebar content is pinned to the exact width by `render_pinned` (see its
    // doc comment) so a wide widget can't push the panel's content rect — and thus
    // egui's placement of the central column and the bottom panel nested in it —
    // out past the requested edge. With overflow pinned away the rendered width is
    // always the requested `width`, so we key everything off `width` directly.
    // Seed these with the stored width so that if a panel is toggled on mid-frame
    // (e.g. the bottom panel's "Open Tasks" button, which runs after this block) the
    // divider falls back to a sane position rather than the activity-bar edge.
    let mut primary_rendered_w = state.ui.layout.primary_sidebar_width;
    let mut secondary_rendered_w = state.ui.layout.secondary_sidebar_width;

    if state.ui.layout.show_primary_sidebar {
        let max_w = sidebar_max_width(ctx.viewport_rect().width());
        let width = state
            .ui
            .layout
            .primary_sidebar_width
            .clamp(SIDEBAR_MIN_WIDTH_PRIMARY, max_w);
        state.ui.layout.primary_sidebar_width = width;
        egui::Panel::left("primary_sidebar")
            .resizable(false)
            .exact_size(width)
            .show_separator_line(false)
            .frame(
                Frame::default()
                    .fill(pal.sidebar)
                    .inner_margin(Margin::symmetric(10, 10)),
            )
            .show_inside(ui, |ui| {
                render_pinned(ui, |ui| render_primary_sidebar(state, ui, actions));
            });
        primary_rendered_w = width;
    }

    if state.ui.layout.show_secondary_sidebar {
        let max_w = sidebar_max_width(ctx.viewport_rect().width());
        let width = state
            .ui
            .layout
            .secondary_sidebar_width
            .clamp(SIDEBAR_MIN_WIDTH_SECONDARY, max_w);
        state.ui.layout.secondary_sidebar_width = width;
        egui::Panel::right("secondary_sidebar")
            .resizable(false)
            .exact_size(width)
            .show_separator_line(false)
            .frame(
                Frame::default()
                    .fill(pal.sidebar)
                    .inner_margin(Margin::symmetric(10, 10)),
            )
            .show_inside(ui, |ui| {
                render_pinned(ui, |ui| render_secondary_sidebar(state, ui, actions));
            });
        secondary_rendered_w = width;
    }

    egui::CentralPanel::default()
        .frame(
            Frame::default()
                .fill(pal.central)
                .inner_margin(Margin::same(0)),
        )
        .show_inside(ui, |ui| render_workspace(state, ui, actions));

    // The bottom panel is fixed-size (see `render_workspace`) to avoid egui's
    // resizable-panel growth bug, so it gets a custom resize handle — a subtle
    // centered pill on hover that drives `panel_height`. Its horizontal divider
    // shares no edge with a scroll bar, so there's no grab conflict. (Sidebars
    // use egui's native resize above.)
    if state.ui.layout.show_panel {
        let viewport_rect = ctx.viewport_rect();
        let content_bottom = viewport_rect.bottom() - 24.0; // above the status bar
        // Use the panels' *rendered* widths (see the note above the sidebar panels)
        // so the bottom-panel divider stays flush with the central column.
        let workspace_left = viewport_rect.left()
            + 52.0
            + if state.ui.layout.show_primary_sidebar {
                primary_rendered_w
            } else {
                0.0
            };
        let workspace_right = viewport_rect.right()
            - if state.ui.layout.show_secondary_sidebar {
                secondary_rendered_w
            } else {
                0.0
            };
        let y = content_bottom - state.ui.layout.panel_height;
        let max_panel_height = (viewport_rect.height() * 0.6).max(160.0);
        render_resize_divider(
            &ctx,
            "bottom_panel_resize",
            DividerKind::Horizontal,
            // Inset the grab strip past the sidebar dividers (which now run full
            // height at workspace_left / workspace_right) so the bottom corners
            // aren't an ambiguous two-axis drag target.
            Rect::from_min_max(
                egui::pos2(workspace_left + DIVIDER_GRAB_HALF_WIDTH, y - 4.0),
                egui::pos2(workspace_right - DIVIDER_GRAB_HALF_WIDTH, y + 4.0),
            ),
            y,
            &mut state.ui.layout.panel_height,
            -1.0,
            120.0,
            max_panel_height,
            180.0,
            &pal,
        );
    }

    // Sidebar resize dividers — proximity-revealed, matching the bottom panel.
    // Drawn over the central panel so the soft bar floats on the shared edge;
    // the panels themselves are fixed-width (see above).
    {
        let vp = ctx.viewport_rect();
        let content_top = vp.top() + 32.0; // below the title bar
        let content_bottom = vp.bottom() - 24.0; // above the status bar
        // The sidebar spans the full content height — the bottom panel is nested
        // inside the central column (to the right of the sidebar), not under it —
        // so its resize divider runs the whole way down to the status bar rather
        // than stopping at the bottom panel's top edge.
        let divider_bottom = content_bottom;
        let max_w = sidebar_max_width(vp.width());
        if state.ui.layout.show_primary_sidebar {
            // Draw the divider at the panel's rendered edge (see the note above the
            // sidebar panels); drag still adjusts the user-chosen `primary_sidebar_width`.
            let line_x = vp.left() + 52.0 + primary_rendered_w;
            render_resize_divider(
                &ctx,
                "primary_sidebar_resize",
                DividerKind::Vertical,
                Rect::from_min_max(
                    egui::pos2(line_x - DIVIDER_GRAB_HALF_WIDTH, content_top),
                    egui::pos2(line_x + DIVIDER_GRAB_HALF_WIDTH, divider_bottom),
                ),
                line_x,
                &mut state.ui.layout.primary_sidebar_width,
                1.0,
                SIDEBAR_MIN_WIDTH_PRIMARY,
                max_w,
                SIDEBAR_DEFAULT_WIDTH_PRIMARY,
                &pal,
            );
        }
        if state.ui.layout.show_secondary_sidebar {
            let line_x = vp.right() - secondary_rendered_w;
            render_resize_divider(
                &ctx,
                "secondary_sidebar_resize",
                DividerKind::Vertical,
                Rect::from_min_max(
                    egui::pos2(line_x - DIVIDER_GRAB_HALF_WIDTH, content_top),
                    egui::pos2(line_x + DIVIDER_GRAB_HALF_WIDTH, divider_bottom),
                ),
                line_x,
                &mut state.ui.layout.secondary_sidebar_width,
                -1.0,
                SIDEBAR_MIN_WIDTH_SECONDARY,
                max_w,
                SIDEBAR_DEFAULT_WIDTH_SECONDARY,
                &pal,
            );
        }
    }

    // Hairline border hugging the rounded window. Painted last so it sits atop
    // the panel fills; `StrokeKind::Inside` keeps the full 1px within the window
    // so it isn't clipped at the physical edge.
    #[cfg(not(target_os = "macos"))]
    ui.painter().rect_stroke(
        ctx.viewport_rect(),
        egui::CornerRadius::same(WINDOW_CORNER_RADIUS),
        ctx.global_style().visuals.window_stroke,
        egui::StrokeKind::Inside,
    );

    render_structure_editor_window(state, actions, &ctx);
    render_pdb_fetch_window(state, actions, &ctx);
}

const WINDOW_RESIZE_HANDLE_THICKNESS: f32 = 6.0;
const WINDOW_RESIZE_CORNER_SIZE: f32 = 18.0;

fn render_window_resize_handles(ctx: &egui::Context) {
    // Runtime guard rather than a cfg'd call site: this keeps the helper types
    // below referenced on macOS (which has no app-drawn handles) so they don't
    // trip dead-code warnings.
    if cfg!(target_os = "macos") {
        return;
    }

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

/// Whether a resize divider runs vertically (between side-by-side panels, drags
/// horizontally) or horizontally (between stacked panels, drags vertically).
#[derive(Clone, Copy, PartialEq)]
enum DividerKind {
    Vertical,
    Horizontal,
}

/// Proximity-reveal tuning for the resize dividers (sidebars + bottom panel).
/// How near the pointer must come (in points) before the bar begins to fade in.
const DIVIDER_PROXIMITY_RADIUS: f32 = 24.0;
/// Half-width of the slim interactive grab strip centered on the divider line.
const DIVIDER_GRAB_HALF_WIDTH: f32 = 4.0;
/// Fade in/out duration for the indicator bar.
const DIVIDER_FADE_SECONDS: f32 = 0.15;
/// Alpha of the always-visible at-rest separator hairline (subtle light gray, so
/// the sidebar keeps a clear edge and doesn't read as detached from the content).
const DIVIDER_REST_ALPHA: u8 = 180;
/// Alpha of the bar when revealed on approach.
const DIVIDER_ACTIVE_ALPHA: u8 = 220;
/// Width of the fully-revealed bar (it thins to 1 px at rest).
const DIVIDER_BAR_WIDTH: f32 = 2.0;

const SIDEBAR_MIN_WIDTH_PRIMARY: f32 = 220.0;
const SIDEBAR_MIN_WIDTH_SECONDARY: f32 = 240.0;
const SIDEBAR_DEFAULT_WIDTH_PRIMARY: f32 = 240.0;
const SIDEBAR_DEFAULT_WIDTH_SECONDARY: f32 = 320.0;

/// Largest a sidebar may be dragged: half the window, capped at 480 px, but never
/// below the *larger* of the two sidebar minimums. The floor must not drop below
/// `SIDEBAR_MIN_WIDTH_SECONDARY`, or a narrow window makes `max_w < min` and the
/// `width.clamp(min, max_w)` for the secondary sidebar panics (std clamp requires
/// `min <= max`).
fn sidebar_max_width(viewport_width: f32) -> f32 {
    (viewport_width * 0.5).clamp(SIDEBAR_MIN_WIDTH_SECONDARY, 480.0)
}

/// Interactive resize handle for a panel divider, Claude-style: a faint hairline
/// at rest that fades into a soft, theme-inverting indicator bar as the pointer
/// nears it (within `DIVIDER_PROXIMITY_RADIUS`) — hinting that the edge is
/// draggable without the harsh full-height line egui's native resize paints.
///
/// `hit_rect` is the slim `Sense::click_and_drag` strip (`±DIVIDER_GRAB_HALF_WIDTH`
/// around the line, spanning its full length) — narrow so it never steals clicks
/// from panel content or overlaps the scroll bar. `divider` is the on-screen
/// position of the line (x for a vertical divider, y for a horizontal one) where
/// the bar is painted. Dragging adjusts `value` (a panel width or height) along
/// the divider's axis; `sign` flips it (`-1.0` for a right/bottom panel that
/// grows as the divider moves toward it). Double-clicking resets `value` to
/// `default`.
#[allow(clippy::too_many_arguments)]
fn render_resize_divider(
    ctx: &egui::Context,
    id: &str,
    kind: DividerKind,
    hit_rect: Rect,
    divider: f32,
    value: &mut f32,
    sign: f32,
    min: f32,
    max: f32,
    default: f32,
    pal: &crate::frontend::theme::Palette,
) {
    // Proximity is a wider band than the grab strip: the bar reveals as the
    // pointer approaches, but only the slim strip senses drags/clicks.
    let proximity = ctx
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| match kind {
            DividerKind::Vertical => {
                (p.x - divider).abs() <= DIVIDER_PROXIMITY_RADIUS
                    && p.y >= hit_rect.top()
                    && p.y <= hit_rect.bottom()
            }
            DividerKind::Horizontal => {
                (p.y - divider).abs() <= DIVIDER_PROXIMITY_RADIUS
                    && p.x >= hit_rect.left()
                    && p.x <= hit_rect.right()
            }
        });
    egui::Area::new(Id::new(id))
        .order(Order::Foreground)
        .fixed_pos(hit_rect.min)
        .interactable(true)
        .show(ctx, |ui| {
            let (_, response) = ui.allocate_exact_size(hit_rect.size(), Sense::click_and_drag());
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(match kind {
                    DividerKind::Vertical => CursorIcon::ResizeHorizontal,
                    DividerKind::Horizontal => CursorIcon::ResizeVertical,
                });
            }
            if response.double_clicked() {
                *value = default;
            } else if response.dragged() {
                let delta = match kind {
                    DividerKind::Vertical => response.drag_delta().x,
                    DividerKind::Horizontal => response.drag_delta().y,
                };
                *value = (*value + sign * delta).clamp(min, max);
            }
            // Fade the bar in on approach / drag and out when the pointer leaves;
            // `animate_bool_with_time` self-requests repaints while in flight.
            let reveal = ui.ctx().animate_bool_with_time(
                Id::new((id, "reveal")),
                proximity || response.dragged(),
                DIVIDER_FADE_SECONDS,
            );
            let mut alpha = egui::lerp(
                DIVIDER_REST_ALPHA as f32..=DIVIDER_ACTIVE_ALPHA as f32,
                reveal,
            );
            if response.dragged() {
                alpha = (alpha + 20.0).min(245.0);
            }
            let thickness = egui::lerp(1.0..=DIVIDER_BAR_WIDTH, reveal);
            // Light-gray `hairline` tone (not the darker neutral tint) kept faint:
            // a soft pale-gray line on light, a soft lighter-than-bg line on dark.
            let [hr, hg, hb, _] = pal.hairline.to_array();
            let color = egui::Color32::from_rgba_unmultiplied(hr, hg, hb, alpha.round() as u8);
            let bar = match kind {
                DividerKind::Vertical => Rect::from_center_size(
                    egui::pos2(divider, hit_rect.center().y),
                    egui::vec2(thickness, hit_rect.height()),
                ),
                DividerKind::Horizontal => Rect::from_center_size(
                    egui::pos2(hit_rect.center().x, divider),
                    egui::vec2(hit_rect.width(), thickness),
                ),
            };
            ui.painter()
                .rect_filled(bar, egui::CornerRadius::same(1), color);
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
    let pal = crate::frontend::theme::palette(ui);
    let title_color = pal.text_primary;
    let muted_text = pal.text_muted;
    let centered_title = state.workspace_label();
    let title_bar_rect = ui.max_rect();

    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        // Omitted on macOS, where the native traffic-light buttons sit here.
        #[cfg(not(target_os = "macos"))]
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
                            .color(pal.text_tertiary),
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
                    ui.menu_button("Appearance", |ui| {
                        let current = state.config.theme;
                        for mode in crate::backend::config::ThemeMode::all() {
                            if ui.radio(current == mode, mode.label()).clicked() {
                                actions.push(AppAction::SetThemeMode(mode));
                                ui.close();
                            }
                        }
                    });
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
        let pal = crate::frontend::theme::palette(ui);
        for (icon, command, hover_fill) in [
            (
                egui_phosphor::regular::X,
                ViewportCommand::Close,
                pal.status_red,
            ),
            (
                if maximized {
                    egui_phosphor::regular::CORNERS_IN
                } else {
                    egui_phosphor::regular::CORNERS_OUT
                },
                ViewportCommand::Maximized(!maximized),
                pal.item_fill_hover,
            ),
            (
                egui_phosphor::regular::MINUS,
                ViewportCommand::Minimized(true),
                pal.item_fill_hover,
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
    let pal = crate::frontend::theme::palette(ui);
    ui.vertical_centered(|ui| {
        for view in PrimaryView::all() {
            let selected = state.ui.layout.active_primary_view == *view;
            let response = with_core_button_style(ui, selected, |ui| {
                ui.add_sized(
                    [36.0, 36.0],
                    Button::new(
                        RichText::new(view.icon())
                            .strong()
                            .color(core_button_text_color(&pal, selected)),
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

/// Render sidebar content pinned to the panel's exact width.
///
/// `Panel::exact_size` clips the panel *fill* to the requested width, but a child
/// widget that can't shrink that far (a Settings slider or combo carrying a fixed
/// label) still grows the content frame's `response.rect`. egui advances the
/// parent layout cursor by that grown rect, so the central column — and the
/// bottom panel nested inside it — get pushed out to the content edge while the
/// sidebar fill and our resize divider stay at the requested width, leaving a
/// blank band beside the sidebar (and the bottom panel failing to follow a narrow
/// drag). Rendering the content into a width-bounded, clipped child and advancing
/// the cursor by that fixed rect pins the response rect to the requested width, so
/// the fill, divider, central column, and bottom panel all stay flush at any
/// width. Content too wide to fit is clipped rather than overflowing.
fn render_pinned(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let rect = ui.max_rect();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    add(&mut child);
    ui.advance_cursor_after_rect(rect);
}

fn render_primary_sidebar(state: &mut AppState, ui: &mut egui::Ui, actions: &mut Vec<AppAction>) {
    let pal = crate::frontend::theme::palette(ui);
    ui.horizontal(|ui| {
        let btn_w = 28.0 + ui.spacing().item_spacing.x;
        let heading_w = (ui.available_width() - btn_w).max(0.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(heading_w, 28.0), Sense::hover());
        let mut heading_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        heading_ui.add(
            egui::Label::new(RichText::new(state.ui.layout.active_primary_view.label()).heading())
                .truncate(),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if with_core_button_style(ui, false, |ui| {
                ui.add_sized(
                    [28.0, 28.0],
                    Button::new(
                        RichText::new(egui_phosphor::regular::CARET_LEFT)
                            .color(core_button_text_color(&pal, false)),
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
    let pal = crate::frontend::theme::palette(ui);
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if with_core_button_style(ui, false, |ui| {
                ui.add_sized(
                    [24.0, 24.0],
                    Button::new(
                        RichText::new(egui_phosphor::regular::FILE_PLUS)
                            .size(13.0)
                            .color(core_button_text_color(&pal, false)),
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
                            .color(core_button_text_color(&pal, false)),
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
                            .color(core_button_text_color(&pal, false)),
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
            // Reserve the Create button on the right and let the field fill the rest.
            // A plain left-to-right row puts the default-width (280 px) field first,
            // which eats the whole row and pushes Create past the sidebar edge,
            // overflowing and growing the panel on a narrow sidebar.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let create = ui.button("Create").clicked();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.ui.entry_list.new_group_name)
                        .desired_width(f32::INFINITY),
                );
                let submit =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if create || submit {
                    actions.push(AppAction::CreateGroup {
                        name: state.ui.entry_list.new_group_name.clone(),
                    });
                }
            });
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

    let ordered_items: Vec<SelectionItem> = {
        let mut items = Vec::new();
        for group in &groups {
            let has_visible = state.entries.records.iter().any(|e| {
                e.group_id == group.id
                    && (search.is_empty()
                        || e.name.to_lowercase().contains(&search)
                        || e.id.to_string().contains(&search))
            });
            if has_visible || search.is_empty() {
                items.push(SelectionItem::Group(group.id.clone()));
            }
            if !state.ui.entry_list.collapsed_group_ids.contains(&group.id) {
                state
                    .entries
                    .records
                    .iter()
                    .filter(|e| e.group_id == group.id)
                    .filter(|e| {
                        search.is_empty()
                            || e.name.to_lowercase().contains(&search)
                            || e.id.to_string().contains(&search)
                    })
                    .for_each(|e| items.push(SelectionItem::Entry(e.id)));
            }
        }
        items.extend(
            ungrouped_entries
                .iter()
                .map(|(id, _, _)| SelectionItem::Entry(*id)),
        );
        items
    };

    ScrollArea::vertical()
        .max_height(ui.available_height().max(120.0))
        // Scroll only via wheel/trackpad; the scroll bar stays a non-interactive
        // position indicator (Mac-native behaviour). This stops the bar from
        // catching a drag that starts on the adjacent panel resize divider — the
        // bug where dragging the divider scrolled instead of resizing.
        .scroll_source(egui::scroll_area::ScrollSource::MOUSE_WHEEL)
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
                    if render_group_header(
                        state,
                        ui,
                        actions,
                        &group.id,
                        &group.name,
                        collapsed,
                        &ordered_items,
                    ) && !state
                        .ui
                        .entry_list
                        .collapsed_group_ids
                        .insert(group.id.clone())
                    {
                        state.ui.entry_list.collapsed_group_ids.remove(&group.id);
                    }
                    if !collapsed {
                        let ctx = EntryListCtx {
                            group_choices: &all_group_choices,
                            ordered_items: &ordered_items,
                        };
                        for (entry_id, name, entry_group_id) in &entries {
                            render_entry_list_item(
                                state,
                                ui,
                                actions,
                                *entry_id,
                                name,
                                entry_group_id,
                                &ctx,
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
                if render_group_header(
                    state,
                    ui,
                    actions,
                    &group.id,
                    &group.name,
                    collapsed,
                    &ordered_items,
                ) && !state
                    .ui
                    .entry_list
                    .collapsed_group_ids
                    .insert(group.id.clone())
                {
                    state.ui.entry_list.collapsed_group_ids.remove(&group.id);
                }

                if !collapsed {
                    let ctx = EntryListCtx {
                        group_choices: &all_group_choices,
                        ordered_items: &ordered_items,
                    };
                    for (entry_id, name, entry_group_id) in &entries {
                        render_entry_list_item(
                            state,
                            ui,
                            actions,
                            *entry_id,
                            name,
                            entry_group_id,
                            &ctx,
                        );
                    }
                }
                ui.add_space(2.0);
            }

            if !ungrouped_entries.is_empty() && !groups.is_empty() {
                ui.separator();
            }

            let ctx = EntryListCtx {
                group_choices: &all_group_choices,
                ordered_items: &ordered_items,
            };
            for (entry_id, name, group_id) in &ungrouped_entries {
                render_entry_list_item(state, ui, actions, *entry_id, name, group_id, &ctx);
            }
        });
}

fn render_group_header(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
    group_id: &str,
    group_name: &str,
    collapsed: bool,
    ordered_items: &[SelectionItem],
) -> bool {
    let is_selected = state.ui.entry_list.selected_group_ids.contains(group_id);
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

    let row_h = 22.0;
    let full_w = ui.available_width();
    let btn_w = 44.0;
    let left_w = (full_w - btn_w).max(0.0);

    let is_renaming = state.ui.entry_list.renaming_group_id.as_deref() == Some(group_id);

    // The whole row is the click target for selection and the collapse toggle.
    // Icons and name are painted directly so nothing overlaps it; only the
    // action buttons are real widgets, registered later so their rects win.
    // While renaming the row is hover-only and the text editor owns clicks.
    let sense = if is_renaming {
        Sense::hover()
    } else {
        Sense::click()
    };
    let (row_rect, row_resp) = ui.allocate_exact_size(egui::vec2(full_w, row_h), sense);
    let right_rect = egui::Rect::from_min_size(
        egui::pos2(row_rect.max.x - btn_w, row_rect.min.y),
        egui::vec2(btn_w, row_h),
    );

    // Background (selection or hover): a rounded, inset, filled highlight.
    let pal = crate::frontend::theme::palette(ui);
    let bg = if is_selected {
        pal.blue_overlay(40)
    } else if row_resp.hovered() {
        pal.neutral_overlay(30)
    } else {
        egui::Color32::TRANSPARENT
    };
    if bg != egui::Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(row_rect.shrink2(egui::vec2(4.0, 1.0)), 6.0, bg);
    }

    // Paint the caret marker and folder icon.
    let icon_color = pal.text_muted;
    let mut x = row_rect.left() + 4.0;
    let marker_galley = ui.painter().layout_no_wrap(
        marker.to_string(),
        egui::FontId::proportional(11.0),
        icon_color,
    );
    let marker_w = marker_galley.size().x;
    ui.painter().galley(
        egui::pos2(x, row_rect.center().y - marker_galley.size().y / 2.0),
        marker_galley,
        icon_color,
    );
    x += marker_w + 4.0;
    let folder_galley = ui.painter().layout_no_wrap(
        folder_icon.to_string(),
        egui::FontId::proportional(14.0),
        icon_color,
    );
    let folder_w = folder_galley.size().x;
    ui.painter().galley(
        egui::pos2(x, row_rect.center().y - folder_galley.size().y / 2.0),
        folder_galley,
        icon_color,
    );
    x += folder_w + 6.0;

    // Name, or the in-place rename editor occupying the name's original slot.
    let mut rename_done = false;
    if is_renaming {
        let edit_rect = egui::Rect::from_min_max(
            egui::pos2(x, row_rect.min.y),
            egui::pos2(row_rect.left() + left_w, row_rect.max.y),
        );
        let mut edit_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(edit_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        let resp = edit_ui.add(
            egui::TextEdit::singleline(&mut state.ui.entry_list.rename_group_buffer)
                .desired_width(f32::INFINITY),
        );
        if !state.ui.entry_list.rename_group_focus_requested {
            resp.request_focus();
            state.ui.entry_list.rename_group_focus_requested = true;
        }
        // Commit on Enter; cancel on any other focus loss (e.g. click away).
        if resp.lost_focus() {
            if edit_ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                actions.push(AppAction::RenameGroup {
                    group_id: group_id.to_string(),
                    new_name: state.ui.entry_list.rename_group_buffer.clone(),
                });
            }
            rename_done = true;
        }
    } else {
        let name_color = pal.text_primary;
        let avail = (row_rect.left() + left_w - x).max(0.0);
        let mut job = egui::text::LayoutJob::single_section(
            group_name.to_string(),
            egui::TextFormat {
                font_id: egui::FontId::proportional(13.0),
                color: name_color,
                ..Default::default()
            },
        );
        job.wrap = egui::text::TextWrapping {
            max_width: avail,
            max_rows: 1,
            overflow_character: Some('…'),
            break_anywhere: true,
        };
        let galley = ui.painter().fonts_mut(|f| f.layout_job(job));
        ui.painter().galley(
            egui::pos2(x, row_rect.center().y - galley.size().y / 2.0),
            galley,
            name_color,
        );
    }
    if rename_done {
        state.ui.entry_list.renaming_group_id = None;
        state.ui.entry_list.rename_group_focus_requested = false;
    }

    // Edit / delete buttons in the right area.
    let (btn_pencil, btn_trash) = {
        let mut right_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(right_rect)
                .layout(Layout::right_to_left(Align::Center)),
        );
        let trash = right_ui
            .add(
                egui::Button::new(RichText::new(egui_phosphor::regular::TRASH).size(11.0))
                    .frame(false),
            )
            .clicked();
        let pencil = right_ui
            .add(
                egui::Button::new(
                    RichText::new(egui_phosphor::regular::PENCIL_SIMPLE_LINE).size(11.0),
                )
                .frame(false),
            )
            .clicked();
        (pencil, trash)
    };

    if btn_pencil {
        state.ui.entry_list.renaming_group_id = Some(group_id.to_string());
        state.ui.entry_list.rename_group_buffer = group_name.to_string();
        state.ui.entry_list.rename_group_focus_requested = false;
    }
    if btn_trash {
        actions.push(AppAction::DeleteGroup(group_id.to_string()));
    }

    // Pre-collect selection state for the context-menu closure.
    let sel_entry_ids: Vec<u64> = state
        .ui
        .entry_list
        .selected_entry_ids
        .iter()
        .copied()
        .collect();
    let sel_group_ids: Vec<String> = state
        .ui
        .entry_list
        .selected_group_ids
        .iter()
        .cloned()
        .collect();
    row_resp.context_menu(|ui| {
        if ui.button("Rename").clicked() {
            state.ui.entry_list.renaming_group_id = Some(group_id.to_string());
            state.ui.entry_list.rename_group_buffer = group_name.to_string();
            ui.close();
        }
        ui.separator();
        render_delete_menu_items(
            ui,
            actions,
            &sel_entry_ids,
            &sel_group_ids,
            None,
            Some(group_id),
        );
    });

    // Handle selection on plain left-click (not a button click).
    if !btn_pencil && !btn_trash && row_resp.clicked() {
        let shift = ui.input(|i| i.modifiers.shift);
        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
        let this = SelectionItem::Group(group_id.to_string());

        if shift {
            let anchor_pos = state
                .ui
                .entry_list
                .selection_anchor
                .as_ref()
                .and_then(|a| ordered_items.iter().position(|item| item == a));
            let current_pos = ordered_items.iter().position(|item| item == &this);
            if let (Some(a), Some(b)) = (anchor_pos, current_pos) {
                let (lo, hi) = (a.min(b), a.max(b));
                state.ui.entry_list.selected_entry_ids.clear();
                state.ui.entry_list.selected_group_ids.clear();
                for item in &ordered_items[lo..=hi] {
                    match item {
                        SelectionItem::Entry(id) => {
                            state.ui.entry_list.selected_entry_ids.insert(*id);
                        }
                        SelectionItem::Group(id) => {
                            state.ui.entry_list.selected_group_ids.insert(id.clone());
                        }
                    }
                }
            } else {
                state.ui.entry_list.selected_entry_ids.clear();
                state.ui.entry_list.selected_group_ids.clear();
                state
                    .ui
                    .entry_list
                    .selected_group_ids
                    .insert(group_id.to_string());
                state.ui.entry_list.selection_anchor = Some(this);
            }
        } else if ctrl {
            if !state.ui.entry_list.selected_group_ids.remove(group_id) {
                state
                    .ui
                    .entry_list
                    .selected_group_ids
                    .insert(group_id.to_string());
            }
            state.ui.entry_list.selection_anchor = Some(this);
        } else {
            state.ui.entry_list.selected_entry_ids.clear();
            state.ui.entry_list.selected_group_ids.clear();
            state
                .ui
                .entry_list
                .selected_group_ids
                .insert(group_id.to_string());
            state.ui.entry_list.selection_anchor = Some(this);
        }

        return !shift && !ctrl;
    }

    false
}

fn render_delete_menu_items(
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
    sel_entry_ids: &[u64],
    sel_group_ids: &[String],
    focused_entry_id: Option<u64>,
    focused_group_id: Option<&str>,
) {
    let n_entries = sel_entry_ids.len();
    let n_groups = sel_group_ids.len();

    if n_entries == 0 && n_groups == 0 {
        if let Some(eid) = focused_entry_id {
            if ui.button("Delete Entry").clicked() {
                actions.push(AppAction::DeleteEntry(eid));
                ui.close();
            }
        } else if let Some(gid) = focused_group_id {
            if ui.button("Ungroup").clicked() {
                actions.push(AppAction::DeleteGroup(gid.to_string()));
                ui.close();
            }
            if ui.button("Delete Group and All Entries").clicked() {
                actions.push(AppAction::DeleteGroupWithEntries(gid.to_string()));
                ui.close();
            }
        }
        return;
    }

    if n_entries > 0 {
        let lbl = if n_entries == 1 {
            "Delete 1 Entry".to_string()
        } else {
            format!("Delete {} Entries", n_entries)
        };
        if ui.button(lbl).clicked() {
            actions.push(AppAction::DeleteEntries(sel_entry_ids.to_vec()));
            ui.close();
        }
    }
    if n_groups > 0 {
        let lbl = if n_groups == 1 {
            "Ungroup 1 Group".to_string()
        } else {
            format!("Ungroup {} Groups", n_groups)
        };
        if ui.button(lbl).clicked() {
            for gid in sel_group_ids {
                actions.push(AppAction::DeleteGroup(gid.clone()));
            }
            ui.close();
        }
        let lbl2 = if n_groups == 1 {
            "Delete 1 Group and Its Entries".to_string()
        } else {
            format!("Delete {} Groups and Their Entries", n_groups)
        };
        if ui.button(lbl2).clicked() {
            for gid in sel_group_ids {
                actions.push(AppAction::DeleteGroupWithEntries(gid.clone()));
            }
            ui.close();
        }
    }
}

struct EntryListCtx<'a> {
    group_choices: &'a [(String, String)],
    ordered_items: &'a [SelectionItem],
}

fn render_entry_list_item(
    state: &mut AppState,
    ui: &mut egui::Ui,
    actions: &mut Vec<AppAction>,
    entry_id: u64,
    name: &str,
    group_id: &str,
    ctx: &EntryListCtx<'_>,
) {
    let is_workspace_active = state.entries.active_entry_id() == Some(entry_id);
    let is_selected = state.ui.entry_list.selected_entry_ids.contains(&entry_id);
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
        let pal = crate::frontend::theme::palette(ui);
        let bg_fill = if is_workspace_active {
            pal.blue_overlay(80)
        } else if is_selected {
            pal.blue_overlay(40)
        } else if hovered {
            pal.neutral_overlay(30)
        } else {
            egui::Color32::TRANSPARENT
        };
        let text_color = if is_workspace_active {
            pal.text_strong
        } else if is_selected {
            pal.text_primary
        } else {
            pal.text_muted
        };

        ui.painter()
            .rect_filled(rect.shrink2(egui::vec2(4.0, 1.0)), 6.0, bg_fill);

        let text_rect = rect.shrink2(egui::vec2(6.0, 0.0));

        // A small "MD" chip marks entries produced by an MD run (extensible to
        // future provenance kinds). Lay it out first so the name reserves room.
        let is_md = state
            .entries
            .entry(entry_id)
            .map(|entry| entry.origin.is_md_run())
            .unwrap_or(false);
        let chip = is_md.then(|| {
            let galley = ui.painter().fonts_mut(|fonts| {
                fonts.layout_no_wrap(
                    "MD".to_string(),
                    egui::FontId::proportional(9.0),
                    pal.accent,
                )
            });
            let size = egui::vec2(galley.size().x + 8.0, galley.size().y + 3.0);
            (galley, size)
        });
        let name_reserve = chip.as_ref().map_or(0.0, |(_, size)| size.x + 6.0);

        let mut job = egui::text::LayoutJob::single_section(
            name.to_string(),
            egui::TextFormat {
                font_id: egui::FontId::proportional(13.0),
                color: text_color,
                ..Default::default()
            },
        );
        job.wrap = egui::text::TextWrapping {
            max_width: (text_rect.width() - name_reserve).max(10.0),
            max_rows: 1,
            overflow_character: Some('…'),
            break_anywhere: true,
        };
        let galley = ui.painter().fonts_mut(|f| f.layout_job(job));
        let galley_pos = egui::pos2(
            text_rect.left(),
            text_rect.center().y - galley.size().y / 2.0,
        );
        ui.painter().galley(galley_pos, galley, text_color);

        if let Some((chip_galley, chip_size)) = chip {
            let chip_rect = egui::Rect::from_min_size(
                egui::pos2(
                    text_rect.right() - chip_size.x,
                    text_rect.center().y - chip_size.y / 2.0,
                ),
                chip_size,
            );
            ui.painter()
                .rect_filled(chip_rect, 4.0, pal.blue_overlay(45));
            let chip_pos = egui::pos2(
                chip_rect.center().x - chip_galley.size().x / 2.0,
                chip_rect.center().y - chip_galley.size().y / 2.0,
            );
            ui.painter().galley(chip_pos, chip_galley, pal.accent);
        }

        if response.double_clicked() {
            actions.push(AppAction::ActivateEntry(entry_id));
            state.ui.entry_list.selected_entry_ids.insert(entry_id);
        } else if response.clicked() {
            let shift = ui.input(|i| i.modifiers.shift);
            let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);

            let this = SelectionItem::Entry(entry_id);
            if shift {
                let anchor_pos = state
                    .ui
                    .entry_list
                    .selection_anchor
                    .as_ref()
                    .and_then(|a| ctx.ordered_items.iter().position(|item| item == a));
                let current_pos = ctx.ordered_items.iter().position(|item| item == &this);
                if let (Some(a), Some(b)) = (anchor_pos, current_pos) {
                    let (lo, hi) = (a.min(b), a.max(b));
                    state.ui.entry_list.selected_entry_ids.clear();
                    state.ui.entry_list.selected_group_ids.clear();
                    for item in &ctx.ordered_items[lo..=hi] {
                        match item {
                            SelectionItem::Entry(id) => {
                                state.ui.entry_list.selected_entry_ids.insert(*id);
                            }
                            SelectionItem::Group(id) => {
                                state.ui.entry_list.selected_group_ids.insert(id.clone());
                            }
                        }
                    }
                } else {
                    state.ui.entry_list.selected_entry_ids.clear();
                    state.ui.entry_list.selected_group_ids.clear();
                    state.ui.entry_list.selected_entry_ids.insert(entry_id);
                    state.ui.entry_list.selection_anchor = Some(this);
                }
            } else if ctrl {
                if !state.ui.entry_list.selected_entry_ids.remove(&entry_id) {
                    state.ui.entry_list.selected_entry_ids.insert(entry_id);
                }
                state.ui.entry_list.selection_anchor = Some(this);
            } else {
                state.ui.entry_list.selected_entry_ids.clear();
                state.ui.entry_list.selected_group_ids.clear();
                state.ui.entry_list.selected_entry_ids.insert(entry_id);
                state.ui.entry_list.selection_anchor = Some(this);
            }
        }

        let sel_entry_ids: Vec<u64> = state
            .ui
            .entry_list
            .selected_entry_ids
            .iter()
            .copied()
            .collect();
        let sel_group_ids: Vec<String> = state
            .ui
            .entry_list
            .selected_group_ids
            .iter()
            .cloned()
            .collect();
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
            if !ctx.group_choices.is_empty() {
                ui.separator();
                ui.label("Move to group");
                for (target_group_id, target_group_name) in ctx.group_choices {
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
            render_delete_menu_items(
                ui,
                actions,
                &sel_entry_ids,
                &sel_group_ids,
                Some(entry_id),
                None,
            );
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
        ui.add(
            egui::TextEdit::singleline(&mut state.tasks.task_list.search_query)
                .desired_width(f32::INFINITY),
        );
    });
    ui.separator();
    ui.add_space(4.0);

    let search = state.tasks.task_list.search_query.to_lowercase();
    let pal = crate::frontend::theme::palette(ui);
    ScrollArea::vertical()
        // Scroll only via wheel/trackpad; the scroll bar stays a non-interactive
        // position indicator (Mac-native behaviour). This stops the bar from
        // catching a drag that starts on the adjacent panel resize divider — the
        // bug where dragging the divider scrolled instead of resizing.
        .scroll_source(egui::scroll_area::ScrollSource::MOUSE_WHEEL)
        .show(ui, |ui| {
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
                        ui.label(RichText::new(marker).size(11.0).color(pal.text_muted));
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
                            .fill(pal.item_fill)
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
                            ui.painter()
                                .rect_filled(response.rect, 6.0, pal.blue_overlay(18));
                            ui.painter().rect_stroke(
                                response.rect,
                                6.0,
                                Stroke::new(1.0, pal.blue_overlay(72)),
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
    let pal = crate::frontend::theme::palette(ui);
    ui.label(
        RichText::new(versions_caption)
            .small()
            .color(pal.text_tertiary),
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
            .color(pal.status_green)
        } else {
            RichText::new(format!(
                "{}  {}",
                egui_phosphor::regular::X_CIRCLE,
                row.name
            ))
            .color(pal.status_red)
        };
        ui.label(badge.strong());
        if let Some(version) = &row.version {
            ui.label(RichText::new(format!("version {version}")).small());
        }
        ui.label(
            RichText::new(row.description)
                .small()
                .color(pal.text_tertiary),
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
            ui.add(
                egui::TextEdit::singleline(&mut draft.command_prefix).desired_width(f32::INFINITY),
            );
        });
        ui.label(
            RichText::new("e.g. `wsl.exe -e` to run inside WSL; leave blank for a native install")
                .small()
                .color(pal.text_tertiary),
        );
        ui.horizontal(|ui| {
            ui.label("Program:");
            // Reserve the Browse button on the right and let the text field fill
            // the space between it and the label. A plain left-to-right layout
            // gives the singleline edit an infinite desired width, which eats the
            // whole row and pushes Browse off the (clipped) right edge.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Browse").clicked() {
                    actions.push(AppAction::BrowseEngineProgram(row.id));
                }
                ui.add(egui::TextEdit::singleline(&mut draft.program).desired_width(f32::INFINITY));
            });
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
        ui.add(
            egui::TextEdit::singleline(&mut state.ui.settings.search_query)
                .desired_width(f32::INFINITY),
        );
    });
    ui.separator();

    let search = state.ui.settings.search_query.to_lowercase();
    let pal = crate::frontend::theme::palette(ui);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        // No drag-to-scroll: a horizontal drag near the resize divider must
        // resize the sidebar, not pan the settings list.
        // Scroll only via wheel/trackpad; the scroll bar stays a non-interactive
        // position indicator (Mac-native behaviour). This stops the bar from
        // catching a drag that starts on the adjacent panel resize divider — the
        // bug where dragging the divider scrolled instead of resizing.
        .scroll_source(egui::scroll_area::ScrollSource::MOUSE_WHEEL)
        .show(ui, |ui| {
            settings_section(ui, "Appearance", &search, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Theme");
                    let current = state.config.theme;
                    egui::ComboBox::from_id_salt("theme_mode")
                        .selected_text(current.label())
                        .show_ui(ui, |ui| {
                            for mode in crate::backend::config::ThemeMode::all() {
                                if ui.selectable_label(current == mode, mode.label()).clicked() {
                                    actions.push(AppAction::SetThemeMode(mode));
                                }
                            }
                        });
                });
            });

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
                    .color(pal.text_tertiary),
                );

                ui.separator();
                ui.label(
                    RichText::new("Project default style by category")
                        .small()
                        .color(pal.text_tertiary),
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
    let pal = crate::frontend::theme::palette(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(36.0, 24.0), Sense::click());
    let fill = if response.hovered() {
        hover_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    let text_color = if is_close && response.hovered() {
        egui::Color32::WHITE
    } else {
        pal.text_muted
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
    let pal = crate::frontend::theme::palette(ui);
    let inactive_fill = core_button_fill(&pal, selected, false);
    let hovered_fill = core_button_fill(&pal, selected, true);
    let selected_fill = core_button_fill(&pal, true, false);
    let selected_hover_fill = core_button_fill(&pal, true, true);
    let inactive_text = core_button_text_color(&pal, selected);
    let selected_text = core_button_text_color(&pal, true);
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

fn core_button_fill(
    pal: &crate::frontend::theme::Palette,
    selected: bool,
    hovered: bool,
) -> egui::Color32 {
    match (selected, hovered) {
        (false, false) => egui::Color32::TRANSPARENT,
        (false, true) => pal.neutral_overlay(CORE_BUTTON_HOVER_ALPHA),
        (true, false) => pal.neutral_overlay(CORE_BUTTON_SELECTED_ALPHA),
        (true, true) => pal.neutral_overlay(CORE_BUTTON_SELECTED_HOVER_ALPHA),
    }
}

fn core_button_text_color(pal: &crate::frontend::theme::Palette, selected: bool) -> egui::Color32 {
    if selected {
        pal.text_primary
    } else {
        pal.text_muted
    }
}
