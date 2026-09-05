# Canary Agent Notes

## Canary issue loop

For Canary issue implementation, use the maintained
[Canary issue loop skill](.agents/skills/canary-issue-loop/SKILL.md). It defines
worktree handling, scoped verification, PR creation, current-head CI and bot
reviews, resume state, and stop conditions. Never merge; the final step is human
review. Respect planning-only and local-only requests.

For local instruction/configuration maintenance, the skill permits the named
checkout when target files have no unrelated edits; state that choice first.

## Node distro packaging work

Keep Umbrel, StartOS, myNode, and similar node distro packaging changes batched
on the shared unpublished-work branch `canary-next-version` until the full
feature set for the next Canary release is ready.

Do not open node distro packaging PRs early. Commit and push related packaging
changes to the relevant branch first, then create the upstream PRs when the
release is ready. If a PR is useful for visibility while work is still ongoing,
create it as a draft and continue pushing related changes to the same branch/PR.

For MyNode specifically, do not open an upstream `mynodebtc/mynode` PR for
partial Canary packaging work. Push changes only to the fork branch
`schjonhaug/mynode:canary-next-version` until the full node-distro batch is
ready and validated, matching the Umbrel and StartOS workflow.

Do not keep using already released version branches like `canary-version-1.5.0`
for new unpublished packaging work.

The branch name `canary-next-version` is only for downstream packaging repos
such as Umbrel, StartOS, and MyNode. Do not use `canary-next-version` as a
working branch in the main Canary app repo; use a normal feature/fix branch
there and merge through the usual Canary PR flow.

## Project reference

Consult the relevant sections of [agent-reference.md](agent-reference.md) when the task needs architecture context, development commands, or domain constraints. These references supplement the applicable AGENTS.md workflow rules.
