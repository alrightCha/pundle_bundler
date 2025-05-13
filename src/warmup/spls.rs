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
const CYCLE: &str = "HJ2n2a3YK1LTBCRbS932cTtmXw4puhgG8Jb2WcpEpump"; //10
const USDC: &str = "BQQzEvYT4knThhkSPBvSKBLg1LEczisWLhx5ydJipump"; //11
const CBBTC: &str = "cbbtcf3aa214zXHbiAZQwf4122FBYbraNdFqgw4iMij"; //12
const UFD: &str = "TTTpMsuQ1ic6zQrmf3mKqjcZzyHb4ugdNRq9vyvpump"; //13
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
const SHRIMP: &str = "BMnMaiMu5B29o3eMDena8cxwL9R4CDwotxQMG2Tspump"; //23
const CINO: &str = "BUUB7DpQT1mcTrs55oXawgEbxm5khAozsbmyhMdRpump"; //24
const OX: &str = "3E2z4KX7y457xJqK9RQeJhA29oPdoUvAAD3Ea3zQyuG3"; //26
const WIZARD: &str = "8oosbx7jJrZxm5m4ThKhBpvwwG4QpoAe6i4GiG19pump"; //27
const SUGAR: &str = "5iVmFCCwJTuuw7p4FrxYoZ1bUNdjg14j7uv5hMsMpump"; //28
const WOKE: &str = "GT564KpGybkXFE43D8eySwNuy2zdV6hjXsiHREwSpump"; //29
const FRITZ: &str = "4Ge6ejgv7KJHqDgML3w2S48rntuZQ4KZ9WGEVeibpump"; //30
const DELI: &str = "8BdXCskcD98NUk9Ciwx6eZqXUD9zB891sSu3rYBSpump"; //31
const WALE: &str = "AAE7JS7EAHkQzRKn1Cmt7TP5cQR39Df8D3zxWmNjpump"; //32
const CFX: &str = "RhFVq1Zt81VvcoSEMSyCGZZv5SwBdA8MV7w4HEMpump"; //33

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
    let cycle: Pubkey = Pubkey::from_str(CYCLE).unwrap();
    let cbbtc: Pubkey = Pubkey::from_str(CBBTC).unwrap();
    let ufd: Pubkey = Pubkey::from_str(UFD).unwrap();
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
    let shrimp: Pubkey = Pubkey::from_str(SHRIMP).unwrap();
    let cino: Pubkey = Pubkey::from_str(CINO).unwrap();
    let ox: Pubkey = Pubkey::from_str(OX).unwrap();
    let wizard: Pubkey = Pubkey::from_str(WIZARD).unwrap();
    let sugar: Pubkey = Pubkey::from_str(SUGAR).unwrap();
    let woke: Pubkey = Pubkey::from_str(WOKE).unwrap();
    let fritz: Pubkey = Pubkey::from_str(FRITZ).unwrap();
    let deli: Pubkey = Pubkey::from_str(DELI).unwrap();
    let wale: Pubkey = Pubkey::from_str(WALE).unwrap();
    let cfx: Pubkey = Pubkey::from_str(CFX).unwrap();

    vec![
        usdc, house, grok, troll, rfc, dark, trencher, neet, ghibli, moonpig, cycle, cbbtc, ufd,
        fog, hotmom, vitafin, urmom, chill, aura, lilpuf, jobcoin, pharma, anime, shrimp, cino,
        ox, wizard, sugar, woke, fritz, deli, wale, cfx,
    ]
}
