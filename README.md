# Canary

<img src="frontend/public/images/canary.svg" alt="Canary Logo" width="100" height="86">

Canary is a **Bitcoin monitoring and early warning system** built in [Rust](https://www.rust-lang.org/) using [BDK (Bitcoin Development Kit)](https://bitcoindevkit.org/) with a [Next.js](https://nextjs.org/) frontend. It provides real-time transaction intelligence, advanced pattern recognition (RBF, CPFP, consolidation), and instant multilingual notifications via [ntfy.sh](https://ntfy.sh) for Bitcoin wallet activity - designed specifically for monitoring cold storage and Bitcoin holdings you don't actively use.

## Why Use Canary?

**A canary in the cold mine** - When your bitcoins are in cold storage, you seldom check on them. Canary acts as an early warning system that alerts you the moment your coins move, giving you immediate notification of any activity on your wallets.

**Real-time notifications in Norwegian and English for all Bitcoin transactions via ntfy.sh:**
- 📤 Sending bitcoins
- ✅ Transaction sent and confirmed  
- 📥 Receiving bitcoins
- ✅ Transaction received and confirmed
- 📤 **RBF (Replace-By-Fee)** detection - fee bumping notifications
- 🚀 **CPFP (Child-Pays-For-Parent)** detection - transaction acceleration notifications