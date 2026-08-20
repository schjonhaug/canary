# Canary Issue PR Loop Prompt

Use this prompt for ordinary Canary GitHub issues when the desired outcome is a
PR that is green and ready for human final review.

```text
Use the agent-loop-engineering skill.

/goal Work on GitHub issue #ISSUE_NUMBER in Canary through PR review.

Follow AGENTS.md, CLAUDE.md, backend/CLAUDE.md, and frontend/CLAUDE.md as
relevant.

Create a normal feature/fix branch from master.
Do not use canary-next-version in the main Canary repo.

Implement the issue, run the relevant narrow checks, then run
.agent-loop/checks.sh quick before opening the PR.

Open the GitHub PR when the branch is locally ready.
After opening the PR, monitor CI and the Codex, Claude, and Gemini bot reviews.
Read all review comments.
Fix feedback that is relevant to this PR.
Push follow-up commits as needed.
Rerun relevant checks after fixes.

Never merge.
Stop when CI is green, relevant bot feedback is handled or clearly explained as
not applicable, and the PR is ready for my final review.
```

For local-only work, add:

```text
Do not open a PR. Stop with branch name, commit hash, checks run, remaining
risks, and proposed PR title/body.
```
