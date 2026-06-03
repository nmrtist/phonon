//! Centralized egui theme, nudged toward a native-leaning desktop appearance.
//!
//! The app previously shipped no global theme (it inherited egui's defaults and
//! patched a few widget colors per panel), which left selection highlights as a
//! hard 1px outline and controls nearly square. The native look instead calls
//! for a coherent appearance, *filled* and rounded selection in the accent
//! color (never a hard border), and softly rounded controls. This module sets
//! that baseline once at startup; per-panel tweaks still layer on top.

use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

/// Default platform accent ("blue"). The real value is a user-configurable
/// system accent that only the native layer can read; this is the
/// out-of-the-box default.
const ACCENT: Color32 = Color32::from_rgb(0, 122, 255);

/// Fill of the central workspace panel, and — on macOS — the opaque window
/// backing painted behind the UI so the native frame's shadow and rounded
/// corners stay intact without a seam at the title bar. Both call sites read
/// this single value so the backing can never drift from the panel fill.
pub const CENTRAL_PANEL_FILL: Color32 = Color32::from_rgb(245, 247, 249);

/// Apply the native-leaning theme to an egui context.
pub fn apply(ctx: &egui::Context) {
    // The panels are designed light; pin a light base so the look is coherent
    // regardless of the system appearance (dark-mode support is a later step).
    let mut visuals = Visuals::light();

    // Selection is a soft, *filled*, rounded highlight — not a hard outline.
    // A light gray-blue reads as native without shouting.
    visuals.selection.bg_fill = Color32::from_rgb(216, 223, 233);
    visuals.selection.stroke = Stroke::NONE;
    visuals.hyperlink_color = ACCENT;

    // Controls have gently rounded corners. egui defaults to 2-3px; nudge up.
    let control = CornerRadius::same(6);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = control;
    }
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);

    // Inputs (text fields, combo boxes) keep a faint resting hairline, but hover
    // and press become a soft *filled* light-gray block with no outline, rather
    // than the hard wireframe egui draws by default.
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(226, 232, 240));

    let hover_fill = Color32::from_rgb(232, 235, 240);
    visuals.widgets.hovered.weak_bg_fill = hover_fill;
    visuals.widgets.hovered.bg_fill = hover_fill;
    visuals.widgets.hovered.bg_stroke = Stroke::NONE;

    let press_fill = Color32::from_rgb(221, 226, 233);
    visuals.widgets.active.weak_bg_fill = press_fill;
    visuals.widgets.active.bg_fill = press_fill;
    visuals.widgets.active.bg_stroke = Stroke::NONE;

    visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(222, 228, 236));

    ctx.set_visuals(visuals);

    // Slightly roomier controls, closer to macOS metrics.
    let mut style = (*ctx.global_style()).clone();
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    ctx.set_global_style(style);
}
