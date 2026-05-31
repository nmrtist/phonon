use std::{fs, path::Path};

use anyhow::{Context, Result, bail};

use crate::domain::Structure;

use super::{
    structure_codec::{parse_structure, serialize_structure},
    structure_paths,
};

pub use super::{
    structure_format::StructureFormat,
    structure_paths::{
        default_structure_save_path, path_with_format_extension, preferred_save_format,
        readable_extensions, suggested_save_stem, writable_formats,
    },
    structure_text::{to_cif, to_pdb, to_xyz},
};

pub fn load_structure(path: &Path) -> Result<Structure> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let format = structure_paths::format_from_path(path)
        .ok_or_else(|| anyhow::anyhow!(structure_paths::unsupported_read_message(path)))?;

    parse_structure(format, &source)
        .with_context(|| format!("failed to parse {} input", format.label()))
}

pub fn save_structure(structure: &Structure, path: &Path) -> Result<()> {
    let format = structure_paths::format_from_path(path)
        .ok_or_else(|| anyhow::anyhow!(structure_paths::unsupported_write_message(path)))?;
    if !format.supports_write() {
        bail!("{} export is not supported", format.label());
    }

    let contents = serialize_structure(format, structure)?;

    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}
