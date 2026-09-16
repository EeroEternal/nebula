#!/usr/bin/env bash
# Every sidebar href must have a router entry; every non-public route must have a nav href.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

APP="frontend/src/routes/router.tsx"
NAV="frontend/src/lib/nav.ts"

if [[ ! -f "$APP" || ! -f "$NAV" ]]; then
  echo "✓ Admin nav check skipped (no router.tsx / nav.ts)"
  exit 0
fi

fail=0

normalize() {
  local p="$1"
  [[ "$p" == "*" ]] && return 1
  if [[ "$p" == /* ]]; then
    printf '%s\n' "$p"
  else
    printf '/%s\n' "$p"
  fi
}

mapfile -t hrefs < <(grep -oE 'href:[[:space:]]*"[^"]+"' "$NAV" | sed -E 's/href:[[:space:]]*"//;s/"//' | sort -u)

declare -A href_set=()
for h in "${hrefs[@]}"; do
  href_set["$h"]=1
done

declare -A route_set=()
route_set["/"]=1

while IFS= read -r raw; do
  [[ -z "$raw" ]] && continue
  path="$(normalize "$raw")" || continue
  route_set["$path"]=1
done < <(grep -oE "path:[[:space:]]*[\"'][^\"']+[\"']" "$APP" | sed -E "s/path:[[:space:]]*[\"']//;s/[\"']//")

for h in "${hrefs[@]}"; do
  if [[ -z "${route_set[$h]:-}" ]]; then
    echo "✗ nav href $h has no router entry"
    fail=1
  fi
done

public_ok() {
  case "$1" in
    /login|/register|/help) return 0 ;;
    *) return 1 ;;
  esac
}

for r in "${!route_set[@]}"; do
  public_ok "$r" && continue
  if [[ -z "${href_set[$r]:-}" ]]; then
    echo "✗ router path $r has no nav href"
    fail=1
  fi
done

if [[ "$fail" -eq 0 ]]; then
  echo "✓ Admin nav: sidebar hrefs and routes match"
fi
exit "$fail"
