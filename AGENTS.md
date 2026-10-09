## Agent skills

### Issue tracker

Track issues and specs in GitHub Issues. Read `docs/agents/issue-tracker.md` before ticket operations.

### Triage labels

Use the five canonical labels. Read `docs/agents/triage-labels.md` before triaging.

### Domain docs

Use a single-context layout. Read `docs/agents/domain.md` before exploring domain concepts or decisions.

<!-- project-governance:start -->
## Git conventions

- Stay on the current branch; inspect HEAD, staged, unstaged, and untracked changes at the start of each task.
- Only the primary agent commits and operates on remotes; subagents deliver changes and verification evidence.
- Automatically commit actual changes after applicable validation passes. Do not commit read-only work, unchanged content, or changes that fail validation.
- Commit only task changes, preserving others' work and the original staging state. Clarify ownership when mixed changes cannot be reliably separated. Include both old-path deletions and new-path additions in migration commits.
- Follow the existing commit style; otherwise use Conventional Commits.
- Pushes and tags require explicit authorization; existing authorization remains valid within its scope. A push alone does not create a tag. Authorization to publish to GitHub covers the required push, tag, and Release operations.
- Report and stop on remote or tag conflicts. Do not force-push, overwrite, or delete published tags.
<!-- project-governance:end -->
