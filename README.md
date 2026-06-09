# Phonon

Computational environment for molecular & material research.

## Features

- 3D visualization and editing of molecular/crystal structures
- Force field geometry optimization
- Molecular quantum mechanics (quantum chemistry) calculations
- GUI-assisted molecular dynamics system setup and execution
- Reticular structure builder (COFs, MOFs) from building blocks
- Script-driven workflows supported on both GUI and CLI platforms
- Agent-friendly CLI access for automated workflow execution
- Intuitive GUI for versatile interactive operations

## Installation

### Prebuilt Executables

Prebuilt executables can be downloaded from GitHub Releases.

### Build from Source

Install the Rust toolchain, then build the release executable:

```sh
cargo build --release
```

The built binary is written to `target/release/`.

## External Dependencies

### GROMACS (required for molecular dynamics)

Molecular dynamics simulations require [GROMACS](https://www.gromacs.org/) to be installed separately.
GPU acceleration is **strongly recommended** — running MD on CPU alone is technically possible but prohibitively slow for any non-trivial system.

- **Windows:** Install GROMACS inside [WSL](https://learn.microsoft.com/en-us/windows/wsl/install) (`sudo apt install gromacs`). For GPU support, compile from source with CUDA inside WSL.
- **Linux:** `sudo apt install gromacs` for a quick start; compile from source with CUDA/ROCm for GPU acceleration.
- **macOS:** `brew install gromacs`. Note that GPU acceleration is not supported on Apple hardware, so MD performance will be limited.

## License

[Apache-2.0](LICENSE)
