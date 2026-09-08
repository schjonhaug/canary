# Production deployment gate

The production deployment contract is an exact merge commit on `master` whose
security and functional checks have completed successfully. The required checks
are the backend/frontend build, the pull-request dependency scan, the full OSV
scan on the merged `master` push, the secret scan, and the deployable backend
container scan. The workflow now runs the full dependency scan on `push` to
`master`, so a check exists for the exact commit that the VPS receives.

The VPS verifies these check runs through the GitHub API before changing its
checkout or containers. It rejects missing, pending, failed, unavailable or
different-SHA results. Repository rules should require the matching checks and
review approvals before `master` accepts a change; verify those settings
separately because workflow files cannot prove them.

The webhook signature authenticates the event, while the exact SHA and check
gate authenticate the source revision and its evidence. Manual updates and
rollback use the same full-SHA path. Emergency operations remain subject to the
operator audit process and must record actor, reason, scope, approval, result,
and the exact deployed commit without putting credentials or customer data in
the record.

Staging validation must exercise absent, pending, failed, mismatched and
successful check sets, a branch advance after delivery, token/API failure, and
manual tag/force attempts. Production closure requires sanitized proof of the
deployed SHA and corresponding successful checks plus a tested rollback.
