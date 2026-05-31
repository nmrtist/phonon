use crate::{domain::Structure, frontend::ViewportVisualState};

/// Set per-entry view defaults when a structure is first shown. Per-atom styles
/// are NOT materialized here — they resolve at render time from the category
/// tiers (software default → project override → atom override), so a protein
/// reads as cartoon and a ligand as ball-and-stick without storing any rows.
pub fn apply_entry_render_defaults(viewport: &mut ViewportVisualState, structure: &Structure) {
    // Any periodic structure shows its box by default. This matters most for MD
    // systems (built or run output), where the simulation cell is essential
    // context; crystals/materials get it too.
    viewport.show_cell = structure.cell.is_some();
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
