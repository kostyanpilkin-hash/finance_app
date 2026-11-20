---
name: verify
description: Quick verification suite for Finance — tsc, eslint, vitest, cargo fmt/clippy/test. Use after any code change before committing, or when the user says «проверь», «прогони тесты», «verify».
---

Run from the repo root (git-bash), stop at the first failure and fix it:

```bash
npx tsc --noEmit
npm run lint
npm test 2>&1 | tail -8
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | grep -E "^(warning|error)|Finished" | head -30
cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked" | head -30
```

If `cargo fmt --check` fails, run `cargo fmt --all` and re-check.
