# Nebula v1.1.0

Release date: 2026-02-15

## Highlights
- Full frontend bilingual support (中文 / English) with default locale set to Chinese.
- Unified i18n infrastructure with runtime language switching and persisted locale preference.
- Broad UI text migration to translation keys across core views and dialogs.
- Added a practical end-to-end i18n QA checklist for regression verification.

## Added
- New i18n provider and translation runtime:
  - `frontend/src/lib/i18n.tsx`
- App bootstrap integration:
  - `frontend/src/main.tsx` wrapped with `I18nProvider`
- QA checklist document:
  - `docs/i18n_acceptance_checklist.md`

## Changed
- Localized major frontend surfaces, including:
  - App shell/navigation/account menu
  - Login/Profile/Account/Settings
  - Dashboard/Models/Model Detail/Inference/Endpoints/Nodes/Audit
  - Model Catalog/Model Library
  - Templates/Images
  - Load Model dialog and related workflows
- Completed missing key backfill for both `zh` and `en` dictionaries to avoid raw key fallback in UI.

## Validation Summary
- Frontend build passes (`npm run build`).
- Remote smoke checks pass:
  - BFF health endpoint returns 200
  - Auth login and `/auth/me` verified
  - Frontend served successfully on remote host
- Local/remote source parity confirmed for key i18n-related files via hash checks.

## Tag / Commit
- Tag: `v1.1.0`
- Commit: `e1e3248`
