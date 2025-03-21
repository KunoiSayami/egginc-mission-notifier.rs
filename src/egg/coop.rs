use anyhow::anyhow;
use reqwest::Client;
use std::{collections::HashMap, sync::LazyLock};
use types::CoopScore;

use crate::{
    egg::functions::{build_coop_status_request, parse_num_with_unit},
    types::ContractSpec,
};

use super::{
    definitions::{API_BACKEND, OOM_UNIT},
    functions::decode_data,
    proto::{self},
};

#[allow(unused)]
static NUM_STR_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^(\d+(\.\d+)?)(\w{1,2}|A Lot)?$").unwrap());

#[allow(unused)]
fn parse_num_str(s: &str) -> Option<f64> {
    let cap = NUM_STR_RE.captures(s)?;

    let basic = cap.get(1).unwrap().as_str().parse().ok()?;
    let Some(unit) = cap.get(3) else {
        return Some(basic);
    };
    let base = 1000.0f64;
    for (index, u) in OOM_UNIT.iter().enumerate() {
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

pub fn decode_coop_status(
    data: &[u8],
    authorized: bool,
) -> anyhow::Result<proto::ContractCoopStatusResponse> {
    decode_data(data, authorized)
}

pub async fn query_coop_status(
    client: &Client,
    contract_id: &str,
    coop_id: &str,
    ei: Option<String>,
) -> anyhow::Result<proto::ContractCoopStatusResponse> {
    let form = [("data", build_coop_status_request(contract_id, coop_id, ei))]
        .into_iter()
        .collect::<HashMap<_, _>>();

    let resp = client
        .post(format!("{API_BACKEND}/ei/coop_status"))
        .form(&form)
        .send()
        .await?;

    //println!("{API_BACKEND} {:?}", resp.headers().get("X-Cached"));

    let data = resp.bytes().await?.to_vec();

    let res = decode_data(data, true)?;

    //query_contract_status(&client, contract_id, coop_id, res.grade(), ei).await?;
    //println!("{res:#?}");

    Ok(res)
}

pub fn decode_and_calc_score(
    spec: ContractSpec,
    data: &[u8],
    authorized: bool,
) -> anyhow::Result<Option<CoopScore>> {
    let res: proto::ContractCoopStatusResponse = decode_data(data, authorized)?;
    /* let mut output = vec![];

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
    )); */

    //println!("{res:#?}");
    CoopScore::calc(res, &spec).map_err(|e| anyhow!("{e}"))
}

mod types {
    use std::borrow::Cow;

    use chrono::TimeDelta;

    use crate::{
        egg::{
            definitions::{DEFAULT_EARNING_BONUS_ROLE, EARNING_BONUS_ROLE},
            functions::grade_to_big_g,
            proto::contract::PlayerGrade,
            types::ContractGradeSpec,
        },
        types::{fmt_time_delta_short, ContractSpec},
    };

    use super::parse_num_with_unit;

    fn calc_timestamp(timestamp: f64) -> f64 {
        if timestamp <= 0.0 {
            timestamp.abs()
        } else {
            kstool::time::get_current_duration().as_millis() as f64 / 1000.0 - timestamp
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub enum CompletionLevel {
        NotTrack,
        OnTrack,
        Completed,
        Cleared,
    }

    impl CompletionLevel {
        fn from_status(data: &super::proto::ContractCoopStatusResponse, on_time: bool) -> Self {
            if data.cleared_for_exit() || data.all_members_reporting() {
                Self::Cleared
            } else if data.all_goals_achieved() {
                Self::Completed
            } else if on_time {
                Self::OnTrack
            } else {
                Self::NotTrack
            }
        }
    }

    impl std::fmt::Display for CompletionLevel {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "{}",
                match self {
                    CompletionLevel::NotTrack => "🔴",
                    CompletionLevel::OnTrack => "🟡",
                    CompletionLevel::Completed => "🟢",
                    CompletionLevel::Cleared => "✅",
                }
            )
        }
    }

    #[derive(Clone)]
    pub struct UserScore {
        username: String,
        amount: f64,
        shipping_rate: Option<f64>,
        egg_laying_rate: Option<f64>,
        finalized: bool,
        timestamp: Option<f64>,
        soul_power: f64,
        permit_level: Option<u32>,
        coop_buff: (f64, f64),
        score: f64,
    }

    impl UserScore {
        pub fn username(&self) -> &str {
            &self.username
        }

        pub fn real_elr(&self) -> Option<String> {
            Some(parse_num_with_unit(self.egg_laying_rate? * 3600.0))
        }

        /* pub fn elr(&self) -> Option<String> {
            Some(parse_num_with_unit(
                self.egg_laying_rate?.min(self.shipping_rate?) * 3600.0,
            ))
        } */

        pub fn score(&self) -> f64 {
            self.score
        }
        pub fn sr(&self) -> String {
            if let Some(sr) = self.shipping_rate {
                parse_num_with_unit(sr * 3600.0)
            } else {
                "N/A".into()
            }
        }

        pub fn amount(&self) -> String {
            parse_num_with_unit(self.amount)
        }

        pub fn finalized(&self) -> &'static str {
            if self.finalized {
                " ✅"
            } else {
                ""
            }
        }

        pub fn timestamp(&self, cache_timestamp: Option<i64>) -> Option<f64> {
            self.timestamp
                .map(|t| CoopScore::get_timestamp_offset(t, cache_timestamp))
        }

        pub fn print(
            &self,
            detail: bool,
            cache_timestamp: Option<i64>,
            cleared: bool,
            escape_func: fn(&str) -> Cow<'_, str>,
        ) -> Result<String, std::fmt::Error> {
            use std::fmt::Write;
            let mut fmt = String::with_capacity(50);
            write!(
                fmt,
                "*{}* _Shipped:_ {} _ELR:_ {} _SR:_ {}",
                escape_func(self.username()),
                escape_func(&self.amount()),
                if let Some(elr) = self.real_elr() {
                    escape_func(&elr).into_owned()
                } else {
                    "N/A".into()
                },
                escape_func(&self.sr()),
            )?;

            if detail {
                write!(
                    fmt,
                    " _EB%_: {} \\({}\\)",
                    escape_func(&parse_num_with_unit(10.0f64.powf(self.soul_power) * 100.0)),
                    self.eb_role()
                )?;

                if self.permit_level.is_some_and(|x: u32| x == 1) {
                    write!(fmt, "⭐")?;
                }

                if !cleared && !self.finalized {
                    if let Some(timestamp) = self.timestamp(cache_timestamp) {
                        write!(
                            fmt,
                            " _Offline_: {} \\({}\\)",
                            fmt_time_delta_short(TimeDelta::seconds(timestamp.abs() as i64)),
                            escape_func(&parse_num_with_unit(
                                timestamp.abs() * self.egg_laying_rate.unwrap_or(0.0)
                            ))
                        )?;
                    } else {
                        write!(fmt, " \\[Private\\]")?;
                    }
                }

                write!(
                    fmt,
                    " \\(E:{:.0}%, L:{:.0}%\\)",
                    self.coop_buff.0, self.coop_buff.1
                )?;
            }

            write!(
                fmt,
                " _Score:_ __{}__ {}",
                self.score() as i64,
                self.finalized()
            )?;
            Ok(fmt)
        }

        fn eb_role(&self) -> &str {
            EARNING_BONUS_ROLE
                .get(self.soul_power.floor() as usize)
                .unwrap_or(&DEFAULT_EARNING_BONUS_ROLE)
        }
    }

    enum CoopStatus {
        Normal(f64, f64, f64, Vec<UserScore>),
        OutOfTime,
    }

    #[derive(Clone)]
    pub struct CoopScore {
        spec: ContractGradeSpec,
        //token_time: f64,
        current_amount: f64,
        grade: PlayerGrade,
        completion_time: f64,
        status: CompletionLevel,
        expect_remain_time: f64,
        contract_remain_time: f64,
        member: Vec<UserScore>,
    }

    impl CoopScore {
        pub fn calc(
            data: super::proto::ContractCoopStatusResponse,
            spec: &ContractSpec,
        ) -> Result<Option<Self>, &'static str> {
            let Some(grade_spec) = spec.get(&data.grade()) else {
                return Err("Grade spec not found");
            };

            /* output.push(format!(
                "Total amount: {}, time remain: {}, target: {}",
                replace_all(&parse_num_with_unit(res.total_amount())),
                replace_all(&fmt_time_delta(TimeDelta::seconds(
                    res.seconds_remaining() as i64
                ))),
                replace_all(&parse_num_with_unit(grade_spec.goal3())),
            )); */

            let CoopStatus::Normal(completion_time, expect_remain_time, _remain_time, players) =
                Self::calc_total_score(&data, grade_spec, spec)
            else {
                return Ok(None);
            };

            Ok(Some(Self {
                //token_time: spec.token_time(),
                spec: *grade_spec,
                expect_remain_time,
                grade: data.grade(),
                status: CompletionLevel::from_status(&data, completion_time < grade_spec.length()),
                completion_time,
                current_amount: data.total_amount(),
                contract_remain_time: data.seconds_remaining(),
                member: players,
            }))
        }

        fn calc_total_score(
            coop: &super::proto::ContractCoopStatusResponse,
            grade_spec: &ContractGradeSpec,
            spec: &ContractSpec,
        ) -> CoopStatus {
            /* let pu = crate::egg::functions::parse_num_with_unit;
            let s2h = |value: f64| value * 3600.0; */

            //let mut output = vec![];

            let coop_total_time = grade_spec.length();

            let (completion_time, expect_remain_time, remain_time) = if !coop.all_goals_achieved() {
                let remain = grade_spec.goal3() - coop.total_amount();
                let (total_elr, offline_egg) = coop
                    .contributors
                    .iter()
                    .filter_map(|x| {
                        let farm_prams = x.production_params.as_ref()?;
                        let farm_info = x.farm_info.as_ref()?;
                        let farm_elr = farm_prams
                            .sr()
                            .min(farm_prams.elr() * farm_prams.farm_population());

                        // offline laying
                        let player_offline_egg = calc_timestamp(farm_info.timestamp()) * farm_elr;
                        //log::trace!("Player {} egg {}", x.user_name(), pu(player_offline_egg));
                        Some((farm_elr, player_offline_egg))
                    })
                    .fold(
                        (0.0, 0.0),
                        |(mut acc, mut offline_egg), (farm_elr, player_offline_egg)| {
                            acc += farm_elr;
                            offline_egg += player_offline_egg;
                            //log::trace!("Player {} egg {}", x.user_name(), pu(player_offline_egg));
                            (acc, offline_egg)
                        },
                    );
                let total_elr = if total_elr == 0.0 { 0.001 } else { total_elr };
                //log::trace!("{} {} {total_elr}", pu(remain), pu(offline_egg));
                let expect_remain_time = (remain - offline_egg) / total_elr;

                if expect_remain_time > coop_total_time {
                    return CoopStatus::OutOfTime;
                }

                /* let completion_time =
                coop_total_time - coop.seconds_remaining() + expect_remain_time; */

                (
                    coop_total_time - coop.seconds_remaining() + expect_remain_time,
                    expect_remain_time,
                    coop.seconds_remaining() - expect_remain_time,
                )
            } else {
                (
                    coop_total_time
                        - coop.seconds_remaining()
                        - coop.seconds_since_all_goals_achieved(),
                    0.0,
                    coop.seconds_remaining() + coop.seconds_since_all_goals_achieved(),
                )
            };

            /* if expect_remain_time > 0.0 {
                output.push(format!(
                    "Expect complete time: {}\\({}\\)",
                    replace_all(&timestamp_to_string(
                        (kstool::time::get_current_second() as f64 + expect_remain_time) as i64
                    )),
                    fmt_time_delta(TimeDelta::seconds(expect_remain_time as i64))
                ));
            } */

            //log::trace!("Completion time: {completion_time}, Expect remain time: {expect_remain_time}, Remain time: {remain_time}" );

            let big_g = grade_to_big_g(coop.grade());
            let mut players = vec![];

            for player in &coop.contributors {
                let (egg_laying_rate, shipping_rate) = player
                    .production_params
                    .map(|s| (s.elr() * s.farm_population(), s.sr()))
                    .unzip();

                let user_timestamp = player.farm_info.as_ref().map(|x| x.timestamp());

                let score = Self::calc_score(
                    egg_laying_rate.and_then(|elr| Some(elr.min(shipping_rate?))),
                    player.contribution_amount(),
                    big_g,
                    grade_spec,
                    grade_spec.goal3().max(coop.total_amount()),
                    spec,
                    completion_time - expect_remain_time.min(0.0),
                    expect_remain_time.max(0.0),
                    calc_timestamp(user_timestamp.unwrap_or_default()),
                );

                players.push(UserScore {
                    egg_laying_rate,
                    shipping_rate,
                    finalized: player.finalized(),
                    amount: player.contribution_amount(),
                    username: player.user_name().into(),
                    timestamp: user_timestamp,
                    soul_power: player.soul_power(),
                    permit_level: player.farm_info.as_ref().map(|x| x.permit_level()),
                    coop_buff: player
                        .buff_history
                        .last()
                        .map(|x| {
                            (
                                (x.earnings() - 1.0) * 100.0,
                                (x.egg_laying_rate() - 1.0) * 100.0,
                            )
                        })
                        .unwrap_or_default(),
                    score,
                });
                /* print!(
                    "Player: {} completion time {completion_time}",
                    player.user_name()
                ); */
                /* output.push(format!(
                    "*{}* elr: _{}_ shipped: _{}_ score: _{}_",
                    replace_all(player.user_name()),
                    replace_all(&pu(s2h(production.elr() * production.farm_population()))),
                    replace_all(&pu(player.production_params.as_ref().unwrap().delivered())),
                    score as i64
                )); */
            }

            /* log::debug!(
                "{completion_time} {} {expect_remain_time} {remain_time}",
                completion_time - expect_remain_time.min(0.0)
            ); */
            CoopStatus::Normal(completion_time, expect_remain_time, remain_time, players)
        }

        #[allow(clippy::too_many_arguments)]
        fn calc_score(
            // Egg laying per second per hen * population
            total_elr: Option<f64>,
            contributions: f64,
            big_g: f64,
            grade_spec: &ContractGradeSpec,
            total_delivered: f64,
            spec: &ContractSpec,
            completion_time: f64,
            expect_remain_time: f64,
            user_offline_time: f64,
        ) -> f64 {
            let coop_total_time = grade_spec.length();
            let coop_size = spec.max_coop_size() as f64;
            let token_time = spec.token_time();

            let user_total_delivered =
                contributions + total_elr.unwrap_or(0.0) * (expect_remain_time + user_offline_time);
            let ratio = (user_total_delivered * coop_size)
                / grade_spec
                    .goal3()
                    .min(grade_spec.goal1().max(total_delivered));

            let big_c = 1.0
                + if ratio > 2.5 {
                    3.386486 + 0.02221 * ratio.min(12.5)
                } else {
                    3.0 * ratio.powf(0.15)
                };
            let t = 0.0075 * 0.8 * completion_time * 0.12 * 10.0;
            let _big_b = 5.0 * 2.0f64.min(t / completion_time);

            let big_a = completion_time / token_time;
            let big_v = if big_a <= 42.0 { 3.0 } else { 0.07 * big_a };
            let _big_t = 2.0 * (big_v.min(4.0) + 4.0 * big_v.min(2.0)) / big_v;

            //let run_cap = 4.0;
            let _big_r = 6.0f64.min(0.3f64.max(12.0 / coop_size / coop_total_time * 86400.0));
            187.5
                * big_g
                * big_c
                * (1.0 + coop_total_time / 86400.0 / 3.0)
                * (1.0 + 4.0 * (1.0 - completion_time / coop_total_time).powi(3))
            //* (1.0 + (big_b + big_r + big_t) / 100.0)
        }

        pub fn grade_str(&self) -> &'static str {
            match self.grade {
                PlayerGrade::GradeUnset => "N/A",
                PlayerGrade::GradeC => "C",
                PlayerGrade::GradeB => "B",
                PlayerGrade::GradeA => "A",
                PlayerGrade::GradeAa => "AA",
                PlayerGrade::GradeAaa => "AAA",
            }
        }

        pub fn current_amount(&self) -> String {
            parse_num_with_unit(self.current_amount)
        }

        pub fn target_amount(&self) -> String {
            parse_num_with_unit(self.spec.goal3())
        }

        fn get_timestamp_offset(original_timestamp: f64, cache_timestamp: Option<i64>) -> f64 {
            let current = kstool::time::get_current_second() as f64;
            if original_timestamp > 100000000.0 {
                return current - original_timestamp;
            }

            let Some(cache_timestamp) = cache_timestamp else {
                return original_timestamp;
            };

            let remain_time_point = cache_timestamp as f64 + original_timestamp;

            remain_time_point - current
        }

        pub fn expect_finish_time(&self, cache_timestamp: Option<i64>) -> f64 {
            Self::get_timestamp_offset(self.expect_remain_time, cache_timestamp)
        }

        pub fn contract_remain_time(&self, cache_timestamp: Option<i64>) -> f64 {
            Self::get_timestamp_offset(self.contract_remain_time, cache_timestamp)
        }

        pub fn is_finished(&self) -> bool {
            matches!(
                self.status,
                CompletionLevel::Completed | CompletionLevel::Cleared
            )
        }

        pub fn is_cleared(&self) -> bool {
            matches!(self.status, CompletionLevel::Cleared)
        }

        pub fn emoji(&self) -> String {
            self.status.to_string()
        }

        pub fn member(&self) -> &[UserScore] {
            &self.member
        }

        pub fn completion_time(&self) -> f64 {
            self.completion_time
        }

        pub fn total_known_elr(&self) -> String {
            parse_num_with_unit(
                self.member
                    .iter()
                    .filter_map(|x| x.egg_laying_rate)
                    .sum::<f64>()
                    * 3600.0,
            )
        }

        fn total_buff(&self) -> (f64, f64) {
            self.member.iter().fold((0.0, 0.0), |(mut eb, mut lb), x| {
                eb += x.coop_buff.0;
                lb += x.coop_buff.1;
                (eb, lb)
            })
        }

        pub fn display_buff(&self) -> String {
            let (e, l) = self.total_buff();
            format!("E: {e:.0}%, L: {l:.0}%")
        }
    }
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
