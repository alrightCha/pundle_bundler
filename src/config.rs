use std::path::PathBuf;
use axum_server::tls_rustls::RustlsConfig;
use std::env;

pub const RPC_URL: &str = "https://mainnet.helius-rpc.com/?api-key=9291504f-fedf-4c67-8335-1a415862badf";
pub const ORCHESTRATOR_URL: &str = "http://localhost:3001/bundled";
pub const ADMIN_PUBKEY: &str = "FDB2pWkG8CXwVop6xi8rw8Np8HN1DFV1KMBuJhGpSFaH";
pub const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
pub const JITO_TIP_AMOUNT: u64 = 300_000;
pub const SOLANA_TIP: u64 = 5_000_000;
pub const MAX_RETRIES: u32 = 25;
pub const FEE_AMOUNT: u64 = 10_000;
pub const BUFFER_AMOUNT: u64 = 2_000_000;
pub const TOKEN_AMOUNT_MULTIPLIER: u64 = 1_000_000;
pub const PORT: u16 = 3000;
pub const JITO_TIP_SIZE: usize = 80;
pub const MAX_TX_PER_BUNDLE: usize = 5;

pub async fn setup_https_config() -> RustlsConfig {
    let cert_dir = env::var("CERTIFICATE_DIR")
        .expect("Certificate directory not defined");
    let cert_path = PathBuf::from(&cert_dir).join("fullchain.pem");
    let key_path = PathBuf::from(&cert_dir).join("privkey.pem");

    RustlsConfig::from_pem_file(cert_path, key_path)
        .await
        .expect("Failed to configure HTTPS")
}
