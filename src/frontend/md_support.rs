use std::{collections::HashSet, path::Path, path::PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{
    backend::tasks::TaskKind,
    engines::gromacs::{
        FileRef, FreezeSelection, MdpSettings, StageFileRole, StageLinks, StageSpec,
        framework_freeze_selection, input::FreezeGroup,
    },
    frontend::state::{AppState, MdRunStepPrompt},
    workflows::molecular_dynamics::{MdProtocolOptions, MdTopology, full_protocol},
};

pub const MD_TOPOLOGY_FILE: &str = "system_topology.json";

/// The GROMACS topology a [`TaskKind::BuildMdSystem`] run writes when GROMACS is
/// the build engine (`pdb2gmx -p topol.top`, updated by solvate/genion).
pub const MD_GROMACS_TOPOLOGY_FILE: &str = "topol.top";

/// Run hints a framework (nanosheet) build records so a later MD run applies the
/// right `.mdp`/freeze settings — written into the build run directory.
pub const MD_FRAMEWORK_FILE: &str = "framework_run.json";

/// What a framework MD system needs a run to do: keep the molecule periodic
/// (flexible model) and/or freeze the sheet (rigid model). Persisted by the
/// build and reapplied to every stage of the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkRunMetadata {
    pub periodic_molecules: bool,
    pub freeze_group: Option<String>,
    pub framework_atom_count: usize,
}

impl FrameworkRunMetadata {
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).context("serializing framework run data")?;
        std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let json =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&json).with_context(|| format!("parsing {}", path.display()))
    }

    /// The freeze selection a run's [`prepare_system`] needs, if this system
    /// freezes its framework.
    ///
    /// [`prepare_system`]: crate::engines::gromacs::prepare_system
    pub fn freeze_selection(&self) -> Option<FreezeSelection> {
        self.freeze_group
            .as_ref()
            .map(|_| framework_freeze_selection(self.framework_atom_count))
    }

    /// Apply this system's run hints to a stage's `.mdp` settings.
    pub fn apply_to(&self, settings: &mut MdpSettings) {
        settings.periodic_molecules = self.periodic_molecules;
        settings.freeze = self.freeze_group.clone().map(|group| FreezeGroup { group });
    }
}

/// Load the framework run hints recorded by the entry's latest completed MD
/// system build, if it was a framework build.
pub fn load_framework_metadata_for_entry(
    state: &AppState,
    entry_id: u64,
) -> Option<FrameworkRunMetadata> {
    let run = state
        .tasks
        .latest_completed_run_for_result(TaskKind::BuildMdSystem, entry_id)?;
    let path = run.run_dir.as_ref()?.join(MD_FRAMEWORK_FILE);
    path.exists()
        .then(|| FrameworkRunMetadata::load(&path).ok())?
}

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
    fn rigid_framework_metadata_freezes_and_round_trips() {
        let meta = FrameworkRunMetadata {
            periodic_molecules: false,
            freeze_group: Some("Framework".to_string()),
            framework_atom_count: 50,
        };
        // Round-trips through JSON.
        let dir = std::env::temp_dir().join("phonon_framework_meta_roundtrip");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(MD_FRAMEWORK_FILE);
        meta.save(&path).unwrap();
        assert_eq!(FrameworkRunMetadata::load(&path).unwrap(), meta);

        // A rigid system freezes its first N atoms and does not mark periodicity.
        let sel = meta.freeze_selection().expect("rigid freezes");
        assert_eq!(sel.atom_indices.len(), 50);
        let mut settings = MdpSettings::nvt(300.0);
        meta.apply_to(&mut settings);
        assert!(settings.freeze.is_some());
        assert!(!settings.periodic_molecules);
    }

    #[test]
    fn flexible_framework_metadata_marks_periodic_without_freezing() {
        let meta = FrameworkRunMetadata {
            periodic_molecules: true,
            freeze_group: None,
            framework_atom_count: 50,
        };
        assert!(meta.freeze_selection().is_none());
        let mut settings = MdpSettings::nvt(300.0);
        meta.apply_to(&mut settings);
        assert!(settings.freeze.is_none());
        assert!(settings.periodic_molecules);
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
