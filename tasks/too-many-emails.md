when we send too many emails for notifications in succession we get an error from Resend:


empt(s)); changes=true, new_transactions=1, confirmations=0, conflicts_marked=0
backend-1  | ❌ Failed to send email to andreas@schjonhaug.no: Resend API error: Too many requests. Limit is Some(2) per Some(1) seconds.
backend-1  | 🔔 Notified 4 contacts for Multisig: ✅ Sent 0.08231752 BTC (3×email, 2×twilio)




figure out how to deal with this