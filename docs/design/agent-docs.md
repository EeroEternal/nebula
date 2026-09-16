# Agent docs (public Markdown)

Products that expose an API to coding agents need a **machine-readable** guide. Humans get HTML; agents get the same Markdown without logging in or executing JavaScript.

This is not a seventh chrome and not a marketing site. Reader job for the HTML view: **browse a catalog** (双栏目录型).

## Two audiences, one source

| Audience | URL | Notes |
| --- | --- | --- |
| Agents (prefer this) | `/llms.txt`, `/docs/guide.md`, `/docs/changelog.md` | Static files under `admin/public/`. No auth. |
| Humans | `/help` | React page that **fetches and renders** those files. |

Do **not** maintain a second prose tree in TSX. Do **not** stand up Docusaurus / a second Vite app (**Greenfield HTML** / **Marketing stack**).

Canonical files:

```
admin/public/llms.txt              # index: what the product is + links to the .md files
admin/public/docs/guide.md         # how to auth, call the API, honest limits
admin/public/docs/changelog.md     # product log for agents (not git history)
frontend/src/components/views/help.tsx           # public renderer
```

`guide.md` is English-first (agents). UI chrome (`help.*`, `auth.agentGuide`) stays in `t()` zh/en. Document only shipped behavior (AGENTS.md: no phantom capabilities).

## HTML page `/help`

- Public route, **outside** `DashboardLayout` and **not** in `nav.ts`.
- `scripts/check_admin_nav.sh` treats `help` like `login` / `register` (no sidebar href).
- Left: doc list (Guide, Changelog). Selected row: `bg-primary/10 font-medium text-primary`.
- Right: markdown in a `Card` (`react-markdown` + `remark-gfm`). A “Markdown source” link opens the `.md`.
- Header: product name, language switcher, Sign in / Console. One outline control — not a second primary.
- No page subtitle. No raw paths (`/llms.txt`) in the visible copy.

User menu **Help** (already allowlisted in [`layout.md`](layout.md) → User menu) navigates to `/help`.

## Login / register

Auth split stays the branding + form card ([`layout.md`](layout.md) → Auth split).

Put **Agent guide** in the **top-right** of the auth page, beside the language switcher (`text-sm text-muted-foreground`, underline on hover). It is chrome, not an auth action.

Do **not**:

- Pair it with Register / Sign in on the form-card footer (those are account actions).
- Put `/llms.txt` or `/docs/guide.md` on the **left branding panel** (**Raw agent paths**).
- Make it a filled primary button (Sign in stays the only primary).

Agents do not need that string. They discover docs from static HTML (below).

## Discoverability without JavaScript

The SPA shell at `/` is empty until React runs. Crawlers that do not execute JS never see the footer link.

Put the URLs where a fetch of `/` still works:

1. `admin/index.html` — `meta name="description"`, `<link rel="alternate" type="text/markdown" href="/docs/guide.md">`, `<link rel="alternate" type="text/plain" href="/llms.txt">`, and a `<noscript>` list of the same links.
2. `admin/public/robots.txt` — comments with the absolute doc URLs.
3. `/.well-known/llms.txt` → `/llms.txt` (Pages `_redirects` rewrite).

Cloudflare Pages serves real files in `dist/` **before** `/* /index.html 200`. Keep the markdown in `public/` so the build copies them.

## Anti-patterns

| Name | Looks like |
| --- | --- |
| **Raw agent paths** | Branding panel: `Agent guide — Agents: /llms.txt and /docs/guide.md — no login.` |
| **JS-only docs** | Guide exists only as a React route; `curl` of `/docs/guide.md` 404s |
| **Login-walled guide** | `/help` inside `RequireAuth` |
| **Sidebar Help** | `/help` in `nav.ts` as if it were an operational page |
