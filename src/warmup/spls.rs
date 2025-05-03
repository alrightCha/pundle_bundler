use std::str::FromStr;

use solana_sdk::pubkey::Pubkey;

const TRUMP: &str = "6p6xgHyF7AeE6TZkSmFsko444wqoP15icUSqi2jfGiPN"; //1
const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"; //2
pub const JUP: &str = "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN"; //3
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
const JITOSOL: &str = "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn"; //23
const MSOL: &str = "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So"; //24
const WBTC: &str = "3NZ9JMVBmGAqocybic2c7LQCJScmgsAZ6vQqTDzcqmJh"; //25
const EURC: &str = "HzwqbKZw8HxMN6bF2yFZNrht3c2iXXzpKcFu7uBEDKtr"; //26
const ZBTC: &str = "zBTCug3er3tLyffELcvDNrKkCymbPWysGcWihESYfLg"; //27

const AG1: &str = "BQ72nSv9f3PRyRKCBnHLVrerrv37CYTHm5h3s9VSGQDV";
const AG2: &str = "2MFoS3MPtvyQ4Wh4M9pdfPjz6UhVoNbFbGJAskCPCj3h";
const AG3: &str = "HU23r7UoZbqTUuh3vA7emAGztFtqwTeVips789vqxxBw";
const AG4: &str = "3CgvbiM3op4vjrrjH2zcrQUwsqh5veNVRjFCB9N6sRoD";
const AG5: &str = "6LXutJvKUw8Q5ue2gCgKHQdAN4suWW8awzFVC6XCguFx";
const AG6: &str = "CapuXNQoDviLvU1PxFiizLgPNQCxrsag1uMeyk6zLVps";
const AG7: &str = "GGztQqQ6pCPaJQnNpXBgELr5cs3WwDakRbh1iEMzjgSJ";
const AG8: &str = "9nnLbotNTcUhvbrsA6Mdkx45Sm82G35zo28AqUvjExn8";
const AG9: &str = "3LoAYHuSd7Gh8d7RTFnhvYtiTiefdZ5ByamU42vkzd76";
const AG10: &str = "DSN3j1ykL3obAVNv7ZX49VsFCPe4LqzxHnmtLiPwY6xg";
const AG11: &str = "69yhtoJR4JYPPABZcSNkzuqbaFbwHsCkja1sP1Q2aVT5";
const AG12: &str = "6U91aKa8pmMxkJwBCfPTmUEfZi6dHe7DcFq2ALvB2tbB";
const AG13: &str = "7iWnBRRhBCiNXXPhqiGzvvBkKrvFSWqqmxRyu9VyYBxE";
const AG14: &str = "4xDsmeTWPNjgSVSS1VTfzFq3iHZhp77ffPkAmkZkdu71";
const AG15: &str = "GP8StUXNYSZjPikyRsvkTbvRV1GBxMErb59cpeCJnDf1";
const AG16: &str = "HFqp6ErWHY6Uzhj8rFyjYuDya2mXUpYEk8VW75K9PSiY";

pub fn get_aggregators() -> Vec<Pubkey> {
    let ag_1 = Pubkey::from_str(AG1).unwrap();
    let ag_2 = Pubkey::from_str(AG2).unwrap();
    let ag_3 = Pubkey::from_str(AG3).unwrap();
    let ag_4 = Pubkey::from_str(AG4).unwrap();
    let ag_5 = Pubkey::from_str(AG5).unwrap();
    let ag_6 = Pubkey::from_str(AG6).unwrap();
    let ag_7 = Pubkey::from_str(AG7).unwrap();
    let ag_8 = Pubkey::from_str(AG8).unwrap();
    let ag_9 = Pubkey::from_str(AG9).unwrap();
    let ag_10 = Pubkey::from_str(AG10).unwrap();
    let ag_11 = Pubkey::from_str(AG11).unwrap();
    let ag_12 = Pubkey::from_str(AG12).unwrap();
    let ag_13 = Pubkey::from_str(AG13).unwrap();
    let ag_14 = Pubkey::from_str(AG14).unwrap();
    let ag_15 = Pubkey::from_str(AG15).unwrap();
    let ag_16 = Pubkey::from_str(AG16).unwrap();

    vec![
        ag_1, ag_2, ag_3, ag_4, ag_5, ag_6, ag_7, ag_8, ag_9, ag_10, ag_11, ag_12, ag_13, ag_14,
        ag_15, ag_16,
    ]
}

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
    let jitosol: Pubkey = Pubkey::from_str(JITOSOL).unwrap();
    let msol: Pubkey = Pubkey::from_str(MSOL).unwrap();
    let wbtc: Pubkey = Pubkey::from_str(WBTC).unwrap();
    let eurc: Pubkey = Pubkey::from_str(EURC).unwrap();
    let zbtc: Pubkey = Pubkey::from_str(ZBTC).unwrap();

    vec![
        trump, usdc, jup, popcat, fartcoin, wif, pnut, fwog, giga, goat, griffain, michi, moodeng,
        alch, ray, ufd, retardio, butthole, harambe, sigma, pwease, ai16z, jitosol, msol, wbtc,
        eurc, zbtc,
    ]
}
