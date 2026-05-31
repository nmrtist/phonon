//! Render an engine-neutral [`MdTopology`] into a GROMACS `.top`.
//!
//! This is the only place GROMACS-specific topology syntax lives. It holds no
//! chemical knowledge of its own: every parameter (masses, Lennard-Jones
//! sigma/epsilon, molecule composition) comes from the neutral
//! [`MdTopology`], which is built once at system-build time and reused across
//! engines. Other MD engines add their own renderer alongside this one,
//! consuming the same [`MdTopology`].

use crate::workflows::molecular_dynamics::{BondedParam, BondedTerm, MdTopology, MoleculeType};

/// Angstrom -> nanometer (GROMACS uses nm for Lennard-Jones sigma and SETTLE
/// geometry).
const ANGSTROM_TO_NM: f32 = 0.1;

/// Render `topology` as a complete, self-contained GROMACS `.top`. Handles
/// monatomic species, rigid (SETTLE) water, and monatomic ions, as well as a
/// bonded biopolymer molecule type (bonds/angles/dihedrals/impropers) whose
/// parameters grompp resolves from the emitted force-field directives.
pub fn render_top(topology: &MdTopology) -> String {
    let mut top = String::new();
    top.push_str("; Phonon-generated topology\n\n");

    top.push_str("[ defaults ]\n");
    top.push_str("; nbfunc  comb-rule  gen-pairs  fudgeLJ  fudgeQQ\n");
    match &topology.defaults {
        Some(d) => top.push_str(&format!(
            "1         {}          {}        {}      {}\n\n",
            d.comb_rule,
            if d.gen_pairs { "yes" } else { "no" },
            d.fudge_lj,
            d.fudge_qq,
        )),
        None => top.push_str("1         2          no         1.0      1.0\n\n"),
    }

    top.push_str("[ atomtypes ]\n");
    top.push_str("; name  at.num  mass        charge  ptype  sigma      epsilon\n");
    for s in &topology.species {
        top.push_str(&format!(
            "  {:<4}  {:<6}  {:<10}  {:<6}  {:<5}  {:<9}  {}\n",
            s.element,
            s.atomic_number,
            s.mass_u,
            format!("{:.3}", s.charge),
            "A",
            s.sigma_angstrom * ANGSTROM_TO_NM,
            s.epsilon_kj_mol,
        ));
    }
    top.push('\n');

    // Force-field parameter directives (only present for bonded systems); grompp
    // matches the index-only bonded terms below against these.
    render_param_directives(&mut top, &topology.bonded_params);

    for mol in &topology.molecules {
        top.push_str("[ moleculetype ]\n");
        top.push_str("; name  nrexcl\n");
        top.push_str(&format!("  {:<6}  {}\n\n", mol.name, mol.nrexcl));

        top.push_str("[ atoms ]\n");
        top.push_str("; nr  type  resnr  residue  atom  cgnr  charge  mass\n");
        for (i, atom) in mol.atoms.iter().enumerate() {
            let mass = topology
                .species
                .iter()
                .find(|s| s.element == atom.species)
                .map_or(0.0, |s| s.mass_u);
            let resnr = atom.residue_number.unwrap_or(1);
            let residue = atom.residue_name.as_deref().unwrap_or(&mol.name);
            top.push_str(&format!(
                "  {:<3}  {:<6}  {:<4}  {:<6}  {:<4}  {:<3}  {:>7.4}  {}\n",
                i + 1,
                atom.species,
                resnr,
                residue,
                atom.atom_name,
                i + 1,
                atom.charge,
                mass,
            ));
        }
        top.push('\n');

        render_bonded_sections(&mut top, mol);

        // Rigid three-site water: a SETTLE constraint plus full OW–HW exclusions.
        if let Some(settle) = &mol.settle {
            top.push_str("[ settles ]\n");
            top.push_str("; OW  funct  doh        dhh\n");
            top.push_str(&format!(
                "  1    1      {:.5}    {:.5}\n\n",
                settle.doh_angstrom * ANGSTROM_TO_NM,
                settle.dhh_angstrom * ANGSTROM_TO_NM
            ));
            top.push_str("[ exclusions ]\n");
            top.push_str("  1   2   3\n");
            top.push_str("  2   1   3\n");
            top.push_str("  3   1   2\n\n");
        }
    }

    top.push_str("[ system ]\n");
    top.push_str(&topology.title);
    top.push_str("\n\n");

    top.push_str("[ molecules ]\n");
    top.push_str("; name  count\n");
    for run in &topology.composition {
        top.push_str(&format!("{:<6}  {}\n", run.molecule, run.count));
    }

    top
}

/// Emit the force-field parameter directives (`[bondtypes]`, `[pairtypes]`,
/// `[angletypes]`, `[dihedraltypes]`, `[constrainttypes]`), grouped by kind in
/// the order grompp expects, so the index-only bonded terms can be resolved.
fn render_param_directives(top: &mut String, params: &[BondedParam]) {
    if params.is_empty() {
        return;
    }
    for kind in [
        "bondtypes",
        "pairtypes",
        "angletypes",
        "dihedraltypes",
        "constrainttypes",
    ] {
        let rows: Vec<&BondedParam> = params.iter().filter(|p| p.kind == kind).collect();
        if rows.is_empty() {
            continue;
        }
        top.push_str(&format!("[ {kind} ]\n"));
        for p in rows {
            top.push_str("  ");
            top.push_str(&p.atoms);
            if let Some(func) = p.func {
                top.push_str(&format!("  {func}"));
            }
            for value in &p.params {
                top.push_str(&format!("  {value}"));
            }
            top.push('\n');
        }
        top.push('\n');
    }
}

/// Emit a bonded molecule type's index-only `[bonds]`, `[pairs]`, `[angles]`,
/// and `[dihedrals]` sections (proper dihedrals func 9, then impropers func 4).
/// Parameters are omitted; grompp resolves them from the directives above.
fn render_bonded_sections(top: &mut String, mol: &MoleculeType) {
    if !mol.has_bonded_terms() {
        return;
    }
    render_term_section(top, "bonds", &mol.bonds);
    render_term_section(top, "pairs", &mol.pairs);
    render_term_section(top, "angles", &mol.angles);
    if !mol.dihedrals.is_empty() || !mol.impropers.is_empty() {
        top.push_str("[ dihedrals ]\n");
        for term in mol.dihedrals.iter().chain(&mol.impropers) {
            push_term_line(top, term);
        }
        top.push('\n');
    }
}

fn render_term_section(top: &mut String, name: &str, terms: &[BondedTerm]) {
    if terms.is_empty() {
        return;
    }
    top.push_str(&format!("[ {name} ]\n"));
    for term in terms {
        push_term_line(top, term);
    }
    top.push('\n');
}

fn push_term_line(top: &mut String, term: &BondedTerm) {
    top.push_str("  ");
    for atom in &term.atoms {
        top.push_str(&format!("{atom:>4} "));
    }
    top.push_str(&format!("{}\n", term.func));
}

#[cfg(test)]
mod tests {
    use nalgebra::Point3;

    use super::*;
    use crate::domain::{Atom, Structure, UnitCell};

    fn argon_topology(n: usize) -> MdTopology {
        let atoms = (0..n)
            .map(|i| Atom {
                element: "Ar".to_string(),
                position: Point3::new(i as f32 * 5.0, 0.0, 0.0),
                charge: 0.0,
            })
            .collect();
        let structure = Structure::with_cell(
            "argon",
            atoms,
            UnitCell::from_parameters(100.0, 100.0, 100.0, 90.0, 90.0, 90.0),
        );
        MdTopology::from_structure(&structure).unwrap()
    }

    #[test]
    fn renders_expected_sections_and_argon_parameters() {
        let top = render_top(&argon_topology(8));
        assert!(top.contains("[ defaults ]"));
        assert!(top.contains("1         2          no"));
        assert!(top.contains("[ atomtypes ]"));
        // Sigma is converted angstrom -> nm; argon -> 0.3405 nm, epsilon 0.996.
        assert!(top.contains("0.3405"));
        assert!(top.contains("0.996"));
        assert!(top.contains("[ moleculetype ]"));
        assert!(top.contains("[ molecules ]"));
        // Eight contiguous argon atoms collapse to a single run.
        assert!(top.contains("AR      8"));
    }

    #[test]
    fn distinct_species_each_get_a_moleculetype() {
        let atoms = vec![
            Atom {
                element: "Ar".to_string(),
                position: Point3::new(0.0, 0.0, 0.0),
                charge: 0.0,
            },
            Atom {
                element: "Ne".to_string(),
                position: Point3::new(10.0, 0.0, 0.0),
                charge: 0.0,
            },
        ];
        let structure = Structure::with_cell(
            "mix",
            atoms,
            UnitCell::from_parameters(100.0, 100.0, 100.0, 90.0, 90.0, 90.0),
        );
        let top = render_top(&MdTopology::from_structure(&structure).unwrap());
        assert_eq!(top.matches("[ moleculetype ]").count(), 2);
        assert!(top.contains("AR      1"));
        assert!(top.contains("NE      1"));
    }
}
