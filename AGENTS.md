# Canary Agent Notes

## Node distro packaging work

Keep Umbrel, StartOS, myNode, and similar node distro packaging changes batched
on their shared feature/release branches until the full feature set for the next
Canary release is ready.

Do not open node distro packaging PRs early. Commit and push related packaging
changes to the relevant branch first, then create the upstream PRs when the
release is ready. If a PR is useful for visibility while work is still ongoing,
create it as a draft and continue pushing related changes to the same branch/PR.
