#!/usr/bin/env bash
# Fail closed on AGENTS.md creep (AGENTS.md §Knowledge Tiering & Token Budget).
#
# The line cap is the only machine-checkable part of the budget: it is exact,
# reviewable in a diff, and enforces the zero-sum discipline (add one line,
# remove one). The ~1.2k token ceiling is the reason for the cap, not a
# separately computed number — no tokenizer here, so nothing is asserted
# about it.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

max_lines="${AGENTS_MAX_LINES:-80}"
lines="$(awk 'END { print NR }' AGENTS.md)"

if [[ "$lines" -gt "$max_lines" ]]; then
  echo "✗ AGENTS.md has $lines lines (limit $max_lines)." >&2
  echo "  Standing constraints are zero-sum: remove or merge lines before adding." >&2
  echo "  Move procedure detail into .agents/skills/*/SKILL.md or docs/ai/agents/." >&2
  exit 1
fi

echo "✓ AGENTS.md budget: $lines/$max_lines lines"
