use std::f32::consts::TAU;

use eframe::egui::{Color32, Pos2, Rect};
use nalgebra::{Point3, Rotation3, Unit, Vector3};

use crate::{
    domain::{
        Biopolymer, ChainRecord, ResidueRecord, SecondaryStructureKind, SecondaryStructureSpan,
        Structure,
    },
    frontend::{LightPreset, ViewportCartoonState, state::AtomStyle},
};

use super::super::camera::Projector;
use super::backend::{LineSegmentPrimitive, RenderScene};
use super::{
    PrimitiveMeshVertex, PrimitiveTriangle, ViewportVisualState, chain_color, darken,
    edge_function, initial_cartoon_side, interpolate_orientation_hint, lighten, mix_color,
    normalize_vector3, orthogonalize_to_tangent, usable_biopolymer,
};

const CARTOON_DEPTH_BUFFER_RESOLUTION: usize = 384;

pub(crate) struct ScreenDepthBuffer {
    pub(super) bounds: Rect,
    pub(super) scale: f32,
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) depths: Vec<f32>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CartoonSegmentKind {
    Helix,
    Sheet,
    Coil,
}

#[derive(Clone, Copy)]
struct CartoonStyle {
    half_width: f32,
    half_thickness: f32,
}

#[derive(Clone, Copy)]
struct CartoonControlPoint {
    position: Point3<f32>,
    style: CartoonStyle,
    color: Color32,
    orientation_hint: Option<Vector3<f32>>,
}

#[derive(Clone, Copy)]
struct CartoonSweepSample {
    position: Point3<f32>,
    tangent: Vector3<f32>,
    side: Vector3<f32>,
    normal: Vector3<f32>,
    style: CartoonStyle,
    color: Color32,
}

#[derive(Clone, Copy)]
struct ChainResiduePoint {
    residue_index: usize,
    position: Point3<f32>,
}

pub(crate) fn build_biopolymer_cartoon_depth_buffer(
    structure: &Structure,
    viewport: &Projector,
    visual_state: &ViewportVisualState,
) -> Option<ScreenDepthBuffer> {
    let biopolymer = usable_biopolymer(structure)?;
    let triangles = cartoon_fragments(structure, biopolymer, viewport, visual_state)
        .into_iter()
        .flat_map(|fragment| fragment.triangles)
        .collect::<Vec<_>>();
    rasterize_cartoon_depth_buffer(viewport.rect, &triangles)
}

pub(crate) fn build_biopolymer_cartoon_scene(
    structure: &Structure,
    viewport: &Projector,
    visual_state: &ViewportVisualState,
) -> RenderScene {
    let Some(biopolymer) = usable_biopolymer(structure) else {
        return RenderScene::default();
    };

    let mut opaque_meshes = Vec::new();
    let mut lines = Vec::new();
    for fragment in cartoon_fragments(structure, biopolymer, viewport, visual_state) {
        if visual_state.lighting.silhouettes && visual_state.lighting.silhouette_width > 0.0 {
            append_cartoon_silhouette(
                &mut lines,
                viewport,
                &fragment.controls,
                visual_state.lighting.silhouette_width,
            );
        }
        opaque_meshes.extend(fragment.triangles);
    }
    let mut scene = RenderScene::default();
    scene.push_opaque_meshes(opaque_meshes);
    scene.push_lines(lines);
    scene.sorted()
}

pub(super) fn sample_depth_buffer(depth_buffer: &ScreenDepthBuffer, pos: Pos2) -> Option<f32> {
    if !depth_buffer.bounds.contains(pos) {
        return None;
    }

    let sample = depth_buffer_pos(pos, depth_buffer.bounds, depth_buffer.scale);
    let x = sample.x.floor() as isize;
    let y = sample.y.floor() as isize;
    if x < 0 || y < 0 || x >= depth_buffer.width as isize || y >= depth_buffer.height as isize {
        return None;
    }

    let depth = depth_buffer.depths[y as usize * depth_buffer.width + x as usize];
    (depth > f32::NEG_INFINITY).then_some(depth)
}

pub(super) fn mesh_sample_visible(depth_buffer: &ScreenDepthBuffer, pos: Pos2, depth: f32) -> bool {
    match sample_depth_buffer(depth_buffer, pos) {
        Some(cartoon_depth) => depth >= cartoon_depth - super::MESH_OCCLUSION_DEPTH_EPSILON,
        None => true,
    }
}

fn cartoon_fragments(
    structure: &Structure,
    biopolymer: &Biopolymer,
    viewport: &Projector,
    visual_state: &ViewportVisualState,
) -> Vec<CartoonFragment> {
    let mut fragments_out = Vec::new();

    for (chain_index, chain) in biopolymer.chains.iter().enumerate() {
        let chain_color = visual_state
            .chain_colors
            .get(&chain.id)
            .copied()
            .unwrap_or_else(|| chain_color(chain, biopolymer, chain_index));
        let residue_trace = chain_residue_trace(structure, biopolymer, chain, visual_state);
        if residue_trace.len() < 2 {
            continue;
        }

        for (start, end) in chain_contiguous_fragments(biopolymer, &residue_trace) {
            let fragment = &residue_trace[start..end];
            if fragment.len() < 2 {
                continue;
            }

            let controls = fragment
                .iter()
                .enumerate()
                .map(|(fragment_index, entry)| {
                    let residue = &biopolymer.residues[entry.residue_index];
                    let kind = residue_cartoon_kind(residue, biopolymer, chain.id);
                    CartoonControlPoint {
                        position: entry.position,
                        style: cartoon_style(kind, &visual_state.cartoon),
                        color: cartoon_segment_color(chain_color, kind),
                        orientation_hint: cartoon_orientation_hint(fragment, fragment_index, kind),
                    }
                })
                .collect::<Vec<_>>();

            let smoothed = smooth_cartoon_controls(&controls, visual_state.cartoon.smoothing);
            let sweep_samples = build_cartoon_sweep_samples(&smoothed);
            fragments_out.push(CartoonFragment {
                controls: smoothed,
                triangles: build_cartoon_triangles(viewport, &sweep_samples, visual_state),
            });
        }
    }

    fragments_out
}

struct CartoonFragment {
    controls: Vec<CartoonControlPoint>,
    triangles: Vec<PrimitiveTriangle>,
}

fn chain_residue_trace(
    structure: &Structure,
    biopolymer: &Biopolymer,
    chain: &ChainRecord,
    visual_state: &ViewportVisualState,
) -> Vec<ChainResiduePoint> {
    chain
        .residue_indices
        .iter()
        .filter_map(|&residue_index| {
            let residue = biopolymer.residues.get(residue_index)?;
            let atom_index = residue.alpha_carbon?;
            // Only residues whose alpha carbon is styled Cartoon are drawn as
            // ribbon, so the user can switch a protein selection to/from cartoon.
            let is_cartoon = residue.is_standard_amino_acid
                && visual_state.resolved_atom_style(structure, atom_index) == AtomStyle::Cartoon;
            is_cartoon.then_some(ChainResiduePoint {
                residue_index,
                position: structure.atoms[atom_index].position,
            })
        })
        .collect()
}

fn chain_contiguous_fragments(
    biopolymer: &Biopolymer,
    residue_trace: &[ChainResiduePoint],
) -> Vec<(usize, usize)> {
    let mut fragments = Vec::new();
    if residue_trace.is_empty() {
        return fragments;
    }

    let mut fragment_start = 0;
    for index in 1..residue_trace.len() {
        let is_contiguous = residues_are_contiguous(
            &biopolymer.residues[residue_trace[index - 1].residue_index],
            &biopolymer.residues[residue_trace[index].residue_index],
        );

        if !is_contiguous {
            fragments.push((fragment_start, index));
            fragment_start = index;
        }
    }
    fragments.push((fragment_start, residue_trace.len()));
    fragments
}

fn cartoon_orientation_hint(
    fragment: &[ChainResiduePoint],
    index: usize,
    kind: CartoonSegmentKind,
) -> Option<Vector3<f32>> {
    if fragment.len() < 3 {
        return None;
    }
    match kind {
        CartoonSegmentKind::Helix | CartoonSegmentKind::Coil => {
            let previous = if index > 0 {
                fragment[index - 1].position
            } else {
                fragment[index].position
            };
            let current = fragment[index].position;
            let next = if index + 1 < fragment.len() {
                fragment[index + 1].position
            } else {
                fragment[index].position
            };
            let incoming = current - previous;
            let outgoing = next - current;
            let normal = incoming.cross(&outgoing);
            (normal.norm_squared() > 0.0001)
                .then(|| normalize_vector3(normal, Vector3::new(0.0, 1.0, 0.0)))
        }
        CartoonSegmentKind::Sheet => None,
    }
}

fn residue_cartoon_kind(
    residue: &ResidueRecord,
    biopolymer: &Biopolymer,
    chain_id: char,
) -> CartoonSegmentKind {
    match residue_secondary_structure_kind(residue, biopolymer, chain_id) {
        Some(SecondaryStructureKind::Helix) => CartoonSegmentKind::Helix,
        Some(SecondaryStructureKind::Sheet) => CartoonSegmentKind::Sheet,
        None => CartoonSegmentKind::Coil,
    }
}

fn residue_secondary_structure_kind(
    residue: &ResidueRecord,
    biopolymer: &Biopolymer,
    chain_id: char,
) -> Option<SecondaryStructureKind> {
    biopolymer
        .secondary_structures
        .iter()
        .find(|span| {
            span.start.chain_id == chain_id
                && residue_in_span(residue, span)
                && matches!(
                    span.kind,
                    SecondaryStructureKind::Helix | SecondaryStructureKind::Sheet
                )
        })
        .map(|span| span.kind)
}

fn residue_in_span(residue: &ResidueRecord, span: &SecondaryStructureSpan) -> bool {
    residue.id.chain_id == span.start.chain_id
        && residue.id.ordering_key() >= span.start.ordering_key()
        && residue.id.ordering_key() <= span.end.ordering_key()
}

fn residues_are_contiguous(previous: &ResidueRecord, current: &ResidueRecord) -> bool {
    previous.id.chain_id == current.id.chain_id
        && current.id.sequence_number - previous.id.sequence_number <= 1
        && current.id.sequence_number >= previous.id.sequence_number
}

fn smooth_cartoon_controls(
    points: &[CartoonControlPoint],
    subdivisions: usize,
) -> Vec<CartoonControlPoint> {
    if points.len() < 3 || subdivisions < 2 {
        return points.to_vec();
    }

    let mut smoothed = Vec::with_capacity((points.len() - 1) * subdivisions + 1);
    for index in 0..points.len() - 1 {
        let p0 = if index == 0 {
            points[0]
        } else {
            points[index - 1]
        };
        let p1 = points[index];
        let p2 = points[index + 1];
        let p3 = if index + 2 < points.len() {
            points[index + 2]
        } else {
            points[points.len() - 1]
        };

        for step in 0..subdivisions {
            let t = step as f32 / subdivisions as f32;
            let eased = smoothstep(t);
            smoothed.push(CartoonControlPoint {
                position: catmull_rom_point(p0.position, p1.position, p2.position, p3.position, t),
                style: lerp_cartoon_style(p1.style, p2.style, eased),
                color: mix_color(p1.color, p2.color, eased),
                orientation_hint: interpolate_orientation_hint(
                    p1.orientation_hint,
                    p2.orientation_hint,
                    eased,
                ),
            });
        }
    }
    smoothed.push(*points.last().expect("non-empty point list"));
    smoothed
}

fn catmull_rom_point(
    p0: Point3<f32>,
    p1: Point3<f32>,
    p2: Point3<f32>,
    p3: Point3<f32>,
    t: f32,
) -> Point3<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    let coords = 0.5
        * ((2.0 * p1.coords)
            + (-p0.coords + p2.coords) * t
            + (2.0 * p0.coords - 5.0 * p1.coords + 4.0 * p2.coords - p3.coords) * t2
            + (-p0.coords + 3.0 * p1.coords - 3.0 * p2.coords + p3.coords) * t3);
    Point3::from(coords)
}

fn build_cartoon_sweep_samples(controls: &[CartoonControlPoint]) -> Vec<CartoonSweepSample> {
    if controls.len() < 2 {
        return Vec::new();
    }

    let tangents = controls
        .iter()
        .enumerate()
        .map(|(index, control)| {
            let previous = if index > 0 {
                controls[index - 1].position
            } else {
                control.position
            };
            let next = if index + 1 < controls.len() {
                controls[index + 1].position
            } else {
                control.position
            };
            normalize_vector3(next - previous, Vector3::new(0.0, 0.0, 1.0))
        })
        .collect::<Vec<_>>();

    let mut samples = Vec::with_capacity(controls.len());
    let mut side = controls[0]
        .orientation_hint
        .map(|hint| align_orientation_hint(hint, tangents[0], initial_cartoon_side(tangents[0])))
        .unwrap_or_else(|| initial_cartoon_side(tangents[0]));
    let mut normal = normalize_vector3(tangents[0].cross(&side), Vector3::new(0.0, 1.0, 0.0));
    side = normalize_vector3(normal.cross(&tangents[0]), side);

    samples.push(CartoonSweepSample {
        position: controls[0].position,
        tangent: tangents[0],
        side,
        normal,
        style: controls[0].style,
        color: controls[0].color,
    });

    for index in 1..controls.len() {
        side = if let Some(hint) = controls[index].orientation_hint {
            let aligned = align_orientation_hint(hint, tangents[index], side);
            if aligned.dot(&side) < 0.0 {
                -aligned
            } else {
                aligned
            }
        } else {
            transport_frame_vector(side, tangents[index - 1], tangents[index])
        };
        side = orthogonalize_to_tangent(side, tangents[index], side);
        normal = normalize_vector3(tangents[index].cross(&side), samples[index - 1].normal);
        side = normalize_vector3(normal.cross(&tangents[index]), side);

        samples.push(CartoonSweepSample {
            position: controls[index].position,
            tangent: tangents[index],
            side,
            normal,
            style: controls[index].style,
            color: controls[index].color,
        });
    }

    samples
}

fn build_cartoon_triangles(
    viewport: &Projector,
    sweep_samples: &[CartoonSweepSample],
    visual_state: &ViewportVisualState,
) -> Vec<PrimitiveTriangle> {
    let profile_segments = visual_state.cartoon.profile_segments.clamp(6, 48);

    if sweep_samples.len() < 2 {
        return Vec::new();
    }

    let rings = sweep_samples
        .iter()
        .map(|sample| build_cartoon_ring(viewport, sample, profile_segments, visual_state))
        .collect::<Vec<_>>();
    let mut triangles = Vec::with_capacity((rings.len() - 1) * profile_segments * 2 + 28);

    for ring_pair in rings.windows(2) {
        let current = &ring_pair[0];
        let next = &ring_pair[1];
        for index in 0..profile_segments {
            let next_index = (index + 1) % profile_segments;
            triangles.push(cartoon_triangle(
                current[index],
                next[index],
                next[next_index],
            ));
            triangles.push(cartoon_triangle(
                current[index],
                next[next_index],
                current[next_index],
            ));
        }
    }

    append_cartoon_cap(
        &mut triangles,
        viewport,
        sweep_samples[0],
        &rings[0],
        -sweep_samples[0].tangent,
        visual_state,
    );
    append_cartoon_cap(
        &mut triangles,
        viewport,
        *sweep_samples.last().expect("non-empty sweep samples"),
        rings.last().expect("non-empty rings"),
        sweep_samples
            .last()
            .expect("non-empty sweep samples")
            .tangent,
        visual_state,
    );
    triangles.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    triangles
}

fn build_cartoon_ring(
    viewport: &Projector,
    sample: &CartoonSweepSample,
    segments: usize,
    visual_state: &ViewportVisualState,
) -> Vec<PrimitiveMeshVertex> {
    let mut ring = Vec::with_capacity(segments);
    for index in 0..segments {
        let angle = index as f32 * TAU / segments as f32;
        let (sin_angle, cos_angle) = angle.sin_cos();
        let offset = sample.side * (cos_angle * sample.style.half_width)
            + sample.normal * (sin_angle * sample.style.half_thickness);
        let position = sample.position + offset;
        let normal = normalize_vector3(
            sample.side * (cos_angle / sample.style.half_width.max(0.15))
                + sample.normal * (sin_angle / sample.style.half_thickness.max(0.15)),
            sample.normal,
        );
        let projected = viewport.project(position);
        ring.push(PrimitiveMeshVertex {
            pos: projected.pos,
            depth: projected.depth,
            color: shade_cartoon_color(
                viewport,
                sample.color,
                normal,
                visual_state.lighting.preset,
            ),
        });
    }
    ring
}

fn append_cartoon_cap(
    triangles: &mut Vec<PrimitiveTriangle>,
    viewport: &Projector,
    sample: CartoonSweepSample,
    ring: &[PrimitiveMeshVertex],
    cap_normal: Vector3<f32>,
    visual_state: &ViewportVisualState,
) {
    let projected = viewport.project(sample.position);
    let center = PrimitiveMeshVertex {
        pos: projected.pos,
        depth: projected.depth,
        color: shade_cartoon_color(
            viewport,
            darken(sample.color, 0.08),
            cap_normal,
            visual_state.lighting.preset,
        ),
    };

    for index in 0..ring.len() {
        let next_index = (index + 1) % ring.len();
        triangles.push(cartoon_triangle(center, ring[next_index], ring[index]));
    }
}

fn append_cartoon_silhouette(
    lines: &mut Vec<LineSegmentPrimitive>,
    viewport: &Projector,
    controls: &[CartoonControlPoint],
    width: f32,
) {
    for pair in controls.windows(2) {
        let start = viewport.project(pair[0].position).pos;
        let end = viewport.project(pair[1].position).pos;
        let local_width = pair[0].style.half_width.max(pair[0].style.half_thickness)
            + pair[1].style.half_width.max(pair[1].style.half_thickness);
        lines.push(LineSegmentPrimitive {
            start,
            end,
            color: Color32::from_rgba_unmultiplied(25, 28, 32, 90),
            width: width + local_width * 2.0,
        });
    }
}

fn rasterize_cartoon_depth_buffer(
    rect: Rect,
    triangles: &[PrimitiveTriangle],
) -> Option<ScreenDepthBuffer> {
    if triangles.is_empty() || rect.width() <= 1.0 || rect.height() <= 1.0 {
        return None;
    }

    let max_dimension = rect.width().max(rect.height()).max(1.0);
    let scale = (CARTOON_DEPTH_BUFFER_RESOLUTION as f32 / max_dimension).min(1.0);
    let width = (rect.width() * scale).ceil().max(2.0) as usize;
    let height = (rect.height() * scale).ceil().max(2.0) as usize;
    let mut depths = vec![f32::NEG_INFINITY; width * height];

    for triangle in triangles {
        rasterize_cartoon_triangle_depth(&mut depths, width, height, rect, scale, triangle);
    }

    Some(ScreenDepthBuffer {
        bounds: rect,
        scale,
        width,
        height,
        depths,
    })
}

fn rasterize_cartoon_triangle_depth(
    depth_buffer: &mut [f32],
    width: usize,
    height: usize,
    rect: Rect,
    scale: f32,
    triangle: &PrimitiveTriangle,
) {
    let a = depth_buffer_pos(triangle.vertices[0].pos, rect, scale);
    let b = depth_buffer_pos(triangle.vertices[1].pos, rect, scale);
    let c = depth_buffer_pos(triangle.vertices[2].pos, rect, scale);
    let area = edge_function(a, b, c);
    if area.abs() <= 0.0001 {
        return;
    }

    let min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as usize;
    let min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as usize;
    let max_x = a.x.max(b.x).max(c.x).ceil().min((width - 1) as f32) as usize;
    let max_y = a.y.max(b.y).max(c.y).ceil().min((height - 1) as f32) as usize;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let sample = Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
            let w0 = edge_function(b, c, sample) / area;
            let w1 = edge_function(c, a, sample) / area;
            let w2 = edge_function(a, b, sample) / area;
            if w0 < -0.0001 || w1 < -0.0001 || w2 < -0.0001 {
                continue;
            }

            let depth = triangle.vertices[0].depth * w0
                + triangle.vertices[1].depth * w1
                + triangle.vertices[2].depth * w2;
            let index = y * width + x;
            if depth > depth_buffer[index] {
                depth_buffer[index] = depth;
            }
        }
    }
}

fn depth_buffer_pos(pos: Pos2, rect: Rect, scale: f32) -> Pos2 {
    Pos2::new((pos.x - rect.min.x) * scale, (pos.y - rect.min.y) * scale)
}

fn cartoon_triangle(
    first: PrimitiveMeshVertex,
    second: PrimitiveMeshVertex,
    third: PrimitiveMeshVertex,
) -> PrimitiveTriangle {
    super::primitive_triangle(first, second, third)
}

fn cartoon_style(kind: CartoonSegmentKind, settings: &ViewportCartoonState) -> CartoonStyle {
    let section = match kind {
        CartoonSegmentKind::Helix => settings.helix,
        CartoonSegmentKind::Sheet => settings.sheet,
        CartoonSegmentKind::Coil => settings.coil,
    };
    CartoonStyle {
        half_width: section.width * 0.5,
        half_thickness: section.thickness * 0.5,
    }
}

fn cartoon_segment_color(base: Color32, kind: CartoonSegmentKind) -> Color32 {
    match kind {
        CartoonSegmentKind::Helix => lighten(base, 0.04),
        CartoonSegmentKind::Sheet => lighten(base, 0.12),
        CartoonSegmentKind::Coil => darken(base, 0.02),
    }
}

fn lerp_cartoon_style(start: CartoonStyle, end: CartoonStyle, t: f32) -> CartoonStyle {
    CartoonStyle {
        half_width: start.half_width + (end.half_width - start.half_width) * t,
        half_thickness: start.half_thickness + (end.half_thickness - start.half_thickness) * t,
    }
}

fn shade_cartoon_color(
    viewport: &Projector,
    base_color: Color32,
    surface_normal: Vector3<f32>,
    light_preset: LightPreset,
) -> Color32 {
    let view_normal = normalize_vector3(
        viewport.rotate_to_view(surface_normal),
        Vector3::new(0.0, 0.0, 1.0),
    );
    let light_direction =
        normalize_vector3(Vector3::new(-0.35, 0.45, 1.0), Vector3::new(0.0, 0.0, 1.0));
    let diffuse = view_normal.dot(&light_direction).max(0.0);
    let rim = (1.0 - view_normal.z.abs()).powi(2) * 0.18;
    let (ambient, diffuse_strength, rim_strength) = match light_preset {
        LightPreset::Soft => (0.38, 0.55, 1.0),
        LightPreset::Gentle => (0.48, 0.34, 0.65),
        LightPreset::Studio => (0.30, 0.72, 1.2),
    };
    let brightness = (ambient + diffuse * diffuse_strength + rim * rim_strength).clamp(0.0, 1.0);
    if brightness >= 0.5 {
        lighten(base_color, (brightness - 0.5) * 0.75)
    } else {
        darken(base_color, (0.5 - brightness) * 1.2)
    }
}

fn smoothstep(t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn align_orientation_hint(
    hint: Vector3<f32>,
    tangent: Vector3<f32>,
    fallback: Vector3<f32>,
) -> Vector3<f32> {
    let projected = hint - tangent * hint.dot(&tangent);
    normalize_vector3(projected, fallback)
}

fn transport_frame_vector(
    vector: Vector3<f32>,
    previous_tangent: Vector3<f32>,
    tangent: Vector3<f32>,
) -> Vector3<f32> {
    let axis = previous_tangent.cross(&tangent);
    let axis_norm_sq = axis.norm_squared();
    if axis_norm_sq <= 0.000001 {
        return vector;
    }

    let angle = previous_tangent.dot(&tangent).clamp(-1.0, 1.0).acos();
    if angle <= 0.0001 {
        return vector;
    }

    Rotation3::from_axis_angle(&Unit::new_normalize(axis), angle) * vector
}
