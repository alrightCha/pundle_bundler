use std::str::FromStr;

use solana_sdk::pubkey::Pubkey;

pub const TRUMP: &str = "6p6xgHyF7AeE6TZkSmFsko444wqoP15icUSqi2jfGiPN"; //1

const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"; //2
const JUP: &str = "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN"; //3
const POPCAT: &str = "7GCihgDB8fe6KNjn2MYtkzZcRjQy3t9GHdC8uHYmW2hr"; //4
const FARTCOIN: &str = "9BB6NFEcjBCtnNLFko2FqVQBq8HHM13kCyYcdQbgpump"; //5
const WIF: &str = "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm"; //6
const PNUT: &str = "2qEHjDLDLbuBgRYvsxhc5D6uDWAivNFZGan56P1tpump"; //7
const FWOG: &str = "A8C3xuqscfmyLrte3VmTqrAq8kgMASius9AFNANwpump"; //8
const GIGA: &str = "63LfDmNb3MQ8mw9MtZ2To9bEA2M71kZUUGq5tiJxcqj9"; //9
const GOAT: &str = "CzLSujWBLFsSjncfkh59rUFqvafWcY5tzedWJSuypump"; //10
const GRIFFAIN: &str = "KENJSUYLASHUMfHyy5o4Hp2FdNqZg1AsUPhfH2kYvEP"; //11
const MICHI: &str = "5mbK36SZ7J19An8jFochhQS4of8g6BwUjbeCSxBSoWdp"; //12
const MOODENG: &str = "ED5nyyWEzpPPiWimP8vYm7sD7TD3LAt3Q3gRTWHzPJBY"; //13
const ALCH: &str = "HNg5PYJmtqcmzXrv6S9zP1CDKk5BgDuyFBxbvNApump"; //14
const RAY: &str = "4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R"; //15
const UFD: &str = "eL5fUxj2J4CiQsmW85k5FG9DvuQjjUoBHoQBi2Kpump"; //16
const RETARDIO: &str = "6ogzHhzdrQr9Pgv6hZ2MNze7UrzBMAFyBBWUYp1Fhitx"; //17
const BUTTHOLE: &str = "CboMcTUYUcy9E6B3yGdFn6aEsGUnYV6yWeoeukw6pump"; //18
const HARAMBE: &str = "Fch1oixTPri8zxBnmdCEADoJW2toyFHxqDZacQkwdvSP"; //19
const SIGMA: &str = "5SVG3T9CNQsm2kEwzbRq6hASqh1oGfjqTtLXYUibpump"; //20
const PWEASE: &str = "CniPCE4b3s8gSUPhUiyMjXnytrEqUrMfSsnbBjLCpump"; //21
const AI16Z: &str = "HeLp6NuQkmYB4pYWo2zYs22mESHXPQYzXbB8n4V98jwC"; //22

pub fn init_mints() -> Vec<Pubkey> {
    let usdc: Pubkey = Pubkey::from_str(USDC).unwrap();
    let trump: Pubkey = Pubkey::from_str(TRUMP).unwrap();
    let jup: Pubkey = Pubkey::from_str(JUP).unwrap();
    let popcat: Pubkey = Pubkey::from_str(POPCAT).unwrap();
    let fartcoin: Pubkey = Pubkey::from_str(FARTCOIN).unwrap();
    let wif: Pubkey = Pubkey::from_str(WIF).unwrap();
    let pnut: Pubkey = Pubkey::from_str(PNUT).unwrap();
    let fwog: Pubkey = Pubkey::from_str(FWOG).unwrap();
    let giga: Pubkey = Pubkey::from_str(GIGA).unwrap();
    let goat: Pubkey = Pubkey::from_str(GOAT).unwrap();
    let griffain: Pubkey = Pubkey::from_str(GRIFFAIN).unwrap();
    let michi: Pubkey = Pubkey::from_str(MICHI).unwrap();
    let moodeng: Pubkey = Pubkey::from_str(MOODENG).unwrap();
    let alch: Pubkey = Pubkey::from_str(ALCH).unwrap();
    let ray: Pubkey = Pubkey::from_str(RAY).unwrap();
    let ufd: Pubkey = Pubkey::from_str(UFD).unwrap();
    let retardio: Pubkey = Pubkey::from_str(RETARDIO).unwrap();
    let butthole: Pubkey = Pubkey::from_str(BUTTHOLE).unwrap();
    let harambe: Pubkey = Pubkey::from_str(HARAMBE).unwrap();
    let sigma: Pubkey = Pubkey::from_str(SIGMA).unwrap();
    let pwease: Pubkey = Pubkey::from_str(PWEASE).unwrap();
    let ai16z: Pubkey = Pubkey::from_str(AI16Z).unwrap();

    vec![trump, usdc , jup, popcat, fartcoin, wif, pnut, fwog, giga, goat, griffain, michi, moodeng, alch, ray, ufd, retardio, butthole, harambe, sigma, pwease, ai16z]
}
