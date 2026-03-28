# 🛡️ Branch Protection Rules — Argos Search

This document describes the branch protection rules configured for this repository.

## `main` Branch

| Rule | Setting |
|------|---------|
| **Require pull request reviews** | ✅ Required (1 reviewer) |
| **Require status checks** | ✅ CI must pass |
| **Required checks** | `check`, `test`, `clippy`, `fmt` |
| **Require branches be up to date** | ✅ Yes |
| **No force push** | ✅ Blocked |
| **No deletions** | ✅ Blocked |
| **Restrict direct pushes** | ✅ Only via PR |

## `develop` Branch

| Rule | Setting |
|------|---------|
| **Require status checks** | ✅ CI must pass |
| **No force push** | ✅ Blocked |
| **Allow direct pushes** | ✅ Yes (for quick fixes) |

## Git Workflow

```
feature/* ──PR──> develop ──PR──> main ──tag──> Release
```

1. Create `feature/my-feature` from `develop`
2. Work, commit, push feature branch
3. Open PR to `develop` → CI runs → merge
4. When ready for release: PR from `develop` → `main`
5. Tag `main` with `v*.*.*` → Release workflow triggers automatically

## Labels

| Label | Use |
|-------|-----|
| `feature` | New functionality |
| `bugfix` | Bug fixes |
| `docs` | Documentation only |
| `ci` | CI/CD changes |
| `refactor` | Code refactoring |
| `breaking` | Breaking changes |

## Release Process

1. Ensure `develop` is stable and tested
2. PR `develop` → `main` with summary of changes
3. After merge, tag: `git tag v0.5.0 && git push origin v0.5.0`
4. GitHub Actions automatically builds binaries and creates Release
