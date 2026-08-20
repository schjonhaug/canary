# Canary Agent Notes

## Canary issue loop

For ordinary Canary GitHub issues, the useful default is a full PR-readiness
loop, not a local-only loop:

1. create a normal feature/fix branch from `master`
2. implement the issue
3. run the relevant `.agent-loop/checks.sh` gates
4. open the GitHub PR
5. watch CI and the Codex, Claude, and Gemini PR review bots
6. fix review feedback that is relevant to the PR
7. push follow-up commits until CI is green and relevant bot feedback is handled

Never merge the PR. Stop when the PR is ready for human final review.

If the user explicitly asks for "local only", "PR-ready only", or "do not open
a PR", stop before creating the PR and provide the proposed PR title/body
instead.

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
