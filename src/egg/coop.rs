use anyhow::anyhow;
use chrono::TimeDelta;
use reqwest::Client;
use std::{collections::HashMap, sync::LazyLock};

use crate::{
    bot::replace_all,
    egg::functions::{build_coop_status_request, parse_num_with_unit},
    types::{fmt_time_delta, timestamp_to_string, ContractSpec},
};

use super::{
    definitions::{API_BACKEND, UNIT},
    functions::{decode_data, grade_to_big_g},
    proto::{self, FarmProductionParams},
};

#[allow(unused)]
static NUM_STR_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^(\d+(\.\d+)?)(\w{1,2}|A Lot)?$").unwrap());

#[allow(unused)]
fn parse_num_str(s: &str) -> Option<f64> {
    let Some(cap) = NUM_STR_RE.captures(s) else {
        return None;
    };

    let basic = cap.get(1).unwrap().as_str().parse().ok()?;
    let Some(unit) = cap.get(3) else {
        return Some(basic);
    };
    let base = 1000.0f64;
    for (index, u) in UNIT.iter().enumerate() {
        if u.eq(&unit.as_str()) {
            return Some(basic * base.powi(index as i32));
        }
    }
    None
}

/* pub async fn query_contract_status(
    client: &Client,
    contract_id: &str,
    coop_id: &str,
    grade: proto::contract::PlayerGrade,
    ei: &str,
) -> anyhow::Result<proto::QueryCoopResponse> {
    let form = [(
        "data",
        build_query_coop_request(contract_id, coop_id, Some(ei.to_string()), grade),
    )]
    .into_iter()
    .collect::<HashMap<_, _>>();

    let resp = client
        .post(format!("{API_BACKEND}/ei/query_coop"))
        .form(&form)
        .send()
        .await?;

    println!("{API_BACKEND} {:?}", resp.headers().get("X-Cached"));

    let data = resp.bytes().await?;
    //println!("{data:?}");
    let res = decode_data(data, false)?;
    println!("{res:#?}");

    Ok(res)
} */

/*  pub async fn query_coop_status_basic(
    client: &Client,
    contract_id: &str,
    coop_id: &str,
    ei: &str,
    is_join_request: bool,
) -> anyhow::Result<proto::JoinCoopResponse> {
    let form = [(
        "data",
        if is_join_request {
            build_join_request
        } else {
            build_coop_status_request
        }(contract_id, coop_id, Some(ei.to_string())),
    )]
    .into_iter()
    .collect::<HashMap<_, _>>();

    let resp = client
        .post(format!("{API_BACKEND}/ei/coop_status_basic"))
        .form(&form)
        .send()
        .await?;

    println!("{API_BACKEND} {:?}", resp.headers().get("X-Cached"));

    let data = resp.bytes().await?;
    //println!("{data:?}");
    let res: proto::JoinCoopResponse = decode_data(data, true)?;

    //query_contract_status(&client, contract_id, coop_id, res.grade(), ei).await?;
    println!("{res:#?}");

    Ok(res)
} */

pub async fn query_coop_status(
    client: &Client,
    contract_id: &str,
    coop_id: &str,
    ei: &str,
) -> anyhow::Result<proto::ContractCoopStatusResponse> {
    let form = [(
        "data",
        build_coop_status_request(contract_id, coop_id, Some(ei.to_string())),
    )]
    .into_iter()
    .collect::<HashMap<_, _>>();

    let resp = client
        .post(format!("{API_BACKEND}/ei/coop_status"))
        .form(&form)
        .send()
        .await?;

    //println!("{API_BACKEND} {:?}", resp.headers().get("X-Cached"));

    let data = resp.bytes().await?;
    println!("{data:?}");
    let res: proto::ContractCoopStatusResponse = decode_data(data, true)?;

    //query_contract_status(&client, contract_id, coop_id, res.grade(), ei).await?;
    //println!("{res:#?}");

    Ok(res)
}

fn calc_timestamp(timestamp: f64) -> f64 {
    if timestamp < 0.0 {
        timestamp.abs()
    } else {
        kstool::time::get_current_duration().as_millis() as f64 / 1000.0 - timestamp
    }
}

fn calc_total_score(
    coop: &proto::ContractCoopStatusResponse,
    goal1: f64,
    goal3: f64,
    coop_size: i64,
    token_time: f64,
    coop_total_time: f64,
) -> String {
    let pu = parse_num_with_unit;
    let s2h = |value: f64| value * 3600.0;

    let mut output = vec![];

    let (completion_time, expect_remain_time, remain_time) = if !coop.all_goals_achieved() {
        let remain = goal3 - coop.total_amount();
        let (total_elr, offline_egg) = coop
            .contributors
            .iter()
            .filter(|x| x.production_params.is_some() && x.farm_info.is_some())
            .fold((0.001, 0.0), |(mut acc, mut offline_egg), x| {
                let farm_prams = x.production_params.as_ref().unwrap();
                let farm_elr = farm_prams.elr() * farm_prams.farm_population();
                acc += farm_elr;
                // offline laying
                let player_offline_egg =
                    calc_timestamp(x.farm_info.as_ref().unwrap().timestamp()) * farm_elr;
                offline_egg += player_offline_egg;
                //log::trace!("Player {} egg {}", x.user_name(), pu(player_offline_egg));
                (acc, offline_egg)
            });
        //log::trace!("{} {} {total_elr}", pu(remain), pu(offline_egg));
        let expect_remain_time = (remain - offline_egg) / total_elr;
        (
            coop_total_time - coop.seconds_remaining() + expect_remain_time,
            expect_remain_time,
            coop.seconds_remaining() - expect_remain_time,
        )
    } else {
        (
            coop_total_time - coop.seconds_remaining() - coop.seconds_since_all_goals_achieved(),
            0.0,
            coop.seconds_remaining() + coop.seconds_since_all_goals_achieved(),
        )
    };

    if expect_remain_time > 0.0 {
        output.push(format!(
            "Expect completion time: {}, {}",
            replace_all(&timestamp_to_string(
                (kstool::time::get_current_second() as f64 + expect_remain_time) as i64
            )),
            fmt_time_delta(TimeDelta::seconds(expect_remain_time as i64))
        ));
    }

    //log::trace!("Completion time: {completion_time}, Expect remain time: {expect_remain_time}, Remain time: {remain_time}" );

    let big_g = grade_to_big_g(coop.grade());

    for player in &coop.contributors {
        let Some(ref production) = player.production_params else {
            output.push(format!("{} skipped", replace_all(player.user_name())));
            continue;
        };
        let score = calc_score(
            production,
            big_g,
            goal1,
            goal3,
            coop.total_amount(),
            coop_size,
            token_time,
            coop_total_time,
            completion_time,
            expect_remain_time,
            remain_time,
        )
        .abs();
        /* print!(
            "Player: {} completion time {completion_time}",
            player.user_name()
        ); */
        output.push(format!(
            "*{}* elr: _{}_ shipped: _{}_ score: _{}_",
            replace_all(player.user_name()),
            replace_all(&pu(s2h(production.elr() * production.farm_population()))),
            replace_all(&pu(player.production_params.as_ref().unwrap().delivered())),
            score as i64
        ));
    }
    output.join("\n")
}

#[allow(unused)]
fn calc_score(
    production: &FarmProductionParams,
    big_g: f64,
    goal1: f64,
    goal3: f64,
    total_delivered: f64,
    coop_size: i64,
    token_time: f64,
    coop_total_time: f64,
    completion_time: f64,
    expect_remain_time: f64,
    remain_time: f64,
) -> f64 {
    let user_total_delivered = production.delivered()
        + production.elr() * production.farm_population() * expect_remain_time;
    let ratio = (user_total_delivered * coop_size as f64) / goal3.min(goal1.max(total_delivered));

    let big_c = 1.0
        + if ratio > 2.5 {
            3.386486 + 0.02221 * ratio.min(12.5)
        } else {
            3.0 * ratio.powf(0.15)
        };
    let t = 0.0075 * 0.8 * completion_time * 0.12 * 10.0;
    let big_b = 5.0 * 2.0f64.min(t / completion_time);

    let big_a = completion_time / token_time;
    let big_v = if big_a <= 42.0 { 3.0 } else { 0.07 * big_a };
    let big_t = 2.0 * (big_v.min(4.0) + 4.0 * big_v.min(2.0)) / big_v;

    //let run_cap = 4.0;
    let big_r = 6.0f64.min(0.3f64.max(12.0 / coop_size as f64 / coop_total_time * 86400.0));
    187.5
        * big_g
        * big_c
        * (1.0 + coop_total_time / 86400.0 / 3.0)
        * (1.0 + 4.0 * (1.0 - completion_time / coop_total_time).powi(3))
    //* (1.0 + (big_b + big_r + big_t) / 100.0)
}

pub fn decode_2(spec: ContractSpec, data: &[u8], authorized: bool) -> anyhow::Result<String> {
    let res: proto::ContractCoopStatusResponse = decode_data(data, authorized)?;
    let Some(grade_spec) = spec.get(&res.grade()) else {
        return Err(anyhow!("Spec not found"));
    };
    let mut output = vec![];

    output.push(format!(
        "Total amount: {}, time remain: {}, target: {}",
        replace_all(&parse_num_with_unit(res.total_amount())),
        replace_all(&fmt_time_delta(TimeDelta::seconds(
            res.seconds_remaining() as i64
        ))),
        replace_all(&parse_num_with_unit(grade_spec.goal3())),
    ));
    output.push(calc_total_score(
        &res,
        grade_spec.goal1(),
        grade_spec.goal3(),
        spec.max_coop_size(),
        spec.token_time(),
        grade_spec.length(),
    ));

    //println!("{res:#?}");
    Ok(output.join("\n"))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_num() {
        assert_eq!(parse_num_str("2.5Q"), Some(2.5e18));
        assert_eq!(parse_num_str("2.5"), Some(2.5));
        assert_eq!(parse_num_str("2Q"), Some(2e18));
        assert_eq!(parse_num_str("3.5s"), Some(3.5e21));
        assert_eq!(parse_num_str("0.00"), Some(0.0));
        assert_eq!(parse_num_str("3.5e16"), None);
    }
}
