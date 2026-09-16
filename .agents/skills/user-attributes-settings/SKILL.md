---
name: user-attributes-settings
description: "Place user attributes and global user policy on the correct Admin surface. Use when adding user fields, registration/SSO/identity-source config, Settings sections, or deciding whether a knob belongs on Users, Settings, or outside the gateway."
---

# User attributes and Settings placement

Canonical home for **where a user-related field lives**. Visual rules stay in `docs/design.md` + skill `admin-ui-change`. Domain API layering stays in skill `admin-domain-resource`.

Source pattern: xrouter Users page (list+detail) + Settings section nav. Do not copy xrouter-only product knobs (pool AIMD, smart routing, license) into a kit stub unless the product actually has them.

## Symptom / misjudgment

- A new "user property" (department, title, cost center, customer tier) is added to `users`.
- Registration, SSO, or identity-source config lands on the Users page, user-menu, or a new sidebar item.
- A business page duplicates a Settings form so there are two sources of truth.
- List/detail APIs return `password_hash` or a plaintext password.
- Bulk create ships 200 initial passwords instead of activation links.

## Placement (authoritative)

| Kind | Home | Examples |
| --- | --- | --- |
| Per-user identity | Users page (master–detail) + create/edit dialog | `username`, `email`, `role_id`, `status`, org/project membership |
| Per-user operations | Users detail cards, not Settings | quota, granted instances, that user's API keys |
| Self-service secret | User-menu **dialog** | change **own** password |
| Account chrome | Top-right user menu | avatar; Settings → `/settings`; logout |
| System-wide user policy | **Settings page only** | `registration_enabled`, identity providers / SSO, default JIT role |
| Customer business model | **Never in the gateway** | department, job title, cost center, approval chain, customer value |

AGENTS.md already requires: global config only on Settings. This skill says **which** knobs are global vs per-user vs out of scope.

Business pages may **show** current global state and link to Settings. They must not host a second copy of the form.

## User attribute allowlist

Store only generic identity. Typical columns / DTO fields:

- `id`, `username` (unique), `email` (unique, nullable display/fill), `role_id`, `status`, `created_at` / `updated_at`
- Membership via join tables (`org_users`, `project_users`), not denormalized org/project name on `users`
- `password_hash` (nullable for SSO users) — **never serialized** on list/detail (`serde(skip_serializing)` or equivalent)
- SSO binding in `user_identities`: `provider_id` + `external_subject` (stable IdP id). Email is **not** the SSO match key

Search placeholders on Users must list the fields actually searched (usually username / email).

## Forbidden on `users` (and on Settings)

Do not invent gateway fields for the customer's org chart or commerce:

- department, title, employee grade, cost center, approval chain
- "VIP / paying customer" flags that drive routing or quota
- Inferring quota, billing, or route policy from IdP groups except by mapping groups → **roles**

The customer system decides those meanings, then writes gateway parameters (role, quota, Key, route). The gateway does not model the business.

## Settings page shape

When adding or extending Settings:

1. **Extend Settings IA first** (a section id + nav label), then build the form. Do not add a sibling nav entry for Identity / License / Tokens / Pool.
2. Page composition: `PageShell` + `PageHeader` (title only, no casual subtitle) + section nav + one section at a time.
3. Section cards use **view vs edit**: load current values; Edit copies into a draft; Cancel restores; a sticky save bar commits. Do not autosave each toggle unless the product already does for that section.
4. Hide admin-only sections from non-admins (do not render a disabled tease).
5. Secret config (IdP client secret, bind password, service token) follows the same class as API keys: masked on read, single-reveal on create/rotate. See skill `api-key-lifecycle-security`.
6. Registration / SSO belong in Settings → general or an Identity section, not on Users. Verification email is `Mailer` (Cloudflare-first); see [`docs/architecture.md`](../../../docs/architecture.md) § Outbound mail. Do not speak SMTP from Admin or Settings.

## Password and provisioning

- Create user: optional local password **or** one-time activation link. Do not generate bulk plaintext initial passwords.
- Admin edit password is write-only; empty means unchanged.
- Self-service change-password requires current password, enforces complexity, revokes sessions on success.
- SSO users with `password_hash IS NULL` must be rejected on the local-password login path (negative test required).
- Batch create: dry-run then per-row independent commits; reuse the single-create authz checks on **every** row.

## Procedure (adding a field or settings knob)

1. Classify with the placement table. If "customer business", **stop** and keep it in the customer system.
2. If global policy: add a Settings section or a field in an existing section. Wire resource functions under `frontend/src/lib/resources/`. Do not add a new sidebar item or put the form in the user-menu. The user-menu Settings row only **navigates** to `/settings`.
3. If per-user identity: extend the Users DTO + create/edit dialog + detail card. Do not put it on Settings.
4. If per-user operations: add a detail card on Users (quota, grants), not a column dump of every related entity.
5. i18n: `t()` + both `zh.ts` and `en.ts`. No mixed-language labels.
6. Visible Users search/sort must stay honest (skill `admin-ui-change` + `docs/ai/agents/ui-entry.md`).

## Checklist

- [ ] New field classified; no department/title/cost-center-style column on `users`.
- [ ] Global knobs live only under Settings; no duplicate form on Users or a new top-level nav item.
- [ ] `password_hash` / IdP secrets never returned on list/detail reads.
- [ ] Settings section is view/edit + draft + save, admin-gated when required.
- [ ] Users page remains master–detail; create/edit stay in Dialogs (`max-h-[85vh]`).
- [ ] `zh.ts` / `en.ts` updated symmetrically for every new string.
