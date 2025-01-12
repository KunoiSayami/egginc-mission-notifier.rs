pub(super) const VERSION: &str = "1.33.6";
pub(super) const BUILD: &str = "1.33.6.0";
pub(super) const VERSION_NUM: u32 = 67;
pub(super) const PLATFORM_STRING: &str = "IOS";
pub(super) const DEVICE_ID: &str = "egginc-bot";
pub(super) const PLATFORM: i32 = super::proto::Platform::Ios as i32;
pub(super) const DEFAULT_API_BACKEND: &str = "https://ctx-dot-auxbrainhome.appspot.com";

// Copied from https://github.com/carpetsage/egg/blob/78cd2bdd7e020a3364e5575884135890cc01105c/lib/api/index.ts
pub(super) const DEFAULT_USER: &[u8] = &[
    69, 73, 54, 50, 57, 49, 57, 52, 48, 57, 54, 56, 50, 51, 53, 48, 48, 56,
];

pub(super) const UNIT: &[&'static str] = &[
    "", "K", "M", "B", "T", "q", "Q", "s", "S", "o", "N", "d", "U", "D", "Td", "qd", "Qd", "sd",
    "Sd", "Od", "Nd", "V", "uV", "dV", "tV", "qV", "QV", "sV", "SV", "OV", "NV", "tT",
];

pub(super) const DEFAULT_UNIT: &str = "A Lot";

pub(super) const API_BACKEND: &str = determine_api();

const fn determine_api() -> &'static str {
    match option_env!("API_BACKEND") {
        Some(s) => s,
        None => &DEFAULT_API_BACKEND,
    }
}
