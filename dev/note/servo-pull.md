# Upstream Servo Cherry-Pick Procedure

Pull useful commits from upstream Servo (`origin`) into `havi` without breaking our patches.

## Setup (one-time)

Track the last reviewed upstream commit:

    git update-ref refs/upstream-reviewed origin/main

## Workflow

### 1. Fetch and review new commits

    git fetch origin
    git log --oneline refs/upstream-reviewed..origin/main -- \
      components/ python/ resources/ \
      --not -- components/vendored/

Skip directories we never care about (WPT tests, CI, docs) and directories we've heavily patched (`components/vendored/`). Adjust paths over time.

### 2. Check each candidate for overlap

    git show <hash> --stat

Skip anything touching files we've modified. Quick overlap check:

    git diff havi...origin/main -- <file>

### 3. Cherry-pick clean commits

    git cherry-pick <hash>

If it conflicts, abort (`git cherry-pick --abort`) and note it. Conflicting commits go on the revisit list.

### 4. Update the marker

    git update-ref refs/upstream-reviewed origin/main

## Tracking

Record each sync session below.

| Date | Range reviewed | Picked | Skipped (reason) |
|------|----------------|--------|-------------------|
| 2026-02-24 | 5853926db03..d5edb268ab8 | 960a20c2ab9 devtools: Fix breakpoint panic, d68760964cd script: Cleanup async html parser naming, d5edb268ab8 wpt: Enable LCP paint timing | 5853926db03 script: Move contenteditable to dedicated file (massive commit, 132K files, conflicts everywhere) |

## Danger zones

Files with our patches — need manual merge if upstream touches them:

- `components/vendored/svgtypes/src/color.rs` — oklch/oklab color support
- `ports/havishell/` — entirely ours, upstream won't touch

Get full list of our modified files:

    git log --name-only havi --not origin/main --diff-filter=M --pretty=format: | sort -u

## Frequency

Monthly or when a specific upstream feature is wanted. Don't try to stay fully current — pick what's useful.
