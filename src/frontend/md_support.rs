use std::{collections::HashSet, path::PathBuf};

use anyhow::{Result, bail};

use crate::{
    backend::tasks::TaskKind,
    engines::gromacs::{FileRef, StageFileRole, StageLinks, StageSpec},
    frontend::state::{AppState, MdRunStepPrompt},
    workflows::molecular_dynamics::{MdProtocolOptions, MdTopology, full_protocol},
};

pub const MD_TOPOLOGY_FILE: &str = "system_topology.json";

/// The GROMACS topology a [`TaskKind::BuildMdSystem`] run writes when GROMACS is
/// the build engine (`pdb2gmx -p topol.top`, updated by solvate/genion).
pub const MD_GROMACS_TOPOLOGY_FILE: &str = "topol.top";

pub fn md_topology_path_for_entry(state: &AppState, entry_id: u64) -> Option<PathBuf> {
    let run = state
        .tasks
        .latest_completed_run_for_result(TaskKind::BuildMdSystem, entry_id)?;
    let run_dir = run.run_dir.as_ref()?;
    let path = run_dir.join(MD_TOPOLOGY_FILE);
    path.exists().then_some(path)
}

/// Path to the GROMACS `topol.top` produced by the entry's latest completed MD
/// system build, if that build used the GROMACS engine. This is the force-field
/// topology a run reuses directly.
pub fn gromacs_topology_path_for_entry(state: &AppState, entry_id: u64) -> Option<PathBuf> {
    let run = state
        .tasks
        .latest_completed_run_for_result(TaskKind::BuildMdSystem, entry_id)?;
    let run_dir = run.run_dir.as_ref()?;
    let path = run_dir.join(MD_GROMACS_TOPOLOGY_FILE);
    path.exists().then_some(path)
}

pub fn load_md_topology_for_entry(state: &AppState, entry_id: u64) -> Option<MdTopology> {
    let path = md_topology_path_for_entry(state, entry_id)?;
    MdTopology::load(&path).ok()
}

pub fn protocol_stage_specs(options: &MdProtocolOptions) -> Vec<StageSpec> {
    full_protocol(options)
}

pub fn build_md_stage_specs(steps: &[MdRunStepPrompt]) -> Result<Vec<StageSpec>> {
    if steps.is_empty() {
        bail!("Add at least one MD step");
    }

    let mut stage_specs = Vec::with_capacity(steps.len());
    let mut seen_stage_names = HashSet::new();
    let mut previous_stage: Option<String> = None;
    let mut last_checkpoint_stage: Option<String> = None;

    for step in steps {
        let stage_name = step.stage_name.trim();
        if stage_name.is_empty() {
            bail!("Each MD step needs a non-empty name");
        }
        let sanitized = sanitize_md_stage_name(stage_name);
        if !seen_stage_names.insert(sanitized.clone()) {
            bail!("MD step names must be unique");
        }

        let links = if let Some(previous_stage) = previous_stage.as_ref() {
            StageLinks {
                coordinates: FileRef::Stage {
                    stage: previous_stage.clone(),
                    role: StageFileRole::OutputGro,
                },
                checkpoint: if step.settings.continuation {
                    last_checkpoint_stage.clone().map(|stage| FileRef::Stage {
                        stage,
                        role: StageFileRole::Checkpoint,
                    })
                } else {
                    None
                },
            }
        } else {
            StageLinks::from_prepared()
        };

        stage_specs.push(StageSpec {
            stage_name: stage_name.to_string(),
            settings: step.settings.clone(),
            links,
        });
        previous_stage = Some(stage_name.to_string());
        if !step.settings.integrator.is_minimization() {
            last_checkpoint_stage = Some(stage_name.to_string());
        }
    }

    Ok(stage_specs)
}

fn sanitize_md_stage_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if cleaned.is_empty() {
        "stage".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::state::{MdRunStepPreset, MdRunStepPrompt};

    fn step(preset: MdRunStepPreset) -> MdRunStepPrompt {
        MdRunStepPrompt::from_preset(preset, 300.0, 0.002)
    }

    fn checkpoint_stage(spec: &StageSpec) -> Option<String> {
        match spec.links.checkpoint.as_ref()? {
            FileRef::Stage { stage, .. } => Some(stage.clone()),
            FileRef::PreparedConf => None,
        }
    }

    #[test]
    fn continuation_after_minimization_starts_fresh() {
        // NPT placed directly after EM must not dangle on the (checkpoint-less)
        // minimization stage; it starts fresh instead.
        let specs = build_md_stage_specs(&[
            step(MdRunStepPreset::EnergyMinimization),
            step(MdRunStepPreset::Npt),
        ])
        .unwrap();
        assert_eq!(specs.len(), 2);
        assert!(specs[0].links.checkpoint.is_none());
        assert!(
            checkpoint_stage(&specs[1]).is_none(),
            "NPT after EM should not reference a checkpoint"
        );
    }

    #[test]
    fn continuation_links_to_last_md_stage_checkpoint() {
        let em = step(MdRunStepPreset::EnergyMinimization);
        let nvt = step(MdRunStepPreset::Nvt);
        let npt = step(MdRunStepPreset::Npt);
        let nvt_name = nvt.stage_name.clone();
        let specs = build_md_stage_specs(&[em, nvt, npt]).unwrap();

        // NVT is not a continuation; NPT continues from NVT's checkpoint.
        assert!(specs[1].links.checkpoint.is_none());
        assert_eq!(
            checkpoint_stage(&specs[2]).as_deref(),
            Some(nvt_name.as_str())
        );
        // Coordinates always chain from the immediately previous stage.
        match &specs[2].links.coordinates {
            FileRef::Stage { stage, .. } => assert_eq!(stage, &nvt_name),
            FileRef::PreparedConf => panic!("NPT should chain coordinates from NVT"),
        }
    }
}
