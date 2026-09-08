# Identity-to-wallet privacy assessment

This is the design assessment requested by private security issue #10. It uses
synthetic workflows and repository structure only. It does not recommend an
encryption design before the threat and recovery constraints are understood.

## Data-flow inventory

| Workflow | Association required | Minimum fields | Retention decision |
| --- | --- | --- | --- |
| Wallet monitoring | account → wallet ownership → descriptor | opaque user ID, wallet ID/checksum, descriptor, sync state | retain while the wallet is active; delete ownership and descriptor on removal |
| Billing | account → billing provider customer | opaque user ID and provider customer reference | retain only while billing, refunds, or statutory records require it; keep payment details with the provider |
| Notifications | wallet → contact method | wallet ID, provider, destination, verification state | retain while enabled; delete destination and verification history on removal subject to abuse/audit retention |
| Support | authorized operator → customer-scoped wallet | opaque IDs, scope, reason, expiry, result | retain the access event; do not copy descriptors or contact values into the audit record |

Email, provider customer IDs, descriptors, balances, activity timestamps and
notification destinations are separate disclosure classes. A UUID is not
anonymous if a stolen database contains the mapping table, so pseudonymization
alone does not solve a database-theft scenario.

## Threat comparison

An offline database or backup thief can join the user, billing, wallet and
notification tables and infer identity-linked financial activity. An application
attacker can query the same associations through the running service and may
also obtain runtime keys. A privileged operator can bypass application checks
unless least-privilege support access and audit review are enforced. Filesystem
encryption or an encrypted database protects a lost disk only when its key is
not available to the stolen copy; a key mounted beside the database does not
protect against full-host compromise.

## Options and tradeoffs

1. Minimize first: use opaque internal IDs in application tables and logs,
   separate billing references from wallet records, remove unnecessary contact
   history, and apply deletion/retention schedules. This reduces linkability but
   monitoring still needs a current ownership relation.
2. Separate stores and privileges: isolate identity/billing, wallet metadata,
   and notification destinations with narrow service interfaces. This limits a
   single read path but adds operational complexity and does not protect a
   fully compromised application process.
3. Field-level encryption: encrypt descriptors, destinations and provider IDs
   with a service key held by a separate secret manager. Use independent key
   scopes, rotation versions, staged re-encryption, backup recovery ceremony,
   and a break-glass audit path. This protects offline database copies when the
   key is absent, but the running API still needs plaintext and recovery becomes
   more complex.
4. Database/storage encryption: use encrypted volumes and encrypted off-host
   backups with keys held separately. This is valuable for lost media and is
   simpler than per-field encryption, but it does not address a live database
   read or a host compromise.

The selected direction is staged minimization and separation first, followed by
an architecture decision on field-level encryption for the highest-linkability
fields. The decision must name a key owner, manager, rotation interval,
recovery test, migration plan and residual risks before implementation. The
backup key must not be stored with the data or in the same VPS checkout.

## Synthetic stolen-copy exercise

Create three synthetic users with distinct emails, billing references,
descriptors and notification destinations. Export copies at each stage:

* current schema: measure which joins identify a customer and wallet;
* minimized schema: remove inactive contact and billing links and repeat;
* encrypted-field prototype: remove the field key and repeat, then restore the
  key in a controlled recovery test.

The expected result is that monitoring retains an opaque current ownership edge,
while an offline copy without field keys cannot recover descriptors or contact
destinations. A running service test must demonstrate that the API can still
perform monitoring only through the approved service identity. Record the
remaining balance/activity inference and operator risks; do not use customer
records for the exercise.

## Decision and follow-up criteria

Before implementation, the owners must approve the data inventory, retention
periods, support scope, key custody and recovery RTO/RPO. Follow-up issues must
include synthetic migration tests, deletion verification, key rotation,
backup/restore drills and an audit query that reports access scope without
exposing descriptor, email, destination or billing values.
