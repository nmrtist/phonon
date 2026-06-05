use std::{path::PathBuf, time::Duration};

use anyhow::{anyhow, bail};
use eframe::egui;

use crate::backend::config::save_config;
use crate::{
    backend::{
        entries::EntryStore,
        housekeeping,
        project::{
            ProjectSession, WorkspaceSession, create_project, open_project as open_project_dir,
            remember_opened_project, save_project as save_project_session, save_project_ref,
        },
        runs::ensure_run_dir,
        storage::{ProjectSnapshot, ProjectSnapshotRef},
        tasks::{TaskKind, TaskManager, TaskPanelKind, TaskStatus, task_controller_by_id},
    },
    domain::Structure,
    engines::{
        gromacs::{BuildRequest, IonOptions, StageSpec, TopologySource, render_top},
        registry::{EngineId, EngineRegistry},
    },
    frontend::{
        actions::AppAction,
        jobs::{
            EngineWorkerMessage, GromacsPipelineRequest, OptimizationWorkerMessage,
            engine_poll_frame, optimization_finished_message, request_next_optimization_poll,
            spawn_gromacs_build_job, spawn_gromacs_pipeline_job, spawn_optimization_job,
        },
        md_support::{gromacs_topology_path_for_entry, load_md_topology_for_entry},
        services::{
            BuildingBlockService, NanosheetService, ReticularService, StructureService,
            require_periodic_structure,
        },
        state::AppState,
        structure_import::{import_document, load_document},
        task_executor::task_executor,
    },
    io::{pdb_fetch, structure_io},
};

pub fn dispatch(state: &mut AppState, action: AppAction, ctx: &egui::Context) {
    // Project lifecycle actions persist themselves (open/create/close/save), so
    // they opt out of change-detected autosave to avoid a redundant save.
    let manages_own_persistence = matches!(
        action,
        AppAction::OpenProject
            | AppAction::OpenRecentProject(_)
            | AppAction::CreateProject
            | AppAction::CloseProject
            | AppAction::SaveProject
    );
    // Autosave only when the persisted entry state actually changes — an entry
    // added, removed, or edited. View-only changes (camera, render styles,
    // selection, active tab) don't move this fingerprint and are saved at exit
    // instead, so navigating or restyling never schedules a save.
    let fingerprint_before = (!manages_own_persistence).then(|| state.entries_fingerprint());
    match action {
        AppAction::CreateProject => create_project_action(state),
        AppAction::OpenProject => open_project_action(state),
        AppAction::OpenRecentProject(path) => open_project_path(state, path),
        AppAction::CloseProject => close_project(state),
        AppAction::SaveProject => save_project(state),
        AppAction::NewEmptyEntry => new_empty_entry(state),
        AppAction::OpenFile => open_file(state),
        AppAction::OpenPdbFetchDialog => open_pdb_fetch_dialog(state),
        AppAction::FetchPdb => fetch_pdb(state),
        AppAction::CancelPdbFetch => state.ui.pending_pdb_fetch = None,
        AppAction::Save => save(state),
        AppAction::SaveAs => save_as(state),
        AppAction::Undo => undo(state),
        AppAction::Redo => redo(state),
        AppAction::EditStructure => edit_structure(state),
        AppAction::ApplyStructureEdits => apply_structure_edits(state),
        AppAction::CancelStructureEdits => cancel_structure_edits(state),
        AppAction::SelectAll => select_all(state),
        AppAction::InvertSelection => invert_selection(state),
        AppAction::ClearSelection => clear_selection(state),
        AppAction::SelectCategory(category) => select_category(state, category),
        AppAction::SelectAtom { atom_index, toggle } => select_atom(state, atom_index, toggle),
        AppAction::SetSelectionStyle(style) => set_selection_style(state, style),
        AppAction::ResetSelectionStyle => reset_selection_style(state),
        AppAction::SetCategoryStyle(category, style) => set_category_style(state, category, style),
        AppAction::ActivateEntry(entry_id) => activate_entry(state, entry_id),
        AppAction::DeleteEntry(entry_id) => delete_entry(state, entry_id),
        AppAction::DeleteEntries(ids) => delete_entries(state, ids),
        AppAction::RenameEntry { entry_id, new_name } => {
            state.entries.rename_entry(entry_id, new_name)
        }
        AppAction::CreateGroup { name } => create_group(state, name),
        AppAction::RenameGroup { group_id, new_name } => rename_group(state, &group_id, &new_name),
        AppAction::DeleteGroup(group_id) => delete_group(state, &group_id),
        AppAction::DeleteGroupWithEntries(group_id) => delete_group_with_entries(state, &group_id),
        AppAction::MoveEntryToGroup { entry_id, group_id } => {
            move_entry_to_group(state, entry_id, &group_id)
        }
        AppAction::CreateTask(template_id) => create_task_from_template(state, template_id),
        AppAction::RunTask(task_run_id) => run_task(state, task_run_id),
        AppAction::OpenTaskPanel(task_run_id) => open_task_panel(state, task_run_id),
        AppAction::CloseTaskPanel(task_run_id) => close_task_panel(state, task_run_id),
        AppAction::ActivateTaskPanel(task_run_id) => activate_task_panel(state, task_run_id),
        AppAction::PreviewFramework => preview_framework_task(state),
        AppAction::BuildFramework => accept_framework_task(state),
        AppAction::CancelFramework => cancel_framework_task(state),
        AppAction::PreviewNanosheet => preview_nanosheet_task(state),
        AppAction::BuildNanosheet => accept_nanosheet_task(state),
        AppAction::CancelNanosheet => cancel_nanosheet_task(state),
        AppAction::SaveBuildingBlock => save_block_editor_task(state),
        AppAction::CancelBuildingBlock => cancel_block_editor_task(state),
        AppAction::StartOptimization => start_pending_optimization(state),
        AppAction::CancelOptimizationPrompt => cancel_pending_optimization_request(state),
        AppAction::ConfirmSupercell => confirm_pending_supercell(state),
        AppAction::CancelSupercellPrompt => cancel_pending_supercell_request(state),
        AppAction::ConfirmProteinPrep => confirm_pending_protein_prep(state),
        AppAction::CancelProteinPrepPrompt => cancel_pending_protein_prep_request(state),
        AppAction::ConfirmMdSystem => confirm_pending_md_system(state),
        AppAction::CancelMdSystemPrompt => cancel_pending_md_system_request(state),
        AppAction::PickMdTopologyOverride => pick_md_topology_override(state),
        AppAction::SelectCustomForceField(name) => select_custom_force_field(state, name.clone()),
        AppAction::SaveCustomForceField => save_custom_force_field(state),
        AppAction::DeleteCustomForceField(name) => delete_custom_force_field(state, name.as_str()),
        AppAction::ImportCustomForceFieldFile => import_custom_force_field_file(state),
        AppAction::StartMdRun => start_pending_md_run(state),
        AppAction::CancelMdRunPrompt => cancel_pending_md_run_request(state),
        AppAction::RefreshEngineRegistry => reprobe_engines(state),
        AppAction::DetectEngineVersions => detect_engine_versions(state),
        AppAction::ApplyEngineOverride(id) => apply_engine_override(state, id),
        AppAction::ClearEngineOverride(id) => clear_engine_override(state, id),
        AppAction::BrowseEngineProgram(id) => browse_engine_program(state, id),
        AppAction::RunConsoleCommand(command) => run_console_command(state, &command),
        AppAction::SetThemeMode(mode) => set_theme_mode(state, mode, ctx),
    }
    if let Some(before) = fingerprint_before
        && state.entries_fingerprint() != before
    {
        // Entries changed (add/remove/edit). Coalesce rather than save
        // synchronously: a burst of edits collapses into one save once the user
        // pauses (see `flush_pending_autosave`). The flush still skips
        // re-serializing the (large) undo/redo history; that is persisted only at
        // explicit checkpoints (save, open, close, shutdown).
        let now = ctx.input(|input| input.time);
        state.request_autosave(now, AUTOSAVE_DEBOUNCE_SECS);
    }
}

/// Apply and persist the light/dark appearance preference. egui switches the
/// active theme immediately; the choice is written to the global settings file.
fn set_theme_mode(
    state: &mut AppState,
    mode: crate::backend::config::ThemeMode,
    ctx: &egui::Context,
) {
    state.config.theme = mode;
    crate::frontend::theme::set_preference(ctx, mode);
    if let Err(error) = save_config(&state.config) {
        state.set_message(format!("Could not save theme preference: {error}"));
    }
}

/// How long after an entry change a coalesced autosave waits before flushing.
/// Long enough to absorb a burst of edits, short enough that an isolated change
/// is saved promptly.
const AUTOSAVE_DEBOUNCE_SECS: f64 = 0.5;

/// Flush a coalesced autosave once its debounce window has elapsed. Called every
/// frame from the app loop; a no-op when nothing is pending. While a save is
/// still pending it requests a repaint at the deadline so the flush fires even
/// if the user stops interacting.
pub fn flush_pending_autosave(state: &mut AppState, ctx: &egui::Context) {
    let Some(deadline) = state.autosave_deadline() else {
        return;
    };
    let now = ctx.input(|input| input.time);
    if now >= deadline {
        // `persist_project` clears the deadline itself.
        persist_project(state, false);
    } else {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(deadline - now));
    }
}

/// Clean-shutdown checkpoint for window close: persist the project (including
/// undo history) and release the session lock so the next launch knows the
/// session ended cleanly. Skips database compaction to keep exit responsive.
pub fn shutdown(state: &mut AppState) {
    if !state.workspace.is_project() {
        return;
    }
    persist_project(state, true);
    if let Some(project) = state.workspace.project() {
        housekeeping::release_lock(project);
    }
}

fn persist_project(state: &mut AppState, persist_history: bool) {
    // Any pending coalesced autosave is subsumed by this save.
    state.clear_autosave_deadline();
    let Some(project) = state.workspace.project() else {
        return;
    };
    // Save from borrowed references into the live state rather than cloning the
    // whole workspace: in an entry-heavy project (e.g. a 20-model NMR ensemble)
    // the clone dominated and made every interaction lag. `view` is the only
    // small owned value the snapshot needs.
    let view = state.project_view_settings();
    let snapshot = ProjectSnapshotRef {
        name: project.name.as_str(),
        entries: &state.entries,
        tasks: &state.tasks,
        view: &view,
        history: &state.history,
    };
    let result = save_project_ref(project, &snapshot, persist_history);
    if let Err(error) = result {
        state.set_message(format!("Project save failed: {error}"));
    }
}

fn run_console_command(state: &mut AppState, command: &str) {
    let prompt = format!("psh> {command}");
    state.output_log.push(prompt);
    state.ui.console.history.push(command.to_string());
    match crate::frontend::console::execute_console_line(state, command) {
        Ok(message) => {
            if !message.is_empty() {
                state.set_message(message);
            }
        }
        Err(error) => state.set_message(format!("command failed: {error}")),
    }
}

pub fn handle_history_shortcuts(state: &mut AppState, ctx: &egui::Context) {
    if !state.history_navigation_enabled() || ctx.egui_wants_keyboard_input() {
        return;
    }

    let (undo_pressed, redo_pressed) = ctx.input(|input| {
        let command = input.modifiers.command || input.modifiers.ctrl;
        (
            command && input.key_pressed(egui::Key::Z) && !input.modifiers.shift,
            command
                && (input.key_pressed(egui::Key::Y)
                    || (input.modifiers.shift && input.key_pressed(egui::Key::Z))),
        )
    });

    if undo_pressed {
        dispatch(state, AppAction::Undo, ctx);
    } else if redo_pressed {
        dispatch(state, AppAction::Redo, ctx);
    }
}

pub fn poll_jobs(state: &mut AppState, ctx: &egui::Context) {
    poll_engine_job(state, ctx);
    poll_optimization_job(state, ctx);
}

fn poll_engine_job(state: &mut AppState, ctx: &egui::Context) {
    let Some(mut running) = state.jobs.take_engine() else {
        return;
    };
    let task_run_id = state.active_task_run;
    let mut before = state.optimization_origin.take();
    let fingerprint_before = state.entries_fingerprint();

    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        running
            .cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        state.set_message(format!("{} {} stopping", running.engine, running.job_kind));
    }

    let mut finished = false;
    let mut saw_progress = false;
    let mut commit_history = false;
    let engine_name = running.engine;
    // The same engine-job machinery backs both "build the MD system" and "run
    // MD"; the job_kind says which task to mark done on completion.
    let task_kind = if running.job_kind == "build-md" {
        TaskKind::BuildMdSystem
    } else {
        TaskKind::RunMd
    };

    while let Ok(message) = running.receiver.try_recv() {
        match message {
            EngineWorkerMessage::Stage(stage) => {
                state.set_message(format!("{engine_name}: {stage}"));
                running.latest_stage = Some(stage);
            }
            EngineWorkerMessage::Log(line) => {
                running.append_log(line);
            }
            EngineWorkerMessage::Finished(success) => {
                let save_path = structure_io::default_structure_save_path(&success.structure, None);
                let entry_id = add_and_show_entry(state, success.structure, None, save_path);
                if let Some(task_run_id) = task_run_id {
                    record_task_result_entry(state, task_run_id, entry_id);
                }
                state.set_message(success.summary);
                saw_progress = false;
                commit_history = false;
                complete_active_task(state, task_kind, TaskStatus::Completed);
                finished = true;
            }
            EngineWorkerMessage::Failed(error) => {
                state.set_message(format!("{engine_name} failed: {error}"));
                complete_active_task(state, task_kind, TaskStatus::Failed);
                finished = true;
            }
        }
    }

    if !finished {
        state.optimization_origin = before;
        state.jobs.set_engine(running);
        ctx.request_repaint_after(engine_poll_frame());
    } else {
        if commit_history || saw_progress {
            if let Some(before) = before.take() {
                state.history.push_undo(before);
            }
        } else {
            before.take();
        }
        // A completed build adds/edits an entry; persist that result (debounced).
        if state.entries_fingerprint() != fingerprint_before {
            let now = ctx.input(|input| input.time);
            state.request_autosave(now, AUTOSAVE_DEBOUNCE_SECS);
        }
        ctx.request_repaint();
    }
}

fn poll_optimization_job(state: &mut AppState, ctx: &egui::Context) {
    let Some(mut running) = state.jobs.take_optimizer() else {
        return;
    };
    let mut before = state.optimization_origin.take();
    let fingerprint_before = state.entries_fingerprint();

    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        running
            .cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        state.set_message(match running.latest_report {
            Some(report) => format!(
                "forcefield optimization stopping: energy {:.3} -> {:.3} in {} steps",
                report.initial_energy, report.final_energy, report.steps
            ),
            None => "forcefield optimization stopping".to_string(),
        });
    }

    let mut finished = false;
    let mut saw_progress = false;
    let mut commit_history = false;
    while let Ok(message) = running.receiver.try_recv() {
        match message {
            OptimizationWorkerMessage::Progress { structure, report } => {
                *state.structure_mut() = structure;
                state.mark_structure_changed();
                running.latest_report = Some(report);
                saw_progress = true;
                state.set_source_path(None);
                state.set_message(format!(
                    "forcefield optimizing: step {}, energy {:.3}; press Esc to stop",
                    report.steps, report.final_energy
                ));
            }
            OptimizationWorkerMessage::Finished { structure, report } => {
                *state.structure_mut() = structure;
                state.mark_structure_changed();
                running.latest_report = Some(report);
                saw_progress = true;
                commit_history = true;
                state.set_source_path(None);
                state.set_message(optimization_finished_message(report));
                complete_active_task(state, TaskKind::OptimizeGeometry, TaskStatus::Completed);
                complete_active_task(
                    state,
                    TaskKind::OptimizeCrystalGeometry,
                    TaskStatus::Completed,
                );
                finished = true;
            }
            OptimizationWorkerMessage::Failed(error) => {
                state.set_message(format!("forcefield optimization failed: {error}"));
                complete_active_task(state, TaskKind::OptimizeGeometry, TaskStatus::Failed);
                complete_active_task(state, TaskKind::OptimizeCrystalGeometry, TaskStatus::Failed);
                finished = true;
            }
        }
    }

    if !finished {
        state.optimization_origin = before;
        state.jobs.set_optimizer(running);
        request_next_optimization_poll(ctx);
    } else {
        if commit_history || saw_progress {
            if let Some(before) = before.take() {
                state.history.push_undo(before);
            }
        } else if let Some(before) = before.take() {
            state.restore_edit_snapshot(before);
        }
        // Persist the finished (or reverted) geometry once, not per step.
        if state.entries_fingerprint() != fingerprint_before {
            let now = ctx.input(|input| input.time);
            state.request_autosave(now, AUTOSAVE_DEBOUNCE_SECS);
        }
        ctx.request_repaint();
    }
}

fn reset_transient_state(state: &mut AppState) {
    state.cancel_transient_jobs();
    state.ui.pending_optimization = None;
    state.ui.pending_supercell = None;
    state.ui.pending_md_system = None;
    state.ui.pending_md_run = None;
    state.ui.editor = None;
    state.ui.reticular_builder = None;
    state.ui.block_editor = None;
    state.edit_origin = None;
    state.builder_origin = None;
    state.optimization_origin = None;
    state.ui.hovered_atom = None;
    state.ui.viewport_cache.clear();
    state.active_task_run = None;
}

fn replace_workspace_from_project(
    state: &mut AppState,
    project: ProjectSession,
    snapshot: ProjectSnapshot,
) {
    // Release the lock on the project we are leaving, then take the new one.
    if let Some(previous) = state.workspace.project() {
        housekeeping::release_lock(previous);
    }
    let recovered_from_crash = housekeeping::acquire_lock(&project);
    state.workspace = WorkspaceSession::Project(project.clone());
    state.entries = snapshot.entries;
    state.tasks = snapshot.tasks;
    state.history = snapshot.history;
    state
        .history
        .set_active_entry(state.entries.active_entry_id());
    state.ui.project_viewport = snapshot.view.viewport;
    state.ui.viewport = state.ui.project_viewport.clone();
    state.ui.entry_viewports = snapshot.view.entry_viewports;
    state.ui.entry_list.selected_entry_ids.clear();
    if let Some(id) = state.entries.active_entry_id() {
        state.ui.entry_list.selected_entry_ids.insert(id);
    }
    reset_transient_state(state);
    state.load_viewport_for_active_entry();
    let set_current_dir_error = std::env::set_current_dir(&project.root).err();
    if let Err(error) =
        remember_opened_project(&mut state.config, &mut state.recent_projects, &project)
    {
        state.set_message(format!(
            "Opened project, but settings update failed: {error}"
        ));
    } else if let Some(error) = set_current_dir_error {
        state.set_message(format!(
            "Opened project {}, but working directory update failed: {error}",
            project.name
        ));
    } else {
        state.set_message(format!("Opened project {}", project.name));
    }
    if recovered_from_crash {
        state.set_message(format!(
            "Opened project {} (recovered: previous session did not close cleanly)",
            project.name
        ));
    }
}

fn create_project_action(state: &mut AppState) {
    let Some(path) = rfd::FileDialog::new()
        .set_directory(&state.config.default_project_dir)
        .set_file_name("New Project")
        .save_file()
    else {
        state.set_message("Create project canceled");
        return;
    };
    let Some(parent) = path.parent() else {
        state.set_message("Project path must have a parent directory");
        return;
    };
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        state.set_message("Project name cannot be empty");
        return;
    };

    match create_project(parent, name).and_then(|project| {
        let snapshot = state.project_snapshot().unwrap_or_else(|| ProjectSnapshot {
            name: project.name.clone(),
            entries: state.entries.clone(),
            tasks: state.tasks.clone(),
            view: state.project_view_settings(),
            history: state.history.clone(),
        });
        let snapshot = ProjectSnapshot {
            name: project.name.clone(),
            ..snapshot
        };
        save_project_session(&project, &snapshot, true)?;
        Ok((project, snapshot))
    }) {
        Ok((project, snapshot)) => replace_workspace_from_project(state, project, snapshot),
        Err(error) => state.set_message(format!("Create project failed: {error}")),
    }
}

fn open_project_action(state: &mut AppState) {
    let Some(path) = rfd::FileDialog::new()
        .set_directory(&state.config.default_project_dir)
        .pick_folder()
    else {
        return;
    };
    open_project_path(state, path);
}

fn open_project_path(state: &mut AppState, path: PathBuf) {
    persist_project(state, true);
    match open_project_dir(&path) {
        Ok((project, snapshot)) => replace_workspace_from_project(state, project, snapshot),
        Err(error) => state.set_message(error.to_string()),
    }
}

fn close_project(state: &mut AppState) {
    persist_project(state, true);
    // Compact the databases and release the lock now that we are leaving cleanly.
    if let Some(project) = state.workspace.project().cloned() {
        if let Err(error) = housekeeping::run_maintenance(&project) {
            state.set_message(format!("Project maintenance failed: {error}"));
        }
        housekeeping::release_lock(&project);
    }
    state.workspace = WorkspaceSession::Scratch;
    state.entries = EntryStore::new_empty();
    state.tasks = TaskManager::default();
    state.ui.project_viewport = Default::default();
    state.ui.viewport = Default::default();
    state.ui.entry_viewports.clear();
    state.config.closed_to_scratch = true;
    state.config.last_project_path = None;
    if let Err(error) = save_config(&state.config) {
        state.set_message(format!(
            "Closed project, but settings update failed: {error}"
        ));
    } else {
        state.set_message("Closed project; opened Scratch");
    }
    reset_transient_state(state);
    state.clear_history();
}

fn save_project(state: &mut AppState) {
    if state.workspace.is_project() {
        persist_project(state, true);
        state.set_message(format!("Saved project {}", state.workspace.label()));
        return;
    }
    create_project_action(state);
}

fn load_active_entry(state: &mut AppState) {
    reset_transient_state(state);
    if let Some(active_id) = state.entries.active_entry_id() {
        state.ensure_entry_loaded(active_id);
    }
    state.sync_history_active_entry();
    if let Some(entry) = state.entries.active_entry() {
        state.ui.selection.retain_valid(entry.structure.atoms.len());
    } else {
        state.ui.selection.clear();
    }
    state.load_viewport_for_active_entry();
    state.ui.camera = crate::frontend::ViewCamera::default();
    state.ui.viewport_cache.clear();
    // The reset above wiped any transient form. If a task dashboard is still
    // open, re-initialize its form against the newly active structure so it
    // stays usable instead of rendering an empty "panel unavailable" body.
    if let Some(task_run_id) = state.tasks.active_panel {
        ensure_panel_form(state, task_run_id);
    }
}

fn require_active_entry(state: &mut AppState, action_label: &str) -> bool {
    if state.has_active_entry() {
        true
    } else {
        state.set_message(format!("{action_label} requires an open entry"));
        false
    }
}

fn create_task_from_template(state: &mut AppState, template_id: &'static str) {
    let Some(controller) = task_controller_by_id(template_id).copied() else {
        state.set_message(format!("Unknown task: {template_id}"));
        return;
    };

    let task_run_id = state.tasks.create_task_run(controller);
    state.ui.layout.active_primary_view = crate::frontend::state::PrimaryView::Tasks;
    state.ui.layout.show_primary_sidebar = true;
    if controller.requires_panel() {
        state.tasks.open_panel(task_run_id);
        state.ui.layout.show_secondary_sidebar = true;
    }
    state.set_message(format!(
        "Opened task #{}: {}",
        task_run_id, controller.title
    ));
    run_task(state, task_run_id);
}

fn run_task(state: &mut AppState, task_run_id: u64) {
    let Some(task) = state.tasks.task_run(task_run_id).cloned() else {
        state.set_message(format!("Task #{task_run_id} not found"));
        return;
    };
    // Direct (non-panel) tasks act on the active structure immediately, so they
    // require an entry up front. Interactive panel tasks only open their
    // dashboard here; their preconditions are validated when the user triggers
    // the action, so they open even on an empty workspace.
    if task.panel == TaskPanelKind::None && !state.has_active_entry() {
        state.tasks.mark_status(task_run_id, TaskStatus::Failed);
        state.set_message("Open or create an entry before running tasks".to_string());
        return;
    }

    let Some(executor) = task_executor(task.kind) else {
        state.tasks.mark_status(task_run_id, TaskStatus::Failed);
        state.set_message(format!("No executor registered for task {}", task.title));
        return;
    };
    (executor.run)(state, task_run_id);
    state.ui.layout.active_primary_view = crate::frontend::state::PrimaryView::Tasks;
}

fn complete_active_task(state: &mut AppState, kind: TaskKind, status: TaskStatus) {
    let Some(task_run_id) = state.active_task_run else {
        return;
    };
    let matches_kind = state
        .tasks
        .task_run(task_run_id)
        .map(|task| task.kind == kind)
        .unwrap_or(false);
    if matches_kind {
        mark_task_status(state, task_run_id, status);
        state.active_task_run = None;
    }
}

fn sync_task_manifest(state: &mut AppState, task_run_id: u64) {
    if let Err(error) = super::task_executor::sync_task_manifest(state, task_run_id) {
        state.set_message(format!("failed to write run manifest: {error}"));
    }
}

fn mark_task_status(state: &mut AppState, task_run_id: u64, status: TaskStatus) {
    if let Err(error) = super::task_executor::mark_task_status(state, task_run_id, status) {
        state.set_message(format!("failed to update task status: {error}"));
    }
}

fn ensure_active_task_run_dir(
    state: &mut AppState,
    kind: TaskKind,
    desired_name: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let task_run_id = state
        .active_task_run
        .ok_or_else(|| anyhow!("no active task run"))?;
    let task = state
        .tasks
        .task_run(task_run_id)
        .ok_or_else(|| anyhow!("task run #{task_run_id} not found"))?
        .clone();
    if task.kind != kind {
        bail!("task run #{task_run_id} is not {kind:?}");
    }
    if let Some(run_dir) = task.run_dir {
        return Ok(run_dir);
    }
    if !task.uses_run_directory {
        bail!("task {} does not use a run directory", task.title);
    }
    // Use the user-chosen run name when supplied (and non-empty), otherwise fall
    // back to the suggested `{controller}-N`. The directory name is purely
    // human-facing; the task's durable identity is its UUID.
    let runs_dir = state.runs_dir();
    let name = desired_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| crate::backend::runs::default_run_name(&runs_dir, task.controller_id));
    let run_dir = ensure_run_dir(&runs_dir, &name)?;
    state.tasks.set_run_dir(task_run_id, run_dir.clone());
    state
        .tasks
        .set_source_entry_id(task_run_id, state.entries.active_entry_id());
    sync_task_manifest(state, task_run_id);
    Ok(run_dir)
}

fn record_task_result_entry(state: &mut AppState, task_run_id: u64, entry_id: u64) {
    if let Err(error) = super::task_executor::record_task_result_entry(state, task_run_id, entry_id)
    {
        state.set_message(format!("failed to record task result entry: {error}"));
    }
}

fn open_task_panel(state: &mut AppState, task_run_id: u64) {
    state.tasks.open_panel(task_run_id);
    state.ui.layout.show_secondary_sidebar = true;
    ensure_panel_form(state, task_run_id);
}

fn close_task_panel(state: &mut AppState, task_run_id: u64) {
    state.tasks.close_panel(task_run_id);
    if state.tasks.panels.is_empty() {
        state.ui.layout.show_secondary_sidebar = false;
    }
}

fn activate_task_panel(state: &mut AppState, task_run_id: u64) {
    state.tasks.activate_panel(task_run_id);
    if let Some(active) = state.tasks.active_panel {
        state.ui.layout.show_secondary_sidebar = true;
        ensure_panel_form(state, active);
    }
}

/// Make a task's dashboard renderable on demand: initialize its form state if
/// it is not already present, so every panel shows its controls immediately
/// (whether freshly created, re-opened, or re-activated) without requiring a
/// run first. Preconditions are deferred to the action handlers, which validate
/// when the user actually triggers the work.
pub(super) fn ensure_panel_form(state: &mut AppState, task_run_id: u64) {
    let Some(task) = state.tasks.task_run(task_run_id).cloned() else {
        return;
    };
    match task.panel {
        TaskPanelKind::OptimizationPrompt => {
            let allow_cell = task.kind == TaskKind::OptimizeCrystalGeometry;
            // Re-init when absent, or when switching between the geometry and
            // crystal tasks that share this panel (they differ by cell scope).
            let stale = state
                .ui
                .pending_optimization
                .as_ref()
                .map(|prompt| prompt.allow_cell_optimization != allow_cell)
                .unwrap_or(true);
            if stale {
                state.ui.pending_optimization =
                    Some(crate::frontend::state::OptimizationPrompt::new(
                        allow_cell,
                        &state.ui.selection,
                    ));
            }
        }
        TaskPanelKind::SupercellPrompt => {
            state
                .ui
                .pending_supercell
                .get_or_insert_with(Default::default);
        }
        TaskPanelKind::ProteinPrepPrompt => {
            state
                .ui
                .pending_protein_prep
                .get_or_insert_with(Default::default);
        }
        TaskPanelKind::MdSystemPrompt => {
            let default_name =
                crate::backend::runs::default_run_name(&state.runs_dir(), task.controller_id);
            // On first open, default the force field to the best fit for the
            // structure's content (protein/nucleic vs. crystal/small molecule).
            if state.ui.pending_md_system.is_none() {
                let force_field = crate::workflows::molecular_dynamics::recommended_force_field(
                    state.structure(),
                )
                .to_string();
                state.ui.pending_md_system = Some(crate::frontend::state::MdSystemPrompt {
                    force_field,
                    ..Default::default()
                });
            }
            // A periodic framework keeps its crystal cell as the MD box; seed the
            // editable lattice parameters from it, opening the out-of-plane axis to
            // a cutoff-safe floor so the default just runs. The in-plane lattice is
            // taken verbatim — it defines how the sheet tiles across the boundary.
            let framework_cell =
                crate::workflows::molecular_dynamics::is_framework(state.structure())
                    .then(|| {
                        state.structure().cell.as_ref().map(|cell| {
                            let c = cell.c.max(FRAMEWORK_C_FLOOR_ANGSTROM);
                            [cell.a, cell.b, c, cell.alpha, cell.beta, cell.gamma]
                        })
                    })
                    .flatten();
            let prompt = state
                .ui
                .pending_md_system
                .get_or_insert_with(Default::default);
            if prompt.run_name.trim().is_empty() {
                prompt.run_name = default_name;
            }
            if prompt.framework_cell.is_none() {
                prompt.framework_cell = framework_cell;
            }
        }
        TaskPanelKind::MdRunPrompt => {
            let default_name =
                crate::backend::runs::default_run_name(&state.runs_dir(), task.controller_id);
            let prompt = state.ui.pending_md_run.get_or_insert_with(Default::default);
            if prompt.run_name.trim().is_empty() {
                prompt.run_name = default_name;
            }
        }
        TaskPanelKind::ReticularBuilder => {
            if state.ui.reticular_builder.is_none() {
                if state.builder_origin.is_none() && state.has_active_entry() {
                    state.builder_origin = Some(state.capture_edit_snapshot());
                }
                let panel = crate::frontend::ReticularBuilderPanel::new(state.structure());
                state.ui.reticular_builder = Some(panel);
            }
        }
        TaskPanelKind::NanosheetBuilder => {
            if state.ui.nanosheet_builder.is_none() {
                if state.builder_origin.is_none() && state.has_active_entry() {
                    state.builder_origin = Some(state.capture_edit_snapshot());
                }
                let panel = crate::frontend::NanosheetBuilderPanel::new(state.structure());
                state.ui.nanosheet_builder = Some(panel);
            }
        }
        TaskPanelKind::BuildingBlockEditor => {
            if state.ui.block_editor.is_none() {
                let editor = crate::frontend::BuildingBlockEditor::new(state.structure());
                state.ui.block_editor = Some(editor);
            }
        }
        TaskPanelKind::None => {}
    }
}

/// Point `active_task_run` at the active panel's task (matching `panel`) when no
/// run is currently bound. Lets action handlers report task status correctly
/// even when the dashboard was opened directly rather than via "Run".
fn bind_active_panel_task(state: &mut AppState, panel: TaskPanelKind) {
    if state.active_task_run.is_some() {
        return;
    }
    if let Some(task_run_id) = state.tasks.active_panel {
        let matches = state
            .tasks
            .task_run(task_run_id)
            .map(|task| task.panel == panel)
            .unwrap_or(false);
        if matches {
            state.active_task_run = Some(task_run_id);
        }
    }
}

fn close_active_task_panel(state: &mut AppState) {
    if let Some(task_run_id) = state.tasks.active_panel {
        close_task_panel(state, task_run_id);
    }
}

fn new_empty_entry(state: &mut AppState) {
    let structure = Structure::empty();
    let save_path = structure_io::default_structure_save_path(&structure, None);
    let entry_id = add_and_show_entry(state, structure, None, save_path);
    state.set_message(format!("Created empty entry #{entry_id}"));
}

/// Insert a freshly produced structure as a new entry and switch to it, running
/// the full app-level load (first-load render defaults, transient reset, camera
/// recenter). Returns the new entry id.
///
/// `EntryStore::add_entry` already marks the entry active in the store, so this
/// must NOT route through [`activate_entry`]: its "already active" early-return
/// would skip [`load_active_entry`], leaving the new structure rendered with the
/// previous entry's styles — which is why a freshly built MD system showed its
/// bulk solvent as ball-and-stick instead of the wireframe default. Mirrors the
/// save → add → load sequence of [`new_empty_entry`].
fn add_and_show_entry(
    state: &mut AppState,
    structure: Structure,
    source_path: Option<PathBuf>,
    save_path: PathBuf,
) -> u64 {
    state.save_viewport_for_active_entry();
    let entry_id = state.entries.add_entry(structure, source_path, save_path);
    state.ui.entry_list.selected_entry_ids.clear();
    state.ui.entry_list.selected_entry_ids.insert(entry_id);
    // `load_active_entry` resets transient state, which includes the active task
    // run. When a task (e.g. an MD system build) produces and shows its result
    // entry, that task context must survive so the caller can still mark the run
    // complete and record this entry as its result — otherwise the run is never
    // marked completed and lookups like the GROMACS topology for the entry fail.
    let active_task_run = state.active_task_run;
    load_active_entry(state);
    state.active_task_run = active_task_run;
    entry_id
}

fn activate_entry(state: &mut AppState, entry_id: u64) {
    if state.entries.active_entry_id() == Some(entry_id) {
        return;
    }
    state.save_viewport_for_active_entry();
    state.entries.activate_entry(entry_id);
    state.ui.entry_list.selected_entry_ids.insert(entry_id);
    load_active_entry(state);
    state.set_message(format!("Loaded entry {}", state.current_entry_label()));
}

fn delete_entry(state: &mut AppState, entry_id: u64) {
    let Some(name) = state
        .entries
        .entry(entry_id)
        .map(|entry| entry.name.clone())
    else {
        state.set_message("Cannot delete entry".to_string());
        return;
    };
    let active_before = state.entries.active_entry_id();
    state.save_viewport_for_active_entry();

    if state.entries.delete_entry(entry_id) {
        state.ui.entry_viewports.remove(&entry_id);
        state.history.forget_entry(entry_id);
        state.ui.entry_list.selected_entry_ids.remove(&entry_id);
        if state.ui.entry_list.renaming_entry_id == Some(entry_id) {
            state.ui.entry_list.renaming_entry_id = None;
            state.ui.entry_list.rename_buffer.clear();
        }
        if active_before == Some(entry_id) {
            reset_transient_state(state);
            load_active_entry(state);
        }
        state.set_message(format!("Deleted entry {name}"));
    } else {
        state.set_message("Cannot delete entry".to_string());
    }
}

fn delete_entries(state: &mut AppState, ids: Vec<u64>) {
    for id in ids {
        delete_entry(state, id);
    }
}

fn open_file(state: &mut AppState) {
    let Some(path) = StructureService::open_dialog() else {
        return;
    };

    open_paths(state, [path]);
}

fn open_pdb_fetch_dialog(state: &mut AppState) {
    state.ui.pending_pdb_fetch.get_or_insert_with(String::new);
}

fn fetch_pdb(state: &mut AppState) {
    let Some(id) = state
        .ui
        .pending_pdb_fetch
        .as_ref()
        .map(|id| id.trim().to_string())
    else {
        return;
    };

    match pdb_fetch::fetch_pdb(
        &id,
        pdb_fetch::RCSB_DEFAULT_BASE_URL,
        &state.structures_dir(),
    ) {
        Ok(fetched) => {
            state.ui.pending_pdb_fetch = None;
            open_paths(state, [fetched.path]);
        }
        Err(error) => state.set_message(format!("Fetch failed: {error}")),
    }
}

pub fn open_paths(state: &mut AppState, paths: impl IntoIterator<Item = PathBuf>) {
    state.save_viewport_for_active_entry();
    let mut opened = Vec::<(u64, PathBuf)>::new();
    let mut failed = Vec::<String>::new();

    for path in paths {
        match load_document(&path) {
            Ok(document) => match import_document(&mut state.entries, document, path.clone()) {
                Some(entry_id) => opened.push((entry_id, path)),
                None => {
                    failed.push(format!("{}: no models found", path.display()));
                }
            },
            Err(error) => failed.push(format!("{}: {error}", path.display())),
        }
    }

    let Some((entry_id, last_path)) = opened.last() else {
        if let Some(error) = failed.first() {
            state.set_message(format!("Failed to open {error}"));
        }
        return;
    };

    state.ui.entry_list.selected_entry_ids.clear();
    state.ui.entry_list.selected_entry_ids.insert(*entry_id);
    load_active_entry(state);
    state.ui.selection.clear();
    state.set_message(format_open_results(opened.len(), failed.len(), last_path));
}

fn format_open_results(
    opened_count: usize,
    failed_count: usize,
    last_path: &std::path::Path,
) -> String {
    match (opened_count, failed_count) {
        (1, 0) => format!("Opened {}", last_path.display()),
        (_, 0) => format!("Opened {opened_count} files"),
        (1, 1) => format!("Opened {}; 1 file failed", last_path.display()),
        (1, _) => format!(
            "Opened {}; {failed_count} files failed",
            last_path.display()
        ),
        (_, 1) => format!("Opened {opened_count} files; 1 file failed"),
        _ => format!("Opened {opened_count} files; {failed_count} files failed"),
    }
}

fn save(state: &mut AppState) {
    if !require_active_entry(state, "Save") {
        return;
    }
    let save_path = state.save_path().clone();
    match StructureService::save(state.structure(), &save_path) {
        Ok(()) => {
            state.set_source_path(Some(save_path.clone()));
            state.set_message(format!("Saved structure to {}", save_path.display()));
        }
        Err(error) => {
            state.set_message(format!("Failed to save {}: {error}", save_path.display()));
        }
    }
}

fn save_as(state: &mut AppState) {
    if !require_active_entry(state, "Save As") {
        return;
    }
    let current_save_path = state.save_path().clone();
    let Some(path) = StructureService::save_as_dialog(state.structure(), &current_save_path) else {
        state.set_message("Save As canceled".to_string());
        return;
    };
    state.set_save_path(path);
    save(state);
}

fn edit_structure(state: &mut AppState) {
    if !require_active_entry(state, "Edit Structure") {
        return;
    }
    state.cancel_transient_jobs();
    state.ui.pending_optimization = None;
    state.edit_origin = Some(state.capture_edit_snapshot());
    state.ui.editor = Some(crate::frontend::StructureEditor::new(state.structure()));
}

fn apply_structure_edits(state: &mut AppState) {
    if let Some(editor) = &state.ui.editor {
        let draft = editor.draft.clone();
        let before = state
            .edit_origin
            .clone()
            .unwrap_or_else(|| state.capture_edit_snapshot());
        state.cancel_transient_jobs();
        state.ui.pending_optimization = None;
        *state.structure_mut() = draft;
        state.mark_structure_changed();
        state.set_source_path(None);
        state
            .ui
            .selection
            .retain_valid(state.structure().atoms.len());
        state.history.push_undo(before);
        state.edit_origin = None;
        state.ui.editor = None;
        state.set_message("Applied structure edits".to_string());
    }
}

fn cancel_structure_edits(state: &mut AppState) {
    if let Some(before) = state.edit_origin.take() {
        state.restore_edit_snapshot(before);
    } else if let Some(editor) = &state.ui.editor {
        *state.structure_mut() = editor.original.clone();
        state.mark_structure_changed();
        state.ui.editor = None;
    } else {
        return;
    }
    state.set_message("Edit canceled".to_string());
}

pub(super) fn build_framework_task(state: &mut AppState) {
    state.cancel_transient_jobs();
    state.ui.pending_optimization = None;
    // Frameworks can be built from an empty workspace, where there is no active
    // entry to snapshot for undo; only capture an origin when one exists.
    state.builder_origin = state
        .has_active_entry()
        .then(|| state.capture_edit_snapshot());
    state.ui.reticular_builder = Some(crate::frontend::ReticularBuilderPanel::new(
        state.structure(),
    ));
}

pub(super) fn build_block_from_current(state: &mut AppState) {
    state.cancel_transient_jobs();
    state.ui.pending_optimization = None;
    state.ui.block_editor = Some(crate::frontend::BuildingBlockEditor::new(state.structure()));
}

fn preview_framework_task(state: &mut AppState) {
    let Some(panel) = &state.ui.reticular_builder else {
        return;
    };
    match ReticularService::preview(&panel.spec) {
        Ok(built) => {
            state.cancel_transient_jobs();
            state.ui.pending_optimization = None;
            *state.structure_mut() = built.structure;
            state.mark_structure_changed();
            state.set_source_path(None);
            state.set_save_path(built.save_path);
            state.ui.camera = crate::frontend::ViewCamera::default();
            state.ui.selection.clear();
            state.set_message(format!(
                "Reticular structure preview generated; {}",
                built.analysis
            ));
        }
        Err(error) => state.set_message(format!("Reticular structure build failed: {error}")),
    }
}

fn accept_framework_task(state: &mut AppState) {
    let Some(panel) = &state.ui.reticular_builder else {
        return;
    };
    match ReticularService::build(&panel.spec) {
        Ok(built) => {
            if let Some(before) = state.builder_origin.take() {
                state.restore_edit_snapshot(before);
            }
            add_and_show_entry(state, built.structure, None, built.save_path);
            state.set_message(format!("Reticular structure built; {}", built.analysis));
            complete_active_task(
                state,
                TaskKind::BuildReticularStructure,
                TaskStatus::Completed,
            );
            close_active_task_panel(state);
        }
        Err(error) => state.set_message(format!("Reticular structure build failed: {error}")),
    }
}

fn cancel_framework_task(state: &mut AppState) {
    if let Some(before) = state.builder_origin.take() {
        state.restore_edit_snapshot(before);
    } else if let Some(panel) = &state.ui.reticular_builder {
        *state.structure_mut() = panel.original.clone();
        state.mark_structure_changed();
        state.ui.reticular_builder = None;
    }
    state.ui.reticular_builder = None;
    state.set_message("Reticular structure build canceled".to_string());
    complete_active_task(state, TaskKind::BuildReticularStructure, TaskStatus::Failed);
    close_active_task_panel(state);
}

pub(super) fn build_nanosheet_task(state: &mut AppState) {
    state.cancel_transient_jobs();
    state.ui.pending_optimization = None;
    // A nanosheet is built from scratch, so the workspace is often empty (no
    // active entry to snapshot for undo); only capture an origin when one exists.
    state.builder_origin = state
        .has_active_entry()
        .then(|| state.capture_edit_snapshot());
    state.ui.nanosheet_builder = Some(crate::frontend::NanosheetBuilderPanel::new(
        state.structure(),
    ));
}

fn preview_nanosheet_task(state: &mut AppState) {
    let Some(panel) = &state.ui.nanosheet_builder else {
        return;
    };
    match NanosheetService::preview(&panel.spec) {
        Ok(built) => {
            state.cancel_transient_jobs();
            state.ui.pending_optimization = None;
            *state.structure_mut() = built.structure;
            state.mark_structure_changed();
            state.set_source_path(None);
            state.set_save_path(built.save_path);
            state.ui.camera = crate::frontend::ViewCamera::default();
            state.ui.selection.clear();
            state.set_message(format!("Nanosheet preview generated; {}", built.analysis));
        }
        Err(error) => state.set_message(format!("Nanosheet build failed: {error}")),
    }
}

fn accept_nanosheet_task(state: &mut AppState) {
    let Some(panel) = &state.ui.nanosheet_builder else {
        return;
    };
    match NanosheetService::build(&panel.spec) {
        Ok(built) => {
            if let Some(before) = state.builder_origin.take() {
                state.restore_edit_snapshot(before);
            }
            add_and_show_entry(state, built.structure, None, built.save_path);
            state.set_message(format!("Nanosheet built; {}", built.analysis));
            complete_active_task(state, TaskKind::BuildNanosheet, TaskStatus::Completed);
            close_active_task_panel(state);
        }
        Err(error) => state.set_message(format!("Nanosheet build failed: {error}")),
    }
}

fn cancel_nanosheet_task(state: &mut AppState) {
    if let Some(before) = state.builder_origin.take() {
        state.restore_edit_snapshot(before);
    } else if let Some(panel) = &state.ui.nanosheet_builder {
        *state.structure_mut() = panel.original.clone();
        state.mark_structure_changed();
        state.ui.nanosheet_builder = None;
    }
    state.ui.nanosheet_builder = None;
    state.set_message("Nanosheet build canceled".to_string());
    complete_active_task(state, TaskKind::BuildNanosheet, TaskStatus::Failed);
    close_active_task_panel(state);
}

fn save_block_editor_task(state: &mut AppState) {
    let Some(editor) = &state.ui.block_editor else {
        return;
    };
    match BuildingBlockService::save(editor, state.structure()) {
        Ok((path, source)) => {
            let current_structure = state.structure().clone();
            state.set_message(format!("Building block saved {}", path.display()));
            state
                .ui
                .reticular_builder
                .get_or_insert_with(|| {
                    crate::frontend::ReticularBuilderPanel::new(&current_structure)
                })
                .spec
                .custom_components
                .push(source);
            state.ui.block_editor = None;
            complete_active_task(state, TaskKind::CreateBuildingBlock, TaskStatus::Completed);
            close_active_task_panel(state);
        }
        Err(error) => state.set_message(format!("Building block save failed: {error}")),
    }
}

fn cancel_block_editor_task(state: &mut AppState) {
    state.ui.block_editor = None;
    state.set_message("Building block creation canceled".to_string());
    complete_active_task(state, TaskKind::CreateBuildingBlock, TaskStatus::Failed);
    close_active_task_panel(state);
}

fn start_pending_optimization(state: &mut AppState) {
    bind_active_panel_task(state, TaskPanelKind::OptimizationPrompt);
    let Some(prompt) = state.ui.pending_optimization else {
        return;
    };
    if state.jobs.optimization_running() {
        state.set_message(
            "forcefield optimization is already running; press Esc to stop".to_string(),
        );
        return;
    }
    if prompt.allow_cell_optimization && state.structure().cell.is_none() {
        state
            .set_message("crystal geometry optimization requires a periodic structure".to_string());
        return;
    }
    let options = prompt.options(&state.ui.selection);
    match spawn_optimization_job(state.structure().clone(), options) {
        Ok(job) => {
            state.optimization_origin = Some(state.capture_edit_snapshot());
            state.set_source_path(None);
            state.ui.editor = None;
            state.ui.pending_optimization = None;
            state.jobs.set_optimizer(job);
            if let Some(task_run_id) = state.active_task_run {
                state.tasks.mark_status(task_run_id, TaskStatus::Running);
            }
            state.set_message("forcefield optimization running; press Esc to stop".to_string());
        }
        Err(error) => {
            state.set_message(format!("forcefield optimization failed to start: {error}"));
            complete_active_task(state, TaskKind::OptimizeGeometry, TaskStatus::Failed);
            complete_active_task(state, TaskKind::OptimizeCrystalGeometry, TaskStatus::Failed);
        }
    }
}

fn cancel_pending_optimization_request(state: &mut AppState) {
    bind_active_panel_task(state, TaskPanelKind::OptimizationPrompt);
    state.ui.pending_optimization = None;
    state.set_message("forcefield optimization canceled".to_string());
    complete_active_task(state, TaskKind::OptimizeGeometry, TaskStatus::Failed);
    complete_active_task(state, TaskKind::OptimizeCrystalGeometry, TaskStatus::Failed);
    close_active_task_panel(state);
}

fn confirm_pending_supercell(state: &mut AppState) {
    if state.ui.pending_supercell.is_none() {
        return;
    }
    bind_active_panel_task(state, TaskPanelKind::SupercellPrompt);
    if let Err(error) = require_periodic_structure(
        state.structure(),
        "supercell expansion requires a periodic structure",
    ) {
        state.set_message(error.to_string());
        return;
    }
    let prompt = state
        .ui
        .pending_supercell
        .take()
        .expect("checked is_some above");
    expand_supercell(state, prompt.repeats);
    close_active_task_panel(state);
}

fn cancel_pending_supercell_request(state: &mut AppState) {
    bind_active_panel_task(state, TaskPanelKind::SupercellPrompt);
    state.ui.pending_supercell = None;
    state.set_message("Supercell expansion canceled".to_string());
    complete_active_task(state, TaskKind::ExpandSupercell, TaskStatus::Failed);
    close_active_task_panel(state);
}

fn confirm_pending_protein_prep(state: &mut AppState) {
    let Some(prompt) = state.ui.pending_protein_prep else {
        return;
    };
    bind_active_panel_task(state, TaskPanelKind::ProteinPrepPrompt);
    if prepare_protein(state, prompt) {
        state.ui.pending_protein_prep = None;
        close_active_task_panel(state);
    }
}

fn cancel_pending_protein_prep_request(state: &mut AppState) {
    bind_active_panel_task(state, TaskPanelKind::ProteinPrepPrompt);
    state.ui.pending_protein_prep = None;
    state.set_message("Protein preparation canceled".to_string());
    complete_active_task(state, TaskKind::PrepareProtein, TaskStatus::Failed);
    close_active_task_panel(state);
}

/// Prepare the active structure for simulation and add the result as a new
/// entry. This round only completes hydrogens; future steps (protonation states,
/// terminus patching, missing-atom repair) will extend the same prompt. Returns
/// `false` (keeping the panel open) on failure.
fn prepare_protein(
    state: &mut AppState,
    prompt: crate::frontend::state::ProteinPrepPrompt,
) -> bool {
    if state.structure().atoms.is_empty() {
        state.set_message("no active structure to prepare".to_string());
        return false;
    }
    if let Some(task_run_id) = state.active_task_run {
        mark_task_status(state, task_run_id, TaskStatus::Running);
    }
    state.cancel_transient_jobs();
    state.ui.pending_optimization = None;
    state.ui.editor = None;
    state.ui.selection.clear();

    let mut prepared = state.structure().clone();
    let mut added_hydrogens = 0usize;
    if prompt.add_hydrogens {
        added_hydrogens = prepared.add_missing_hydrogens();
    }

    let save_path = structure_io::default_structure_save_path(&prepared, None);
    let entry_id = add_and_show_entry(state, prepared, None, save_path);
    if let Some(task_run_id) = state.active_task_run {
        record_task_result_entry(state, task_run_id, entry_id);
    }
    state.set_message(format!(
        "Protein prepared: added {added_hydrogens} hydrogen(s) (new entry)"
    ));
    complete_active_task(state, TaskKind::PrepareProtein, TaskStatus::Completed);
    true
}

fn confirm_pending_md_system(state: &mut AppState) {
    let Some(prompt) = state.ui.pending_md_system.clone() else {
        return;
    };
    bind_active_panel_task(state, TaskPanelKind::MdSystemPrompt);
    if build_md_system(state, &prompt) {
        state.ui.pending_md_system = None;
        close_active_task_panel(state);
    }
}

fn cancel_pending_md_system_request(state: &mut AppState) {
    bind_active_panel_task(state, TaskPanelKind::MdSystemPrompt);
    state.ui.pending_md_system = None;
    state.set_message("MD system build canceled".to_string());
    complete_active_task(state, TaskKind::BuildMdSystem, TaskStatus::Failed);
    close_active_task_panel(state);
}

fn pick_md_topology_override(state: &mut AppState) {
    let Some(prompt) = state.ui.pending_md_run.as_mut() else {
        return;
    };
    let starting_dir = prompt
        .topology_override_path
        .as_ref()
        .and_then(|path| path.parent().map(PathBuf::from))
        .unwrap_or_else(|| state.config.default_project_dir.clone());
    let picked = rfd::FileDialog::new()
        .set_directory(starting_dir)
        .add_filter("GROMACS topology", &["top", "itp"])
        .pick_file();
    if let Some(path) = picked {
        prompt.topology_override_path = Some(path);
    }
}

/// Select (or clear) the framework build's custom force field, caching the
/// library entry's `.itp` text so the panel and build need not re-read it.
fn select_custom_force_field(state: &mut AppState, name: Option<String>) {
    let Some(prompt) = state.ui.pending_md_system.as_mut() else {
        return;
    };
    match name {
        None => {
            prompt.custom_force_field = None;
            prompt.custom_force_field_text = None;
        }
        Some(name) => match crate::backend::force_fields::load_force_field(&name) {
            Ok(text) => {
                prompt.custom_force_field = Some(name);
                prompt.custom_force_field_text = Some(text);
            }
            Err(error) => state.set_message(format!("failed to load force field: {error}")),
        },
    }
}

/// Save the draft custom force field to the reusable library, then select it.
fn save_custom_force_field(state: &mut AppState) {
    let Some(prompt) = state.ui.pending_md_system.as_ref() else {
        return;
    };
    let name = prompt.custom_ff_draft_name.trim().to_string();
    let text = prompt.custom_ff_draft.clone();
    if name.is_empty() {
        state.set_message("enter a name for the force field before saving".to_string());
        return;
    }
    if text.trim().is_empty() {
        state.set_message("the force field is empty".to_string());
        return;
    }
    match crate::backend::force_fields::save_force_field(&name, &text) {
        Ok(()) => {
            if let Some(prompt) = state.ui.pending_md_system.as_mut() {
                prompt.custom_force_field = Some(name.clone());
                prompt.custom_force_field_text = Some(text);
                prompt.custom_ff_draft.clear();
                prompt.custom_ff_draft_name.clear();
            }
            state.set_message(format!("saved force field `{name}`"));
        }
        Err(error) => state.set_message(format!("failed to save force field: {error}")),
    }
}

/// Delete a custom force field from the library; clear the selection if it was
/// the one in use.
fn delete_custom_force_field(state: &mut AppState, name: &str) {
    match crate::backend::force_fields::delete_force_field(name) {
        Ok(()) => {
            if let Some(prompt) = state.ui.pending_md_system.as_mut()
                && prompt.custom_force_field.as_deref() == Some(name)
            {
                prompt.custom_force_field = None;
                prompt.custom_force_field_text = None;
            }
            state.set_message(format!("deleted force field `{name}`"));
        }
        Err(error) => state.set_message(format!("failed to delete force field: {error}")),
    }
}

/// Open a file picker and load a `.itp`/`.top` into the draft custom force field,
/// suggesting a name from the file stem.
fn import_custom_force_field_file(state: &mut AppState) {
    let Some(path) = rfd::FileDialog::new()
        .set_directory(&state.config.default_project_dir)
        .add_filter("GROMACS force field", &["itp", "top"])
        .pick_file()
    else {
        return;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            state.set_message(format!("failed to read {}: {error}", path.display()));
            return;
        }
    };
    if let Some(prompt) = state.ui.pending_md_system.as_mut() {
        if prompt.custom_ff_draft_name.trim().is_empty()
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            prompt.custom_ff_draft_name = stem.to_string();
        }
        prompt.custom_ff_draft = text;
    }
}

fn start_pending_md_run(state: &mut AppState) {
    bind_active_panel_task(state, TaskPanelKind::MdRunPrompt);
    let Some(prompt) = state.ui.pending_md_run.clone() else {
        return;
    };
    if state.jobs.engine_running() {
        state.set_message("another external engine job is already running".to_string());
        return;
    }
    if state.structure().cell.is_none() {
        state.set_message("MD runs need a structure with a simulation box".to_string());
        return;
    }
    let topology = match resolve_md_topology_source(state, &prompt) {
        Ok(topology) => topology,
        Err(error) => {
            state.set_message(error.to_string());
            return;
        }
    };
    let mut stages = match build_md_stage_specs(&prompt.steps) {
        Ok(stages) => stages,
        Err(error) => {
            state.set_message(error.to_string());
            return;
        }
    };
    // A framework (nanosheet) system carries run hints from its build: keep the
    // molecule periodic (flexible) and/or freeze the sheet (rigid). Apply them to
    // every stage and capture the freeze selection for prepare_system.
    let framework_freeze = state
        .entries
        .active_entry_id()
        .and_then(|id| crate::frontend::md_support::load_framework_metadata_for_entry(state, id))
        .and_then(|meta| {
            for spec in &mut stages {
                meta.apply_to(&mut spec.settings);
            }
            meta.freeze_selection()
        });
    let gmx_launch = match resolve_md_engine_launch(state, prompt.engine) {
        Ok(launch) => launch,
        Err(error) => {
            state.set_message(error.to_string());
            return;
        }
    };

    let working_dir =
        match ensure_active_task_run_dir(state, TaskKind::RunMd, Some(prompt.run_name.as_str())) {
            Ok(path) => path,
            Err(error) => {
                state.set_message(format!("failed to create run directory: {error}"));
                complete_active_task(state, TaskKind::RunMd, TaskStatus::Failed);
                return;
            }
        };
    if let Some(task_run_id) = state.active_task_run {
        state
            .tasks
            .set_engine_label(task_run_id, Some(prompt.engine.label().to_string()));
        sync_task_manifest(state, task_run_id);
    }
    let job = spawn_gromacs_pipeline_job(GromacsPipelineRequest {
        structure: state.structure().clone(),
        topology,
        stages,
        working_dir,
        gmx_launch,
        max_duration_per_stage: Duration::from_secs(60 * 60),
        freeze: framework_freeze,
    });
    state.optimization_origin = None;
    state.ui.pending_md_run = None;
    state.jobs.set_engine(job);
    if let Some(task_run_id) = state.active_task_run {
        mark_task_status(state, task_run_id, TaskStatus::Running);
    }
    state.set_message(format!(
        "{} MD running; press Esc to stop",
        prompt.engine.label()
    ));
}

fn cancel_pending_md_run_request(state: &mut AppState) {
    bind_active_panel_task(state, TaskPanelKind::MdRunPrompt);
    state.ui.pending_md_run = None;
    state.set_message("MD run canceled".to_string());
    complete_active_task(state, TaskKind::RunMd, TaskStatus::Failed);
    close_active_task_panel(state);
}

fn resolve_md_topology_source(
    state: &AppState,
    prompt: &crate::frontend::state::MdRunPrompt,
) -> anyhow::Result<TopologySource> {
    if let Some(path) = prompt.topology_override_path.clone() {
        return Ok(TopologySource::File(path));
    }

    if let Some(entry_id) = state.entries.active_entry_id() {
        // Prefer a force-field topology produced by a GROMACS build; it is the
        // real `topol.top` (with FF/water/ion includes) the run reuses directly.
        if let Some(path) = gromacs_topology_path_for_entry(state, entry_id) {
            return Ok(TopologySource::File(path));
        }
        // Otherwise fall back to a captured engine-neutral topology (e.g. from
        // the `md build` console command for a monatomic system).
        if let Some(topology) = load_md_topology_for_entry(state, entry_id) {
            return Ok(TopologySource::Inline(render_top(&topology)));
        }
    }

    let topology = crate::workflows::molecular_dynamics::MdTopology::from_structure(
        state.structure(),
    )
    .map_err(|_| {
        anyhow!(
            "No automatic MD topology is available for this structure. Build an MD system first or choose a custom topology in Advanced."
        )
    })?;
    Ok(TopologySource::Inline(render_top(&topology)))
}

fn resolve_md_engine_launch(
    state: &mut AppState,
    engine: crate::frontend::state::MdEngineChoice,
) -> anyhow::Result<crate::engines::registry::EngineLaunch> {
    let registry = EngineRegistry::probe(&state.config.engine_overrides);
    match engine {
        crate::frontend::state::MdEngineChoice::Gromacs => {
            // A configured override or a native PATH install wins (cheap, already
            // resolved by probe). Otherwise, on Windows GROMACS conventionally
            // lives in WSL: auto-detect it (cold-starts WSL once) so the common
            // setup works with no manual configuration. Only when there is no
            // WSL, or WSL has no gmx, do we surface the not-found guidance.
            if let Some(launch) = registry.launch(EngineId::GROMACS).cloned() {
                return Ok(launch);
            }
            let launch = crate::engines::registry::detect_wsl_gromacs_launch().ok_or_else(|| {
                anyhow!(
                    "Could not find GROMACS. Install it and ensure `gmx` is on PATH, set up WSL with GROMACS installed, or configure its launch in Settings -> Engines."
                )
            })?;
            // Persist the detected launch as an override so later builds reuse it
            // directly instead of cold-starting WSL to re-probe every time (slow),
            // and so it shows up in Settings -> Engines.
            persist_detected_engine_launch(state, EngineId::GROMACS, launch.clone());
            Ok(launch)
        }
    }
}

/// Cache an auto-detected engine launch into `engine_overrides` and save the
/// config, so later builds reuse it without re-probing. No-op when an override
/// already exists (set by the user or a prior detection) so a configured launch
/// is never clobbered.
fn persist_detected_engine_launch(
    state: &mut AppState,
    id: EngineId,
    launch: crate::engines::registry::EngineLaunch,
) {
    if cache_engine_override(&mut state.config.engine_overrides, id, launch) {
        // Keep the Settings panel draft in sync so it reflects the cached launch.
        state.ui.settings.engine_drafts.remove(id.as_str());
        persist_engine_config(state, "GROMACS launch detected and saved");
        // Refresh the Settings registry so the engine's status indicator flips to
        // available (green) immediately — the detection just succeeded, so the
        // user shouldn't have to click "Detect" to see it. Cheap re-probe (reads
        // the override; no `--version` WSL cold-start).
        reprobe_engines(state);
    }
}

/// Insert `launch` as the override for `id` only when none is configured.
/// Returns `true` when newly inserted (the caller should then persist), `false`
/// when an existing override was left untouched.
fn cache_engine_override(
    overrides: &mut std::collections::HashMap<String, crate::engines::registry::EngineLaunch>,
    id: EngineId,
    launch: crate::engines::registry::EngineLaunch,
) -> bool {
    let key = id.as_str().to_string();
    if overrides.contains_key(&key) {
        return false;
    }
    overrides.insert(key, launch);
    true
}

fn build_md_stage_specs(
    steps: &[crate::frontend::state::MdRunStepPrompt],
) -> anyhow::Result<Vec<StageSpec>> {
    crate::frontend::md_support::build_md_stage_specs(steps)
}

/// Cheap availability resolve (no subprocess). Used to populate the panel on
/// first open and after edits, without paying the WSL `--version` cost.
fn reprobe_engines(state: &mut AppState) {
    state.ui.settings.engine_registry = Some(EngineRegistry::probe(&state.config.engine_overrides));
}

/// Slow, user-initiated: resolve availability *and* run each engine's
/// `--version`, then record the time so the panel can show how fresh the
/// version strings are.
fn detect_engine_versions(state: &mut AppState) {
    state.ui.settings.engine_registry = Some(EngineRegistry::probe_with_versions(
        &state.config.engine_overrides,
    ));
    state.ui.settings.engine_versions_checked_at = Some(std::time::SystemTime::now());
    state.set_message("Detected engine versions".to_string());
}

fn apply_engine_override(state: &mut AppState, id: EngineId) {
    let key = id.as_str().to_string();
    let draft = state
        .ui
        .settings
        .engine_drafts
        .entry(key.clone())
        .or_default();
    match draft.to_launch() {
        Some(launch) => {
            state.config.engine_overrides.insert(key, launch);
        }
        None => {
            state.config.engine_overrides.remove(&key);
        }
    }
    // "Apply & Detect" is an explicit user action, so paying the version probe
    // cost here is expected.
    detect_engine_versions(state);
    persist_engine_config(state, "engine launch updated");
}

fn clear_engine_override(state: &mut AppState, id: EngineId) {
    let key = id.as_str().to_string();
    state.config.engine_overrides.remove(&key);
    state.ui.settings.engine_drafts.remove(&key);
    persist_engine_config(state, "engine override cleared; using auto-detection");
    reprobe_engines(state);
}

fn browse_engine_program(state: &mut AppState, id: EngineId) {
    let Some(path) = rfd::FileDialog::new()
        .set_directory(&state.config.default_project_dir)
        .pick_file()
    else {
        return;
    };
    let key = id.as_str().to_string();
    let draft = state.ui.settings.engine_drafts.entry(key).or_default();
    draft.program = path.to_string_lossy().into_owned();
}

fn persist_engine_config(state: &mut AppState, message: &str) {
    match save_config(&state.config) {
        Ok(()) => state.set_message(message.to_string()),
        Err(error) => state.set_message(format!("failed to save engine settings: {error}")),
    }
}

pub(super) fn add_hydrogens(state: &mut AppState) {
    let before = state.capture_edit_snapshot();
    state.cancel_transient_jobs();
    state.ui.pending_optimization = None;
    let added = state.structure_mut().add_missing_hydrogens();
    state.mark_structure_changed();
    state.set_source_path(None);
    state.ui.editor = None;
    state
        .ui
        .selection
        .retain_valid(state.structure().atoms.len());
    state.history.push_undo(before);
    state.set_message(format!("Added {added} hydrogens"));
}

pub(super) fn recompute_bonds(state: &mut AppState) {
    let before = state.capture_edit_snapshot();
    state.cancel_transient_jobs();
    state.ui.pending_optimization = None;
    state.structure_mut().recompute_bonds();
    state.mark_structure_changed();
    state.set_source_path(None);
    state.ui.editor = None;
    state
        .ui
        .selection
        .retain_valid(state.structure().atoms.len());
    state.history.push_undo(before);
    state.set_message(format!(
        "Recomputed bonds: {} bonds detected",
        state.structure().bonds.len()
    ));
}

pub(super) fn translate_atoms_into_first_unit_cell(state: &mut AppState) {
    if let Err(error) = require_periodic_structure(
        state.structure(),
        "translating atoms into the first unit cell requires a periodic structure",
    ) {
        state.set_message(error.to_string());
        return;
    }

    let before = state.capture_edit_snapshot();
    state.cancel_transient_jobs();
    state.ui.pending_optimization = None;
    state.set_source_path(None);
    state.ui.editor = None;
    state
        .structure_mut()
        .wrap_atoms_into_cell_preserving_bonds();
    state.mark_structure_changed();
    state
        .ui
        .selection
        .retain_valid(state.structure().atoms.len());
    state.history.push_undo(before);
    state.set_message("Translated atoms into the first unit cell".to_string());
}

fn expand_supercell(state: &mut AppState, repeats: [u32; 3]) {
    let before = state.capture_edit_snapshot();
    state.cancel_transient_jobs();
    state.ui.pending_optimization = None;
    state.set_source_path(None);
    state.ui.editor = None;
    state.structure_mut().make_supercell(repeats);
    state.mark_structure_changed();
    state.ui.selection.clear();
    state.history.push_undo(before);
    state.set_message(format!(
        "Expanded to {}x{}x{} supercell ({} atoms, {} bonds)",
        repeats[0],
        repeats[1],
        repeats[2],
        state.structure().atoms.len(),
        state.structure().bonds.len()
    ));
    complete_active_task(state, TaskKind::ExpandSupercell, TaskStatus::Completed);
}

/// Build the MD system with the engine the panel selected. Returns `true` once
/// the work is launched/finished and the panel may close; `false` leaves the
/// panel open with a reported reason so the user can adjust inputs and retry.
///
/// GROMACS is the default: it runs the full pdb2gmx pipeline on a worker thread
/// and produces a force-field topology a run can reuse. The built-in path is a
/// geometry-only fallback (box + solvation coordinates, no topology) that the
/// user can opt into explicitly.
fn build_md_system(state: &mut AppState, prompt: &crate::frontend::state::MdSystemPrompt) -> bool {
    use crate::frontend::state::MdBuildEngine;
    use crate::workflows::molecular_dynamics::is_framework_shape;
    match prompt.engine {
        MdBuildEngine::Gromacs => {
            // A covalent framework (celled, bonded, non-biopolymer) has no residue
            // template for pdb2gmx; generate its topology directly from the bonds
            // instead. The material build validates parameters and reports any
            // element the built-in tables and the custom force field don't cover.
            if is_framework_shape(state.structure()) {
                start_material_md_build(state, prompt)
            } else {
                start_gromacs_md_build(state, prompt)
            }
        }
        MdBuildEngine::BuiltIn => build_md_system_builtin(state, prompt),
    }
}

/// Out-of-plane cell length (A) the framework cell editor is seeded to when the
/// crystal's own gap is thinner. It clears a 1.0 nm cutoff plus the Verlet
/// buffer on both sides of the slab (`2·(1.0+0.1) nm + buffer`), so the default
/// box runs without the user having to widen `c` first.
const FRAMEWORK_C_FLOOR_ANGSTROM: f32 = 25.0;

/// Launch the framework (nanosheet) build: generate the topology from the
/// structure's bonds (rigid or flexible per the prompt), use the user-edited
/// crystal cell as the box, and optionally solvate. Writes `topol.top` and
/// `framework_run.json` into the build run directory for a later MD run.
fn start_material_md_build(
    state: &mut AppState,
    prompt: &crate::frontend::state::MdSystemPrompt,
) -> bool {
    use crate::engines::gromacs::MaterialBuildRequest;

    if state.jobs.engine_running() {
        state.set_message("another external engine job is already running".to_string());
        return false;
    }
    let gmx_launch =
        match resolve_md_engine_launch(state, crate::frontend::state::MdEngineChoice::Gromacs) {
            Ok(launch) => launch,
            Err(error) => {
                state.set_message(error.to_string());
                return false;
            }
        };
    let run_dir = match ensure_active_task_run_dir(
        state,
        TaskKind::BuildMdSystem,
        Some(prompt.run_name.as_str()),
    ) {
        Ok(path) => path,
        Err(error) => {
            state.set_message(format!("failed to create run directory: {error}"));
            complete_active_task(state, TaskKind::BuildMdSystem, TaskStatus::Failed);
            return false;
        }
    };
    if let Some(task_run_id) = state.active_task_run {
        mark_task_status(state, task_run_id, TaskStatus::Running);
        state
            .tasks
            .set_engine_label(task_run_id, Some("GROMACS".to_string()));
        sync_task_manifest(state, task_run_id);
    }

    // The box is the user-edited crystal cell, preserving its (e.g. hexagonal)
    // shape. Falling back to the structure's own cell keeps non-GUI callers
    // working. When an explicit cell is supplied, build_material_system uses it
    // verbatim instead of opening the out-of-plane axis itself.
    let cell_override = prompt.framework_cell.map(|[a, b, c, alpha, beta, gamma]| {
        crate::domain::UnitCell::from_parameters(a, b, c, alpha, beta, gamma)
    });

    state.ui.pending_optimization = None;
    let job = crate::frontend::jobs::spawn_material_build_job(MaterialBuildRequest {
        structure: state.structure().clone(),
        mode: prompt.framework_mode,
        working_dir: run_dir,
        gmx_launch,
        solvation: prompt.solvation_options(),
        cell_override,
        custom_force_field: prompt.custom_force_field_text.clone(),
        solvent_gap_angstrom: FRAMEWORK_C_FLOOR_ANGSTROM,
        cutoff_nm: crate::workflows::molecular_dynamics::DEFAULT_CUTOFF_NM,
        max_duration: Duration::from_secs(60 * 60),
    });
    state.jobs.set_engine(job);
    state.set_message("Building framework MD system; press Esc to stop".to_string());
    true
}

/// Launch the GROMACS pdb2gmx → editconf → solvate → genion pipeline as a
/// background engine job, writing its `topol.top` into the build run directory
/// so a later MD run can reuse it. On a setup error (engine missing, run
/// directory) it reports the reason and keeps the panel open.
fn start_gromacs_md_build(
    state: &mut AppState,
    prompt: &crate::frontend::state::MdSystemPrompt,
) -> bool {
    if state.jobs.engine_running() {
        state.set_message("another external engine job is already running".to_string());
        return false;
    }
    // GROMACS is required for this build; we never silently fall back to a
    // topology-less geometry build.
    let gmx_launch =
        match resolve_md_engine_launch(state, crate::frontend::state::MdEngineChoice::Gromacs) {
            Ok(launch) => launch,
            Err(error) => {
                state.set_message(error.to_string());
                return false;
            }
        };
    let run_dir = match ensure_active_task_run_dir(
        state,
        TaskKind::BuildMdSystem,
        Some(prompt.run_name.as_str()),
    ) {
        Ok(path) => path,
        Err(error) => {
            state.set_message(format!("failed to create run directory: {error}"));
            complete_active_task(state, TaskKind::BuildMdSystem, TaskStatus::Failed);
            return false;
        }
    };
    if let Some(task_run_id) = state.active_task_run {
        mark_task_status(state, task_run_id, TaskStatus::Running);
        state
            .tasks
            .set_engine_label(task_run_id, Some("GROMACS".to_string()));
        sync_task_manifest(state, task_run_id);
    }

    // Only attach an ion step when solvation is on and the user asked for ions;
    // genion needs the solvent it replaces.
    let ions = if prompt.solvate && (prompt.neutralize || prompt.add_salt) {
        Some(IonOptions {
            neutralize: prompt.neutralize,
            concentration_molar: prompt.add_salt.then_some(prompt.salt_concentration_molar),
            positive_ion: prompt.positive_ion.clone(),
            negative_ion: prompt.negative_ion.clone(),
        })
    } else {
        None
    };

    state.ui.pending_optimization = None;
    let job = spawn_gromacs_build_job(BuildRequest {
        structure: state.structure().clone(),
        working_dir: run_dir,
        gmx_launch,
        force_field: prompt.force_field.clone(),
        water: prompt.water,
        box_config: prompt.config(),
        solvate: prompt.solvate,
        ions,
        max_duration: Duration::from_secs(60 * 60),
    });
    state.jobs.set_engine(job);
    state.set_message("GROMACS building MD system; press Esc to stop".to_string());
    true
}

/// Returns `true` on success; on failure it reports the reason and leaves the
/// panel open so the user can adjust inputs and retry.
fn build_md_system_builtin(
    state: &mut AppState,
    prompt: &crate::frontend::state::MdSystemPrompt,
) -> bool {
    let config = prompt.config();
    let result = crate::workflows::molecular_dynamics::build_md_system(state.structure(), &config);
    let (boxed, report) = match result {
        Ok(value) => value,
        Err(error) => {
            state.set_message(format!("MD system build failed: {error}"));
            return false;
        }
    };

    // Optionally fill the freshly built box with water and ions — geometry only,
    // no force field (an engine parameterizes the system later). On failure keep
    // the panel open so the user can adjust the box or solvation settings.
    let solvated = match prompt.solvation_options() {
        Some(options) => match crate::workflows::molecular_dynamics::solvate(&boxed, &options) {
            Ok(out) => Some(out),
            Err(error) => {
                state.set_message(format!("MD system solvation failed: {error}"));
                return false;
            }
        },
        None => None,
    };

    if let Err(error) = ensure_active_task_run_dir(
        state,
        TaskKind::BuildMdSystem,
        Some(prompt.run_name.as_str()),
    ) {
        state.set_message(format!("failed to create run directory: {error}"));
        complete_active_task(state, TaskKind::BuildMdSystem, TaskStatus::Failed);
        return false;
    }
    if let Some(task_run_id) = state.active_task_run {
        mark_task_status(state, task_run_id, TaskStatus::Running);
    }
    state.cancel_transient_jobs();
    state.ui.pending_optimization = None;
    state.ui.editor = None;
    state.ui.selection.clear();

    // The entry structure is the solvated system when solvation ran, else the
    // bare box.
    let (final_structure, solvation_note) = match solvated {
        Some((structure, report)) => (
            structure,
            format!(
                "; solvated +{} water, +{} {}, +{} {}",
                report.water_added,
                report.cations_added,
                prompt.positive_ion,
                report.anions_added,
                prompt.negative_ion,
            ),
        ),
        None => (boxed, String::new()),
    };
    let save_path = structure_io::default_structure_save_path(&final_structure, None);
    let entry_id = add_and_show_entry(state, final_structure, None, save_path);
    if let Some(task_run_id) = state.active_task_run {
        record_task_result_entry(state, task_run_id, entry_id);
    }

    let [a, b, c] = report.edges_angstrom;
    let replaced = if report.replaced_existing_cell {
        " (replaced existing cell)"
    } else {
        ""
    };
    state.set_message(format!(
        "Built MD system: {a:.1} x {b:.1} x {c:.1} A box, {} atoms{replaced}{solvation_note}",
        state.structure().atoms.len()
    ));
    complete_active_task(state, TaskKind::BuildMdSystem, TaskStatus::Completed);
    true
}

fn select_all(state: &mut AppState) {
    state.ui.selection.select_all(state.structure().atoms.len());
    state.set_message(format!("Selected {} atom(s)", state.ui.selection.len()));
}

fn invert_selection(state: &mut AppState) {
    state.ui.selection.invert(state.structure().atoms.len());
    state.set_message(format!("Selected {} atom(s)", state.ui.selection.len()));
}

fn clear_selection(state: &mut AppState) {
    state.ui.selection.clear();
    state.set_message("Cleared atom selection".to_string());
}

fn select_category(state: &mut AppState, category: crate::domain::AtomCategory) {
    let indices: Vec<usize> = {
        let structure = state.structure();
        (0..structure.atoms.len())
            .filter(|index| structure.atom_category(*index) == category)
            .collect()
    };
    let count = indices.len();
    state.ui.selection.select_indices(indices);
    if count == 0 {
        state.set_message(format!("No {} atoms found", category.label()));
    } else {
        state.set_message(format!("Selected {count} {} atom(s)", category.label()));
    }
}

fn select_atom(state: &mut AppState, atom_index: usize, toggle: bool) {
    if toggle {
        state.ui.selection.toggle(atom_index);
    } else {
        state.ui.selection.select_only(atom_index);
    }
    state
        .ui
        .selection
        .retain_valid(state.structure().atoms.len());
    if state.ui.selection.is_empty() {
        state.set_message("Cleared atom selection".to_string());
    } else {
        state.set_message(format!("Selected {} atom(s)", state.ui.selection.len()));
    }
}

/// Apply a per-atom drawing style to the current selection. An empty selection
/// means "all atoms" so the user can restyle the whole structure in one click.
fn set_selection_style(state: &mut AppState, style: crate::frontend::state::AtomStyle) {
    // Resolve the (index, category) pairs under an immutable structure borrow,
    // then apply the style to the viewport — keeping the override map sparse.
    let items: Vec<(usize, crate::domain::AtomCategory)> = {
        let structure = state.structure();
        let indices: Vec<usize> = if state.ui.selection.is_empty() {
            (0..structure.atoms.len()).collect()
        } else {
            state.ui.selection.ordered_indices()
        };
        indices
            .into_iter()
            .map(|index| (index, structure.atom_category(index)))
            .collect()
    };
    let count = items.len();
    state.ui.viewport.apply_atom_styles(items, style);
    state.set_message(format!("Set {} atom(s) to {}", count, style.label()));
}

/// Set the project-level style for a category. Written to both the project
/// defaults (so it persists project-wide and new entries inherit it) and the
/// active viewport (so the change is visible immediately).
fn set_category_style(
    state: &mut AppState,
    category: crate::domain::AtomCategory,
    style: crate::frontend::state::AtomStyle,
) {
    state
        .ui
        .project_viewport
        .set_category_style(category, style);
    state.ui.viewport.set_category_style(category, style);
    state.set_message(format!("{} default: {}", category.label(), style.label()));
}

/// Revert per-atom style overrides for the current selection (or all atoms when
/// nothing is selected) back to the category tiers.
fn reset_selection_style(state: &mut AppState) {
    if state.ui.selection.is_empty() {
        state.ui.viewport.atom_styles.clear();
        state.set_message("Reset all atom styles".to_string());
    } else {
        let indices = state.ui.selection.ordered_indices();
        let count = indices.len();
        state.ui.viewport.clear_atom_styles(indices);
        state.set_message(format!("Reset style for {count} atom(s)"));
    }
}

fn undo(state: &mut AppState) {
    let Some(previous) = state.history.take_undo() else {
        return;
    };
    let current = state.capture_edit_snapshot();
    state.history.push_redo(current);
    state.restore_edit_snapshot(previous);
    state.set_message("Undid last change".to_string());
}

fn redo(state: &mut AppState) {
    let Some(next) = state.history.take_redo() else {
        return;
    };
    let current = state.capture_edit_snapshot();
    state.history.push_undo(current);
    state.restore_edit_snapshot(next);
    state.set_message("Redid last change".to_string());
}

fn create_group(state: &mut AppState, name: String) {
    match state.entries.create_group(&name) {
        Some(group_id) => {
            state.ui.entry_list.creating_group = false;
            state.ui.entry_list.new_group_name.clear();
            state.ui.entry_list.collapsed_group_ids.remove(&group_id);
            state.set_message(format!("Created group {}", name.trim()));
        }
        None => state.set_message("Group name cannot be empty".to_string()),
    }
}

fn rename_group(state: &mut AppState, group_id: &str, new_name: &str) {
    state.entries.rename_group(group_id, new_name);
    state.ui.entry_list.renaming_group_id = None;
    state.ui.entry_list.rename_group_buffer.clear();
}

fn delete_group(state: &mut AppState, group_id: &str) {
    if state.entries.delete_group(group_id) {
        state.ui.entry_list.collapsed_group_ids.remove(group_id);
        if state.ui.entry_list.renaming_group_id.as_deref() == Some(group_id) {
            state.ui.entry_list.renaming_group_id = None;
            state.ui.entry_list.rename_group_buffer.clear();
        }
        state.set_message("Deleted group".to_string());
    } else {
        state.set_message("Cannot delete group".to_string());
    }
}

fn delete_group_with_entries(state: &mut AppState, group_id: &str) {
    let ids: Vec<u64> = state
        .entries
        .records
        .iter()
        .filter(|e| e.group_id == group_id)
        .map(|e| e.id)
        .collect();
    for id in ids {
        delete_entry(state, id);
    }
    delete_group(state, group_id);
}

fn move_entry_to_group(state: &mut AppState, entry_id: u64, group_id: &str) {
    if state.entries.move_entry_to_group(entry_id, group_id) {
        if group_id.is_empty() {
            state.set_message("Removed entry from group".to_string());
        } else {
            state.set_message("Moved entry to group".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use eframe::egui::Context;
    use nalgebra::Point3;

    use crate::{
        backend::project::WorkspaceSession,
        domain::{Atom, Structure, UnitCell},
        frontend::{actions::AppAction, state::AppState},
    };

    fn test_structure(title: &str) -> Structure {
        Structure::new(
            title,
            vec![Atom {
                element: "C".to_string(),
                position: Point3::new(0.0, 0.0, 0.0),
                charge: 0.0,
            }],
        )
    }

    #[test]
    fn undo_and_redo_restore_edit_snapshot_metadata() {
        let ctx = Context::default();
        let mut state = AppState::new(
            test_structure("original"),
            Some(PathBuf::from(r"C:\tmp\original.xyz")),
            WorkspaceSession::Scratch,
            Default::default(),
            Vec::new(),
            None,
        );
        state.ui.selection.select_only(0);

        let before = state.capture_edit_snapshot();
        *state.structure_mut() = test_structure("edited");
        state.set_source_path(None);
        state.set_save_path(PathBuf::from(r"C:\tmp\edited.cif"));
        state.ui.selection.clear();
        state.history.push_undo(before);

        super::dispatch(&mut state, AppAction::Undo, &ctx);
        assert_eq!(state.structure().title, "original");
        assert_eq!(
            state
                .entries
                .active_entry()
                .and_then(|entry| entry.source_path.as_ref()),
            Some(&PathBuf::from(r"C:\tmp\original.xyz"))
        );
        assert_eq!(state.save_path(), &PathBuf::from(r"C:\tmp\original.xyz"));
        assert_eq!(state.ui.selection.ordered_indices(), vec![0]);

        super::dispatch(&mut state, AppAction::Redo, &ctx);
        assert_eq!(state.structure().title, "edited");
        assert_eq!(
            state
                .entries
                .active_entry()
                .and_then(|entry| entry.source_path.as_ref()),
            None
        );
        assert_eq!(state.save_path(), &PathBuf::from(r"C:\tmp\edited.cif"));
        assert!(state.ui.selection.is_empty());
    }

    #[test]
    fn entry_changes_move_the_fingerprint_but_view_changes_do_not() {
        let ctx = Context::default();
        let mut state = scratch_state(test_structure("mol"));
        let fingerprint = state.entries_fingerprint();

        // View-only interactions (selection, restyle) must not change the
        // fingerprint, so they never schedule a save.
        super::dispatch(&mut state, AppAction::SelectAll, &ctx);
        assert_eq!(
            state.entries_fingerprint(),
            fingerprint,
            "selection is view-only and must not move the fingerprint"
        );
        super::dispatch(&mut state, AppAction::ResetSelectionStyle, &ctx);
        assert_eq!(
            state.entries_fingerprint(),
            fingerprint,
            "restyling is view-only and must not move the fingerprint"
        );

        // Adding an entry is a persisted change and must move the fingerprint.
        super::dispatch(&mut state, AppAction::NewEmptyEntry, &ctx);
        assert_ne!(
            state.entries_fingerprint(),
            fingerprint,
            "adding an entry must move the fingerprint"
        );
    }

    #[test]
    fn autosave_deadline_is_scheduled_and_cleared() {
        let mut state = scratch_state(test_structure("mol"));
        assert_eq!(state.autosave_deadline(), None);
        state.request_autosave(10.0, 0.5);
        assert_eq!(state.autosave_deadline(), Some(10.5));
        // A later request pushes the deadline back (debounce coalescing).
        state.request_autosave(10.4, 0.5);
        assert_eq!(state.autosave_deadline(), Some(10.9));
        state.clear_autosave_deadline();
        assert_eq!(state.autosave_deadline(), None);
    }

    #[test]
    fn run_task_wraps_periodic_structure() {
        let ctx = Context::default();
        let structure = Structure::with_cell(
            "cell",
            vec![Atom {
                element: "C".to_string(),
                position: Point3::new(12.0, -1.0, 0.0),
                charge: 0.0,
            }],
            UnitCell::from_parameters(10.0, 10.0, 10.0, 90.0, 90.0, 90.0),
        );
        let mut state = AppState::new(
            structure,
            None,
            WorkspaceSession::Scratch,
            Default::default(),
            Vec::new(),
            None,
        );

        super::dispatch(
            &mut state,
            AppAction::CreateTask("translate-into-cell"),
            &ctx,
        );

        let atom = &state.structure().atoms[0];
        assert!(atom.position.x >= 0.0 && atom.position.x < 10.0);
        assert!(atom.position.y >= 0.0 && atom.position.y < 10.0);
    }

    #[test]
    fn nanosheet_task_opens_and_builds_on_empty_workspace() {
        // A nanosheet is the natural first thing to build with nothing loaded;
        // opening and building it must not require (or panic without) an entry.
        let ctx = Context::default();
        let mut state = scratch_state(Structure::empty());
        assert!(!state.has_active_entry());

        super::dispatch(&mut state, AppAction::CreateTask("build-nanosheet"), &ctx);
        assert!(state.ui.nanosheet_builder.is_some(), "panel should open");

        super::dispatch(&mut state, AppAction::BuildNanosheet, &ctx);
        assert!(state.has_active_entry(), "build should create an entry");
        assert!(state.structure().cell.is_some());
        assert!(state.structure().atoms.len() > 2);
    }

    fn scratch_state(structure: Structure) -> AppState {
        AppState::new(
            structure,
            None,
            WorkspaceSession::Scratch,
            Default::default(),
            Vec::new(),
            None,
        )
    }

    #[test]
    fn panel_dashboard_opens_on_empty_workspace() {
        let ctx = Context::default();
        // No atoms/title => no active entry at all.
        let mut state = scratch_state(Structure::empty());
        assert!(!state.has_active_entry());

        super::dispatch(&mut state, AppAction::CreateTask("build-md-system"), &ctx);
        assert!(
            state.ui.pending_md_system.is_some(),
            "interactive dashboard should open even with an empty workspace"
        );
    }

    #[test]
    fn switching_entries_keeps_open_dashboard_populated() {
        let ctx = Context::default();
        let mut state = scratch_state(test_structure("mol"));

        super::dispatch(&mut state, AppAction::CreateTask("build-md-system"), &ctx);
        assert!(state.ui.pending_md_system.is_some());

        // Creating + switching to another entry resets transient state; the
        // open dashboard must re-populate against the new structure.
        super::dispatch(&mut state, AppAction::NewEmptyEntry, &ctx);
        assert!(
            state.ui.pending_md_system.is_some(),
            "dashboard should survive an entry switch"
        );
    }

    #[test]
    fn md_system_confirm_defers_box_fit_check_and_keeps_panel() {
        use crate::frontend::state::{MdBuildEngine, MdSystemSizingMode};
        let ctx = Context::default();
        // Two atoms 2 A apart: a 0.5 A absolute box cannot contain them.
        let structure = Structure::new(
            "mol",
            vec![
                Atom {
                    element: "C".to_string(),
                    position: Point3::new(0.0, 0.0, 0.0),
                    charge: 0.0,
                },
                Atom {
                    element: "C".to_string(),
                    position: Point3::new(2.0, 0.0, 0.0),
                    charge: 0.0,
                },
            ],
        );
        let mut state = scratch_state(structure);

        super::dispatch(&mut state, AppAction::CreateTask("build-md-system"), &ctx);
        let prompt = state
            .ui
            .pending_md_system
            .as_mut()
            .expect("dashboard renders immediately");
        // The synchronous box-fit check is the built-in engine's; GROMACS boxes
        // via a subprocess job that unit tests can't run.
        prompt.engine = MdBuildEngine::BuiltIn;
        prompt.mode = MdSystemSizingMode::Absolute;
        prompt.absolute_angstrom = [0.5, 0.5, 0.5];

        // The undersized-box check runs at confirm time, not at open time: it
        // rejects gracefully and leaves the panel open for correction.
        super::dispatch(&mut state, AppAction::ConfirmMdSystem, &ctx);
        assert!(state.ui.pending_md_system.is_some());
        assert!(state.structure().cell.is_none());
    }

    #[test]
    fn confirm_md_system_boxes_structure_and_completes_task() {
        use crate::frontend::state::MdBuildEngine;
        let ctx = Context::default();
        let mut state = scratch_state(test_structure("mol"));

        super::dispatch(&mut state, AppAction::CreateTask("build-md-system"), &ctx);
        // The built-in engine boxes synchronously; GROMACS (the default) would
        // need a real subprocess this unit test can't run.
        state
            .ui
            .pending_md_system
            .as_mut()
            .expect("panel open after create")
            .engine = MdBuildEngine::BuiltIn;
        super::dispatch(&mut state, AppAction::ConfirmMdSystem, &ctx);

        assert!(state.structure().cell.is_some());
        assert!(state.ui.pending_md_system.is_none());
    }

    #[test]
    fn reopening_md_panel_reinitializes_dashboard() {
        let ctx = Context::default();
        let mut state = scratch_state(test_structure("mol"));

        super::dispatch(&mut state, AppAction::CreateTask("build-md-system"), &ctx);
        let task_id = state.tasks.active_panel.expect("panel open after create");

        // Canceling consumes the form and closes the panel.
        super::dispatch(&mut state, AppAction::CancelMdSystemPrompt, &ctx);
        assert!(state.ui.pending_md_system.is_none());

        // Re-opening restores the dashboard without re-running the task.
        super::dispatch(&mut state, AppAction::OpenTaskPanel(task_id), &ctx);
        assert!(state.ui.pending_md_system.is_some());
    }

    #[test]
    fn supercell_dashboard_defers_periodic_check_to_confirm() {
        let ctx = Context::default();
        // Non-periodic structure: the dashboard still opens.
        let mut state = scratch_state(test_structure("mol"));

        super::dispatch(&mut state, AppAction::CreateTask("expand-supercell"), &ctx);
        assert!(state.ui.pending_supercell.is_some());

        // Confirming without a cell is rejected, leaving the panel open.
        super::dispatch(&mut state, AppAction::ConfirmSupercell, &ctx);
        assert!(state.ui.pending_supercell.is_some());
        assert!(state.structure().cell.is_none());
    }

    #[test]
    fn caching_a_detected_launch_inserts_once_and_never_clobbers() {
        use crate::engines::registry::{EngineId, EngineLaunch};
        use std::collections::HashMap;

        let mut overrides: HashMap<String, EngineLaunch> = HashMap::new();
        let detected = EngineLaunch {
            command_prefix: vec!["wsl.exe".to_string(), "-e".to_string()],
            program: "/usr/local/gromacs/bin/gmx".to_string(),
        };

        // First detection caches the launch.
        assert!(super::cache_engine_override(
            &mut overrides,
            EngineId::GROMACS,
            detected.clone()
        ));
        assert_eq!(
            overrides.get("gromacs").map(|l| l.program.as_str()),
            Some("/usr/local/gromacs/bin/gmx")
        );

        // A later detection must not overwrite a launch already configured.
        let other = EngineLaunch {
            command_prefix: vec!["wsl.exe".to_string(), "-e".to_string()],
            program: "gmx".to_string(),
        };
        assert!(!super::cache_engine_override(
            &mut overrides,
            EngineId::GROMACS,
            other
        ));
        assert_eq!(
            overrides.get("gromacs").map(|l| l.program.as_str()),
            Some("/usr/local/gromacs/bin/gmx")
        );
    }
}
