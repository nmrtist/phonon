use std::collections::{BTreeMap, BTreeSet};

use eframe::egui::Color32;

use crate::{domain::AtomCategory, domain::Structure, frontend::state::AtomStyle};

/// The software's built-in drawing style for each chemical category. These are
/// the lowest tier of style resolution: a project may override them per
/// category, and individual atoms may override the project.
pub fn software_default_style(category: AtomCategory) -> AtomStyle {
    match category {
        AtomCategory::Protein | AtomCategory::NucleicAcid => AtomStyle::Cartoon,
        AtomCategory::Ion => AtomStyle::Sphere,
        // Solvent is shown like any other molecule by default; the user hides it
        // on demand from the View panel (or `view water off`). No automatic
        // program-side hiding.
        AtomCategory::Solvent | AtomCategory::Ligand | AtomCategory::Other => {
            AtomStyle::BallAndStick
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightPreset {
    Soft,
    Gentle,
    Studio,
}

impl LightPreset {
    pub fn all() -> &'static [Self] {
        &[Self::Soft, Self::Gentle, Self::Studio]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Soft => "Soft",
            Self::Gentle => "Gentle",
            Self::Studio => "Studio",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceStyle {
    Fill,
    Mesh,
}

impl SurfaceStyle {
    pub fn all() -> &'static [Self] {
        &[Self::Fill, Self::Mesh]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Fill => "Fill",
            Self::Mesh => "Mesh",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CartoonSectionStyle {
    pub width: f32,
    pub thickness: f32,
}

impl CartoonSectionStyle {
    pub const fn new(width: f32, thickness: f32) -> Self {
        Self { width, thickness }
    }
}

#[derive(Debug, Clone)]
pub struct ViewportCartoonState {
    pub helix: CartoonSectionStyle,
    pub sheet: CartoonSectionStyle,
    pub coil: CartoonSectionStyle,
    pub smoothing: usize,
    pub profile_segments: usize,
}

impl Default for ViewportCartoonState {
    fn default() -> Self {
        Self {
            helix: CartoonSectionStyle::new(2.35, 0.50),
            sheet: CartoonSectionStyle::new(3.05, 0.36),
            coil: CartoonSectionStyle::new(0.58, 0.58),
            smoothing: 8,
            profile_segments: 10,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ViewportLightingState {
    pub preset: LightPreset,
    pub silhouettes: bool,
    pub silhouette_width: f32,
}

impl Default for ViewportLightingState {
    fn default() -> Self {
        Self {
            preset: LightPreset::Soft,
            silhouettes: false,
            silhouette_width: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ViewportSurfaceState {
    pub chains: BTreeSet<char>,
    pub style: SurfaceStyle,
    pub transparency: f32,
}

impl Default for ViewportSurfaceState {
    fn default() -> Self {
        Self {
            chains: BTreeSet::new(),
            style: SurfaceStyle::Fill,
            transparency: 0.8,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ViewportIonState {
    pub show_within: Option<f32>,
    pub color: Option<Color32>,
}

#[derive(Debug, Clone)]
pub struct ViewportVisualState {
    pub background_color: Color32,
    /// Project-level style override for each chemical category, overriding the
    /// [`software_default_style`]. Empty categories fall back to the software
    /// default. This is the "project display style" the user sets in View.
    pub category_styles: BTreeMap<AtomCategory, AtomStyle>,
    /// Per-atom style overrides, keyed by atom index. Sparse — only atoms the
    /// user explicitly restyled appear here; everything else resolves through
    /// the category tiers. Pruned when the active structure changes.
    pub atom_styles: BTreeMap<usize, AtomStyle>,
    pub show_atom_labels: bool,
    pub show_cell: bool,
    pub lighting: ViewportLightingState,
    pub cartoon: ViewportCartoonState,
    pub chain_colors: BTreeMap<char, Color32>,
    pub surface: ViewportSurfaceState,
    pub ions: ViewportIonState,
    pub hetero_atom_colors: bool,
}

impl ViewportVisualState {
    /// Factory background. Also the sentinel for "no explicit choice — follow
    /// the app theme"; the viewport swaps in the theme's background when the
    /// stored color still equals this.
    pub const DEFAULT_BACKGROUND: Color32 = Color32::from_rgb(245, 247, 249);

    /// Whether the background should track the active light/dark theme, i.e. the
    /// user hasn't picked a custom color in settings.
    pub fn background_follows_theme(&self) -> bool {
        self.background_color == Self::DEFAULT_BACKGROUND
    }
}

impl Default for ViewportVisualState {
    fn default() -> Self {
        Self {
            background_color: Self::DEFAULT_BACKGROUND,
            category_styles: BTreeMap::new(),
            atom_styles: BTreeMap::new(),
            show_atom_labels: false,
            show_cell: true,
            lighting: ViewportLightingState::default(),
            cartoon: ViewportCartoonState::default(),
            chain_colors: BTreeMap::new(),
            surface: ViewportSurfaceState::default(),
            ions: ViewportIonState::default(),
            hetero_atom_colors: false,
        }
    }
}

impl ViewportVisualState {
    /// The project-level style for a category: its override if set, otherwise the
    /// software default.
    pub fn category_style(&self, category: AtomCategory) -> AtomStyle {
        self.category_styles
            .get(&category)
            .copied()
            .unwrap_or_else(|| software_default_style(category))
    }

    /// The effective drawing style for one atom, resolving the three tiers:
    /// per-atom override → project category override → software default.
    pub fn resolved_atom_style(&self, structure: &Structure, atom_index: usize) -> AtomStyle {
        if let Some(style) = self.atom_styles.get(&atom_index) {
            return *style;
        }
        self.category_style(structure.atom_category(atom_index))
    }

    /// Set (or clear) the project-level style for a category. Setting it to the
    /// software default clears the override to keep the map sparse.
    pub fn set_category_style(&mut self, category: AtomCategory, style: AtomStyle) {
        if style == software_default_style(category) {
            self.category_styles.remove(&category);
        } else {
            self.category_styles.insert(category, style);
        }
    }

    /// Apply an explicit per-atom style to atoms given as `(index, category)`
    /// pairs. When the chosen style already equals what the atom would resolve
    /// to from the category tiers, the override is removed instead — so the map
    /// stays sparse and an atom that matches its project/software default carries
    /// no row. Callers precompute the categories (via
    /// [`crate::domain::Structure::atom_category`]) so this needs no structure
    /// borrow.
    pub fn apply_atom_styles(
        &mut self,
        items: impl IntoIterator<Item = (usize, AtomCategory)>,
        style: AtomStyle,
    ) {
        for (index, category) in items {
            if style == self.category_style(category) {
                self.atom_styles.remove(&index);
            } else {
                self.atom_styles.insert(index, style);
            }
        }
    }

    /// Remove per-atom overrides for `indices`, reverting them to the category
    /// tiers.
    pub fn clear_atom_styles(&mut self, indices: impl IntoIterator<Item = usize>) {
        for index in indices {
            self.atom_styles.remove(&index);
        }
    }

    /// Drop overrides for atom indices that no longer exist.
    pub fn retain_atom_styles(&mut self, atom_count: usize) {
        self.atom_styles.retain(|index, _| *index < atom_count);
    }
}
