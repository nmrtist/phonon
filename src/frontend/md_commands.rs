//! High-level `md` console commands: the scriptable mirror of the GUI MD flow.
//!
//! The flow's steps and the commands match them:
//! * `md build`    — the MD System Builder: wrap the active structure in a
//!   simulation box (if it has none) and capture the engine-neutral
//!   [`MdTopology`] (species + nonbonded parameters) for the project.
//! * `md solvate`  — fill the box with water and ions (Phonon-native), replacing
//!   the structure with the solvated system and extending the captured topology.
//! * `md simulate` — run EM → NVT → NPT → production on that system, generating
//!   every GROMACS file internally from the captured topology, then write
//!   analysis data.
//!
//! Users make only physical choices (simulation time, temperature, whether to
//! relax). No `.mdp`, topology, or group selection is ever surfaced. The
//! commands run synchronously, which suits headless `.psh`/CLI use; the GUI
//! drives the same engine functions on a worker thread.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use anyhow::{Result, anyhow, bail};

use crate::{
    backend::{
        runs::ensure_run_dir,
        tasks::{TaskStatus, task_controller_by_id},
    },
    domain::Structure,
    engines::{
        gromacs::{
            AnalysisContext, framework_run_hints, render_top, run_pipeline,
            runner::{PrepareSystemRequest, prepare_system},
            topology::TopologySource,
        },
        registry::{EngineId, EngineLaunch, EngineRegistry},
    },
    frontend::{
        md_support::{
            FrameworkRunMetadata, MD_FRAMEWORK_FILE, MD_TOPOLOGY_FILE, md_topology_path_for_entry,
            protocol_stage_specs,
        },
        state::AppState,
    },
    io::structure_io,
    workflows::molecular_dynamics::{
        FrameworkMode, MdProtocolOptions, MdSystemConfig, MdTopology, SolvationOptions, WaterModel,
        build_md_system, is_framework_shape, solvate,
    },
};

const STAGE_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);
const ANALYSIS_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Dispatch `md <subcommand> ...`.
pub fn md_command(state: &mut AppState, args: &[String]) -> Result<String> {
    let Some(sub) = args.first().map(String::as_str) else {
        bail!("usage: md <build|solvate|simulate> [options]");
    };
    match sub {
        "build" => md_build(state, &args[1..]),
        "solvate" => md_solvate(state, &args[1..]),
        "simulate" => md_simulate(state, &args[1..]),
        other => bail!("unknown md subcommand `{other}` (expected build, solvate, or simulate)"),
    }
}

/// Parse a `--water` token into a [`WaterModel`].
fn parse_water_model(token: &str) -> Result<WaterModel> {
    match token.trim().to_ascii_lowercase().as_str() {
        "tip4p" => Ok(WaterModel::Tip4p),
        "tip4pew" => Ok(WaterModel::Tip4pEw),
        "tip3p" => Ok(WaterModel::Tip3p),
        "tip5p" => Ok(WaterModel::Tip5p),
        "tip5pe" => Ok(WaterModel::Tip5pEwald),
        "spc" => Ok(WaterModel::Spc),
        "spce" | "spc/e" => Ok(WaterModel::SpcE),
        other => bail!(
            "unknown water model `{other}` (expected tip4p, tip4pew, tip3p, tip5p, tip5pe, spc, or spce)"
        ),
    }
}

// ---- option parsing ---------------------------------------------------------

/// Pull `--key value` / `--flag` pairs from the argument list.
struct Flags {
    values: std::collections::BTreeMap<String, String>,
    flags: std::collections::BTreeSet<String>,
}

impl Flags {
    fn parse(args: &[String]) -> Result<Self> {
        let mut values = std::collections::BTreeMap::new();
        let mut flags = std::collections::BTreeSet::new();
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            let Some(key) = arg.strip_prefix("--") else {
                bail!("unexpected argument `{arg}` (expected --key value)");
            };
            if let Some((k, v)) = key.split_once('=') {
                values.insert(k.to_string(), v.to_string());
                i += 1;
            } else if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                values.insert(key.to_string(), args[i + 1].clone());
                i += 2;
            } else {
                flags.insert(key.to_string());
                i += 1;
            }
        }
        Ok(Self { values, flags })
    }

    fn flag(&self, key: &str) -> bool {
        self.flags.contains(key)
    }

    fn f32(&self, key: &str) -> Result<Option<f32>> {
        self.values
            .get(key)
            .map(|v| {
                v.parse::<f32>()
                    .map_err(|_| anyhow!("invalid number for --{key}: {v}"))
            })
            .transpose()
    }

    fn str(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}

/// Parse a time like `200ns`, `500ps`, or a bare number (picoseconds).
fn parse_time_ps(value: &str) -> Result<f64> {
    let v = value.trim().to_ascii_lowercase();
    if let Some(ns) = v.strip_suffix("ns") {
        Ok(ns.trim().parse::<f64>()? * 1000.0)
    } else if let Some(ps) = v.strip_suffix("ps") {
        Ok(ps.trim().parse::<f64>()?)
    } else {
        Ok(v.parse::<f64>()?)
    }
}

// ---- shared helpers ---------------------------------------------------------

fn resolve_launch(state: &AppState) -> Result<EngineLaunch> {
    let registry = EngineRegistry::probe(&state.config.engine_overrides);
    registry.launch(EngineId::GROMACS).cloned().ok_or_else(|| {
        anyhow!(
            "Could not find GROMACS. Install it and ensure `gmx` is on PATH, or configure its \
             launch (including WSL) in Settings -> Engines."
        )
    })
}

// ---- md build ---------------------------------------------------------------

fn md_build(state: &mut AppState, args: &[String]) -> Result<String> {
    if state.structure().atoms.is_empty() {
        bail!("no active structure; open one before `md build`");
    }
    let flags = Flags::parse(args)?;
    // A periodic framework (nanosheet) is captured with its bond-derived
    // topology; `--framework rigid|flexible` picks the model (default rigid).
    let framework_mode = match flags.str("framework") {
        Some("flexible") => Some(FrameworkMode::Flexible),
        Some("rigid") | None => Some(FrameworkMode::Rigid),
        Some(other) => bail!("unknown --framework mode `{other}`; use rigid or flexible"),
    };
    // `--custom-ff <name>` merges a saved custom force field, enabling elements
    // the built-in tables lack (or overriding their types) for a framework build.
    let custom_force_field = match flags.str("custom-ff") {
        Some(name) => Some(crate::backend::force_fields::load_force_field(name)?),
        None => None,
    };

    let task_run_id = create_cli_task_run(state, "build-md-system")?;
    let run_dir = ensure_cli_task_run_dir(state, task_run_id)?;
    mark_cli_task_status(state, task_run_id, TaskStatus::Running)?;

    let result = (|| {
        if is_framework_shape(state.structure()) {
            // Keep the periodic cell as built (re-boxing would break the
            // sheet's bonds to its periodic images); capture the framework
            // topology and the run hints a later `md simulate` applies. A custom
            // force field, when given, covers elements the built-in tables lack
            // and is inlined into the captured topology.
            let mode = framework_mode.unwrap_or(FrameworkMode::Rigid);
            let structure = state.structure().clone();
            let custom_types = custom_force_field
                .as_deref()
                .map(crate::engines::gromacs::custom_ff::custom_types)
                .unwrap_or_default();
            let mut topology = MdTopology::framework_with_custom(&structure, mode, &custom_types)?;
            topology.inline_force_field = custom_force_field.clone();
            let atom_count = structure.atoms.len();
            let save_path = structure_io::default_structure_save_path(&structure, None);
            let entry_id = state.entries.add_entry(structure, None, save_path);
            activate_entry(state, entry_id);
            record_cli_task_result_entry(state, task_run_id, entry_id)?;

            topology.save(&run_dir.join(MD_TOPOLOGY_FILE))?;
            let hints = framework_run_hints(mode);
            FrameworkRunMetadata {
                periodic_molecules: hints.periodic_molecules,
                freeze_group: hints.freeze_group,
                framework_atom_count: atom_count,
            }
            .save(&run_dir.join(MD_FRAMEWORK_FILE))?;

            return Ok(format!(
                "Framework MD system ready ({} model): {atom_count} atoms; topology captured",
                mode.label()
            ));
        }

        let structure = if state.structure().cell.is_none() {
            let (boxed, _report) = build_md_system(state.structure(), &MdSystemConfig::default())?;
            boxed
        } else {
            state.structure().clone()
        };
        let save_path = structure_io::default_structure_save_path(&structure, None);
        let entry_id = state.entries.add_entry(structure, None, save_path);
        activate_entry(state, entry_id);
        record_cli_task_result_entry(state, task_run_id, entry_id)?;

        let topology = MdTopology::from_structure(state.structure())?;
        topology.save(&run_dir.join(MD_TOPOLOGY_FILE))?;

        Ok(format!(
            "MD system ready: {} atoms, {} species; topology captured",
            topology.atom_count(),
            topology.species.len()
        ))
    })();

    finish_cli_task(state, task_run_id, result)
}

// ---- md solvate -------------------------------------------------------------

/// Fill the simulation box with water and ions (Phonon-native solvation),
/// replacing the active structure with the solvated system and updating the
/// captured topology.
///
/// Options: `--water spc|spce|tip3p|tip4p|...`, `--conc <mol/L>`, `--cation NA`,
/// `--anion CL`, `--no-neutralize`. Placement is geometry only — no force field.
fn md_solvate(state: &mut AppState, args: &[String]) -> Result<String> {
    let flags = Flags::parse(args)?;

    if state.structure().atoms.is_empty() {
        bail!("no active structure; open one and run `md build` first");
    }
    // Solvation needs a periodic box; build a default one if missing.
    if state.structure().cell.is_none() {
        let (boxed, _report) = build_md_system(state.structure(), &MdSystemConfig::default())?;
        *state.structure_mut() = boxed;
        state.mark_structure_changed();
    }

    let mut options = SolvationOptions::default();
    if let Some(w) = flags.str("water") {
        options.water = parse_water_model(w)?;
    }
    if let Some(c) = flags.str("cation") {
        options.positive_ion = c.to_ascii_uppercase();
    }
    if let Some(a) = flags.str("anion") {
        options.negative_ion = a.to_ascii_uppercase();
    }
    if let Some(conc) = flags.f32("conc")? {
        options.concentration_molar = Some(conc);
    }
    if flags.flag("no-neutralize") {
        options.neutralize = false;
    }

    let (solvated, report) = solvate(state.structure(), &options)?;
    let atom_count = solvated.atoms.len();
    *state.structure_mut() = solvated;
    state.mark_structure_changed();

    Ok(format!(
        "Solvated with {}: added {} water, {} {}, {} {}; system now {} atoms",
        options.water.label(),
        report.water_added,
        report.cations_added,
        options.positive_ion,
        report.anions_added,
        options.negative_ion,
        atom_count,
    ))
}

// ---- md simulate ------------------------------------------------------------

fn md_simulate(state: &mut AppState, args: &[String]) -> Result<String> {
    let flags = Flags::parse(args)?;

    let production_ps = match flags.str("time") {
        Some(t) => parse_time_ps(t)?,
        None => 1000.0,
    };
    let temperature_k = flags.f32("temperature")?.unwrap_or(300.0);
    // Relax (EM/NVT/NPT) by default; `--no-relax` skips straight to production.
    let relax = !flags.flag("no-relax");
    // Save a playable trajectory for every stage by default; `--no-trajectory`
    // keeps only the final structures.
    let save_trajectory = !flags.flag("no-trajectory");

    let options = MdProtocolOptions {
        production_ps,
        timestep_ps: 0.002,
        temperature_k,
        relax_before_production: relax,
        save_trajectory,
    };

    let structure = require_boxed_structure(state)?;

    let task_run_id = create_cli_task_run(state, "run-md")?;
    let work_dir = ensure_cli_task_run_dir(state, task_run_id)?;
    state
        .tasks
        .set_engine_label(task_run_id, Some("GROMACS".to_string()));
    sync_cli_task_manifest(state, task_run_id)?;
    mark_cli_task_status(state, task_run_id, TaskStatus::Running)?;

    let result = (|| {
        // Consume the topology captured at build time, then have the engine
        // render it to a GROMACS `.top` here, at run time.
        let topology = load_active_or_derive_md_topology(state).map_err(|_| {
            anyhow!("no MD system found; run `md build` first to prepare the system")
        })?;

        // Framework (nanosheet) systems carry run hints: keep the molecule
        // periodic (flexible) and/or freeze the sheet (rigid).
        let framework_meta = state.entries.active_entry_id().and_then(|id| {
            crate::frontend::md_support::load_framework_metadata_for_entry(state, id)
        });

        let launch = resolve_launch(state)?;
        let system = prepare_system(PrepareSystemRequest {
            structure,
            topology: TopologySource::Inline(render_top(&topology)),
            working_dir: work_dir.clone(),
            freeze: framework_meta.as_ref().and_then(|m| m.freeze_selection()),
        })?;

        let mut stages = protocol_stage_specs(&options);
        if let Some(meta) = &framework_meta {
            for spec in &mut stages {
                meta.apply_to(&mut spec.settings);
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let results = run_pipeline(
            system,
            stages,
            launch.clone(),
            STAGE_TIMEOUT,
            Arc::clone(&cancel),
            |_| {},
        )?;

        let production = results
            .last()
            .ok_or_else(|| anyhow!("pipeline produced no stages"))?;
        // The production stage writes the compressed `.xtc`; take the last stage
        // that produced one so playback follows the actual MD trajectory.
        let trajectory = results
            .iter()
            .rev()
            .find_map(|stage| stage.trajectory.clone());
        let save_path = structure_io::default_structure_save_path(&production.structure, None);
        let entry_id = state
            .entries
            .add_entry(production.structure.clone(), None, save_path);
        activate_entry(state, entry_id);
        // Mark the entry as an MD-run output (provenance badge + playback gating),
        // mirroring the GUI run path.
        let project_root = state
            .workspace
            .project()
            .map(|project| project.root.clone());
        let origin = super::dispatcher::md_run_origin(trajectory, project_root.as_deref());
        state.entries.set_entry_origin(entry_id, origin);
        record_cli_task_result_entry(state, task_run_id, entry_id)?;

        let analysis_summary = run_analysis(&work_dir, &launch, production, cancel);

        Ok(format!(
            "molecular dynamics complete ({production_ps:.0} ps production at {temperature_k:.0} K){analysis_summary}"
        ))
    })();

    finish_cli_task(state, task_run_id, result)
}

/// Extract thermodynamic terms (Temperature, Potential) from the production
/// energy file. Analysis failures are reported but do not fail the whole command
/// (the trajectory is the primary deliverable).
fn run_analysis(
    work_dir: &Path,
    launch: &EngineLaunch,
    production: &crate::engines::gromacs::StageResult,
    cancel: Arc<AtomicBool>,
) -> String {
    let ctx = AnalysisContext {
        working_dir: work_dir.to_path_buf(),
        gmx_launch: launch.clone(),
        max_duration: ANALYSIS_TIMEOUT,
    };
    match crate::engines::gromacs::gmx_energy(
        &ctx,
        &production.edr,
        "energy.xvg",
        &["Temperature", "Potential"],
        cancel,
        |_| {},
    ) {
        Ok(_) => "; analysis: energy.xvg (Temperature, Potential)".to_string(),
        Err(_) => String::new(),
    }
}

/// Clone the active structure after checking it is a usable MD system: non-empty
/// and carrying a simulation box (as produced by `md build` / the System
/// Builder, or by opening a `.gro` with box vectors).
fn require_boxed_structure(state: &AppState) -> Result<Structure> {
    let structure = state.structure();
    if structure.atoms.is_empty() {
        bail!("no active structure; open or build a system before `md simulate`");
    }
    if structure.cell.is_none() {
        bail!(
            "the active structure has no simulation box; run `md build` (or the MD System \
             Builder) first"
        );
    }
    Ok(structure.clone())
}

fn active_entry_md_topology_path(state: &AppState) -> Option<PathBuf> {
    let entry_id = state.entries.active_entry_id()?;
    md_topology_path_for_entry(state, entry_id)
}

fn load_active_or_derive_md_topology(state: &AppState) -> Result<MdTopology> {
    if let Some(path) = active_entry_md_topology_path(state) {
        return MdTopology::load(&path);
    }
    MdTopology::from_structure(state.structure())
}

fn create_cli_task_run(state: &mut AppState, template_id: &'static str) -> Result<u64> {
    let controller = task_controller_by_id(template_id)
        .copied()
        .ok_or_else(|| anyhow!("unknown task template `{template_id}`"))?;
    let task_run_id = state.tasks.create_task_run(controller);
    state
        .tasks
        .set_source_entry_id(task_run_id, state.entries.active_entry_id());
    sync_cli_task_manifest(state, task_run_id)?;
    Ok(task_run_id)
}

fn ensure_cli_task_run_dir(state: &mut AppState, task_run_id: u64) -> Result<PathBuf> {
    let task = state
        .tasks
        .task_run(task_run_id)
        .ok_or_else(|| anyhow!("task run #{task_run_id} not found"))?
        .clone();
    if let Some(run_dir) = task.run_dir {
        return Ok(run_dir);
    }
    if !task.uses_run_directory {
        bail!("task {} does not use a run directory", task.title);
    }
    let runs_dir = state.runs_dir();
    let name = crate::backend::runs::default_run_name(&runs_dir, task.controller_id);
    let run_dir = ensure_run_dir(&runs_dir, &name)?;
    state.tasks.set_run_dir(task_run_id, run_dir.clone());
    sync_cli_task_manifest(state, task_run_id)?;
    Ok(run_dir)
}

fn sync_cli_task_manifest(state: &AppState, task_run_id: u64) -> Result<()> {
    super::task_executor::sync_task_manifest(state, task_run_id)
}

fn mark_cli_task_status(state: &mut AppState, task_run_id: u64, status: TaskStatus) -> Result<()> {
    super::task_executor::mark_task_status(state, task_run_id, status)
}

fn record_cli_task_result_entry(
    state: &mut AppState,
    task_run_id: u64,
    entry_id: u64,
) -> Result<()> {
    super::task_executor::record_task_result_entry(state, task_run_id, entry_id)
}

fn finish_cli_task(
    state: &mut AppState,
    task_run_id: u64,
    result: Result<String>,
) -> Result<String> {
    match result {
        Ok(message) => {
            mark_cli_task_status(state, task_run_id, TaskStatus::Completed)?;
            Ok(message)
        }
        Err(error) => {
            mark_cli_task_status(state, task_run_id, TaskStatus::Failed)?;
            Err(error)
        }
    }
}

fn activate_entry(state: &mut AppState, entry_id: u64) {
    state.save_viewport_for_active_entry();
    state.entries.activate_entry(entry_id);
    state.ensure_entry_loaded(entry_id);
    state.history.set_active_entry(Some(entry_id));
    state.ui.entry_list.selected_entry_ids.clear();
    state.ui.entry_list.selected_entry_ids.insert(entry_id);
    state.load_viewport_for_active_entry();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn flags_parse_values_flags_and_equals_form() {
        let f = Flags::parse(&args(&["--time", "1ns", "--temperature=300", "--no-relax"])).unwrap();
        assert_eq!(f.str("time"), Some("1ns"));
        assert_eq!(f.str("temperature"), Some("300"));
        assert!(f.flag("no-relax"));
        assert!(!f.flag("relax"));
    }

    #[test]
    fn unprefixed_argument_is_rejected() {
        assert!(Flags::parse(&args(&["time", "1ns"])).is_err());
    }

    #[test]
    fn parse_time_handles_ns_ps_and_bare() {
        assert_eq!(parse_time_ps("200ns").unwrap(), 200_000.0);
        assert_eq!(parse_time_ps("500ps").unwrap(), 500.0);
        assert_eq!(parse_time_ps("250").unwrap(), 250.0);
    }
}
