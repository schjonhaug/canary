# Cloud administrator MFA

Cloud administrator sessions require a six-digit TOTP code in addition to the
password. This is an interim second factor, not a phishing-resistant factor.
Use a device-bound passkey or hardware security key for the operator account
and keep the TOTP seed in a separate authenticator until WebAuthn enrollment is
available.

## Controlled enrollment and recovery

The application never offers enrollment, reset, or recovery through the public
API. A host operator performs those steps over a verified out-of-band channel.
The factors file is a root-owned runtime secret, separate from the SQLite data
directory and backups. It contains a JSON object mapping the immutable Canary
user UUID to one base32 TOTP seed. It must be mode `0600` or `0640`, not a
symlink, and must be mounted at `/run/secrets/canary-admin-mfa.json` (or named
through `CANARY_ADMIN_MFA_SECRETS_FILE`). Mode `0640` is for a root-owned file
that the backend group can read. World access is never allowed.

For enrollment, identify the user UUID from an authenticated administrative
record, generate a fresh 160-bit-or-greater seed on a trusted operator system,
and transmit the enrollment QR/seed only through the verified channel. Do not
put it in chat, tickets, shell history, Git, logs, the application database, or
backups. Install the file atomically with root ownership and mode `0600` or
`0640`, then
have the administrator sign in with the new code. Successful MFA sign-in is
audited without the seed or code.

For recovery or suspected loss, first verify the operator's identity and reason.
Revoke the account sessions, replace the seed through the same controlled
process, verify a new code, and record the actor, scope, reason, and result in
the operational audit system. Removing or rotating a seed immediately invalidates
existing cloud administrator sessions. A password reset must never bypass MFA.

## Runtime behavior and validation

The code accepts a single use of a current or adjacent 30-second TOTP step and
records the consumed step transactionally, so a replay cannot create a second
session. Cloud administrator sessions expire for privileged use after 15 minutes;
the next request requires a fresh password-and-code sign-in. Self-hosted
administrator authentication remains unchanged.

## Customer support access

Cloud administrators do not have personal wallets and cannot list every
customer. After MFA sign-in they land on support access, look up one customer
email with a written reason, and receive a 15-minute read-only view of that
account. The grant is recorded in the administrator audit log with the target
user id and reason, not descriptors or contact destinations. Writes stay
owner-only. Opening another customer replaces the previous grant. A new
password-and-code sign-in clears any previous grant, and the grant ends when
the MFA session ends.

Before production activation, use only synthetic accounts to verify missing,
incorrect, expired, replayed, rotated, and removed factors; a valid factor;
expired admin sessions; and recovery/session revocation. Review `/api` responses
and the audit record without copying the seed or code. Keep the file outside the
VPS backup input and test that a restored backup cannot recreate MFA enrollment.

This control limits a stolen password or cookie. It does not protect a fully
compromised running host that can read the runtime secret, nor phishing against
TOTP. Track passkey/WebAuthn enrollment as the replacement for that residual
risk.
