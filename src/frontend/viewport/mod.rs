use std::time::Duration;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Vec2};

use crate::{domain::Structure, frontend::AtomSelection};

mod camera;
mod composer;
mod export;
mod gpu;
mod interaction;
mod render;
mod visual_state;

pub use camera::ViewCamera;
pub(crate) use export::{ViewportPngExport, export_viewport_png};
pub(crate) use gpu::init as init_gpu_renderer;
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
    gpu: GpuViewCache,
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

/// State for the GPU ball-and-stick path. The instance set is camera-independent
/// (rebuilt only when `instance_key` changes), while pick targets are projected
/// atom centers cached per camera so hover/click picking stays on the CPU.
#[derive(Default)]
pub(super) struct GpuViewCache {
    instance_key: Option<GpuInstanceKey>,
    pick_key: Option<ViewportCacheKey>,
    pick_targets: Vec<PickTarget>,
}

impl ViewportCache {
    pub fn clear(&mut self) {
        self.geometry = GeometryCache::default();
        self.surface = SurfaceCache::default();
        self.gpu = GpuViewCache::default();
    }
}

/// Identifies the camera-independent inputs to the GPU instance set: atom
/// positions (via the structure revision) plus everything that affects an atom's
/// sphere radius, color, or visibility (styling and selection). Camera changes
/// are deliberately excluded so rotation never rebuilds instances.
#[derive(Clone, Copy, PartialEq)]
struct GpuInstanceKey {
    structure_id: u64,
    structure_revision: u64,
    visual_hash: u64,
    selection_hash: u64,
}

impl GpuInstanceKey {
    fn new(
        structure_id: u64,
        structure_revision: u64,
        visual_state: &ViewportVisualState,
        selection: &AtomSelection,
    ) -> Self {
        Self {
            structure_id,
            structure_revision,
            visual_hash: hash_visual_state(visual_state),
            selection_hash: hash_selection(selection),
        }
    }
}

/// Hash only the styling that changes ball-and-stick instances (per-category and
/// per-atom style overrides, plus ion visibility/color); surface/cartoon/lighting
/// fields do not affect the GPU instance set.
fn hash_visual_state(visual_state: &ViewportVisualState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (category, style) in &visual_state.category_styles {
        category.hash(&mut hasher);
        style.hash(&mut hasher);
    }
    for (index, style) in &visual_state.atom_styles {
        index.hash(&mut hasher);
        style.hash(&mut hasher);
    }
    visual_state
        .ions
        .show_within
        .map(f32::to_bits)
        .hash(&mut hasher);
    visual_state
        .ions
        .color
        .map(|color| color.to_array())
        .hash(&mut hasher);
    hasher.finish()
}

fn hash_selection(selection: &AtomSelection) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    selection.primary().hash(&mut hasher);
    selection.ordered_indices().hash(&mut hasher);
    hasher.finish()
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
    /// Whether the GPU molecule renderer initialized successfully at startup.
    /// When false (or for representations the GPU path doesn't cover), the CPU
    /// rasterizer is used instead.
    pub gpu_ready: bool,
}

/// Whether the GPU ball-and-stick path can render this scene. It covers spheres
/// and stick bonds; cartoon and molecular-surface representations still go
/// through the CPU compositor.
fn gpu_path_supported(structure: &Structure, visual_state: &ViewportVisualState) -> bool {
    !any_atoms_drawn_as_cartoon(structure, visual_state) && visual_state.surface.chains.is_empty()
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
        gpu_ready,
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
    let pick_targets = if gpu_ready && gpu_path_supported(structure, visual_state) {
        render_molecules_gpu(
            &painter,
            rect,
            &viewport,
            cache_key,
            structure,
            structure_id,
            structure_revision,
            selection,
            visual_state,
            &mut cache.gpu,
        )
    } else {
        render_molecules_cpu(
            &painter,
            rect,
            &viewport,
            cache_key,
            structure,
            structure_id,
            structure_revision,
            selection,
            visual_state,
            cache,
        )
    };

    if visual_state.show_cell
        && let Some(cell) = &structure.cell
    {
        draw_cell_labels(&painter, &viewport, cell);
    }

    let interaction = InteractionSystem::new(
        structure,
        &pick_targets,
        previous_hovered_atom,
        visual_state,
    )
    .run(ui, &response, camera);

    for atom_projection in &pick_targets {
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

/// CPU rasterizer path: build the full scene (ball-and-stick, cartoon, surface,
/// cell) and submit it to the egui painter. Returns the projected pick targets.
#[allow(clippy::too_many_arguments)]
fn render_molecules_cpu(
    painter: &egui::Painter,
    rect: egui::Rect,
    viewport: &Projector,
    cache_key: ViewportCacheKey,
    structure: &Structure,
    structure_id: u64,
    structure_revision: u64,
    selection: &AtomSelection,
    visual_state: &ViewportVisualState,
    cache: &mut ViewportCache,
) -> Vec<PickTarget> {
    let geometry = cached_geometry(&mut cache.geometry, cache_key, structure, viewport);
    let scene_result = RepresentationComposer::for_viewport(
        structure,
        geometry,
        viewport,
        selection,
        visual_state,
        SurfaceCacheContext::new(&mut cache.surface, structure_id, structure_revision),
    )
    .build();
    let rendered_in_full =
        submit_scene_to_painter_within_budget(painter, &scene_result.scene, MAX_RENDER_TRIANGLES);

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

    scene_result.pick_targets
}

/// GPU path: rebuild the (camera-independent) instance set only when styling or
/// selection changed, then queue a single paint callback. The unit-cell box is
/// still drawn through the painter. Returns projected atom centers for picking.
#[allow(clippy::too_many_arguments)]
fn render_molecules_gpu(
    painter: &egui::Painter,
    rect: egui::Rect,
    viewport: &Projector,
    cache_key: ViewportCacheKey,
    structure: &Structure,
    structure_id: u64,
    structure_revision: u64,
    selection: &AtomSelection,
    visual_state: &ViewportVisualState,
    gpu_cache: &mut GpuViewCache,
) -> Vec<PickTarget> {
    if visual_state.show_cell
        && let Some(cell) = &structure.cell
    {
        submit_scene_to_painter_within_budget(
            painter,
            &build_cell_scene(viewport, cell),
            MAX_RENDER_TRIANGLES,
        );
    }

    let instance_key =
        GpuInstanceKey::new(structure_id, structure_revision, visual_state, selection);
    let upload = if gpu_cache.instance_key == Some(instance_key) {
        None
    } else {
        gpu_cache.instance_key = Some(instance_key);
        Some(build_molecule_instances(structure, selection, visual_state))
    };
    gpu::emit(painter, rect, viewport, upload);

    if gpu_cache.pick_key.as_ref() != Some(&cache_key) {
        gpu_cache.pick_targets = project_pick_targets(structure, viewport);
        gpu_cache.pick_key = Some(cache_key);
    }
    gpu_cache.pick_targets.clone()
}
