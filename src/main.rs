mod config;
mod handlers;
mod jito;
mod jupiter;
mod params;
mod pumpfun;
mod solana;
mod warmup;

use std::sync::Arc;
use jito::bundle::process_bundle;
use params::KeypairWithAmount;
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use tokio::sync::RwLock;
use axum::routing::{get, post};
use axum::Router;
use config::{JITO_TIP_AMOUNT, PORT};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use pumpfun_cpi::instruction::Create;

use dotenv::dotenv;
use std::env;
use std::net::Ipv4Addr;
use std::str::FromStr;

use handlers::{
    complete_bundle, get_pool_information, get_price, handle_post_bundle, health_check,
    pay_recursive, sell_all_leftover_tokens, sell_for_keypair, setup_lut_record, withdraw_all_sol,
};

use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::net::SocketAddr;

pub type SharedLut = Arc<RwLock<HashMap<String, Pubkey>>>;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    
    let dev = Keypair::from_base58_string("4TuhVvQ7fdasFXQqbB9VuPHhyk1C4mYzSTubHih8LSyKew8sNZz7WjFLB6HZ4Fp52j8PsK8EApQBawaUXaKh7cfY"); //0.3
    let kp1 = Keypair::from_base58_string("4PLmk66XBcY3U1TRbD7qQY1ECinpAyBbnoELNM9pYMS9FLooLzhTDt9QzGFP32woPqGfRULxjsJ4DfdoeaTpFtoe"); //0.129
    let kp2 = Keypair::from_base58_string("4LWUUzE8xiHhFZA4XVvE1aCZvBrATapw1p3Ke9swgbyXU2MSmEpyc2Qwsa9JenWgLdJ4TMf8oQAMrnSambrwebWV"); //0.177
    let kp3 = Keypair::from_base58_string("2wkPkB2qmuTje4qbPABngXAKf6yEPukKrSgfRCPZ7CCbDv6xkVanKhnWbqfz5UXzfgfeZizJeJxwcBV7kaktzjbS"); 
    let kp4 = Keypair::from_base58_string("WR5MFmPXuWjMVJydtKn5nAQ2aKgVavo5a6Ziu3eNVvkMUDSKvC9FMM9vXaLp1LRhPH1MrN5GKvDS1XiTeaT3NUx"); 
    let kp5 = Keypair::from_base58_string("PA5oGgrEtKEv29PtpFsqoZuT2W2kWUtDdsyQk7zVDXav2yis1TDW2wgRr7it7mDX9p5WwpJBFQLfs2ephpYT7SM"); 
    let kp6 = Keypair::from_base58_string("VmXr3hVW2DeNB7zBaJ3m8Y9UxrccSunYVMBj1cAvH53Fgnr2EYaSNbXhf8giukzN7nrsvZg1aAtQroVfNNnAZnh"); 
    let kp7 = Keypair::from_base58_string("2zj3aQ4euF3wETBy8FyNWVQf6rz4wrMqXpj4X8k61Q4V43mQrxpH7qM4DX3TAwmk4FYqSTrjVnsrhawJ8P8G9Fkn"); 
    let kp8 = Keypair::from_base58_string("4Mx8o64TFtsB3mRAVdZ961b5ynikEG9CqjxEG4hVu6GBSmoYhCsU18zJRsmxRpnRwKNToFLvh6vCRPycTYHfvx8S"); 
    let kp9 = Keypair::from_base58_string("2Zht4nuHjdbyBwuBMFmycuseAbqCc7jVsW2GH37GyXGXoFFde3TTBRoXkwD3MyEyfCrq3bhLKaHg3M5mRGdi66hA"); 
    let kp10 = Keypair::from_base58_string("5ZGJ1jjKUinaXT49iTq4CyCLDB5LYdnqBD61KzHZ1YoXuM9BdEiYBRvGRpbzqSzWYmDvaGVZXsD9gTNCX98CVQLg"); 
    let kp11 = Keypair::from_base58_string("3NbDe96po525pD3CtXtpN8Dp7yLW78ouAAs1aJy49U21UMAXSGxnKNMrnEJ3Gwb8AiNoUmCwasAFkusVp8ZrSSYo"); 
    let kp12 = Keypair::from_base58_string("ARxo43TQpTLgMKUyEx8KwnDNASQ8BmDgricSrCXqa5q5szr8VDSS2vtubJ5ALSPXp3rbHTtC4Xbs2xFNcHDkoxo"); 
    let kp13 = Keypair::from_base58_string("5ZviTfjexhHCC6WWABGdHbbvYnTPJ2hmrq15dHVEfT64JH7jy6jnzDrfTqA2nhTT2X6iwZwUtquqxHedXHRXB19p"); 
    
    
    
    
    
    

    let dev_with_amount = KeypairWithAmount {
        keypair: dev, 
        amount: 300000000
    }; 

    let one = KeypairWithAmount {
        keypair: kp1, 
        amount: 270000000
    }; 

    let two = KeypairWithAmount {
        keypair: kp2, 
        amount: 280000000
    }; 

    let three = KeypairWithAmount {
        keypair: kp3, 
        amount: 280000000
    }; 

    let four = KeypairWithAmount {
        keypair: kp4, 
        amount: 280000000
    }; 

    let five = KeypairWithAmount {
        keypair: kp5, 
        amount: 260000000
    }; 

    let six = KeypairWithAmount {
        keypair: kp6, 
        amount: 260000000
    }; 

    let seven = KeypairWithAmount {
        keypair: kp7, 
        amount: 244000000
    }; 

    let eight = KeypairWithAmount {
        keypair: kp8, 
        amount: 280000000
    }; 

    let nine = KeypairWithAmount {
        keypair: kp9, 
        amount: 250000000
    }; 

    let ten = KeypairWithAmount {
        keypair: kp10, 
        amount: 240000000
    }; 

    let eleven = KeypairWithAmount {
        keypair: kp11, 
        amount: 290000000
    }; 

    let twelve = KeypairWithAmount {
        keypair: kp12, 
        amount: 270000000
    }; 

    let thirteen = KeypairWithAmount {
        keypair: kp13, 
        amount: 290000000
    }; 

    let with_amounts: Vec<KeypairWithAmount> = vec![one, two, three, four, five, six, seven, eight, nine, ten, eleven, twelve, thirteen]; 

    let mint = Keypair::from_base58_string("26shAYVukAh9ruKmff8DsC2ZpMQjuepRagHt6QrwVJfZNQ3j6mbXyMiVZ5XQfpitPxf27sevL82VQ1Mospsvb6jL"); 

    let priority_fee = 700000; 
    
    let token = Create {
        _name: "x".to_string(), 
        _symbol: "x".to_string(), 
        _uri: "https://ipfs.io/ipfs/QmRKmENhmN8Lq5GT4A3XcbJHM3Yzjiks4KBp5fyxgNqspY".to_string(), 
        _creator: dev_with_amount.keypair.pubkey()

    }; 

    let bundle = process_bundle(with_amounts, dev_with_amount, &mint, token, priority_fee, JITO_TIP_AMOUNT, false).await; 

    Ok(())
}