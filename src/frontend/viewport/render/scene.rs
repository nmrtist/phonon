use nalgebra::{Point3, Vector3};

use crate::domain::{BondType, Structure, UnitCell};

use super::super::camera::{Projected, Projector};
use super::{
    ViewportCache, ViewportCacheKey, ViewportVisualState, atom_screen_radius, atom_visible,
};

#[derive(Clone)]
pub(crate) struct ViewportGeometry {
    pub(crate) atoms: Vec<RenderedAtom>,
    pub(crate) bonds: Vec<RenderedBondSegment>,
}

#[derive(Clone, Copy)]
pub(crate) struct RenderedAtom {
    pub(crate) depth: f32,
    pub(crate) index: usize,
    pub(crate) pos: eframe::egui::Pos2,
    pub(crate) scale: f32,
}

pub(crate) type PickTarget = RenderedAtom;

#[derive(Clone)]
pub(crate) struct RenderedBondSegment {
    pub(super) depth: f32,
    pub(super) a: usize,
    pub(super) b: usize,
    pub(super) start: Point3<f32>,
    pub(super) end: Point3<f32>,
    pub(super) bond_type: BondType,
    pub(super) aromatic_center: Option<Point3<f32>>,
}

pub(crate) fn cached_geometry<'a>(
    cache: &'a mut ViewportCache,
    key: ViewportCacheKey,
    structure: &Structure,
    viewport: &Projector,
) -> &'a ViewportGeometry {
    if cache.key.as_ref() != Some(&key) {
        cache.geometry = Some(build_viewport_geometry(structure, viewport));
        cache.key = Some(key);
    }

    cache
        .geometry
        .as_ref()
        .expect("viewport cache geometry must be initialized")
}

pub(crate) fn build_viewport_geometry(
    structure: &Structure,
    viewport: &Projector,
) -> ViewportGeometry {
    let projected_atoms = structure
        .atoms
        .iter()
        .map(|atom| viewport.project(atom.position))
        .collect::<Vec<_>>();
    let aromatic_centers = aromatic_system_centers(structure);
    let mut bonds =
        projected_bond_segments(structure, viewport, &projected_atoms, &aromatic_centers);
    bonds.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    let mut atoms = projected_atoms
        .iter()
        .enumerate()
        .map(|(index, projected)| RenderedAtom {
            depth: projected.depth,
            index,
            pos: projected.pos,
            scale: projected.scale,
        })
        .collect::<Vec<_>>();
    atoms.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    ViewportGeometry { atoms, bonds }
}

pub(crate) fn pick_atom(
    structure: &Structure,
    projected_atoms: &[PickTarget],
    pointer: eframe::egui::Pos2,
    visual_state: &ViewportVisualState,
) -> Option<usize> {
    projected_atoms
        .iter()
        .rev()
        .filter_map(|atom| {
            if !atom_visible(structure, visual_state, atom.index) {
                return None;
            }
            let style =
                crate::domain::chemistry::element_style(&structure.atoms[atom.index].element);
            let radius = atom_screen_radius(style.display_radius, 1.0, atom.scale) + 5.0;
            let distance = atom.pos.distance(pointer);

            (distance <= radius).then_some((atom.index, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

fn projected_bond_segments(
    structure: &Structure,
    viewport: &Projector,
    projected_atoms: &[Projected],
    aromatic_centers: &[Option<Point3<f32>>],
) -> Vec<RenderedBondSegment> {
    let mut segments = Vec::new();

    for (bond_index, bond) in structure.bonds.iter().enumerate() {
        let start_atom = &structure.atoms[bond.a];
        let end_atom = &structure.atoms[bond.b];
        let projected_start = &projected_atoms[bond.a];
        let projected_end = &projected_atoms[bond.b];

        if let Some(cell) = &structure.cell {
            let (delta, crosses_boundary) =
                periodic_bond_delta(cell, start_atom.position, end_atom.position);
            if crosses_boundary {
                let start_midpoint =
                    viewport.project(Point3::from(start_atom.position.coords + delta * 0.5));
                let end_midpoint =
                    viewport.project(Point3::from(end_atom.position.coords - delta * 0.5));

                segments.push(RenderedBondSegment {
                    depth: (projected_start.depth + start_midpoint.depth) * 0.5,
                    a: bond.a,
                    b: bond.b,
                    start: start_atom.position,
                    end: Point3::from(start_atom.position.coords + delta * 0.5),
                    bond_type: bond.bond_type,
                    aromatic_center: aromatic_centers[bond_index],
                });
                segments.push(RenderedBondSegment {
                    depth: (projected_end.depth + end_midpoint.depth) * 0.5,
                    a: bond.a,
                    b: bond.b,
                    start: end_atom.position,
                    end: Point3::from(end_atom.position.coords - delta * 0.5),
                    bond_type: bond.bond_type,
                    aromatic_center: aromatic_centers[bond_index],
                });
                continue;
            }
        }

        segments.push(RenderedBondSegment {
            depth: (projected_start.depth + projected_end.depth) * 0.5,
            a: bond.a,
            b: bond.b,
            start: start_atom.position,
            end: end_atom.position,
            bond_type: bond.bond_type,
            aromatic_center: aromatic_centers[bond_index],
        });
    }

    segments
}

fn periodic_bond_delta(
    cell: &UnitCell,
    first: Point3<f32>,
    second: Point3<f32>,
) -> (Vector3<f32>, bool) {
    let first_frac = cell.cartesian_to_fractional(first);
    let second_frac = cell.cartesian_to_fractional(second);
    let mut delta = second_frac - first_frac;
    let shift = Vector3::new(delta.x.round(), delta.y.round(), delta.z.round());
    let crosses_boundary =
        shift.x.abs() > 0.0001 || shift.y.abs() > 0.0001 || shift.z.abs() > 0.0001;

    delta -= shift;

    (
        cell.vectors[0] * delta.x + cell.vectors[1] * delta.y + cell.vectors[2] * delta.z,
        crosses_boundary,
    )
}

fn aromatic_system_centers(structure: &Structure) -> Vec<Option<Point3<f32>>> {
    let mut aromatic_neighbors = vec![Vec::new(); structure.atoms.len()];
    for bond in structure
        .bonds
        .iter()
        .filter(|bond| bond.bond_type == BondType::Aromatic)
    {
        aromatic_neighbors[bond.a].push(bond.b);
        aromatic_neighbors[bond.b].push(bond.a);
    }

    let mut component_for_atom = vec![None; structure.atoms.len()];
    let mut component_centers = Vec::new();

    for atom_index in 0..structure.atoms.len() {
        if aromatic_neighbors[atom_index].is_empty() || component_for_atom[atom_index].is_some() {
            continue;
        }

        let mut stack = vec![atom_index];
        let mut component_atoms = Vec::new();
        component_for_atom[atom_index] = Some(component_centers.len());

        while let Some(current) = stack.pop() {
            component_atoms.push(current);
            for &neighbor in &aromatic_neighbors[current] {
                if component_for_atom[neighbor].is_none() {
                    component_for_atom[neighbor] = Some(component_centers.len());
                    stack.push(neighbor);
                }
            }
        }

        let sum = component_atoms
            .iter()
            .fold(Vector3::zeros(), |acc, &index| {
                acc + structure.atoms[index].position.coords
            });
        component_centers.push(Point3::from(sum / component_atoms.len() as f32));
    }

    structure
        .bonds
        .iter()
        .map(|bond| {
            if bond.bond_type != BondType::Aromatic {
                None
            } else {
                component_for_atom[bond.a].map(|component_index| component_centers[component_index])
            }
        })
        .collect()
}
