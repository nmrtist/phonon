//! Generation of GROMACS input files (`.gro`, `.mdp`) from Phonon structures.
//!
//! GROMACS stages (energy minimization, NVT, NPT, production) differ only in
//! the `.mdp` parameters fed to `gmx grompp`, so this module exposes one
//! [`MdpSettings`] type that covers them all. Presets such as
//! [`MdpSettings::energy_minimization`] populate the fields for a specific
//! stage; new stages are added as additional constructors.
//!
//! Generates `.mdp` for homogeneous Lennard-Jones systems: plain cut-off
//! electrostatics, no bond constraints, single `System` thermostat group.

use anyhow::{Result, anyhow};

use crate::domain::Structure;

/// Conversion factor angstroms -> nanometers (GROMACS uses nm).
const ANGSTROM_TO_NM: f32 = 0.1;

/// Which time-integration scheme GROMACS should run for this stage. Maps
/// directly to the `integrator =` line in the generated `.mdp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Integrator {
    /// Steepest-descent energy minimization (`integrator = steep`).
    SteepestDescent,
    /// Leap-frog molecular dynamics (`integrator = md`).
    Leapfrog,
}

impl Integrator {
    pub fn mdp_token(self) -> &'static str {
        match self {
            Self::SteepestDescent => "steep",
            Self::Leapfrog => "md",
        }
    }

    pub fn is_minimization(self) -> bool {
        matches!(self, Self::SteepestDescent)
    }
}

/// Which bonds GROMACS converts to holonomic constraints. `None` on
/// [`MdpSettings`] leaves them flexible (`constraints = none`); constraining
/// hydrogen bonds (`h-bonds`) lets MD use a 2 fs timestep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintKind {
    HBonds,
    AllBonds,
}

impl ConstraintKind {
    pub fn mdp_token(self) -> &'static str {
        match self {
            Self::HBonds => "h-bonds",
            Self::AllBonds => "all-bonds",
        }
    }
}

/// Constraint solver algorithm (`constraint-algorithm =`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintAlgorithm {
    Lincs,
    Shake,
}

impl ConstraintAlgorithm {
    pub fn mdp_token(self) -> &'static str {
        match self {
            Self::Lincs => "lincs",
            Self::Shake => "shake",
        }
    }
}

/// Temperature-coupling configuration. `tc_grps`, `tau_t` and `ref_t` are
/// parallel: one entry per coupling group. The thermostat is velocity-rescaling
/// (`V-rescale`), the standard robust choice.
#[derive(Debug, Clone)]
pub struct TemperatureCoupling {
    pub tc_grps: Vec<String>,
    pub tau_t: Vec<f32>,
    pub ref_t: Vec<f32>,
}

impl TemperatureCoupling {
    /// Couple the whole system as one group at `ref_t` kelvin — the right
    /// arrangement for a homogeneous system with no solute/solvent split.
    /// `System` is the default index group GROMACS always defines.
    pub fn whole_system(ref_t: f32) -> Self {
        Self {
            tc_grps: vec!["System".to_string()],
            tau_t: vec![0.1],
            ref_t: vec![ref_t],
        }
    }
}

/// Pressure-coupling configuration. The barostat is stochastic cell rescaling
/// (`C-rescale`).
#[derive(Debug, Clone)]
pub struct PressureCoupling {
    pub pcoupltype: String,
    pub tau_p: f32,
    pub ref_p: f32,
    pub compressibility: f32,
}

impl PressureCoupling {
    /// Isotropic coupling to 1 bar — the standard NPT/production setting.
    pub fn isotropic() -> Self {
        Self {
            pcoupltype: "isotropic".to_string(),
            tau_p: 2.0,
            ref_p: 1.0,
            compressibility: 4.5e-5,
        }
    }
}

/// Initial-velocity generation (`gen_vel =`). Present only for the first MD
/// stage (NVT); later stages continue from the checkpoint instead.
#[derive(Debug, Clone, Copy)]
pub struct VelocityGen {
    pub gen_temp: f32,
    /// Random seed; `-1` lets GROMACS pick one.
    pub gen_seed: i64,
}

/// Trajectory/energy output frequencies (steps between writes).
#[derive(Debug, Clone, Copy)]
pub struct OutputFrequency {
    pub nstxout: u32,
    pub nstvout: u32,
    pub nstenergy: u32,
    pub nstlog: u32,
    /// Compressed (`.xtc`) trajectory write interval; `0` disables it.
    pub nstxout_compressed: u32,
}

impl OutputFrequency {
    /// Equilibration: log energy regularly, no full-precision trajectory.
    pub fn equilibration() -> Self {
        Self {
            nstxout: 0,
            nstvout: 0,
            nstenergy: 500,
            nstlog: 500,
            nstxout_compressed: 0,
        }
    }

    /// Production: write a compressed `.xtc` trajectory for analysis.
    pub fn production() -> Self {
        Self {
            nstxout: 0,
            nstvout: 0,
            nstenergy: 5_000,
            nstlog: 5_000,
            nstxout_compressed: 5_000,
        }
    }
}

/// `.mdp` parameters shared across all GROMACS stages. Stage-specific fields
/// (e.g. `emtol` for minimization, `dt` for MD) are kept on the same struct;
/// [`render_mdp`] picks the right subset depending on `integrator`.
#[derive(Debug, Clone)]
pub struct MdpSettings {
    pub integrator: Integrator,
    pub nsteps: u64,
    pub timestep_ps: f32,
    pub coulomb_cutoff_nm: f32,
    pub vdw_cutoff_nm: f32,
    /// Force tolerance for minimization (kJ/mol/nm). Only used when
    /// `integrator == SteepestDescent`.
    pub emtol: f32,
    /// Initial step size for minimization (nm). Only used when
    /// `integrator == SteepestDescent`.
    pub emstep: f32,
    /// `continuation = yes/no` — rendered for MD stages only. NPT/production
    /// continue a prior run; the first NVT stage does not.
    pub continuation: bool,
    /// Temperature coupling; `None` leaves the thermostat off.
    pub temperature_coupling: Option<TemperatureCoupling>,
    /// Pressure coupling; `None` renders `pcoupl = no` for MD stages.
    pub pressure_coupling: Option<PressureCoupling>,
    /// Initial velocity generation; `None` renders `gen_vel = no` for MD.
    pub velocity_generation: Option<VelocityGen>,
    /// Output write frequencies; `None` falls back to GROMACS defaults.
    pub output: Option<OutputFrequency>,
    /// Bonds to constrain; `None` renders `constraints = none`. MD stages
    /// default to `h-bonds` (for a 2 fs step); minimization leaves it `None`.
    pub constraints: Option<ConstraintKind>,
    /// Constraint solver used when `constraints` is set.
    pub constraint_algorithm: ConstraintAlgorithm,
}

impl MdpSettings {
    /// Defaults for a steepest-descent energy minimization stage.
    pub fn energy_minimization() -> Self {
        Self {
            integrator: Integrator::SteepestDescent,
            nsteps: 5_000,
            timestep_ps: 0.0,
            coulomb_cutoff_nm: 1.0,
            vdw_cutoff_nm: 1.0,
            emtol: 1_000.0,
            emstep: 0.01,
            continuation: false,
            temperature_coupling: None,
            pressure_coupling: None,
            velocity_generation: None,
            output: None,
            constraints: None,
            constraint_algorithm: ConstraintAlgorithm::Lincs,
        }
    }

    /// NVT (constant volume/temperature) equilibration with freshly generated
    /// velocities and a single whole-`System` thermostat. 100 ps at a 2 fs step,
    /// which `h-bonds` constraints make stable.
    pub fn nvt(ref_temp: f32) -> Self {
        Self {
            integrator: Integrator::Leapfrog,
            nsteps: 50_000,
            timestep_ps: 0.002,
            continuation: false,
            temperature_coupling: Some(TemperatureCoupling::whole_system(ref_temp)),
            pressure_coupling: None,
            velocity_generation: Some(VelocityGen {
                gen_temp: ref_temp,
                gen_seed: -1,
            }),
            output: Some(OutputFrequency::equilibration()),
            constraints: Some(ConstraintKind::HBonds),
            ..Self::energy_minimization()
        }
    }

    /// NPT (constant pressure/temperature) equilibration: continues from the NVT
    /// checkpoint and adds an isotropic barostat.
    pub fn npt(ref_temp: f32) -> Self {
        Self {
            continuation: true,
            velocity_generation: None,
            pressure_coupling: Some(PressureCoupling::isotropic()),
            ..Self::nvt(ref_temp)
        }
    }

    /// Production MD: full temperature and pressure coupling, and a compressed
    /// trajectory for analysis.
    pub fn production(nsteps: u64, ref_temp: f32) -> Self {
        Self {
            nsteps,
            continuation: true,
            velocity_generation: None,
            pressure_coupling: Some(PressureCoupling::isotropic()),
            output: Some(OutputFrequency::production()),
            ..Self::nvt(ref_temp)
        }
    }
}

impl Default for MdpSettings {
    fn default() -> Self {
        Self::energy_minimization()
    }
}

/// Serialize the active structure as a GROMACS coordinate (`.gro`) file.
pub fn to_gro(structure: &Structure, title: &str) -> Result<String> {
    let cell = structure
        .cell
        .as_ref()
        .ok_or_else(|| anyhow!("GROMACS coordinate files need a simulation box vector"))?;
    if structure.atoms.is_empty() {
        return Err(anyhow!("cannot write a GROMACS .gro file with zero atoms"));
    }

    let header = title.lines().next().unwrap_or(title).trim();
    let mut output = String::new();
    output.push_str(if header.is_empty() {
        "phonon-structure"
    } else {
        header
    });
    output.push('\n');
    output.push_str(&format!("{:>5}\n", structure.atoms.len()));

    for (index, atom) in structure.atoms.iter().enumerate() {
        let serial = (index + 1) as u32 % 100_000;
        let residue_id = 1u32;
        let residue_name = "MOL";
        let atom_name = atom_name_for(&atom.element, index + 1);

        output.push_str(&format!(
            "{:>5}{:<5}{:>5}{:>5}{:>8.3}{:>8.3}{:>8.3}\n",
            residue_id,
            residue_name,
            atom_name,
            serial,
            atom.position.x * ANGSTROM_TO_NM,
            atom.position.y * ANGSTROM_TO_NM,
            atom.position.z * ANGSTROM_TO_NM,
        ));
    }

    output.push_str(&format_box_vectors(cell.vectors));
    Ok(output)
}

/// Emit a `.mdp` parameter file appropriate for the configured stage.
///
/// Every MD-only directive (thermostat, barostat, velocity generation, output
/// frequencies) is emitted only for non-minimization stages or when the
/// relevant field is populated.
pub fn render_mdp(settings: &MdpSettings) -> String {
    let mut body = String::new();
    body.push_str("; Phonon-generated GROMACS run parameters\n");

    body.push_str(&format!(
        "integrator               = {}\n",
        settings.integrator.mdp_token()
    ));
    body.push_str(&format!("nsteps                   = {}\n", settings.nsteps));

    if settings.integrator.is_minimization() {
        body.push_str(&format!(
            "emtol                    = {:.3}\n",
            settings.emtol
        ));
        body.push_str(&format!(
            "emstep                   = {:.5}\n",
            settings.emstep
        ));
    } else {
        body.push_str(&format!(
            "dt                       = {:.5}\n",
            settings.timestep_ps
        ));
    }

    body.push_str("nstlist                  = 10\n");
    body.push_str("cutoff-scheme            = Verlet\n");
    body.push_str("ns_type                  = grid\n");
    body.push_str("coulombtype              = cutoff\n");
    body.push_str(&format!(
        "rcoulomb                 = {:.4}\n",
        settings.coulomb_cutoff_nm
    ));
    body.push_str(&format!(
        "rvdw                     = {:.4}\n",
        settings.vdw_cutoff_nm
    ));
    body.push_str("pbc                      = xyz\n");
    match settings.constraints {
        Some(kind) => {
            body.push_str(&format!(
                "constraints              = {}\n",
                kind.mdp_token()
            ));
            body.push_str(&format!(
                "constraint-algorithm     = {}\n",
                settings.constraint_algorithm.mdp_token()
            ));
        }
        None => body.push_str("constraints              = none\n"),
    }

    if !settings.integrator.is_minimization() {
        render_md_coupling(&mut body, settings);
    }

    body
}

/// Append the MD-only coupling, velocity-generation, continuation, and output
/// directives. Split out to keep [`render_mdp`] readable.
fn render_md_coupling(body: &mut String, settings: &MdpSettings) {
    if let Some(tc) = &settings.temperature_coupling {
        body.push_str("tcoupl                   = V-rescale\n");
        body.push_str(&format!(
            "tc-grps                  = {}\n",
            tc.tc_grps.join(" ")
        ));
        body.push_str(&format!(
            "tau-t                    = {}\n",
            join_floats(&tc.tau_t)
        ));
        body.push_str(&format!(
            "ref-t                    = {}\n",
            join_floats(&tc.ref_t)
        ));
    }

    if let Some(pc) = &settings.pressure_coupling {
        body.push_str("pcoupl                   = C-rescale\n");
        body.push_str(&format!("pcoupltype               = {}\n", pc.pcoupltype));
        body.push_str(&format!("tau-p                    = {}\n", pc.tau_p));
        body.push_str(&format!("ref-p                    = {}\n", pc.ref_p));
        body.push_str(&format!(
            "compressibility          = {}\n",
            pc.compressibility
        ));
    } else {
        body.push_str("pcoupl                   = no\n");
    }

    if let Some(vg) = &settings.velocity_generation {
        body.push_str("gen_vel                  = yes\n");
        body.push_str(&format!("gen_temp                 = {}\n", vg.gen_temp));
        body.push_str(&format!("gen_seed                 = {}\n", vg.gen_seed));
    } else {
        body.push_str("gen_vel                  = no\n");
    }

    body.push_str(&format!(
        "continuation             = {}\n",
        if settings.continuation { "yes" } else { "no" }
    ));

    if let Some(out) = &settings.output {
        body.push_str(&format!("nstxout                  = {}\n", out.nstxout));
        body.push_str(&format!("nstvout                  = {}\n", out.nstvout));
        body.push_str(&format!("nstenergy                = {}\n", out.nstenergy));
        body.push_str(&format!("nstlog                   = {}\n", out.nstlog));
        body.push_str(&format!(
            "nstxout-compressed       = {}\n",
            out.nstxout_compressed
        ));
    }
}

/// Render a list of floats space-separated using their compact `Display` form
/// (`0.1`, `300`), matching how the GROMACS examples write coupling vectors.
fn join_floats(values: &[f32]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_box_vectors(vectors: [nalgebra::Vector3<f32>; 3]) -> String {
    let nm = |v: f32| v * ANGSTROM_TO_NM;
    let v1 = vectors[0];
    let v2 = vectors[1];
    let v3 = vectors[2];

    let off_diag_tolerance = 1.0e-5_f32;
    let triclinic = v1.y.abs() > off_diag_tolerance
        || v1.z.abs() > off_diag_tolerance
        || v2.x.abs() > off_diag_tolerance
        || v2.z.abs() > off_diag_tolerance
        || v3.x.abs() > off_diag_tolerance
        || v3.y.abs() > off_diag_tolerance;

    if triclinic {
        format!(
            "{:>10.5}{:>10.5}{:>10.5}{:>10.5}{:>10.5}{:>10.5}{:>10.5}{:>10.5}{:>10.5}\n",
            nm(v1.x),
            nm(v2.y),
            nm(v3.z),
            nm(v1.y),
            nm(v1.z),
            nm(v2.x),
            nm(v2.z),
            nm(v3.x),
            nm(v3.y),
        )
    } else {
        format!("{:>10.5}{:>10.5}{:>10.5}\n", nm(v1.x), nm(v2.y), nm(v3.z))
    }
}

fn atom_name_for(element: &str, serial: usize) -> String {
    let element = element.trim();
    if element.is_empty() {
        return format!("X{}", serial % 1_000);
    }
    let candidate = format!("{element}{}", serial % 1_000);
    let trimmed: String = candidate.chars().take(5).collect();
    if trimmed.is_empty() {
        "X".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use nalgebra::Point3;

    use super::*;
    use crate::domain::{Atom, Structure, UnitCell};

    fn ethane_cell() -> Structure {
        let atoms = vec![
            Atom {
                element: "C".to_string(),
                position: Point3::new(0.0, 0.0, 0.0),
                charge: 0.0,
            },
            Atom {
                element: "C".to_string(),
                position: Point3::new(1.54, 0.0, 0.0),
                charge: 0.0,
            },
        ];
        Structure::with_cell(
            "ethane",
            atoms,
            UnitCell::from_parameters(10.0, 10.0, 10.0, 90.0, 90.0, 90.0),
        )
    }

    #[test]
    fn rectangular_box_uses_three_field_form() {
        let structure = ethane_cell();
        let gro = to_gro(&structure, "ethane").expect("serialized");

        let last_line = gro.lines().last().expect("box line");
        let fields = last_line.split_whitespace().count();
        assert_eq!(fields, 3);
        assert!(gro.contains("ethane"));
        assert!(gro.contains("MOL"));
    }

    #[test]
    fn positions_are_converted_to_nanometers() {
        let structure = ethane_cell();
        let gro = to_gro(&structure, "ethane").expect("serialized");

        let second_atom_line = gro.lines().nth(3).expect("second atom line");
        assert!(second_atom_line.contains("0.154"));
    }

    #[test]
    fn em_mdp_contains_user_visible_parameters() {
        let settings = MdpSettings {
            nsteps: 250,
            emtol: 500.0,
            ..MdpSettings::energy_minimization()
        };
        let mdp = render_mdp(&settings);

        assert!(mdp.contains("integrator               = steep"));
        assert!(mdp.contains("emtol                    = 500.000"));
        assert!(mdp.contains("nsteps                   = 250"));
    }

    #[test]
    fn md_mdp_uses_timestep_instead_of_emtol() {
        let settings = MdpSettings {
            integrator: Integrator::Leapfrog,
            nsteps: 5_000,
            timestep_ps: 0.002,
            ..MdpSettings::energy_minimization()
        };
        let mdp = render_mdp(&settings);

        assert!(mdp.contains("integrator               = md"));
        assert!(mdp.contains("dt                       = 0.00200"));
        assert!(!mdp.contains("emtol"));
    }

    #[test]
    fn triclinic_box_round_trips_through_parser() {
        use crate::io::formats::gro::parse_gro;

        let structure = Structure::with_cell(
            "triclinic",
            vec![Atom {
                element: "C".to_string(),
                position: Point3::new(1.0, 2.0, 3.0),
                charge: 0.0,
            }],
            UnitCell::from_parameters(10.0, 12.0, 15.0, 70.0, 80.0, 100.0),
        );

        let gro = to_gro(&structure, "triclinic").expect("serialized");
        let box_fields = gro
            .lines()
            .last()
            .expect("box line")
            .split_whitespace()
            .count();
        assert_eq!(
            box_fields, 9,
            "non-orthogonal cell must use the nine-field box form"
        );

        let reparsed = parse_gro(&gro).expect("round-trip parse");
        let original = structure.cell.as_ref().expect("cell").vectors;
        let restored = reparsed.cell.as_ref().expect("cell").vectors;
        for (o, r) in original.iter().zip(restored.iter()) {
            assert!((o.x - r.x).abs() < 1.0e-3, "x mismatch {o:?} vs {r:?}");
            assert!((o.y - r.y).abs() < 1.0e-3, "y mismatch {o:?} vs {r:?}");
            assert!((o.z - r.z).abs() < 1.0e-3, "z mismatch {o:?} vs {r:?}");
        }
    }

    #[test]
    fn energy_minimization_mdp_is_byte_stable() {
        // Guards backward compatibility with the committed EM integration: this
        // is the exact historical output.
        let mdp = render_mdp(&MdpSettings::energy_minimization());
        let expected = "\
; Phonon-generated GROMACS run parameters
integrator               = steep
nsteps                   = 5000
emtol                    = 1000.000
emstep                   = 0.01000
nstlist                  = 10
cutoff-scheme            = Verlet
ns_type                  = grid
coulombtype              = cutoff
rcoulomb                 = 1.0000
rvdw                     = 1.0000
pbc                      = xyz
constraints              = none
";
        assert_eq!(mdp, expected);
    }

    #[test]
    fn nvt_mdp_has_thermostat_and_genvel_but_no_pressure() {
        let mdp = render_mdp(&MdpSettings::nvt(94.0));
        assert!(mdp.contains("integrator               = md"));
        assert!(mdp.contains("coulombtype              = cutoff"));
        assert!(mdp.contains("constraints              = h-bonds"));
        assert!(mdp.contains("constraint-algorithm     = lincs"));
        assert!(mdp.contains("tcoupl                   = V-rescale"));
        assert!(mdp.contains("tc-grps                  = System"));
        assert!(mdp.contains("ref-t                    = 94"));
        assert!(mdp.contains("gen_vel                  = yes"));
        assert!(mdp.contains("pcoupl                   = no"));
    }

    #[test]
    fn npt_mdp_adds_barostat_and_continuation() {
        let mdp = render_mdp(&MdpSettings::npt(94.0));
        assert!(mdp.contains("pcoupl                   = C-rescale"));
        assert!(mdp.contains("continuation             = yes"));
        assert!(mdp.contains("gen_vel                  = no"));
    }

    #[test]
    fn production_mdp_writes_compressed_trajectory() {
        let mdp = render_mdp(&MdpSettings::production(10_000, 94.0));
        assert!(mdp.contains("nstxout-compressed       = 5000"));
        assert!(mdp.contains("coulombtype              = cutoff"));
        assert!(mdp.contains("pcoupl                   = C-rescale"));
    }

    #[test]
    fn constraints_render_only_when_set() {
        // Minimization leaves bonds flexible and emits no algorithm line.
        let em = render_mdp(&MdpSettings::energy_minimization());
        assert!(em.contains("constraints              = none"));
        assert!(!em.contains("constraint-algorithm"));

        // An explicit h-bonds setting renders both lines.
        let settings = MdpSettings {
            constraints: Some(ConstraintKind::HBonds),
            constraint_algorithm: ConstraintAlgorithm::Lincs,
            ..MdpSettings::energy_minimization()
        };
        let mdp = render_mdp(&settings);
        assert!(mdp.contains("constraints              = h-bonds"));
        assert!(mdp.contains("constraint-algorithm     = lincs"));
    }

    #[test]
    fn requires_periodic_cell() {
        let structure = Structure::new(
            "no-cell",
            vec![Atom {
                element: "C".to_string(),
                position: Point3::new(0.0, 0.0, 0.0),
                charge: 0.0,
            }],
        );

        let error = to_gro(&structure, "no-cell").expect_err("should fail");
        assert!(error.to_string().contains("simulation box"));
    }
}
