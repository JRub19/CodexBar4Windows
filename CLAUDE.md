# CLAUDE.md

## Git

- Commit after every completed task. A task is any change that leaves the code working: a fix, a feature, a refactor, a config change. When in doubt, commit.
- Atomic commits only. One logical change per commit. If you touched unrelated things, split them.
- Conventional commits: `type(scope): description` (feat, fix, refactor, chore, docs, test). Lowercase, under 72 chars, no period.
- Push after every commit. If CI fails, read the error, fix it, commit, push again. Do not wait for instructions.

## Style

- No em dashes or dashes in prose. Use commas, periods, colons.

## Workflow

- Do not ask permission for git ops, running tests, installing deps, or reading files. Just do it. If something breaks, fix it.
