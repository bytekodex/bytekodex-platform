---
name: update-deps
description: Updates every dependency of bytekodex-platform to its newest stable version, Rust, crates, and GitHub Actions, proves the library still works, and commits. Use when the owner asks to update dependencies, crates, or tools.
argument-hint: "[only these, e.g. rust logos]"
---

# Update dependencies

Bring every pinned version in this project up to the newest stable one, prove the project still works, and commit. The same file sits in every Bytekodex repository, each with its own list; this project's own facts live in its CLAUDE.md.

If the owner named only some dependencies (`$ARGUMENTS`), update just those.

## The one limit: the C ABI

`include/bytekodex.h` and `BK_ABI_VERSION` stay as they are. A dependency update never changes what a host links against. If a new crate version forces a change there, keep the old version and say so in the report: an ABI change is the owner's decision, and the bot has to follow it in the same breath.

## Steps

1. **Read CLAUDE.md.** Its “Versions” section lists where versions are pinned, its “Checking a change” section says how to prove the project works. Then run `git status`: leave other people's uncommitted changes alone and keep them out of your commits.
2. **List every pinned version:** `rust-toolchain.toml`, `rust-version` and `[workspace.dependencies]` in `Cargo.toml`, `Cargo.lock`, and `uses:` in `.github/workflows/`.
3. **Find each newest version** where `sources.md` (next to this file) says to ask. Note old and new side by side. If nothing is behind, say so and stop: no commit, no changelog, no release.
4. **Update in groups:** Rust, crates, Actions. Build and run the checks after each group, so a failure points at its cause. `cargo update` moves `Cargo.lock` within the ranges, and a new major or a new 0.x minor needs its range in `Cargo.toml` moved by hand.
5. **Majors:** read the release notes or migration guide and fix the code. If a major breaks too much to fix well, keep the old major and say so in the report.
6. **Check** as CLAUDE.md's “Checking a change” says, all of it.
7. **Changelog.** When the update reaches what the project ships (the library, `bk`, or Rust itself), add a “## X.Y.Z” section to CHANGELOG.md with the next patch version, or add a line to the topmost section if it isn't released yet (`gh release view vX.Y.Z` fails). Updates that only touch CI don't need one.
8. **Commit and push,** one commit per group, in the project's commit style: “Update Rust to 1.101.0 and logos to 0.17.0”. Release when the owner asks or when the autopilot runs you (its workflow says so).

## Running unattended

The autopilot (`.github/workflows/autopilot.yml`) runs this every Monday in GitHub Actions, with nobody to answer. Where a step says to ask the owner, take the choice that keeps the project as it works today, and name it in the report. A failure you can't fix stays out of the commit: keep the old version and go on with the rest.

## Report

A short table: each dependency, old version, new version. Then what stayed behind and why, and how the check went.
