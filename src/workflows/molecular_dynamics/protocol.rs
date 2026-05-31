//! MD protocol: the ordered stage chain a simulation run performs internally.
//!
//! Given a simulation-ready system this builds the energy-minimization → NVT →
//! NPT → production sequence as [`StageSpec`]s wired with [`StageLinks`], so
//! [`crate::engines::gromacs::run_pipeline`] threads each stage's coordinates and
//! continuation checkpoint to the next. Callers express physical intent
//! (temperature, simulation time, whether to relax first); this module
//! translates that into the engine stage chain.

use crate::engines::gromacs::{FileRef, MdpSettings, StageFileRole, StageLinks, StageSpec};

/// Canonical stage names, used both as `-deffnm` basenames and as the keys
/// [`StageLinks`] reference.
pub const STAGE_EM: &str = "em";
pub const STAGE_NVT: &str = "nvt";
pub const STAGE_NPT: &str = "npt";
pub const STAGE_PROD: &str = "md";

/// Physical parameters for a molecular-dynamics run — the choices a user makes
/// in the MD panel. Everything else is derived internally.
#[derive(Debug, Clone, Copy)]
pub struct MdProtocolOptions {
    /// Production simulation length, picoseconds.
    pub production_ps: f64,
    /// MD integration timestep, picoseconds (2 fs default).
    pub timestep_ps: f32,
    /// Target temperature, kelvin.
    pub temperature_k: f32,
    /// Run EM → NVT → NPT equilibration before production ("relax model system
    /// before simulation"). When false, only production runs.
    pub relax_before_production: bool,
}

impl Default for MdProtocolOptions {
    fn default() -> Self {
        Self {
            production_ps: 1_000.0,
            timestep_ps: 0.002,
            temperature_k: 300.0,
            relax_before_production: true,
        }
    }
}

impl MdProtocolOptions {
    /// Production length expressed as a step count for the given timestep.
    pub fn production_steps(&self) -> u64 {
        (self.production_ps / self.timestep_ps as f64).round() as u64
    }
}

/// A [`FileRef`] pointing at a named stage's produced file.
fn stage_ref(stage: &str, role: StageFileRole) -> FileRef {
    FileRef::Stage {
        stage: stage.to_string(),
        role,
    }
}

/// Build the equilibration stage specs: EM → NVT (from the EM coordinates) →
/// NPT (continues from the NVT checkpoint).
pub fn equilibration_stages(options: &MdProtocolOptions) -> Vec<StageSpec> {
    let t = options.temperature_k;

    let em = StageSpec {
        stage_name: STAGE_EM.to_string(),
        settings: MdpSettings::energy_minimization(),
        links: StageLinks::from_prepared(),
    };

    let nvt = StageSpec {
        stage_name: STAGE_NVT.to_string(),
        settings: MdpSettings::nvt(t),
        links: StageLinks {
            coordinates: stage_ref(STAGE_EM, StageFileRole::OutputGro),
            checkpoint: None,
        },
    };

    let npt = StageSpec {
        stage_name: STAGE_NPT.to_string(),
        settings: MdpSettings::npt(t),
        links: StageLinks {
            coordinates: stage_ref(STAGE_NVT, StageFileRole::OutputGro),
            checkpoint: Some(stage_ref(STAGE_NVT, StageFileRole::Checkpoint)),
        },
    };

    vec![em, nvt, npt]
}

/// Build the production stage spec. Continues from the NPT checkpoint (or, if
/// equilibration was skipped, from the prepared coordinates).
pub fn production_stage(options: &MdProtocolOptions) -> StageSpec {
    let mut settings = MdpSettings::production(options.production_steps(), options.temperature_k);
    settings.timestep_ps = options.timestep_ps;

    let links = if options.relax_before_production {
        StageLinks {
            coordinates: stage_ref(STAGE_NPT, StageFileRole::OutputGro),
            checkpoint: Some(stage_ref(STAGE_NPT, StageFileRole::Checkpoint)),
        }
    } else {
        StageLinks::from_prepared()
    };

    StageSpec {
        stage_name: STAGE_PROD.to_string(),
        settings,
        links,
    }
}

/// The full stage chain a run executes: equilibration (if requested) followed by
/// production.
pub fn full_protocol(options: &MdProtocolOptions) -> Vec<StageSpec> {
    let mut stages = if options.relax_before_production {
        equilibration_stages(options)
    } else {
        Vec::new()
    };
    stages.push(production_stage(options));
    stages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage<'a>(stages: &'a [StageSpec], name: &str) -> &'a StageSpec {
        stages
            .iter()
            .find(|s| s.stage_name == name)
            .expect("stage present")
    }

    #[test]
    fn full_protocol_has_four_stages_when_relaxing() {
        let stages = full_protocol(&MdProtocolOptions::default());
        let names: Vec<&str> = stages.iter().map(|s| s.stage_name.as_str()).collect();
        assert_eq!(names, vec![STAGE_EM, STAGE_NVT, STAGE_NPT, STAGE_PROD]);
    }

    #[test]
    fn skipping_relaxation_runs_production_only_from_prepared_coords() {
        let opts = MdProtocolOptions {
            relax_before_production: false,
            ..MdProtocolOptions::default()
        };
        let stages = full_protocol(&opts);
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].stage_name, STAGE_PROD);
        assert_eq!(stages[0].links.coordinates, FileRef::PreparedConf);
        assert!(stages[0].links.checkpoint.is_none());
    }

    #[test]
    fn nvt_starts_from_em_output() {
        let stages = equilibration_stages(&MdProtocolOptions::default());
        assert_eq!(
            stage(&stages, STAGE_NVT).links.coordinates,
            FileRef::Stage {
                stage: STAGE_EM.to_string(),
                role: StageFileRole::OutputGro,
            }
        );
    }

    #[test]
    fn npt_continues_from_nvt_checkpoint() {
        let stages = equilibration_stages(&MdProtocolOptions::default());
        assert_eq!(
            stage(&stages, STAGE_NPT).links.checkpoint,
            Some(FileRef::Stage {
                stage: STAGE_NVT.to_string(),
                role: StageFileRole::Checkpoint,
            })
        );
    }

    #[test]
    fn production_continues_from_npt() {
        let prod = production_stage(&MdProtocolOptions::default());
        assert_eq!(
            prod.links.checkpoint,
            Some(FileRef::Stage {
                stage: STAGE_NPT.to_string(),
                role: StageFileRole::Checkpoint,
            })
        );
    }

    #[test]
    fn production_steps_derive_from_time_and_timestep() {
        let opts = MdProtocolOptions {
            production_ps: 1_000.0,
            timestep_ps: 0.002,
            ..MdProtocolOptions::default()
        };
        assert_eq!(opts.production_steps(), 500_000);
    }
}
