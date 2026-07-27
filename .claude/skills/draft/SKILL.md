---
description: Push current branch and create a draft PR. Use when ready to share work for review or collaborate on a branch.
allowed-tools:
  - Bash(git:*)
  - Bash(gh pr:*)
  - Bash(gh repo:*)
  - Read
  - Glob
  - Grep
---

Create a draft pull request for the current branch.

## Steps

1. Get the current branch name:
   ```
   git branch --show-current
   ```

2. Check for uncommitted changes:
   ```
   git status --porcelain
   ```
   If there are uncommitted changes:
   a. Inspect the diff and separate intended changes from unrelated work
   b. Run the checks required by `docs/patterns/success-criteria.md`
   c. Stage only files intended for this PR, using explicit paths
   d. Create a conventional commit (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`)
   e. Never commit secrets, generated local state, or unrelated user changes

3. Check if the branch has an upstream:
   ```
   git status -sb
   ```
   - If no upstream: `git push -u origin <branch>`
   - If upstream exists: `git push`

4. Determine the base branch. Check in order:
   - `main` branch exists? Use `main`
   - `master` branch exists? Use `master`
   - Fall back to the repo's default branch: `gh repo view --json defaultBranchRef -q .defaultBranchRef.name`

5. Gather context — commits unique to this branch:
   ```
   git log <base>..HEAD --oneline
   ```

6. Create a draft PR:
   ```
   gh pr create --draft --base <base>
   ```
   - Title: descriptive of what the branch accomplishes
   - Body: summarize ALL changes based on the commits

7. Return the PR URL to the user.

## Important

- Commit only changes intended for the PR; unrelated work may remain unstaged
- Inspect `git status`, `git diff`, and recent commits before committing or opening the PR
- Do NOT use `--no-verify` when pushing — let git hooks run
- If the linter/formatter finds issues, fix them before committing
