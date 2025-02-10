use std::path::PathBuf;
use axum_server::tls_rustls::RustlsConfig;
use std::env;

pub const RPC_URL: &str = "https://special-little-snowflake.solana-mainnet.quiknode.pro/74a5c79ac72ed2ff15a570cc8aabf8b33ffb368b";

pub const JITO_TIP_AMOUNT: u64 = 3_000_000;
pub const MAX_RETRIES: u32 = 30;
pub const FEE_AMOUNT: u64 = 10_000;
pub const BUFFER_AMOUNT: u64 = 100_000;

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const HTTPS_PORT: u16 = 443;
pub const CERTIFICATE_DIR_ENV_KEY: &str = "CERTIFICATE_DIR";

pub async fn setup_https_config() -> RustlsConfig {
    let cert_dir = env::var(CERTIFICATE_DIR_ENV_KEY)
        .expect("Certificate directory not defined");
    let cert_path = PathBuf::from(&cert_dir).join("fullchain.pem");
    let key_path = PathBuf::from(&cert_dir).join("privkey.pem");

    RustlsConfig::from_pem_file(cert_path, key_path)
        .await
        .expect("Failed to configure HTTPS")
}
