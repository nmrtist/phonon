use crate::{
    domain::{AtomCategory, Structure},
    frontend::{ViewportVisualState, state::AtomStyle},
};

/// Past this many solvent (water) atoms in an entry, every water is shown as
/// wireframe lines instead of ball-and-stick spheres. Thousands of full water
/// spheres are slow to tessellate every interaction frame; lines are cheap and
/// keep the box readable. The decision is made once, here, at the entry's first
/// load and baked into per-atom config (below), rather than re-derived from atom
/// counts inside the renderer.
const WATER_WIREFRAME_THRESHOLD: usize = 64;

/// Set per-entry view defaults when a structure is first shown, then persisted
/// with the entry. Most styles resolve lazily from the category tiers (software
/// default → project override → atom override) and are not materialized here.
/// The exception is bulk solvent: a heavily solvated entry bakes an explicit
/// per-atom wireframe style for each water, so the choice is stable config (not
/// a render-time heuristic) and a user can still promote individual waters — an
/// active-site water, say — back to a fuller representation.
pub fn apply_entry_render_defaults(viewport: &mut ViewportVisualState, structure: &Structure) {
    // Any periodic structure shows its box by default. This matters most for MD
    // systems (built or run output), where the simulation cell is essential
    // context; crystals/materials get it too.
    viewport.show_cell = structure.cell.is_some();
    apply_solvent_render_default(viewport, structure);
}

/// Bake the bulk-solvent display choice into per-atom config: past
/// [`WATER_WIREFRAME_THRESHOLD`] waters, every water becomes wireframe lines.
/// Split out from [`apply_entry_render_defaults`] so it can also migrate entries
/// saved before this default existed without disturbing their other view state
/// (e.g. the cell toggle).
pub fn apply_solvent_render_default(viewport: &mut ViewportVisualState, structure: &Structure) {
    let waters: Vec<(usize, AtomCategory)> = (0..structure.atoms.len())
        .map(|index| (index, structure.atom_category(index)))
        .filter(|(_, category)| *category == AtomCategory::Solvent)
        .collect();
    if waters.len() > WATER_WIREFRAME_THRESHOLD {
        viewport.apply_atom_styles(waters, AtomStyle::Wireframe);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Atom, PdbAtomAnnotation, build_biopolymer};
    use crate::frontend::state::AtomStyle;
    use nalgebra::Point3;

    fn atom(element: &str) -> Atom {
        Atom {
            element: element.to_string(),
            position: Point3::origin(),
            charge: 0.0,
        }
    }

    fn annotation(atom_name: &str, residue_name: &str, seq: i32) -> PdbAtomAnnotation {
        PdbAtomAnnotation {
            atom_name: atom_name.to_string(),
            residue_name: residue_name.to_string(),
            chain_id: 'A',
            residue_seq: seq,
            insertion_code: ' ',
        }
    }

    #[test]
    fn category_resolution_cartoons_protein_shows_solvent_spheres_ions() {
        // Protein CA, water O, and a sodium ion — no per-atom overrides stored.
        let annotations = vec![
            annotation("CA", "ALA", 1),
            annotation("OW", "SOL", 2),
            annotation("NA", "NA", 3),
        ];
        let biopolymer = build_biopolymer(&annotations, Vec::new()).expect("biopolymer");
        let structure = Structure {
            title: "t".to_string(),
            atoms: vec![atom("C"), atom("O"), atom("Na")],
            bonds: Vec::new(),
            cell: None,
            biopolymer: Some(biopolymer),
        };

        let mut viewport = ViewportVisualState::default();
        apply_entry_render_defaults(&mut viewport, &structure);

        // Styles come purely from the category tiers; nothing is materialized.
        assert!(viewport.atom_styles.is_empty());
        assert_eq!(
            viewport.resolved_atom_style(&structure, 0),
            AtomStyle::Cartoon
        );
        // Solvent is now shown by default (ball-and-stick), not auto-hidden.
        assert_eq!(
            viewport.resolved_atom_style(&structure, 1),
            AtomStyle::BallAndStick
        );
        assert_eq!(
            viewport.resolved_atom_style(&structure, 2),
            AtomStyle::Sphere
        );
    }

    /// One protein anchor (so a biopolymer exists and SOL classifies as Solvent)
    /// plus `water` SOL oxygens.
    fn solvated(water: usize) -> Structure {
        let mut atoms = vec![atom("C")];
        let mut annotations = vec![annotation("CA", "ALA", 1)];
        for i in 0..water {
            atoms.push(atom("O"));
            annotations.push(annotation("OW", "SOL", 2 + i as i32));
        }
        let biopolymer = build_biopolymer(&annotations, Vec::new()).expect("biopolymer");
        Structure {
            title: "s".to_string(),
            atoms,
            bonds: Vec::new(),
            cell: None,
            biopolymer: Some(biopolymer),
        }
    }

    #[test]
    fn heavily_solvated_entry_bakes_per_atom_wireframe_water() {
        let structure = solvated(WATER_WIREFRAME_THRESHOLD + 1);
        let mut viewport = ViewportVisualState::default();
        apply_entry_render_defaults(&mut viewport, &structure);

        // Every water (indices 1..) carries an explicit wireframe override, baked
        // in once at load rather than recomputed in the renderer.
        for index in 1..structure.atoms.len() {
            assert_eq!(
                viewport.atom_styles.get(&index),
                Some(&AtomStyle::Wireframe),
                "water atom {index} should be stored as wireframe"
            );
        }
        // The protein anchor is untouched (resolves to cartoon, no stored row).
        assert!(!viewport.atom_styles.contains_key(&0));
        assert_eq!(
            viewport.resolved_atom_style(&structure, 0),
            AtomStyle::Cartoon
        );

        // A user can still promote a specific (e.g. active-site) water back to a
        // fuller representation on top of the baked default.
        viewport.apply_atom_styles([(1usize, AtomCategory::Solvent)], AtomStyle::BallAndStick);
        assert_eq!(
            viewport.resolved_atom_style(&structure, 1),
            AtomStyle::BallAndStick
        );
        assert_eq!(
            viewport.resolved_atom_style(&structure, 2),
            AtomStyle::Wireframe
        );
    }

    #[test]
    fn lightly_solvated_entry_keeps_default_water() {
        // At the threshold (not above it), nothing is materialized; water resolves
        // to its category default.
        let structure = solvated(WATER_WIREFRAME_THRESHOLD);
        let mut viewport = ViewportVisualState::default();
        apply_entry_render_defaults(&mut viewport, &structure);
        assert!(viewport.atom_styles.is_empty());
        assert_eq!(
            viewport.resolved_atom_style(&structure, 1),
            AtomStyle::BallAndStick
        );
    }

    #[test]
    fn periodic_structure_shows_cell_by_default() {
        use crate::domain::UnitCell;

        let cell = UnitCell::from_parameters(10.0, 10.0, 10.0, 90.0, 90.0, 90.0);
        let structure = Structure {
            title: "md".to_string(),
            atoms: vec![atom("O")],
            bonds: Vec::new(),
            cell: Some(cell),
            biopolymer: None,
        };

        let mut viewport = ViewportVisualState {
            show_cell: false,
            ..Default::default()
        };
        apply_entry_render_defaults(&mut viewport, &structure);
        assert!(viewport.show_cell);
    }
}
