use std::str::FromStr;

use solana_sdk::pubkey::Pubkey;

const HOUSE: &str = "DitHyRMQiSDhn5cnKMJV2CDDt6sVct96YrECiM49pump"; //1
const GORK: &str = "38PgzpJYu2HkiYvV8qePFakB8tuobPdGm2FFEn7Dpump"; //2
const TROLL: &str = "5UUH9RTDiSpq6HKS6bp4NdU9PNJpXRXuiw6ShBTBhgH2"; //3
const RFC: &str = "C3DwDjT17gDvvCYC2nsdGHxDHVmQRdhKfpAdqQ29pump"; //4
const DARK: &str = "8BtoThi2ZoXnF7QQK1Wjmh2JuBw9FjVvhnGMVZ2vpump"; //5
const TRENCHER: &str = "8ncucXv6U6epZKHPbgaEBcEK399TpHGKCquSt4RnmX4f"; //6
const NEET: &str = "Ce2gx9KGXJ6C9Mp5b5x1sn9Mg87JwEbrQby4Zqo3pump"; //7
const GHIBLI: &str = "4TBi66vi32S7J8X1A6eWfaLHYmUXu7CStcEmsJQdpump"; //8
const MOONPIG: &str = "Ai3eKAWjzKMV8wRwd41nVP83yqfbAVJykhvJVPxspump"; //9
const PUDGY: &str = "2zMMhcVQEXDtdE6vsFS7S7D5oUodfJHE8vd1gnBouauv"; //10
const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"; //11
const CBBTC: &str = "cbbtcf3aa214zXHbiAZQwf4122FBYbraNdFqgw4iMij"; //12
const UFD: &str = "eL5fUxj2J4CiQsmW85k5FG9DvuQjjUoBHoQBi2Kpump"; //13
const RETARDIO: &str = "6ogzHhzdrQr9Pgv6hZ2MNze7UrzBMAFyBBWUYp1Fhitx"; //14

pub fn init_mints() -> Vec<Pubkey> {
    let usdc: Pubkey = Pubkey::from_str(USDC).unwrap();
    let house: Pubkey = Pubkey::from_str(HOUSE).unwrap();
    let grok: Pubkey = Pubkey::from_str(GORK).unwrap();
    let troll: Pubkey = Pubkey::from_str(TROLL).unwrap();
    let rfc: Pubkey = Pubkey::from_str(RFC).unwrap();
    let dark: Pubkey = Pubkey::from_str(DARK).unwrap();
    let trencher: Pubkey = Pubkey::from_str(TRENCHER).unwrap();
    let neet: Pubkey = Pubkey::from_str(NEET).unwrap();
    let ghibli: Pubkey = Pubkey::from_str(GHIBLI).unwrap();
    let moonpig: Pubkey = Pubkey::from_str(MOONPIG).unwrap();
    let pudgy: Pubkey = Pubkey::from_str(PUDGY).unwrap();
    let cbbtc: Pubkey = Pubkey::from_str(CBBTC).unwrap();
    let ufd: Pubkey = Pubkey::from_str(UFD).unwrap();
    let retardio: Pubkey = Pubkey::from_str(RETARDIO).unwrap();

    vec![
        usdc, house, grok, troll, rfc, dark, trencher, neet, ghibli, moonpig, pudgy, cbbtc, ufd,
        retardio,
    ]
}
