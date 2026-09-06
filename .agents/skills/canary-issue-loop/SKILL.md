---
name: canary-issue-loop
description: Implement Canary GitHub issues through PR readiness, including CI and bot reviews. Never merge.
---

# Canary Issue Loop

Use for “Canary issue 425” or “Work on Canary issue #425”. Resolve the checkout from this skill's repository or the user's explicit checkout, and the GitHub repository from its remote. Read applicable `AGENTS.md` files, the issue, relevant CI configuration, and affected code.

## Scope and worktree

Infer routine details from repository evidence and preserve accepted decisions and user steering. Ask only when missing input materially changes scope, correctness, or authorization; continue independent work while waiting. Planning-only requests end with a grounded plan. Implementation requests proceed through this workflow without additional plan acceptance, mode switching, or context clearing unless the user chose those steps. Respect runtime permissions; cite the exact file and clause if an instruction blocks progress.

Create a feature/fix branch in an isolated worktree from freshly fetched `origin/master`, unless the user specifies another base or asks for the current checkout. Verify and reuse the existing issue worktree for continuations. Preserve unrelated changes and local-only commits; do not switch, pull, reset, or rebase the user's default checkout to prepare the task. Never use `canary-next-version` in the main application.

Local instruction/configuration maintenance may stay in the named checkout when target files have no unrelated edits; state that choice first. Before removing a worktree, inspect its status and local-only commits. Remove only task-owned disposable checkouts and do not delete branches or discard user work without authorization.

## Implement and verify

Define acceptance criteria and select checks for the affected area. Implement scoped changes and add tests when they demonstrate meaningful behavior or prevent a regression.

Use [checks.sh](../../../.agent-loop/checks.sh) with `backend` or `frontend` for isolated changes and `ci` for cross-cutting changes; `quick` is a compatibility alias for `ci`. For instruction/prose-only changes, validate affected files and links. Run application checks when runtime behavior, build inputs, or executable examples are affected. Use system/upgrade modes when relevant and authorized, accounting for service, database, or volume mutations.

Run narrow checks while iterating and selected final gates on the final relevant state. Reuse passing results until subsequent changes or new evidence invalidate them. Preserve required CI gates and the repository's custom check adapter.

Delegate bounded independent investigation or review when permitted and useful, with clear outputs and ownership boundaries. Keep integration with the parent and isolate mutable resources.

## PR and review

Open or update the PR when locally ready. Monitor required CI and configured Codex, Claude, and Gemini reviews for the current PR head. Fix relevant findings, rerun affected checks, and push follow-ups.

Classify failures before editing code. Assertion failures and applicable review findings are actionable; quota, runner, authentication, and network failures require diagnosis or bounded retries, not speculative code changes. A failed or unavailable review job is not an approval.

## Resume state

Track live state in gitignored `.agent-loop/state.md`, using [state.example.md](../../../.agent-loop/state.example.md). Record accepted scope and steering, worktree/base/branch, HEAD and PR, checks and the state they verified, pending jobs/reviews for the PR head, failures and attempts by cause, and the next action. Verify saved state against the repository when resuming.

Use persistent goal tooling only on an explicit user request and when the runtime permits it. Otherwise record the objective and stop condition in loop state.

## Finish

Never merge. Finish when required CI is green, relevant feedback for the current PR head is handled or explained, and the PR is ready for human final review. Report unresolved blockers truthfully. After three unsuccessful fixes for the same failure cause, record the attempts and concrete blocker and hand back.

Stop before unauthorized destructive actions, credential changes, production deployments, billing mutations, or cross-repository releases; finish already-authorized investigation and preparation first. Preserve repository release boundaries.

For “local only”, “PR-ready only”, or “do not open a PR”, finish before PR creation with the branch, changes, checks, remaining work, and proposed PR title/body. Include a commit hash only if a commit exists.
