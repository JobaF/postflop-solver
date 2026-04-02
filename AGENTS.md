# Project Overview

Postflop poker solver with a Rust API backend (`app/`) and a standalone Svelte + TypeScript frontend (`app/frontend/`).

## Architecture

- **Rust backend** (`app/`): Axum API server on port 3000, wraps the `postflop-solver` library. Pure API — no static file serving.
- **Svelte frontend** (`app/frontend/`): Vite dev server on port 5173, proxies `/api` to the Rust backend. Uses Svelte 5 with TypeScript.

## Frontend Verification

After any frontend change, always run both of these commands from `app/frontend/`:

```bash
cd app/frontend && npx eslint . --fix
cd app/frontend && npx svelte-check --tsconfig ./tsconfig.json
```

- `npx eslint . --fix` — lint and auto-fix style/import issues (antfu config)
- `npx svelte-check` — TypeScript type checking for `.ts` and `.svelte` files

Both must pass with zero errors before considering a change complete.

## Frontend Conventions

- All code must be TypeScript (`lang="ts"` in Svelte, `.ts` files only)
- ESLint uses `@antfu/eslint-config` with Svelte plugin
- Imports must use `import type` for type-only imports
- All `{#each}` blocks require a key expression

## Database

PostgreSQL is used for persisting solved spots (metadata + serialized game binary).

- Connection string via `DATABASE_URL` env var (see `app/.env.example`)
- Migration in `app/migrations/001_create_spots.sql` (runs automatically on startup)
- Games are serialized with the solver's bincode + zstd compression and stored as `BYTEA`

## Backend Notes

- `bincode` and `bincode_derive` are pinned to `=2.0.0-rc.3` in root `Cargo.toml` — do not change these
- The `zstd` feature is enabled for compressed game storage
