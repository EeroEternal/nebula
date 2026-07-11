# Nebula Agent Rules

This document defines rules and conventions for AI agents and developers working on the Nebula project.

## File Organization

- **Documentation:** Keep filenames in the `docs/` directory concise and descriptive. Avoid excessively long names.
  - `docs/manual/` — user manuals, install and deployment
  - `docs/dev/` — developer docs, engineering checklists, component designs
  - `docs/arch/` — architecture and optimization plans
  - Index: `docs/README.md`
- **Scripts & Tests:**
    - All standalone test scripts, debug scripts, and utility scripts must be placed in the `scripts/` directory.
    - Production-ready binaries and service management scripts belong in `bin/`.
- **Temporary Data:** Do not store temporary data (like `default.etcd`) in the project root. Use `/tmp` or other designated temporary locations.

## Versioning

- Follow semantic versioning for releases.
- Ensure `CHANGELOG.md` is updated when releasing a new version.
