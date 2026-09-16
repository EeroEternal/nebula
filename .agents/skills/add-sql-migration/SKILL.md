---
name: add-sql-migration
description: Add a SQL schema change for BFF/Postgres. Nebula BFF currently uses CREATE TABLE IF NOT EXISTS in crates/nebula-bff (not sqlx::migrate!). Use when adding tables/columns or introducing sqlx migrations.
---

# Add a SQL migration (sqlx compile-time embed)

## Symptom / misjudgment
BFF schema today lives in `crates/nebula-bff` as `CREATE TABLE IF NOT EXISTS` (see `auth.rs`). There is no `sqlx::migrate!` embed. If you add a column only in docs or only in a new `.sql` file, the running BFF will not apply it.

If the product later introduces `migrations/NNN_*.sql` + `sqlx::migrate!`: cargo does not rebuild after a **new file** under `migrations/`, so the version never appears in `_sqlx_migrations`.

## Root cause
`sqlx::migrate!("./migrations")` embeds migration files at **compile time**. Cargo does not treat a newly added file in that directory as a reason to rebuild the crate that embeds it.

## Procedure
1. Create `migrations/NNN_<short_snake_name>.sql` with the next unused version prefix.
2. If using in-code DDL: edit `crates/nebula-bff` and `cargo build -p nebula-bff`.
   If using `sqlx::migrate!`: `touch` a source file in that crate, then `cargo build -p nebula-bff`.
3. Start BFF with a valid `DATABASE_URL`; confirm the table/column exists.
4. Confirm: `\dt` / query `_sqlx_migrations` shows the new `version`, or logs show it applied.

## Editing an existing migration
- Changing an already-applied file triggers sqlx checksum checks.
- `src/db/pool.rs` has a local repair path; for a clean re-run locally you may `DELETE FROM _sqlx_migrations WHERE version = <N>;` then restart (dev only).
- Do not rewrite applied migrations on shared/prod DBs — add a new forward migration instead.

## Verification
- After touch + build + start, `_sqlx_migrations` contains the new version.
- Do not conclude "SQL bug" until this rebuild step was done.
