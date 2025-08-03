# Pundle.fun — Solana Token Bundler

**Pundle.fun** is a high-performance, fully manual Solana bundling service designed to interact seamlessly with multiple dApps and core Solana functionalities. This project is built from the ground up with minimal reliance on external SDKs, providing fine-grained control and efficiency for advanced token operations.

## Features

- **Automated Token Launch & Purchase on Pump.fun**
  - Instantly launches new tokens on Pump.fun.
  - Purchases tokens using multiple generated wallets in a single, efficient bundle transaction.
  - Handles wallet generation, funding, and transaction orchestration end-to-end.

- **Multi-dApp Integration**
  - Communicates directly with Solana dApps such as:
    - **Jito** (bundling, MEV)
    - **Jupiter** (swaps, routing)
    - **Pump.fun** (token launches, purchases)
  - Implements Solana primitives manually, including token accounts, SPL tokens, and address lookup tables.

- **Advanced Algorithms**
  - **Warm-Up Algorithm:** Simulates natural wallet activity by creating realistic transaction histories, improving on-chain wallet reputation.
  - **Bump Algorithm:** Generates micro-transactions to artificially increase token volume, supporting liquidity and visibility strategies.

- **Efficient Bundling**
  - Utilizes Solana address lookup tables for highly efficient, compact transactions.
  - Optimized for speed and cost, suitable for high-frequency and competitive environments.

## How It Works

1. **Wallet Generation:** Creates and funds multiple wallets for token operations.
2. **Token Launch:** Deploys a new token on Pump.fun.
3. **Bundled Purchase:** Executes a bundled transaction to buy the token with all generated wallets, leveraging lookup tables for efficiency.
4. **Warm-Up:** Optionally runs a warm-up phase to simulate organic wallet activity.
5. **Bump:** Optionally performs micro-transactions to boost token volume.

## Project Structure

- `src/jito/` — Jito integration and utilities
- `src/jupiter/` — Jupiter swap and routing logic
- `src/pumpfun/` — Pump.fun token launch, bonding curve, and swap logic
- `src/solana/` — Core Solana utilities (token accounts, lookup tables, etc.)
- `src/warmup/` — Warm-up and token manager algorithms
- `src/handlers.rs` — Main service handlers
- `src/main.rs` — Entry point

## Getting Started

> **Note:** This project is intended for advanced users familiar with Solana, Rust, and DeFi protocols. Use responsibly and at your own risk.

### Prerequisites

- Rust (latest stable)
- Solana CLI tools
- Access to Solana RPC endpoints

### Build & Run

```bash
git clone https://github.com/yourusername/pundle.fun.git
cd pundle.fun
cargo build --release
# Configure your environment and run:
cargo run --release
```

### Configuration

Edit `src/config.rs` to set up your RPC endpoints, wallet paths, and other parameters.

## Security & Disclaimer

- This project is for educational and research purposes.
- Use at your own risk. The authors are not responsible for any financial loss or misuse.
- Always comply with the terms of service of the dApps and networks you interact with.

## Contributing

Contributions, issues, and feature requests are welcome! Please open an issue or submit a pull request.

## License

[MIT](LICENSE) — See LICENSE file for details.
