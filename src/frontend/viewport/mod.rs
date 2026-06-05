use std::time::Duration;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Vec2};

use crate::{domain::Structure, frontend::AtomSelection};

mod camera;
mod composer;
mod export;
mod interaction;
mod render;
mod visual_state;

pub use camera::ViewCamera;
pub(crate) use export::{ViewportPngExport, export_viewport_png};
pub use visual_state::{
    CartoonSectionStyle, LightPreset, SurfaceStyle, ViewportCartoonState, ViewportIonState,
    ViewportLightingState, ViewportSurfaceState, ViewportVisualState, software_default_style,
};

use camera::{Projector, view_center_and_radius};
use composer::{RepresentationComposer, SurfaceCacheContext};
use interaction::{InteractionSystem, ViewportInteraction};
use render::*;

pub const STRUCTURE_INTERACTION_FRAME: Duration = Duration::from_millis(16);
pub const HOVER_FRAME: Duration = Duration::from_millis(100);

/// Per-viewport render caches. The projected ball-and-stick geometry and the
/// (much more expensive) molecular surface are cached independently so a frame
/// can borrow the geometry immutably while still updating the surface cache —
/// avoiding a full clone of the geometry every frame.
#[derive(Default)]
pub struct ViewportCache {
    geometry: GeometryCache,
    surface: SurfaceCache,
}

#[derive(Default)]
pub(super) struct GeometryCache {
    key: Option<ViewportCacheKey>,
    geometry: Option<ViewportGeometry>,
}

#[derive(Default)]
pub(super) struct SurfaceCache {
    key: Option<SurfaceCacheKey>,
    geometry: Option<SurfaceSceneGeometry>,
}

impl ViewportCache {
    pub fn clear(&mut self) {
        self.geometry = GeometryCache::default();
        self.surface = SurfaceCache::default();
    }
}

#[derive(Clone, PartialEq)]
struct ViewportCacheKey {
    structure_id: u64,
    structure_revision: u64,
    rect_min: Pos2,
    rect_max: Pos2,
    camera: ViewCamera,
    show_cell: bool,
}

#[derive(Clone, PartialEq)]
pub(super) struct SurfaceCacheKey {
    pub(super) structure_id: u64,
    pub(super) structure_revision: u64,
    pub(super) style: SurfaceStyle,
    pub(super) surface_chains: Vec<char>,
}

pub struct ViewportDrawArgs<'a> {
    pub structure: &'a Structure,
    pub structure_id: u64,
    pub structure_revision: u64,
    pub camera: &'a mut ViewCamera,
    pub selection: &'a AtomSelection,
    pub visual_state: &'a ViewportVisualState,
    pub previous_hovered_atom: Option<usize>,
    pub cache: &'a mut ViewportCache,
    pub empty_state_hint: Option<&'a str>,
}

pub fn draw_viewport(ui: &mut egui::Ui, args: ViewportDrawArgs<'_>) -> ViewportInteraction {
    let ViewportDrawArgs {
        structure,
        structure_id,
        structure_revision,
        camera,
        selection,
        visual_state,
        previous_hovered_atom,
        cache,
        empty_state_hint,
    } = args;
    let available = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(available, Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 0.0, visual_state.background_color);

    if structure.atoms.is_empty() {
        if let Some(hint) = empty_state_hint {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                hint,
                FontId::proportional(18.0),
                Color32::from_rgb(80, 84, 90),
            );
        }

        cache.clear();
        return ViewportInteraction::default();
    }

    let (center, radius) = view_center_and_radius(structure, visual_state.show_cell);
    let viewport = Projector::new(
        rect,
        center,
        rect.width().min(rect.height()) * 0.35 * (1.0 + camera.zoom) / radius,
        radius * 3.2,
        camera.yaw,
        camera.pitch,
        camera.pan,
    );
    let cache_key = ViewportCacheKey {
        structure_id,
        structure_revision,
        rect_min: rect.min,
        rect_max: rect.max,
        camera: *camera,
        show_cell: visual_state.show_cell,
    };
    let geometry = cached_geometry(&mut cache.geometry, cache_key, structure, &viewport);
    let scene_result = RepresentationComposer::for_viewport(
        structure,
        geometry,
        &viewport,
        selection,
        visual_state,
        SurfaceCacheContext::new(&mut cache.surface, structure_id, structure_revision),
    )
    .build();
    let rendered_in_full =
        submit_scene_to_painter_within_budget(&painter, &scene_result.scene, MAX_RENDER_TRIANGLES);

    if visual_state.show_cell
        && let Some(cell) = &structure.cell
    {
        draw_cell_labels(&painter, &viewport, cell);
    }

    if !rendered_in_full {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            format!(
                "Structure too large to render ({} atoms).\nThe view is simplified; reduce the system or hide water to see more.",
                structure.atoms.len()
            ),
            FontId::proportional(16.0),
            Color32::from_rgb(150, 60, 60),
        );
    }

    let interaction = InteractionSystem::new(
        structure,
        &scene_result.pick_targets,
        previous_hovered_atom,
        visual_state,
    )
    .run(ui, &response, camera);

    for atom_projection in &scene_result.pick_targets {
        if !visual_state.show_atom_labels
            || !atom_visible(structure, visual_state, atom_projection.index)
        {
            continue;
        }
        let atom = &structure.atoms[atom_projection.index];
        painter.text(
            atom_projection.pos,
            Align2::CENTER_CENTER,
            &atom.element,
            FontId::proportional(12.0),
            Color32::BLACK,
        );
    }

    if let Some(index) = interaction.hovered_atom {
        draw_hovered_atom_label(&painter, rect, structure, index);
    }

    painter.text(
        rect.left_top() + Vec2::new(12.0, 12.0),
        Align2::LEFT_TOP,
        "Click select | Ctrl+Click add/remove | Left drag rotate | Right/Middle drag pan | Wheel zoom",
        FontId::proportional(13.0),
        Color32::from_rgb(70, 74, 80),
    );

    interaction
}
