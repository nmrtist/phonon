use std::path::PathBuf;

use anyhow::Result;
use eframe::{egui, egui_wgpu, wgpu};

use crate::{
    backend::{
        config::{load_config, load_recent_projects},
        housekeeping,
        project::{WorkspaceSession, open_project, remember_opened_project},
    },
    domain::Structure,
    frontend::{actions::AppAction, dispatcher, state::AppState, ui},
};

pub fn run(structure: Structure, source_path: Option<PathBuf>) -> Result<()> {
    let options = eframe::NativeOptions {
        // Keep the GUI paced for tooling workloads instead of chasing high-refresh displays.
        vsync: true,
        multisampling: 0,
        // A depth buffer for egui's render pass, so the GPU molecule renderer can
        // depth-test impostors against it. 32 bits → `Depth32Float`, matched by
        // `viewport::gpu::DEPTH_FORMAT`.
        depth_buffer: 32,
        wgpu_options: low_power_wgpu_options(),
        viewport: window_viewport(),
        ..Default::default()
    };

    eframe::run_native(
        "Phonon",
        options,
        Box::new(|cc| {
            let mut fonts = egui::FontDefinitions::default();
            egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
            install_system_fonts(&mut fonts);
            cc.egui_ctx.set_fonts(fonts);
            crate::frontend::theme::apply(&cc.egui_ctx);
            let mut app = PhononApp::new(structure, source_path);
            if let Some(render_state) = cc.wgpu_render_state.as_ref() {
                crate::frontend::viewport::init_gpu_renderer(render_state);
                app.state.ui.gpu_ready = true;
            }
            crate::frontend::theme::set_preference(&cc.egui_ctx, app.state.config.theme);
            // Install the frosted-glass material behind the content (macOS),
            // only when the vibrancy path is enabled. Runs on the main thread
            // here, as `apply_vibrancy` requires.
            #[cfg(target_os = "macos")]
            if crate::frontend::glass::supported() {
                crate::frontend::glass::install(cc);
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

/// Build the main window's viewport.
///
/// On macOS we use the *native* window frame — standard traffic-light buttons,
/// continuous-curvature (squircle) corners, and the system drop shadow — via a
/// transparent titlebar plus a full-size content view, so our custom title bar
/// draws behind the native buttons.
/// Windows/Linux keep a borderless, transparent window with app-drawn chrome
/// (custom controls, rounded corners, resize handles).
fn window_viewport() -> egui::ViewportBuilder {
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1180.0, 760.0])
        .with_min_inner_size([860.0, 560.0]);

    #[cfg(target_os = "macos")]
    {
        let viewport = viewport
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_titlebar_buttons_shown(true)
            .with_has_shadow(true);
        // Only make the NSWindow non-opaque when the vibrancy path is enabled
        // (see `glass::supported`). A transparent surface without a correctly
        // layered effect view behind it renders blank, so the default stays
        // opaque.
        if crate::frontend::glass::supported() {
            viewport.with_transparent(true)
        } else {
            viewport
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        viewport.with_decorations(false).with_transparent(true)
    }
}

fn low_power_wgpu_options() -> egui_wgpu::WgpuConfiguration {
    let mut options = egui_wgpu::WgpuConfiguration::default();
    if let egui_wgpu::WgpuSetup::CreateNew(create_new) = &mut options.wgpu_setup {
        create_new.power_preference =
            wgpu::PowerPreference::from_env().unwrap_or(wgpu::PowerPreference::LowPower);
    }
    options
}

/// On macOS, prefer the system font (SF Pro for UI text, SF Mono for code) so
/// the interface reads as native. Falls back silently to egui's bundled fonts
/// on other platforms or if the system files are unavailable.
fn install_system_fonts(fonts: &mut egui::FontDefinitions) {
    #[cfg(target_os = "macos")]
    {
        let mut install = |name: &str, path: &str, family: egui::FontFamily| {
            if let Ok(bytes) = std::fs::read(path) {
                fonts.font_data.insert(
                    name.to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                if let Some(list) = fonts.families.get_mut(&family) {
                    list.insert(0, name.to_owned());
                }
            }
        };
        install(
            "SF Pro",
            "/System/Library/Fonts/SFNS.ttf",
            egui::FontFamily::Proportional,
        );
        install(
            "SF Mono",
            "/System/Library/Fonts/SFNSMono.ttf",
            egui::FontFamily::Monospace,
        );
    }
    #[cfg(not(target_os = "macos"))]
    let _ = fonts;
}

pub struct PhononApp {
    state: AppState,
    last_viewport_title: String,
}

impl PhononApp {
    fn new(structure: Structure, source_path: Option<PathBuf>) -> Self {
        let mut config = load_config();
        let mut recent_projects = load_recent_projects();
        let mut startup_message = None;
        let mut state = if !config.closed_to_scratch {
            if let Some(last_project_path) = config.last_project_path.clone() {
                match open_project(&last_project_path) {
                    Ok((project, snapshot)) => {
                        let _ =
                            remember_opened_project(&mut config, &mut recent_projects, &project);
                        let recovered_from_crash = housekeeping::acquire_lock(&project);
                        let mut state = AppState::new(
                            structure,
                            source_path,
                            WorkspaceSession::Project(project),
                            config,
                            recent_projects,
                            Some(snapshot),
                        );
                        if recovered_from_crash {
                            startup_message = Some(
                                "Recovered project: previous session did not close cleanly"
                                    .to_string(),
                            );
                        }
                        if let Some(message) = startup_message.take() {
                            state.set_message(message);
                        }
                        return Self {
                            state,
                            last_viewport_title: String::new(),
                        };
                    }
                    Err(error) => {
                        startup_message =
                            Some(format!("Last project unavailable; opened Scratch: {error}"));
                        config.last_project_path = None;
                        AppState::scratch(config, recent_projects)
                    }
                }
            } else {
                AppState::scratch(config, recent_projects)
            }
        } else {
            AppState::scratch(config, recent_projects)
        };
        if let Some(message) = startup_message {
            state.set_message(message);
        }
        Self {
            state,
            last_viewport_title: String::new(),
        }
    }

    fn open_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped_paths = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect::<Vec<_>>()
        });
        if dropped_paths.is_empty() {
            return;
        }

        dispatcher::open_paths(&mut self.state, dropped_paths);
    }

    fn show_file_drop_overlay(&self, ctx: &egui::Context) {
        let hovered_count = ctx.input(|input| {
            input
                .raw
                .hovered_files
                .iter()
                .filter(|file| file.path.is_some())
                .count()
        });
        if hovered_count == 0 {
            return;
        }

        egui::Area::new(egui::Id::new("file_drop_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(260.0);
                    ui.vertical_centered(|ui| {
                        ui.heading("Drop to open");
                        if hovered_count == 1 {
                            ui.label("Release to open the structure file");
                        } else {
                            ui.label(format!("Release to open {hovered_count} structure files"));
                        }
                    });
                });
            });
    }
}

impl eframe::App for PhononApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let viewport_title = format!(
            "Phonon - {} - {}",
            self.state.workspace_label(),
            self.state.current_entry_label()
        );
        if viewport_title != self.last_viewport_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(viewport_title.clone()));
            self.last_viewport_title = viewport_title;
        }
        if ctx.input(|input| input.viewport().close_requested()) {
            dispatcher::shutdown(&mut self.state);
        }
        self.open_dropped_files(&ctx);
        dispatcher::poll_jobs(&mut self.state, &ctx);
        dispatcher::handle_history_shortcuts(&mut self.state, &ctx);

        // Resolve once per frame whether the frosted glass is revealed; read by
        // the chrome fills below and by `clear_color`. Re-evaluated every frame
        // so toggling the preference or the OS "Reduce Transparency" setting
        // takes effect live.
        self.state.ui.glass_active = crate::frontend::glass::glass_active(self.state.config.glass);
        self.state.ui.glass_alpha = self
            .state
            .ui
            .glass_active
            .then(|| crate::frontend::theme::glass_alpha(self.state.config.glass_intensity));

        let mut actions = Vec::<AppAction>::new();
        ui::show_workbench(&mut self.state, ui, &mut actions);
        self.show_file_drop_overlay(&ctx);
        for action in actions {
            dispatcher::dispatch(&mut self.state, action, &ctx);
        }
        dispatcher::flush_pending_autosave(&mut self.state, &ctx);
        self.state.record_message_change();
    }

    /// Backing color behind the UI.
    ///
    /// macOS backing is opaque and matched to the active theme's window backing
    /// (which equals the central panel fill), so the native title bar shows no
    /// seam and the native shadow stays intact, in light or dark. Other
    /// platforms use a transparent backing so the app-drawn rounded corners read
    /// as empty (revealing the desktop behind them).
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        #[cfg(target_os = "macos")]
        {
            // With glass revealed, clear fully transparent so the vibrancy
            // material behind the window shows through the semi-transparent
            // chrome. Otherwise keep the opaque backing matched to the central
            // panel fill (seamless native title bar, intact shadow).
            if self.state.ui.glass_active {
                return [0.0, 0.0, 0.0, 0.0];
            }
            crate::frontend::theme::Palette::for_dark_mode(visuals.dark_mode)
                .window_backing
                .to_normalized_gamma_f32()
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = visuals;
            [0.0, 0.0, 0.0, 0.0]
        }
    }

    /// Persist only the window geometry, not egui's transient widget memory.
    ///
    /// The `eframe` "persistence" feature (enabled for window size/position recall)
    /// otherwise also serializes the entire egui `Memory` typemap — collapsing-header
    /// open/closed state, scroll offsets, text-edit undo buffers, focus, etc. — which
    /// we don't want surviving restarts. Window geometry is saved separately (gated on
    /// `persist_window`, default true), so it is unaffected by returning false here.
    fn persist_egui_memory(&self) -> bool {
        false
    }
}
