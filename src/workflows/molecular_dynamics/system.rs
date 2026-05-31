//! Build a periodic MD simulation box around a non-periodic structure.
//!
//! Molecular-dynamics engines need a simulation cell. Structures the user drew
//! or imported from formats without a box have `cell == None`. This pure-domain
//! transform wraps such a structure in a periodic cell, centering the molecule
//! inside it.
//!
//! Two concerns are kept orthogonal:
//!
//! * [`BoxSizing`] — how the cell's per-axis edge lengths are chosen: a vacuum
//!   [`BoxSizing::Padding`] margin around the molecule, or an [`BoxSizing::Absolute`]
//!   set of edge lengths independent of the molecule's extent.
//! * [`BoxShape`] — the lattice geometry the edges map onto: the orthorhombic
//!   family ([`BoxShape::Orthorhombic`], [`BoxShape::Cubic`]) and two
//!   space-filling triclinic shapes
//!   ([`BoxShape::RhombicDodecahedron`], [`BoxShape::TruncatedOctahedron`]).
//!
//! Units are angstroms throughout (`Atom.position` and `UnitCell` are in Å; an
//! engine adapter converts Å→its own units). This module knows nothing about any
//! engine — it only emits a [`Structure`] with a cell.

use anyhow::{Result, bail};
use nalgebra::Point3;

use crate::domain::{Atom, Structure, UnitCell};

/// Padding to leave between the molecule's bounding box and the cell walls, in
/// angstroms. The default of 10 Å (= 1.0 nm) comfortably exceeds the typical
/// 1.0 nm nonbonded cutoff, so a later EM run does not error with "cutoff longer
/// than box".
pub const DEFAULT_PADDING_ANGSTROM: f32 = 10.0;

/// How the simulation cell's per-axis edge lengths are determined.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoxSizing {
    /// Edge along each axis = molecule extent on that axis + 2·padding (a vacuum
    /// margin on each side). Always encloses the molecule.
    Padding { padding_angstrom: [f32; 3] },
    /// Explicit edge lengths in angstroms, independent of the molecule's extent.
    /// The build fails if the molecule does not fit inside the requested box.
    Absolute { edges_angstrom: [f32; 3] },
}

impl Default for BoxSizing {
    fn default() -> Self {
        Self::Padding {
            padding_angstrom: [DEFAULT_PADDING_ANGSTROM; 3],
        }
    }
}

/// Lattice geometry of the periodic simulation cell.
///
/// The orthorhombic family ([`BoxShape::Orthorhombic`], [`BoxShape::Cubic`])
/// plus two space-filling triclinic shapes
/// ([`BoxShape::RhombicDodecahedron`], [`BoxShape::TruncatedOctahedron`]) are
/// implemented. The space-filling shapes enclose a solute in less volume than a
/// cube of the same periodic-image distance, so they need fewer solvent
/// molecules. The enum is `#[non_exhaustive]` to leave room for further
/// geometries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum BoxShape {
    /// Rectangular cuboid: three independent edge lengths, all angles 90°.
    #[default]
    Orthorhombic,
    /// Cube: the largest required edge applied uniformly to all three axes.
    Cubic,
    /// Rhombic dodecahedron (≈70.7% of a cube's volume for the same image
    /// distance).
    RhombicDodecahedron,
    /// Truncated octahedron (≈77.0% of a cube's volume).
    TruncatedOctahedron,
}

impl BoxShape {
    /// Human-readable label for UI menus.
    pub fn label(self) -> &'static str {
        match self {
            Self::Orthorhombic => "Orthorhombic",
            Self::Cubic => "Cubic",
            Self::RhombicDodecahedron => "Rhombic dodecahedron",
            Self::TruncatedOctahedron => "Truncated octahedron",
        }
    }

    /// Whether this shape is a single-length, space-filling triclinic cell whose
    /// three lattice vectors all share one defining length.
    pub fn is_space_filling(self) -> bool {
        matches!(self, Self::RhombicDodecahedron | Self::TruncatedOctahedron)
    }

    /// All shapes selectable in the UI.
    pub fn selectable() -> &'static [BoxShape] {
        &[
            Self::Orthorhombic,
            Self::Cubic,
            Self::RhombicDodecahedron,
            Self::TruncatedOctahedron,
        ]
    }
}

/// Configuration for [`build_md_system`]: the sizing strategy and the lattice
/// geometry the resulting edges map onto.
#[derive(Debug, Clone, Copy, Default)]
pub struct MdSystemConfig {
    pub sizing: BoxSizing,
    pub shape: BoxShape,
}

impl MdSystemConfig {
    /// Uniform vacuum padding around the molecule on every axis.
    pub fn with_uniform_padding(padding_angstrom: f32, shape: BoxShape) -> Self {
        Self {
            sizing: BoxSizing::Padding {
                padding_angstrom: [padding_angstrom; 3],
            },
            shape,
        }
    }

    /// Explicit per-axis edge lengths.
    pub fn with_absolute_edges(edges_angstrom: [f32; 3], shape: BoxShape) -> Self {
        Self {
            sizing: BoxSizing::Absolute { edges_angstrom },
            shape,
        }
    }
}

/// Summary of the cell that [`build_md_system`] produces, returned alongside the
/// structure so callers can report the box dimensions.
#[derive(Debug, Clone, Copy)]
pub struct MdSystemReport {
    /// Resulting cell edge lengths (a, b, c) in angstroms.
    pub edges_angstrom: [f32; 3],
    /// Whether the input structure already had a cell that was replaced.
    pub replaced_existing_cell: bool,
}

/// Non-mutating preview of the cell [`build_md_system`] would produce, for live
/// UI readouts before the user commits.
#[derive(Debug, Clone, Copy)]
pub struct MdSystemPreview {
    /// Resulting cell edge lengths (a, b, c) in angstroms.
    pub edges_angstrom: [f32; 3],
    /// Whether the molecule fits inside the box on every axis. Always true for
    /// [`BoxSizing::Padding`]; can be false for [`BoxSizing::Absolute`].
    pub fits: bool,
}

/// Compute the resulting cell for a structure without mutating it. Returns
/// `None` if the structure has no atoms.
pub fn preview(structure: &Structure, config: &MdSystemConfig) -> Option<MdSystemPreview> {
    let (min, max) = bounding_box(&structure.atoms)?;
    let extents = extents(min, max);
    let edges_angstrom = resolve_edges(config, extents);
    let fits = (0..3).all(|i| edges_angstrom[i] >= extents[i] - 1e-4);
    Some(MdSystemPreview {
        edges_angstrom,
        fits,
    })
}

/// Convenience wrapper returning only the resolved edge lengths.
pub fn preview_edges(structure: &Structure, config: &MdSystemConfig) -> Option<[f32; 3]> {
    preview(structure, config).map(|p| p.edges_angstrom)
}

/// Wrap a structure in a periodic simulation cell.
///
/// The molecule is translated so it sits centered in the new box. Bonds,
/// charges, elements, biopolymer metadata, and the title are preserved
/// unchanged. An existing cell is replaced.
pub fn build_md_system(
    structure: &Structure,
    config: &MdSystemConfig,
) -> Result<(Structure, MdSystemReport)> {
    let Some((min, max)) = bounding_box(&structure.atoms) else {
        bail!("cannot build an MD system from an empty structure");
    };

    let extents = extents(min, max);
    let edges = resolve_edges(config, extents);

    // An absolute box smaller than the molecule would leave atoms outside the
    // cell (and clashing with their periodic images), so reject it up front.
    for axis in 0..3 {
        if edges[axis] + 1e-4 < extents[axis] {
            let name = ["X", "Y", "Z"][axis];
            bail!(
                "requested box edge on {name} ({:.2} A) is smaller than the molecule's extent ({:.2} A)",
                edges[axis],
                extents[axis]
            );
        }
    }

    // Build the lattice vectors for the chosen shape, then center the molecule
    // by mapping its bounding-box center onto the cell center (½·(a+b+c)). For
    // the orthorhombic family this reduces to the per-axis margin formula; for
    // the triclinic space-filling shapes it places the solute at the geometric
    // center of the (non-axis-aligned) cell.
    let vectors = cell_vectors(config.shape, edges);
    let cell_center = (vectors[0] + vectors[1] + vectors[2]) * 0.5;
    let bbox_center = nalgebra::Vector3::new(
        (min.x + max.x) * 0.5,
        (min.y + max.y) * 0.5,
        (min.z + max.z) * 0.5,
    );
    let offset = cell_center - bbox_center;

    let atoms = structure
        .atoms
        .iter()
        .map(|atom| Atom {
            element: atom.element.clone(),
            position: Point3::new(
                atom.position.x + offset.x,
                atom.position.y + offset.y,
                atom.position.z + offset.z,
            ),
            charge: atom.charge,
        })
        .collect();

    let cell = UnitCell::from_vectors(vectors);

    let report = MdSystemReport {
        edges_angstrom: edges,
        replaced_existing_cell: structure.cell.is_some(),
    };

    let boxed = Structure {
        title: structure.title.clone(),
        atoms,
        bonds: structure.bonds.clone(),
        cell: Some(cell),
        biopolymer: structure.biopolymer.clone(),
    };

    Ok((boxed, report))
}

/// Axis-aligned bounding box of the atom positions, or `None` if there are no
/// atoms.
fn bounding_box(atoms: &[Atom]) -> Option<(Point3<f32>, Point3<f32>)> {
    let first = atoms.first()?.position;
    let mut min = first;
    let mut max = first;

    for atom in &atoms[1..] {
        let p = atom.position;
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        min.z = min.z.min(p.z);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
        max.z = max.z.max(p.z);
    }

    Some((min, max))
}

fn extents(min: Point3<f32>, max: Point3<f32>) -> [f32; 3] {
    [max.x - min.x, max.y - min.y, max.z - min.z]
}

/// Resolve final cell edges from the sizing strategy and the lattice shape.
fn resolve_edges(config: &MdSystemConfig, extents: [f32; 3]) -> [f32; 3] {
    let mut edges = match config.sizing {
        BoxSizing::Padding { padding_angstrom } => [
            extents[0] + 2.0 * padding_angstrom[0],
            extents[1] + 2.0 * padding_angstrom[1],
            extents[2] + 2.0 * padding_angstrom[2],
        ],
        BoxSizing::Absolute { edges_angstrom } => edges_angstrom,
    };

    match config.shape {
        BoxShape::Orthorhombic => {}
        // Cubic and the space-filling shapes are defined by a single length:
        // the largest required edge, applied uniformly.
        BoxShape::Cubic | BoxShape::RhombicDodecahedron | BoxShape::TruncatedOctahedron => {
            let edge = edges[0].max(edges[1]).max(edges[2]);
            edges = [edge; 3];
        }
    }

    edges
}

/// Lattice vectors (angstroms) for a shape given its characteristic edges.
///
/// For the orthorhombic family the vectors are simply axis-aligned. The
/// space-filling shapes use a single defining length `d = edges[0]` (all three
/// `edges` are equal after [`resolve_edges`]) and the standard lower-triangular
/// box vectors, so the cell's periodic-image distance is `d` while its volume is
/// well below a cube of edge `d`.
fn cell_vectors(shape: BoxShape, edges: [f32; 3]) -> [nalgebra::Vector3<f32>; 3] {
    use nalgebra::Vector3;
    use std::f32::consts::SQRT_2;

    match shape {
        BoxShape::Orthorhombic | BoxShape::Cubic => [
            Vector3::new(edges[0], 0.0, 0.0),
            Vector3::new(0.0, edges[1], 0.0),
            Vector3::new(0.0, 0.0, edges[2]),
        ],
        BoxShape::RhombicDodecahedron => {
            let d = edges[0];
            [
                Vector3::new(d, 0.0, 0.0),
                Vector3::new(0.0, d, 0.0),
                Vector3::new(d * 0.5, d * 0.5, d * SQRT_2 * 0.5),
            ]
        }
        BoxShape::TruncatedOctahedron => {
            let d = edges[0];
            let sqrt6 = 6.0_f32.sqrt();
            [
                Vector3::new(d, 0.0, 0.0),
                Vector3::new(d / 3.0, 2.0 * SQRT_2 / 3.0 * d, 0.0),
                Vector3::new(-d / 3.0, SQRT_2 / 3.0 * d, sqrt6 / 3.0 * d),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Bond, BondType};

    fn atom(element: &str, x: f32, y: f32, z: f32) -> Atom {
        Atom {
            element: element.to_string(),
            position: Point3::new(x, y, z),
            charge: 0.0,
        }
    }

    fn diatomic() -> Structure {
        Structure::with_bonds(
            "diatomic",
            vec![atom("C", 0.0, 0.0, 0.0), atom("O", 2.0, 4.0, 6.0)],
            vec![Bond::with_type(0, 1, BondType::Double)],
        )
    }

    #[test]
    fn empty_structure_errors() {
        let empty = Structure::empty();
        assert!(build_md_system(&empty, &MdSystemConfig::default()).is_err());
    }

    #[test]
    fn produces_a_cell() {
        let (boxed, report) = build_md_system(&diatomic(), &MdSystemConfig::default()).unwrap();
        assert!(boxed.cell.is_some());
        assert!(!report.replaced_existing_cell);
    }

    #[test]
    fn atoms_lie_inside_box_with_padding_margin() {
        let config = MdSystemConfig::with_uniform_padding(10.0, BoxShape::Orthorhombic);
        let (boxed, _) = build_md_system(&diatomic(), &config).unwrap();
        let cell = boxed.cell.as_ref().unwrap();

        for atom in &boxed.atoms {
            let p = atom.position;
            assert!(p.x >= 0.0 && p.x <= cell.a, "x={} a={}", p.x, cell.a);
            assert!(p.y >= 0.0 && p.y <= cell.b, "y={} b={}", p.y, cell.b);
            assert!(p.z >= 0.0 && p.z <= cell.c, "z={} c={}", p.z, cell.c);
        }

        let min_x = boxed
            .atoms
            .iter()
            .map(|a| a.position.x)
            .fold(f32::INFINITY, f32::min);
        let max_x = boxed
            .atoms
            .iter()
            .map(|a| a.position.x)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((min_x - 10.0).abs() < 1e-3, "min margin {min_x}");
        assert!((cell.a - max_x - 10.0).abs() < 1e-3, "max margin");
    }

    #[test]
    fn preserves_atoms_bonds_and_charges() {
        let mut source = diatomic();
        source.atoms[0].charge = -0.5;
        source.atoms[1].charge = 0.5;

        let (boxed, _) = build_md_system(&source, &MdSystemConfig::default()).unwrap();

        assert_eq!(boxed.atoms.len(), source.atoms.len());
        assert_eq!(boxed.bonds.len(), source.bonds.len());
        assert_eq!(boxed.bonds[0].bond_type, BondType::Double);
        assert_eq!(boxed.title, "diatomic");
        assert_eq!(boxed.atoms[0].element, "C");
        assert_eq!(boxed.atoms[1].element, "O");
        assert!((boxed.atoms[0].charge + 0.5).abs() < 1e-6);
        assert!((boxed.atoms[1].charge - 0.5).abs() < 1e-6);
    }

    #[test]
    fn extent_drives_edge_length_for_padding() {
        // Extent is 2, 4, 6 in x, y, z; with 10 Å padding edges are 22, 24, 26.
        let config = MdSystemConfig::with_uniform_padding(10.0, BoxShape::Orthorhombic);
        let (boxed, report) = build_md_system(&diatomic(), &config).unwrap();
        let cell = boxed.cell.as_ref().unwrap();
        assert!((cell.a - 22.0).abs() < 1e-3);
        assert!((cell.b - 24.0).abs() < 1e-3);
        assert!((cell.c - 26.0).abs() < 1e-3);
        assert!((report.edges_angstrom[2] - 26.0).abs() < 1e-3);
    }

    #[test]
    fn cubic_padding_yields_equal_edges() {
        let config = MdSystemConfig::with_uniform_padding(10.0, BoxShape::Cubic);
        let (boxed, _) = build_md_system(&diatomic(), &config).unwrap();
        let cell = boxed.cell.as_ref().unwrap();
        // Largest required edge is the z axis: 6 + 20 = 26.
        assert!((cell.a - 26.0).abs() < 1e-3);
        assert!((cell.a - cell.b).abs() < 1e-3);
        assert!((cell.b - cell.c).abs() < 1e-3);
    }

    #[test]
    fn absolute_orthorhombic_uses_requested_edges() {
        let config =
            MdSystemConfig::with_absolute_edges([30.0, 40.0, 50.0], BoxShape::Orthorhombic);
        let (boxed, report) = build_md_system(&diatomic(), &config).unwrap();
        let cell = boxed.cell.as_ref().unwrap();
        assert!((cell.a - 30.0).abs() < 1e-3);
        assert!((cell.b - 40.0).abs() < 1e-3);
        assert!((cell.c - 50.0).abs() < 1e-3);
        assert_eq!(report.edges_angstrom, [30.0, 40.0, 50.0]);

        // Molecule is centered: bbox min sits at (edge - extent)/2 on each axis.
        let min_x = boxed
            .atoms
            .iter()
            .map(|a| a.position.x)
            .fold(f32::INFINITY, f32::min);
        assert!((min_x - (30.0 - 2.0) / 2.0).abs() < 1e-3);
    }

    #[test]
    fn absolute_cubic_takes_largest_requested_edge() {
        let config = MdSystemConfig::with_absolute_edges([30.0, 40.0, 25.0], BoxShape::Cubic);
        let (boxed, _) = build_md_system(&diatomic(), &config).unwrap();
        let cell = boxed.cell.as_ref().unwrap();
        assert!((cell.a - 40.0).abs() < 1e-3);
        assert!((cell.b - 40.0).abs() < 1e-3);
        assert!((cell.c - 40.0).abs() < 1e-3);
    }

    #[test]
    fn absolute_box_smaller_than_molecule_errors() {
        // Molecule extent on z is 6 Å; a 5 Å box cannot contain it.
        let config = MdSystemConfig::with_absolute_edges([30.0, 30.0, 5.0], BoxShape::Orthorhombic);
        assert!(build_md_system(&diatomic(), &config).is_err());
    }

    #[test]
    fn single_atom_gives_box_of_twice_padding() {
        let single = Structure::new("single", vec![atom("Ar", 5.0, 5.0, 5.0)]);
        let config = MdSystemConfig::with_uniform_padding(10.0, BoxShape::Orthorhombic);
        let (boxed, _) = build_md_system(&single, &config).unwrap();
        let cell = boxed.cell.as_ref().unwrap();
        assert!((cell.a - 20.0).abs() < 1e-3);
        assert!((cell.b - 20.0).abs() < 1e-3);
        assert!((cell.c - 20.0).abs() < 1e-3);
        assert!((boxed.atoms[0].position.x - 10.0).abs() < 1e-3);
    }

    #[test]
    fn default_padding_gives_box_over_twenty_angstrom() {
        let (boxed, _) = build_md_system(&diatomic(), &MdSystemConfig::default()).unwrap();
        let cell = boxed.cell.as_ref().unwrap();
        assert!(cell.a > 20.0 && cell.b > 20.0 && cell.c > 20.0);
    }

    #[test]
    fn replaces_existing_cell() {
        let cell = UnitCell::from_parameters(5.0, 5.0, 5.0, 90.0, 90.0, 90.0);
        let source = Structure::with_cell("boxed", vec![atom("C", 1.0, 1.0, 1.0)], cell);
        let (boxed, report) = build_md_system(&source, &MdSystemConfig::default()).unwrap();
        assert!(report.replaced_existing_cell);
        assert!((boxed.cell.as_ref().unwrap().a - 20.0).abs() < 1e-3);
    }

    fn cell_volume(cell: &UnitCell) -> f32 {
        let [a, b, c] = cell.vectors;
        a.dot(&b.cross(&c)).abs()
    }

    #[test]
    fn dodecahedron_produces_triclinic_cell() {
        let config = MdSystemConfig::with_uniform_padding(10.0, BoxShape::RhombicDodecahedron);
        let (boxed, _) = build_md_system(&diatomic(), &config).unwrap();
        let cell = boxed.cell.as_ref().unwrap();

        // A genuine triclinic cell: at least one angle departs from 90°.
        let off_90 = (cell.alpha - 90.0).abs() > 1.0
            || (cell.beta - 90.0).abs() > 1.0
            || (cell.gamma - 90.0).abs() > 1.0;
        assert!(off_90, "expected non-orthorhombic angles, got {cell:?}");

        // All three lattice vectors share the defining length d (= cubic edge).
        let d = 6.0 + 2.0 * 10.0; // max extent (z=6) + 2·padding
        for v in cell.vectors {
            assert!(
                (v.norm() - d).abs() < 1e-2,
                "vector norm {} != {d}",
                v.norm()
            );
        }
    }

    #[test]
    fn dodecahedron_volume_is_smaller_than_cubic_for_same_clearance() {
        let dodeca = MdSystemConfig::with_uniform_padding(10.0, BoxShape::RhombicDodecahedron);
        let cubic = MdSystemConfig::with_uniform_padding(10.0, BoxShape::Cubic);
        let (dodeca_box, _) = build_md_system(&diatomic(), &dodeca).unwrap();
        let (cubic_box, _) = build_md_system(&diatomic(), &cubic).unwrap();

        let v_dodeca = cell_volume(dodeca_box.cell.as_ref().unwrap());
        let v_cubic = cell_volume(cubic_box.cell.as_ref().unwrap());

        // Rhombic dodecahedron is √2/2 ≈ 0.707 of the cube's volume.
        let ratio = v_dodeca / v_cubic;
        assert!(
            (ratio - std::f32::consts::SQRT_2 / 2.0).abs() < 1e-2,
            "volume ratio {ratio} should be ~0.707"
        );
    }

    #[test]
    fn truncated_octahedron_volume_ratio() {
        let oct = MdSystemConfig::with_uniform_padding(10.0, BoxShape::TruncatedOctahedron);
        let cubic = MdSystemConfig::with_uniform_padding(10.0, BoxShape::Cubic);
        let (oct_box, _) = build_md_system(&diatomic(), &oct).unwrap();
        let (cubic_box, _) = build_md_system(&diatomic(), &cubic).unwrap();

        let ratio = cell_volume(oct_box.cell.as_ref().unwrap())
            / cell_volume(cubic_box.cell.as_ref().unwrap());
        // Truncated octahedron is ≈0.7698 of the cube's volume.
        assert!(
            (ratio - 0.7698).abs() < 1e-2,
            "volume ratio {ratio} should be ~0.77"
        );
    }

    #[test]
    fn dodecahedron_gro_round_trips_box_vectors() {
        use crate::engines::gromacs::input::to_gro;
        use crate::io::formats::gro::parse_gro;

        let config = MdSystemConfig::with_uniform_padding(10.0, BoxShape::RhombicDodecahedron);
        let (boxed, _) = build_md_system(&diatomic(), &config).unwrap();

        let gro = to_gro(&boxed, "dodeca").expect("serialized");
        let box_fields = gro
            .lines()
            .last()
            .expect("box line")
            .split_whitespace()
            .count();
        assert_eq!(
            box_fields, 9,
            "triclinic cell must use the nine-field box form"
        );

        let reparsed = parse_gro(&gro).expect("round-trip parse");
        let original = boxed.cell.as_ref().unwrap().vectors;
        let restored = reparsed.cell.as_ref().unwrap().vectors;
        for (o, r) in original.iter().zip(restored.iter()) {
            assert!((o.x - r.x).abs() < 1e-2, "x mismatch {o:?} vs {r:?}");
            assert!((o.y - r.y).abs() < 1e-2, "y mismatch {o:?} vs {r:?}");
            assert!((o.z - r.z).abs() < 1e-2, "z mismatch {o:?} vs {r:?}");
        }
    }

    #[test]
    fn preview_flags_when_molecule_does_not_fit() {
        let too_small =
            MdSystemConfig::with_absolute_edges([5.0, 5.0, 5.0], BoxShape::Orthorhombic);
        let p = preview(&diatomic(), &too_small).unwrap();
        assert!(!p.fits);

        let roomy = MdSystemConfig::with_absolute_edges([30.0, 30.0, 30.0], BoxShape::Orthorhombic);
        assert!(preview(&diatomic(), &roomy).unwrap().fits);

        // Padding always fits.
        assert!(
            preview(&diatomic(), &MdSystemConfig::default())
                .unwrap()
                .fits
        );
    }
}
