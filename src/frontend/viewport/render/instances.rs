//! Builds the camera-independent GPU instance set (sphere + cylinder) for the
//! ball-and-stick representation, reusing the same per-atom style/visibility/
//! color resolution and bond segmentation as the CPU path so the two stay
//! visually consistent.

use eframe::egui::Color32;
use nalgebra::{Point3, Vector3};

use crate::{
    domain::Structure,
    frontend::{AtomSelection, ViewportVisualState, state::AtomStyle},
};

use super::super::gpu::{CylinderInstance, MoleculeInstances, SphereInstance};
use super::ball_stick::build_atom_draw_table;
use super::scene::bond_world_segments;
use super::{BALL_RADIUS_SCALE, SINGLE_BOND_RADIUS, atom_ball_radius};

/// World radius (relative to the full ball-and-stick radius) used to draw atoms
/// whose style is the lightweight "dots" point representation. On the GPU these
/// become small shaded spheres rather than flat screen-space discs.
const POINT_SPHERE_SCALE: f32 = 0.5;

pub(crate) fn build_molecule_instances(
    structure: &Structure,
    selection: &AtomSelection,
    visual_state: &ViewportVisualState,
) -> MoleculeInstances {
    let atom_draw = build_atom_draw_table(structure, selection, visual_state);

    let mut spheres = Vec::new();
    for (index, draw) in atom_draw.iter().enumerate() {
        if !draw.visible {
            continue;
        }
        if let Some(radius) = sphere_radius(structure, index, draw.style, selection) {
            let position = structure.atoms[index].position;
            spheres.push(SphereInstance {
                pos_radius: [position.x, position.y, position.z, radius],
                color: draw.color.to_normalized_gamma_f32(),
            });
        }
    }

    let mut cylinders = Vec::new();
    for segment in bond_world_segments(structure) {
        let start = atom_draw[segment.start_atom];
        let end = atom_draw[segment.end_atom];
        if !(start.visible || end.visible) {
            continue;
        }
        if !(start.style.draws_stick_bonds() || end.style.draws_stick_bonds()) {
            continue;
        }
        if let Some(cylinder) =
            cylinder_instance(segment.start, segment.end, start.color, end.color)
        {
            cylinders.push(cylinder);
        }
    }

    MoleculeInstances { spheres, cylinders }
}

fn sphere_radius(
    structure: &Structure,
    index: usize,
    style: AtomStyle,
    selection: &AtomSelection,
) -> Option<f32> {
    let base = atom_ball_radius(&structure.atoms[index].element);
    let mut radius = if let Some(scale) = style.sphere_radius_scale() {
        base * (scale / BALL_RADIUS_SCALE)
    } else if style.draws_point() {
        base * POINT_SPHERE_SCALE
    } else {
        return None;
    };
    if selection.primary() == Some(index) {
        radius *= 1.18;
    } else if selection.contains(index) {
        radius *= 1.10;
    }
    Some(radius)
}

fn cylinder_instance(
    start: Point3<f32>,
    end: Point3<f32>,
    color_a: Color32,
    color_b: Color32,
) -> Option<CylinderInstance> {
    let axis_vector = end - start;
    let length = axis_vector.norm();
    let axis = axis_vector.try_normalize(1e-5)?;
    let (side_u, side_v) = perpendicular_basis(axis);
    Some(CylinderInstance {
        start_len: [start.x, start.y, start.z, length],
        axis_radius: [axis.x, axis.y, axis.z, SINGLE_BOND_RADIUS],
        side_u: [side_u.x, side_u.y, side_u.z, 0.0],
        side_v: [side_v.x, side_v.y, side_v.z, 0.0],
        color_a: color_a.to_normalized_gamma_f32(),
        color_b: color_b.to_normalized_gamma_f32(),
    })
}

fn perpendicular_basis(axis: Vector3<f32>) -> (Vector3<f32>, Vector3<f32>) {
    let reference = if axis.x.abs() < 0.9 {
        Vector3::new(1.0, 0.0, 0.0)
    } else {
        Vector3::new(0.0, 1.0, 0.0)
    };
    let u = axis.cross(&reference).normalize();
    let v = axis.cross(&u);
    (u, v)
}
