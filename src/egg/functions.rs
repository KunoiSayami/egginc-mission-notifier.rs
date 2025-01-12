use std::{collections::HashMap, io::Cursor};

use anyhow::anyhow;
use base64::{prelude::BASE64_STANDARD, Engine};
use flate2::bufread::ZlibDecoder;
use reqwest::Client;

use super::definitions::*;
use super::proto;
//use super::proto::contract::GradeSpec;
use super::types::SpaceShipInfo;

pub(crate) fn parse_num_with_unit(mut num: f64) -> String {
    let mut count = 0;
    while num > 1000.0 {
        num /= 1000.0;
        count += 1;
        if count > UNIT.len() {
            break;
        }
    }
    let unit = UNIT.get(count).unwrap_or(&DEFAULT_UNIT);
    format!("{num:.2}{}", unit)
}

pub(super) fn encode_to_base64<T: prost::Message>(input: &T) -> String {
    BASE64_STANDARD.encode(&encode_to_byte(input))
}

pub(super) fn encode_to_byte<T: prost::Message>(input: &T) -> Vec<u8> {
    let mut v = Vec::with_capacity(input.encoded_len());

    input.encode(&mut v).unwrap();
    v
}

pub fn build_basic_info(ei: Option<String>) -> Option<proto::BasicRequestInfo> {
    Some(proto::BasicRequestInfo {
        ei_user_id: Some(ei.unwrap_or_default()),
        client_version: Some(VERSION_NUM),
        version: Some(VERSION.into()),
        build: Some(BUILD.into()),
        platform: Some(PLATFORM_STRING.into()),
        country: None,
        language: None,
        debug: Some(false),
    })
}

/// /ei/coop_status_basic
/* pub fn build_join_request(contract_id: &str, coop_id: &str, ei: Option<String>) -> String {
    let user = ei
        .map(std::borrow::Cow::Owned)
        .unwrap_or(String::from_utf8_lossy(DEFAULT_USER));
    let request = proto::JoinCoopRequest {
        rinfo: build_basic_info(Some(user.to_string())),
        contract_identifier: Some(contract_id.to_string()),
        coop_identifier: Some(coop_id.to_string()),
        user_id: Some(user.to_string()),
        client_version: Some(VERSION_NUM),
        ..Default::default()
    };
    encode_to_base64(request)
} */

/// /ei/query_coop
/* pub fn build_query_coop_request(
    contract_id: &str,
    coop_id: &str,
    ei: Option<String>,
    grade: proto::contract::PlayerGrade,
) -> String {
    let user = ei
        .map(std::borrow::Cow::Owned)
        .unwrap_or(String::from_utf8_lossy(DEFAULT_USER));
    let request = proto::QueryCoopRequest {
        rinfo: build_basic_info(Some(user.to_string())),
        contract_identifier: Some(contract_id.to_string()),
        coop_identifier: Some(coop_id.to_string()),
        grade: Some(grade.into()),
        client_version: Some(VERSION_NUM),
        ..Default::default()
    };

    encode_to_base64(request)
} */

/// /ei/coop_status
pub fn build_coop_status_request(contract_id: &str, coop_id: &str, ei: Option<String>) -> String {
    let user = ei
        .map(std::borrow::Cow::Owned)
        .unwrap_or(String::from_utf8_lossy(DEFAULT_USER));
    let request = proto::ContractCoopStatusRequest {
        rinfo: build_basic_info(Some(user.to_string())),
        contract_identifier: Some(contract_id.to_string()),
        coop_identifier: Some(coop_id.to_string()),
        user_id: Some(user.to_string()),
        client_version: Some(VERSION_NUM),
        ..Default::default()
    };

    encode_to_base64(&request)
}

// Source: https://github.com/carpetsage/egg/blob/78cd2bdd7e020a3364e5575884135890cc01105c/lib/api/index.ts
pub fn build_first_contract_request(ei: String) -> String {
    let request = proto::EggIncFirstContactRequest {
        rinfo: build_basic_info(None),
        ei_user_id: Some(ei),
        user_id: None,
        game_services_id: None,
        device_id: Some(DEVICE_ID.into()),
        username: None,
        client_version: Some(VERSION_NUM),
        platform: Some(PLATFORM),
    };

    encode_to_base64(&request)
}

pub fn decode_data<T: AsRef<[u8]>, Output: prost::Message + std::default::Default>(
    base64_encoded: T,
    authorized: bool,
) -> anyhow::Result<Output> {
    if !authorized {
        return if let Ok(raw) = BASE64_STANDARD.decode(base64_encoded.as_ref()) {
            Output::decode(&mut Cursor::new(raw))
        } else {
            Output::decode(&mut Cursor::new(base64_encoded))
        }
        .map_err(|e| anyhow!("Decode user data error: {e:?}"));
    }
    let tmp: proto::AuthenticatedMessage = decode_data(base64_encoded, false)?;
    if tmp.message().is_empty() {
        return Err(anyhow!("Message is empty"));
    }
    if tmp.compressed() {
        let decoder = ZlibDecoder::new(tmp.message());
        decode_data(decoder.into_inner(), false)
    } else {
        decode_data(tmp.message(), false)
    }
}

pub fn get_missions(data: proto::EggIncFirstContactResponse) -> Option<Vec<SpaceShipInfo>> {
    Some(
        data.backup?
            .artifacts_db?
            .mission_infos
            .into_iter()
            .map(SpaceShipInfo::from)
            .collect(),
    )
}

pub async fn request(
    client: &Client,
    ei: &str,
) -> anyhow::Result<proto::EggIncFirstContactResponse> {
    let form = [("data", build_first_contract_request(ei.to_string()))]
        .into_iter()
        .collect::<HashMap<_, _>>();
    let resp = client
        .post(format!("{API_BACKEND}/ei/bot_first_contact"))
        .form(&form)
        .send()
        .await?
        .error_for_status()?;
    let data = decode_data(&resp.text().await?, false)?;
    Ok(data)
}

pub fn grade_to_big_g(grade: proto::contract::PlayerGrade) -> f64 {
    match grade {
        proto::contract::PlayerGrade::GradeUnset => 1.0,
        proto::contract::PlayerGrade::GradeC => 1.0,
        proto::contract::PlayerGrade::GradeB => 2.0,
        proto::contract::PlayerGrade::GradeA => 3.5,
        proto::contract::PlayerGrade::GradeAa => 5.0,
        proto::contract::PlayerGrade::GradeAaa => 7.0,
    }
}
