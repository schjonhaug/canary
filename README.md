# Canary

Canary is a Bitcoin wallet management service built in Rust that provides REST API endpoints for creating and managing Bitcoin wallets using BDK (Bitcoin Development Kit).

## Future Improvements

### Additional Transaction Pattern Detection

Currently, Canary detects and classifies the following transaction patterns:
- 🔄 **Consolidation** - Combining multiple UTXOs into one
- 📤 **RBF (Replace-By-Fee)** - Fee bumping existing unconfirmed transactions  
- 🚀 **CPFP (Child-Pays-For-Parent)** - Spending unconfirmed outputs to boost parent transaction fees
