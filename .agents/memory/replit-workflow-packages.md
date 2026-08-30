---
name: Replit workflow package preservation
description: Preserve installed Nix dependency entries when replacing the Replit workflow configuration.
---

When replacing the full Replit workflow configuration, retain any Nix package entries added by the package manager.

**Why:** A validated full-file replacement can silently drop previously installed system dependencies, leaving interactive shells working while restarted workflows cannot find required binaries.

**How to apply:** After any full workflow-config replacement, inspect the resulting Nix package list before restarting managed workflows.