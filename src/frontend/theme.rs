//! Centralized egui theme, nudged toward a native-leaning desktop appearance.
//!
//! The app previously shipped no global theme (it inherited egui's defaults and
//! patched a few widget colors per panel), which left selection highlights as a
//! hard 1px outline and controls nearly square. The native look instead calls
//! for a coherent appearance, *filled* and rounded selection in the accent
//! color (never a hard border), and softly rounded controls. This module sets
//! that baseline once at startup; per-panel tweaks still layer on top.
//!
//! Light and dark are both registered so egui can follow the system appearance
//! (or an explicit user preference) and switch live. Panels draw their chrome
//! from [`Palette`] — a small set of semantic color roles — so a single
//! `palette(ui)` lookup per frame flips every surface with the theme.

use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

use crate::backend::config::ThemeMode;

/// Semantic color roles for app-drawn chrome (panels, text, hairlines, hovers).
///
/// Standard egui widgets read their colors from [`Visuals`] and flip for free;
/// this palette covers the surfaces we paint ourselves. Look it up once per
/// draw function with [`palette`] (reads the live, resolved theme), never cache
/// it on app state.
#[derive(Clone, Copy)]
pub struct Palette {
    // Surfaces.
    /// Opaque window backing (macOS `clear_color`) and central panel base.
    pub window_backing: Color32,
    pub title_bar: Color32,
    pub status_bar: Color32,
    pub activity_bar: Color32,
    pub sidebar: Color32,
    pub central: Color32,
    pub bottom_panel: Color32,
    /// Default background behind the 3D viewport scene.
    pub viewport_bg: Color32,
    // Text.
    pub text_primary: Color32,
    pub text_strong: Color32,
    pub text_muted: Color32,
    pub text_tertiary: Color32,
    // Items / lines.
    pub hairline: Color32,
    pub item_fill: Color32,
    pub item_fill_hover: Color32,
    pub item_fill_active: Color32,
    pub selection_fill: Color32,
    /// Ink used to build low-alpha neutral overlays; dark in light mode and
    /// light in dark mode so hovers read as a highlight either way.
    pub neutral_tint: Color32,
    // Accent / status (saturated; read on either background).
    pub accent: Color32,
    pub selection_blue_tint: Color32,
    pub status_blue: Color32,
    pub status_amber: Color32,
    pub status_green: Color32,
    pub status_red: Color32,
}

impl Palette {
    pub const fn light() -> Self {
        Self {
            window_backing: Color32::from_rgb(245, 247, 249),
            title_bar: Color32::from_rgb(246, 248, 251),
            status_bar: Color32::from_rgb(229, 236, 244),
            activity_bar: Color32::from_rgb(240, 243, 247),
            sidebar: Color32::from_rgb(252, 252, 253),
            central: Color32::from_rgb(245, 247, 249),
            bottom_panel: Color32::from_rgb(248, 249, 251),
            viewport_bg: Color32::from_rgb(245, 247, 249),
            text_primary: Color32::from_rgb(32, 37, 43),
            text_strong: Color32::from_rgb(18, 22, 30),
            text_muted: Color32::from_rgb(92, 100, 112),
            text_tertiary: Color32::from_rgb(120, 128, 138),
            hairline: Color32::from_rgb(226, 232, 240),
            item_fill: Color32::from_rgb(249, 251, 253),
            item_fill_hover: Color32::from_rgb(242, 247, 252),
            item_fill_active: Color32::from_rgb(221, 226, 233),
            selection_fill: Color32::from_rgb(216, 223, 233),
            neutral_tint: Color32::from_rgb(64, 70, 82),
            accent: Color32::from_rgb(0, 122, 255),
            selection_blue_tint: Color32::from_rgb(54, 97, 164),
            status_blue: Color32::from_rgb(120, 146, 184),
            status_amber: Color32::from_rgb(201, 145, 62),
            status_green: Color32::from_rgb(64, 160, 108),
            status_red: Color32::from_rgb(232, 84, 82),
        }
    }

    /// Near-black ("deep") dark theme, slightly cool to echo the light theme's
    /// blue tint. Panels step up in lightness from the window backing.
    pub const fn dark() -> Self {
        Self {
            window_backing: Color32::from_rgb(22, 22, 24),
            title_bar: Color32::from_rgb(30, 30, 33),
            status_bar: Color32::from_rgb(26, 26, 29),
            activity_bar: Color32::from_rgb(28, 28, 31),
            sidebar: Color32::from_rgb(32, 32, 36),
            central: Color32::from_rgb(22, 22, 24),
            bottom_panel: Color32::from_rgb(28, 28, 31),
            viewport_bg: Color32::from_rgb(18, 18, 20),
            text_primary: Color32::from_rgb(228, 231, 236),
            text_strong: Color32::from_rgb(244, 246, 249),
            text_muted: Color32::from_rgb(150, 157, 167),
            text_tertiary: Color32::from_rgb(120, 127, 137),
            hairline: Color32::from_rgb(52, 54, 60),
            item_fill: Color32::from_rgb(40, 40, 45),
            item_fill_hover: Color32::from_rgb(50, 50, 56),
            item_fill_active: Color32::from_rgb(60, 60, 67),
            selection_fill: Color32::from_rgb(44, 58, 82),
            neutral_tint: Color32::from_rgb(210, 215, 222),
            accent: Color32::from_rgb(10, 132, 255),
            selection_blue_tint: Color32::from_rgb(94, 145, 220),
            status_blue: Color32::from_rgb(126, 158, 200),
            status_amber: Color32::from_rgb(216, 162, 84),
            status_green: Color32::from_rgb(98, 184, 122),
            status_red: Color32::from_rgb(238, 104, 102),
        }
    }

    pub fn for_dark_mode(dark: bool) -> Self {
        if dark { Self::dark() } else { Self::light() }
    }

    /// Low-alpha neutral overlay (hover/press) that inverts with the theme:
    /// dark ink over light surfaces, light ink over dark ones.
    pub fn neutral_overlay(&self, alpha: u8) -> Color32 {
        let [r, g, b, _] = self.neutral_tint.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }

    /// Low-alpha blue overlay for selection/active tints.
    pub fn blue_overlay(&self, alpha: u8) -> Color32 {
        let [r, g, b, _] = self.selection_blue_tint.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }
}

/// The palette for the theme egui currently resolves for this `Ui`.
pub fn palette(ui: &egui::Ui) -> Palette {
    Palette::for_dark_mode(ui.visuals().dark_mode)
}

/// Alpha applied to chrome surface fills when frosted glass is active, so the
/// macOS vibrancy material behind the window shows through. Tunable: lower means
/// more see-through glass, higher means a more solid tint (and better text
/// contrast over busy wallpapers).
pub const GLASS_FILL_ALPHA: u8 = 150;

/// Fill for an app-drawn chrome surface (title bar, activity bar, sidebars,
/// status bar). When `glass` is on, the opaque palette color is made
/// semi-transparent so the window's vibrancy material shows through; otherwise
/// it is returned unchanged. The central panel and 3D viewport keep their opaque
/// fills, so the glass never sits behind dense content or the GPU scene.
pub fn chrome_fill(base: Color32, glass: bool) -> Color32 {
    if glass {
        let [r, g, b, _] = base.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, GLASS_FILL_ALPHA)
    } else {
        base
    }
}

/// Register the light and dark themes and start following the system.
///
/// Both visual sets are installed so egui can switch live when the OS
/// appearance changes (or when an explicit preference is set via
/// [`set_preference`]). The actual preference is applied afterwards once the
/// stored config is available.
pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals_of(egui::Theme::Light, build_visuals(false));
    ctx.set_visuals_of(egui::Theme::Dark, build_visuals(true));

    // A slim, subtle, always-visible scroll bar (rather than egui's wide,
    // bright default): a thin handle drawn from the foreground color at low
    // opacity reads as a soft gray on either theme, brightening on hover.
    let mut scroll = egui::style::ScrollStyle::solid();
    scroll.bar_width = 4.0;
    scroll.bar_inner_margin = 4.0;
    scroll.bar_outer_margin = 2.0;
    scroll.handle_min_length = 24.0;
    scroll.foreground_color = true;
    scroll.dormant_background_opacity = 0.0;
    scroll.active_background_opacity = 0.0;
    scroll.interact_background_opacity = 0.0;
    scroll.dormant_handle_opacity = 0.18;
    scroll.active_handle_opacity = 0.30;
    scroll.interact_handle_opacity = 0.50;

    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        ctx.style_mut_of(theme, |style| {
            // Slightly roomier controls, closer to macOS metrics.
            style.spacing.button_padding = egui::vec2(8.0, 4.0);
            style.spacing.scroll = scroll;
            // A wider grab zone for panel resize, so the sidebar divider is easy
            // to catch (and reliably beats the inset scroll bar next to it).
            style.interaction.resize_grab_radius_side = 7.0;
        });
    }

    ctx.options_mut(|options| {
        options.theme_preference = egui::ThemePreference::System;
        // If the OS appearance can't be detected, fall back to light (the
        // app's historical default) rather than egui's dark default.
        options.fallback_theme = egui::Theme::Light;
    });
}

/// Apply a user theme preference (Light / Dark / follow System).
pub fn set_preference(ctx: &egui::Context, mode: ThemeMode) {
    let preference = match mode {
        ThemeMode::System => egui::ThemePreference::System,
        ThemeMode::Light => egui::ThemePreference::Light,
        ThemeMode::Dark => egui::ThemePreference::Dark,
    };
    ctx.set_theme(preference);
}

/// Build the native-leaning [`Visuals`] for one theme, sourced from its palette.
fn build_visuals(dark: bool) -> Visuals {
    let pal = Palette::for_dark_mode(dark);
    let mut visuals = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    // Selection is a soft, *filled*, rounded highlight — not a hard outline.
    // The stroke width is 0 (so no outline is drawn), but its *color* must stay
    // visible: egui uses `selection.stroke.color` as the text color of selected
    // buttons / `selectable_label`s (see `Style::button_style`). A transparent
    // color there makes the selected combo/menu item's text invisible.
    visuals.selection.bg_fill = pal.selection_fill;
    visuals.selection.stroke = Stroke::new(0.0, pal.text_strong);
    visuals.hyperlink_color = pal.accent;

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

    // Inputs keep a faint resting hairline, but hover and press become a soft
    // *filled* block with no outline, rather than the hard wireframe egui draws
    // by default.
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, pal.hairline);

    visuals.widgets.hovered.weak_bg_fill = pal.item_fill_hover;
    visuals.widgets.hovered.bg_fill = pal.item_fill_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::NONE;

    visuals.widgets.active.weak_bg_fill = pal.item_fill_active;
    visuals.widgets.active.bg_fill = pal.item_fill_active;
    visuals.widgets.active.bg_stroke = Stroke::NONE;

    // `fg_stroke` doubles as the line egui draws over a resizable panel's divider
    // on hover/drag. egui's default is near-white and harsh; soften it to a muted
    // gray (and a clearer tone while actively dragging). This also tints hovered
    // widget foregrounds, which reads fine.
    visuals.widgets.hovered.fg_stroke.color = pal.text_muted;
    visuals.widgets.active.fg_stroke.color = pal.text_primary;

    visuals.window_stroke = Stroke::new(1.0, pal.hairline);

    visuals
}
