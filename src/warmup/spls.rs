use std::str::FromStr;

use solana_sdk::pubkey::Pubkey;

const HOUSE: &str = "DitHyRMQiSDhn5cnKMJV2CDDt6sVct96YrECiM49pump"; //1
const GORK: &str = "38PgzpJYu2HkiYvV8qePFakB8tuobPdGm2FFEn7Dpump"; //2
const WEED: &str = "21nnfR4TkbZNLwvRrqEseAbz7P3kxKjaV7KuboLJpump"; //3
const RFC: &str = "C3DwDjT17gDvvCYC2nsdGHxDHVmQRdhKfpAdqQ29pump"; //4
const DARK: &str = "8BtoThi2ZoXnF7QQK1Wjmh2JuBw9FjVvhnGMVZ2vpump"; //5
const TRENCHER: &str = "8ncucXv6U6epZKHPbgaEBcEK399TpHGKCquSt4RnmX4f"; //6
const NEET: &str = "Ce2gx9KGXJ6C9Mp5b5x1sn9Mg87JwEbrQby4Zqo3pump"; //7
const GHIBLI: &str = "4TBi66vi32S7J8X1A6eWfaLHYmUXu7CStcEmsJQdpump"; //8
const MOONPIG: &str = "Ai3eKAWjzKMV8wRwd41nVP83yqfbAVJykhvJVPxspump"; //9
const CYCLE: &str = "HJ2n2a3YK1LTBCRbS932cTtmXw4puhgG8Jb2WcpEpump"; //10
const DEEPCORE: &str = "3qVpCnqdaJtARzE2dYuCy5pm8X2NgF5hx9q9GosPpump"; //11
const WAGMI: &str = "GnM6XZ7DN9KSPW2ZVMNqCggsxjnxHMGb2t4kiWrUpump"; //12
const PUMPSWAP: &str = "G2AbNxcyXV6QiXptMm6MuQPBDJYp9AVQHTdWAV1Wpump"; //13
const FOG: &str = "6bdTRHhdZenJQYLTxaYc8kH74GBNP9DoGhPnCjfypump"; //13.5
const HOTMOM: &str = "H4SFaUnxrZRoFnhBeotZnuqw4mfVtJ2nCvGrPmQupump"; //14
const VITAFIN: &str = "83mCRQJzvKMeQd9wJbZDUCTPgRbZMDoPdMSx5Sf1pump"; //15
const URMOM: &str = "9j6twpYWrV1ueJok76D9YK8wJTVoG9Zy8spC7wnTpump"; //16
const CHILLHOUSE: &str = "GkyPYa7NnCFbduLknCfBfP7p8564X1VZhwZYJ6CZpump"; //17
const AURA: &str = "4rwPNRSFgcS7EGphFdX7VwXuhjZGxph7gYyb7Zp2pump"; //18
const LILPUF: &str = "5241BVJpTDscdFM5bTmeuchBcjXN5sasBywyF7onkJZP"; //19
const JOBCOIN: &str = "AyrQpt5xsVYiN4BqgZdd2tZJAWswT9yLUZmP1jKqpump"; //20
const PHARMA: &str = "HtrvP4fG9KiFqFeu4f32RuZiwG3nmYwPkPZ61nAbpump"; //21
const ANIME: &str = "3pA668WX5vNjQMw2KdHJ8RpZceG6gEfXWtRvGChjbSnz"; //22
const EIGEN: &str = "Fc7tEqyfHPoWQXdiAqx62d7WeuH7Zq1DHwa2ihDpump"; //23
const CINO: &str = "BUUB7DpQT1mcTrs55oXawgEbxm5khAozsbmyhMdRpump"; //24
const CODAC: &str = "69LjZUUzxj3Cb3Fxeo1X4QpYEQTboApkhXTysPpbpump"; //26
const WIZARD: &str = "8oosbx7jJrZxm5m4ThKhBpvwwG4QpoAe6i4GiG19pump"; //27
const SUGAR: &str = "5iVmFCCwJTuuw7p4FrxYoZ1bUNdjg14j7uv5hMsMpump"; //28
const FIGURE: &str = "7LSsEoJGhLeZzGvDofTdNg7M3JttxQqGWNLo6vWMpump"; //29
const LEMON: &str = "CjqxraDuTMEcfhdqY8qEaMY43icdBrkt3EXciNVpump"; //30
const DELI: &str = "8BdXCskcD98NUk9Ciwx6eZqXUD9zB891sSu3rYBSpump"; //31
const WALE: &str = "AAE7JS7EAHkQzRKn1Cmt7TP5cQR39Df8D3zxWmNjpump"; //32
const CFX: &str = "RhFVq1Zt81VvcoSEMSyCGZZv5SwBdA8MV7w4HEMpump"; //33

pub fn init_mints() -> Vec<Pubkey> {
    let deepcore: Pubkey = Pubkey::from_str(DEEPCORE).unwrap();
    let house: Pubkey = Pubkey::from_str(HOUSE).unwrap();
    let grok: Pubkey = Pubkey::from_str(GORK).unwrap();
    let weed: Pubkey = Pubkey::from_str(WEED).unwrap();
    let rfc: Pubkey = Pubkey::from_str(RFC).unwrap();
    let dark: Pubkey = Pubkey::from_str(DARK).unwrap();
    let trencher: Pubkey = Pubkey::from_str(TRENCHER).unwrap();
    let neet: Pubkey = Pubkey::from_str(NEET).unwrap();
    let ghibli: Pubkey = Pubkey::from_str(GHIBLI).unwrap();
    let moonpig: Pubkey = Pubkey::from_str(MOONPIG).unwrap();
    let cycle: Pubkey = Pubkey::from_str(CYCLE).unwrap();
    let wagmi: Pubkey = Pubkey::from_str(WAGMI).unwrap();
    let pumpswap: Pubkey = Pubkey::from_str(PUMPSWAP).unwrap();
    let fog: Pubkey = Pubkey::from_str(FOG).unwrap();
    let hotmom: Pubkey = Pubkey::from_str(HOTMOM).unwrap();
    let vitafin: Pubkey = Pubkey::from_str(VITAFIN).unwrap();
    let urmom: Pubkey = Pubkey::from_str(URMOM).unwrap();
    let chill: Pubkey = Pubkey::from_str(CHILLHOUSE).unwrap();
    let aura: Pubkey = Pubkey::from_str(AURA).unwrap();
    let lilpuf: Pubkey = Pubkey::from_str(LILPUF).unwrap();
    let jobcoin: Pubkey = Pubkey::from_str(JOBCOIN).unwrap();
    let pharma: Pubkey = Pubkey::from_str(PHARMA).unwrap();
    let anime: Pubkey = Pubkey::from_str(ANIME).unwrap();
    let eigen: Pubkey = Pubkey::from_str(EIGEN).unwrap();
    let cino: Pubkey = Pubkey::from_str(CINO).unwrap();
    let codac: Pubkey = Pubkey::from_str(CODAC).unwrap();
    let wizard: Pubkey = Pubkey::from_str(WIZARD).unwrap();
    let sugar: Pubkey = Pubkey::from_str(SUGAR).unwrap();
    let figure: Pubkey = Pubkey::from_str(FIGURE).unwrap();
    let lemon: Pubkey = Pubkey::from_str(LEMON).unwrap();
    let deli: Pubkey = Pubkey::from_str(DELI).unwrap();
    let wale: Pubkey = Pubkey::from_str(WALE).unwrap();
    let cfx: Pubkey = Pubkey::from_str(CFX).unwrap();

    vec![
        deepcore, house, grok, weed, rfc, dark, trencher, neet, ghibli, moonpig, cycle, wagmi, pumpswap,
        fog, hotmom, vitafin, urmom, chill, aura, lilpuf, jobcoin, pharma, anime, eigen, cino,
        codac, wizard, sugar, lemon, figure, deli, wale, cfx,
    ]
}
