# Versioning System Setup Complete ✅

This document records the setup of the versioning system with release channels for the Kubarr project.

## Setup Date
**2026-02-16**

## What Was Configured

### 1. Branch Protection Rules ✅

**Main Branch Protection** (Applied to `main` branch):
- ✅ Require pull request before merging
  - Requires 1 approval
  - Dismiss stale reviews when new commits are pushed
- ✅ Require status checks to pass before merging
  - Branch must be up to date before merging
  - Required checks:
    - `docker / docker (kubarr-backend-test)` - Backend tests
    - `docker / docker (kubarr-frontend-test)` - Frontend tests
    - `docs` - Documentation build
- ✅ Require conversation resolution before merging
- ✅ Enforce restrictions for administrators (no bypass)
- ✅ Prevent force pushes
- ✅ Prevent branch deletion

**View Settings**: https://github.com/bmartensNL/Kubarr/settings/branches

### 2. Tag Protection Rules ✅

**Version Tag Protection** (Ruleset ID: 12880007):
- Protected pattern: `refs/tags/v*`
- Rules:
  - ✅ Prevent tag deletion
  - ✅ Prevent non-fast-forward updates (force updates)
- No bypass actors (admins must also follow rules)

**View Settings**: https://github.com/bmartensNL/Kubarr/rules/12880007

### 3. Pull Request Created ✅

**PR #22**: Add versioning system with release channels
- URL: https://github.com/bmartensNL/Kubarr/pull/22
- Branch: `feature/versioning-system`
- Status: Open, awaiting review and CI checks

## Release Channels

The versioning system supports three release channels:

| Channel | Tag Pattern | Badge Color | Use Case |
|---------|-------------|-------------|----------|
| **stable** | `v*.*.*` | 🟢 Green | Production releases |
| **release** | `v*.*.*-rc.*`, `v*.*.*-beta.*` | 🟡 Yellow | Release candidates |
| **dev** | No tag | 🔵 Blue | Development builds |

## Workflow Enforcement

With the current setup:

1. **All changes require PR**: Direct pushes to `main` are blocked
2. **Tests must pass**: Backend tests, frontend tests, and docs build must succeed
3. **Code review required**: At least 1 approval needed before merge
4. **Conversations must be resolved**: All review comments must be addressed
5. **Version tags are protected**: Cannot be deleted or force-updated

## Next Steps

### Step 1: Merge the Versioning PR

Once CI passes on PR #22:
```bash
# Review the PR and approve it (via GitHub UI)
# Then merge using "Squash and merge" or "Merge commit"
```

### Step 2: Create Initial Version Tag

After merging, create the initial v0.1.0 tag:
```bash
git checkout main
git pull
git tag -a v0.1.0 -m "Initial release v0.1.0"
git push origin v0.1.0
```

### Step 3: Deploy and Verify

Deploy with the new versioning system:
```bash
./scripts/deploy.sh

# Start port-forward
kubectl port-forward -n kubarr svc/kubarr-backend 8080:8000 &

# Verify version API
curl http://localhost:8080/api/system/version | jq

# Check UI at http://localhost:8080
# Footer should show: [stable] v0.1.0 <commit> (<date>)
```

## Using the Versioning System

### Bump Patch Version (Bug Fix)
```bash
./scripts/version-bump.sh patch --tag
git push && git push origin v0.1.1
```

### Bump Minor Version (New Feature)
```bash
./scripts/version-bump.sh minor --tag
git push && git push origin v0.2.0
```

### Create Release Candidate
```bash
./scripts/version-bump.sh minor --channel release --tag
git push && git push origin v0.2.0-rc.1
```

### Promote RC to Stable
```bash
# Tag the same commit as stable
git tag -a v0.2.0 -m "Release 0.2.0" v0.2.0-rc.1
git push origin v0.2.0
```

## Troubleshooting

### If CI Checks Fail on PR #22

Check the workflow runs:
```bash
gh run list --branch feature/versioning-system
gh run view <run-id>
```

### If You Need to Update Branch Protection

View current settings:
```bash
gh api repos/bmartensNL/Kubarr/branches/main/protection
```

Update settings (example - require 2 approvals):
```bash
gh api \
  --method PUT \
  repos/bmartensNL/Kubarr/branches/main/protection \
  --input protection.json
```

### If You Need to Bypass Protection (Emergency)

Temporarily disable enforcement:
```bash
gh api \
  --method DELETE \
  repos/bmartensNL/Kubarr/branches/main/protection/enforce_admins
```

Remember to re-enable after:
```bash
gh api \
  --method POST \
  repos/bmartensNL/Kubarr/branches/main/protection/enforce_admins
```

## Documentation

Full versioning documentation available at:
- **[docs/versioning.md](../docs/versioning.md)** - Complete versioning guide
- **[README.md](../README.md)** - Quick reference

## Contact

For questions or issues with the versioning system:
- Open an issue: https://github.com/bmartensNL/Kubarr/issues
- Review the PR: https://github.com/bmartensNL/Kubarr/pull/22

---

**Setup completed by**: Claude Code Agent
**Date**: 2026-02-16
**Branch Protection ID**: main
**Tag Protection Ruleset ID**: 12880007
**Initial PR**: #22
