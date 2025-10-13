pub(super) const VERSION: &str = "1.35.1";
pub(super) const BUILD: &str = "111313";
pub(super) const VERSION_NUM: u32 = 70;
pub(super) const PLATFORM_STRING: &str = "IOS";
pub(super) const DEVICE_ID: &str = "egginc-bot";
pub(super) const PLATFORM: i32 = super::proto::Platform::Ios as i32;
pub(super) const DEFAULT_API_BACKEND: &str = "https://ctx-dot-auxbrainhome.appspot.com";

// Copied from https://github.com/carpetsage/egg/blob/78cd2bdd7e020a3364e5575884135890cc01105c/lib/api/index.ts
pub(super) const DEFAULT_USER: &[u8] = &[
    69, 73, 54, 50, 57, 49, 57, 52, 48, 57, 54, 56, 50, 51, 53, 48, 48, 56,
];

pub(super) const OOM_UNIT: &[&str] = &[
    "", "K", "M", "B", "T", "q", "Q", "s", "S", "o", "N", "d", "U", "D", "Td", "qd", "Qd", "sd",
    "Sd", "Od", "Nd", "V", "uV", "dV", "tV", "qV", "QV", "sV", "SV", "OV", "NV", "tT",
];

pub(super) const DEFAULT_OOM_UNIT: &str = "A Lot";

pub(super) const EARNING_BONUS_ROLE: &[&str] = &[
    "Farmer",
    "Farmer II",
    "Farmer III",
    "Kilo",
    "Kilo II",
    "Kilo III",
    "Mega",
    "Mega II",
    "Mega III",
    "Giga",
    "Giga II",
    "Giga III",
    "Tera",
    "Tera II",
    "Tera III",
    "Peta",
    "Peta II",
    "Peta III",
    "Exa",
    "Exa II",
    "Exa III",
    "Zetta",
    "Zetta II",
    "Zetta III",
    "Yotta",
    "Yotta II",
    "Yotta III",
    "Xenna",
    "Xenna II",
    "Xenna III",
    "Wecca",
    "Wecca II",
    "Wecca III",
    "Venda",
    "Venda II",
    "Venda III",
    "Uada",
    "Uada II",
    "Uada III",
    "Treida",
    "Treida II",
    "Treida III",
    "Quada",
    "Quada II",
    "Quada III",
    "Penda",
    "Penda II",
    "Penda III",
    "Exeda",
    "Exeda II",
    "Exeda III",
];

pub(super) const DEFAULT_EARNING_BONUS_ROLE: &str = "Infini";

pub(super) const API_BACKEND: &str = determine_api();

const fn determine_api() -> &'static str {
    match option_env!("API_BACKEND") {
        Some(s) => s,
        None => DEFAULT_API_BACKEND,
    }
}
