mod coop;
mod definitions;
mod functions;
pub mod monitor;
#[allow(clippy::enum_variant_names, dead_code)]
pub mod proto;
pub mod types;

pub use coop::{decode_and_calc_score, decode_coop_status, query_coop_status};
pub(crate) use functions::{
    encode_to_byte, extract_contracts, extract_epic_research, is_contract_cleared,
    request as ei_request,
};
