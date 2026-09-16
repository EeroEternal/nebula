---
name: admin-ui-change
description: Any user-visible page (Admin, report, landing, HTML, dialog, list, dashboard, settings, i18n). Load docs/design.md progressively. Never invent index.html. Never load every design chapter unless editing the whole system.
---

# Admin UI change (progressive design docs)

## Rule
[`docs/design.md`](../../../docs/design.md) is the **entry** (Overview + Hard rules + PR checklist). Detail chapters live under [`docs/design/`](../../../docs/design/). **Do not** Read every chapter by default. Load the entry, then only the files the task needs.

If editing the design system itself, open only the chapters you change (plus Hard rules / PR checklist).

## Always load
1. `docs/design.md` (whole file — judgment, reader jobs, hard rules, PR checklist)
2. `AGENTS.md` §始终生效 → UI + Admin i18n + 列表排序与检索；细则 [`docs/ai/agents/ui-entry.md`](../../../docs/ai/agents/ui-entry.md)
3. Global settings IA: settings page is the only global-config home; user-field vs Settings vs out-of-gateway placement is skill [`user-attributes-settings`](../user-attributes-settings/SKILL.md)
4. Name the **reader job** (list / detail / wizard / dashboard / settings) before choosing layout

## Route by task (chapter files)

| Task signal | Also read |
| --- | --- |
| Colors, dark mode, badges, status, brand narrative | `docs/design/colors.md` |
| Token table / hex values | `docs/design/tokens.md` |
| Page title, subtitle, density, i18n wrap | `docs/design/typography.md`, `docs/design/layout.md` (no casual subtitles) |
| Page shell, sidebar/topbar, page types, dashboard heights, filters | `docs/design/layout.md` |
| User avatar / account dropdown / logout | `docs/design/layout.md` (User menu) + skill [`user-attributes-settings`](../user-attributes-settings/SKILL.md) |
| Login / register / auth branding panel | `docs/design/layout.md` (Auth split) |
| Agent guide / `llms.txt` / public `/help` | `docs/design/agent-docs.md` |
| Button / Input / Select / Switch / Tabs / Tag sizes and states | `docs/design/components.md` (Control states) + `docs/design/tokens.md` |
| Date picker | `docs/design/components.md` (Control states); use a kit primitive, do not invent a second date widget |
| Top-bar Search+AI | `docs/design/components.md` (Control states — unshipped; do not invent a parallel search chrome) |
| Wizard / Select / expand causes jump | `docs/design/layout.md` (Layout stability) |
| Shadows, selected-row depth, radius | `docs/design/surfaces.md` |
| New list page, row actions, create entry | `docs/design/components.md` (Entity list pattern) + `docs/ai/agents/ui-entry.md` (列表排序与检索) |
| Side-by-side list+detail browser | `docs/design/components.md` (Detail modes, Master–detail) |
| Edit/detail dialog | `docs/design/components.md` (Entity detail dialog, Overlay a11y) |
| Create wizard (API Key, Route) | `docs/design/components.md` + `docs/design/layout.md` (stability) |
| Org/project name display | `docs/design/components.md` (naming display) |
| Quick anti-patterns | `docs/design/dos-donts.md` |
| Settings sections, user fields, registration / SSO / identity source | skill [`user-attributes-settings`](../user-attributes-settings/SKILL.md) |
| Token / CSS baseline change | `docs/design/tokens.md` + `frontend/src/index.css` / Tailwind; then change order in PR checklist footer |

## Stack lock
Any user-visible page (not only "Admin") loads this skill and `docs/design.md`.

1. If `frontend/src` is **missing**: **stop**. Do not write HTML. Scaffold the React kit in a separate commit, or refuse.
2. If a kit exists: implement there. Creating `*.html` + inline CSS is **Greenfield HTML**.
3. Title + redundant subtitle + hero is **Marketing stack** / **Casual subtitle**.
4. Run `bash scripts/check_ui_stack.sh` and `bash scripts/check_admin_nav.sh` before claiming the UI is done. New operational page = `frontend/src/components/views/*.tsx` + `frontend/src/lib/nav.ts` href + `frontend/src/routes/router.tsx` route. Public pages (`/login`, `/register`, `/help`) add the route, do **not** add a sidebar href. Agent docs: [`docs/design/agent-docs.md`](../../../docs/design/agent-docs.md).

## Do not invent
If a chapter and an old page disagree, treat the page as drift unless the user asked to change the spec. Prefer shared primitives under `frontend/src/components/ui/*` (or the product UI kit) and tokens in `index.css`. **Do not invent** a parallel palette or class vocabulary. Named anti-patterns: [`docs/design/dos-donts.md`](../../../docs/design/dos-donts.md).

## Verify
1. Re-check **PR checklist** in `docs/design.md`.
2. New/changed user-visible strings exist in both `zh` and `en` maps in `frontend/src/lib/i18n.tsx`.
3. `cd frontend && npm run lint` (or the product frontend lint)
4. Delivery note: reader job, chapters followed, and any named anti-pattern you avoided.
5. Mechanical: dialog `max-h-[85vh]`, table scroll inside its container, no layout jump on select.

## When promoting new UI lessons
Add the lesson to the matching chapter under `docs/design/` (or Hard rules in the entry). Update this routing table if a new file appears. Mirror this skill to `.agents/skills/admin-ui-change/SKILL.md`.
