---
name: release
description: Finance release pipeline — verify, bump version, changelog, tag, push, watch the GitHub Actions build that publishes installers to finance_app_public. Use when the user asks to release, «выпусти версию», «собери релиз».
---

1. Run the `verify` skill; everything must be green.
2. Bump the version in all three places to the same `X.Y.Z`:
   - `package.json` → `"version"`
   - `src-tauri/tauri.conf.json` → `"version"`
   - `Cargo.toml` → `[workspace.package] version`
   Then `npm install --package-lock-only` and `cargo check --workspace` to refresh lockfiles.
3. Add a dated section to `CHANGELOG.md` (Russian).
4. Optional local check: `npm run build:installer > /tmp/build.log 2>&1; tail -20 /tmp/build.log` → `installers/<date>/`.
5. Commit, then `git tag vX.Y.Z && git push origin main && git push origin vX.Y.Z`.
6. The `Release` workflow creates a **draft** release in `kostyanpilkin-hash/finance_app_public`. Tell the user to review and publish it. Requires the `PUBLIC_RELEASES_TOKEN` secret (see README).
