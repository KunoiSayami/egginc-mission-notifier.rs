use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::sync::LazyLock;
use std::{collections::HashMap, time::Duration};

use anyhow::anyhow;
use chrono::TimeDelta;
use itertools::Itertools;
use kstool_helper_generator::Helper;
use reqwest::Client;
use tap::TapOptional as _;
use teloxide::prelude::Requester;
use tokio::{task::JoinHandle, time::interval};

use crate::bot::replace_all;
use crate::egg::functions::parse_num_with_unit;
use crate::functions::build_reqwest_client;
use crate::types::{
    convert_set, fmt_time_delta_short, timestamp_to_string, AccountMap, ContractSpec, QueryError,
    SpaceShip,
};
use crate::CACHE_REQUEST_OFFSET;
use crate::{
    bot::BotType, database::DatabaseHelper, types::Account, CACHE_REFRESH_PERIOD, CHECK_PERIOD,
    FETCH_PERIOD,
};

use super::functions::{decode_data, get_missions, request};
use super::proto::ContractCoopStatusResponse;
use super::types::ContractGradeSpec;

pub static LAST_QUERY: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));

#[derive(Clone, Debug, Helper)]
pub enum MonitorEvent {
    NewClient,
    RefreshCache { invalidate: bool },
    InsertCache { ei: String, land_times: Vec<i64> },
    Exit,
}

pub struct Monitor {
    handle: JoinHandle<anyhow::Result<()>>,
}

impl Monitor {
    pub fn create(database: DatabaseHelper, bot: BotType) -> (Self, MonitorHelper) {
        let (s, r) = MonitorHelper::new(4);
        (
            Self {
                handle: tokio::spawn(Self::run(s.clone(), database, r, bot)),
            },
            s,
        )
    }

    async fn refresh_cache(
        cache: &mut BTreeMap<i64, HashSet<SpaceShip>>,
        database: &DatabaseHelper,
    ) -> anyhow::Result<()> {
        let missions = database
            .mission_query(kstool::time::get_current_second() + CACHE_REQUEST_OFFSET)
            .await
            .ok_or_else(|| anyhow!("Query database mission error"))?;

        for mission in missions {
            cache
                .entry(mission.land())
                .or_insert_with(|| HashSet::with_capacity(1))
                .insert(mission);
        }
        /* if !cache.is_empty() {
            log::debug!(
                "Cache refreshed: {}",
                cache
                    .iter()
                    .map(|(k, v)| v
                        .iter()
                        .map(|s| format!(
                            "{k} {} {}",
                            s.belong(),
                            timestamp_to_string(s.land())
                        ))
                        .join("; "))
                    .join("; ")
            );
        } */
        Ok(())
    }

    async fn run(
        self_helper: MonitorHelper,
        database: DatabaseHelper,
        mut helper: MonitorEventReceiver,
        bot: BotType,
    ) -> anyhow::Result<()> {
        let mut query_timer = interval(Duration::from_secs(*(CHECK_PERIOD.get().unwrap()) as u64));
        let mut notify_timer = interval(Duration::from_secs(3));
        let mut clear_timer = interval(Duration::from_secs(43200));
        let mut cache_refresh_timer = interval(Duration::from_secs(CACHE_REFRESH_PERIOD));
        clear_timer.reset();

        let mut cache = BTreeMap::new();

        let mut works = Vec::new();

        loop {
            tokio::select! {
                Some(event) = helper.recv() => {
                    match event {
                        MonitorEvent::NewClient => {
                            query_timer.reset_immediately();
                            cache_refresh_timer.reset_immediately();
                            continue;
                        }
                        MonitorEvent::RefreshCache { invalidate } => {
                            if invalidate {
                                cache.clear();
                            }
                            cache_refresh_timer.reset_immediately();
                            continue;
                        }
                        MonitorEvent::InsertCache { ei, land_times } => {
                            Self::insert_fake_spaceships(&mut cache, ei, &land_times);
                            continue;
                        }
                        MonitorEvent::Exit => break,
                    }
                }

                _ = query_timer.tick() => {
                    let database = database.clone();
                    let bot = bot.clone();
                    let helper = self_helper.clone();
                    works.push(tokio::spawn(async move {
                        Self::query(database, bot, helper).await
                            .inspect_err(|e| log::error!("Query function error: {e:?}"))
                            .ok();
                    }));
                }

                _ = notify_timer.tick() => {
                    let current_time = kstool::time::get_current_second() as i64;
                    Self::notify(&Self::split_mission(&mut cache, current_time), &database, &bot).await
                        .inspect_err(|e| log::error!("Notify error: {e:?}"))
                        .ok();
                }

                _ = cache_refresh_timer.tick() => {
                    Self::refresh_cache(&mut cache, &database).await
                        .inspect_err(|e| log::error!("Refresh cache error: {e:?}"))
                        .ok();
                }

                _ = clear_timer.tick() => {
                    let before = works.len();
                    let alt = std::mem::take(&mut works);
                    works.extend(alt.into_iter().filter(|s| !s.is_finished()));
                    log::trace!("[GC] Clear {} of {} works", before - works.len(), before);
                }
            }
        }

        match tokio::time::timeout(Duration::from_secs(5), async {
            for handle in works {
                handle.await?;
            }
            Ok::<_, tokio::task::JoinError>(())
        })
        .await
        {
            Ok(ret) => ret?,
            Err(_) => {
                log::error!("Wait querier timeout");
            }
        }

        Ok(())
    }

    fn insert_fake_spaceships(
        cache: &mut BTreeMap<i64, HashSet<SpaceShip>>,
        ei: String,
        land_times: &[i64],
    ) {
        let current = kstool::time::get_current_second() as i64;
        for land_time in land_times {
            let land_time = current + land_time;
            cache
                .entry(land_time)
                .or_default()
                .insert(SpaceShip::random(ei.clone(), land_time));
        }
    }

    async fn check_username(
        account: &Account,
        database: &DatabaseHelper,
        account_map: &AccountMap,
        bot: &BotType,
        backup: &Option<super::proto::Backup>,
    ) -> anyhow::Result<()> {
        let Some(username) = backup.as_ref().map(|s| s.user_name().to_string()) else {
            return Err(anyhow!("Backup is empty"));
        };

        if account.nickname().is_none_or(|name| !name.eq(&username)) {
            let msg = format!(
                "User _{}_ changed their name from _{}_ to _{}_",
                account.ei(),
                replace_all(account.name()),
                replace_all(&username)
            );
            database
                .account_name_update(account.ei().to_string(), username)
                .await;
            for chat in account_map.chat_ids() {
                bot.send_message(chat, &msg).await?;
            }
        }

        Ok(())
    }

    async fn inject_contracts(
        ei: &str,
        database: &DatabaseHelper,
        info: &super::proto::EggIncFirstContactResponse,
    ) -> Option<()> {
        /* {
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open("current.data")
                .await
                .ok();
            if let Some(mut file) = file {
                file.write_all(format!("{info:#?}").as_bytes()).await.ok();
            }
        } */

        let backup = info.backup.as_ref()?;
        let backup_timestamp = backup.approx_time();

        let contracts = backup.contracts.as_ref()?;

        for local_contract in contracts.contracts.iter().chain(contracts.archive.iter()) {
            let Some(ref contract) = local_contract.contract else {
                continue;
            };
            let v = contract
                .grade_specs
                .iter()
                .map(ContractGradeSpec::from)
                .collect_vec();
            database
                .contract_spec_insert(ContractSpec::new(
                    contract.identifier().to_string(),
                    contract.max_coop_size() as i64,
                    contract.minutes_per_token(),
                    v,
                ))
                .await;
            if database
                .account_insert_contract(
                    contract.identifier().into(),
                    local_contract.coop_identifier().into(),
                    ei.into(),
                    true,
                )
                .await
                .unwrap_or(false)
            {
                continue;
            }
            /* log_output.push(format!(
                "{}: {}",
                contract.identifier(),
                local_contract.coop_identifier()
            )); */
        }
        //log::trace!("{ei} found contract: {}", log_output.join("; "));

        //log_output.clear();
        let mut log_output = vec![];

        let contracts = &backup.contracts.as_ref()?.current_coop_statuses;

        for contract in contracts {
            let amount = contract.total_amount();
            let remain = contract.seconds_remaining();
            let seen = database
                .contract_cache_insert(
                    contract.contract_identifier().into(),
                    contract.coop_identifier().into(),
                    super::functions::encode_to_byte(contract),
                    contract.cleared_for_exit() || contract.all_members_reporting(),
                    Some(backup_timestamp as i64),
                    Some((
                        (amount, remain, backup_timestamp as i64),
                        |(original, original_timestamp), (amount, remain, backup_timestamp)| {
                            decode_data::<_, ContractCoopStatusResponse>(original, false).is_ok_and(
                                |x| {
                                    let ret = x.total_amount() <= amount
                                        && original_timestamp <= backup_timestamp;
                                    //&& remain <= x.seconds_remaining()
                                    log::trace!(
                                        "amount: {}, {} remain: {} {} {} {}, final result: {}",
                                        parse_num_with_unit(x.total_amount()),
                                        parse_num_with_unit(amount),
                                        fmt_time_delta_short(TimeDelta::seconds(
                                            x.seconds_remaining() as i64
                                        )),
                                        fmt_time_delta_short(TimeDelta::seconds(remain as i64)),
                                        amount > x.total_amount(),
                                        remain < x.seconds_remaining(),
                                        ret
                                    );
                                    ret
                                },
                            )
                        },
                    )),
                )
                .await
                .unwrap_or(false);
            database
                .contract_update(
                    contract.contract_identifier().into(),
                    contract.coop_identifier().into(),
                    ei.into(),
                    contract.cleared_for_exit() || contract.all_members_reporting(),
                )
                .await;
            if let Some(spec) = database
                .contract_query_spec(contract.contract_identifier().into())
                .await
                .flatten()
            {
                if let Some(spec) = spec.get(&contract.grade()) {
                    database
                        .contract_start_time_update(
                            contract.contract_identifier().into(),
                            contract.coop_identifier().into(),
                            kstool::time::get_current_second() as f64
                                - (spec.length() - contract.seconds_remaining()),
                        )
                        .await;
                }
            }
            if seen {
                continue;
            }
            log_output.push(format!(
                "{}: {}{}",
                contract.contract_identifier(),
                contract.coop_identifier(),
                if contract.cleared_for_exit() {
                    " finished"
                } else {
                    ""
                }
            ))
        }

        if !log_output.is_empty() {
            log::trace!("{ei} found online contract: {}", log_output.join("; "))
        }
        Some(())
    }

    async fn handle_each_account(
        client: &Client,
        current_time: i64,
        account: &Account,
        database: &DatabaseHelper,
        bot: &BotType,
        helper: &MonitorHelper,
    ) -> Result<bool, QueryError> {
        if !account.force_fetch(current_time)
            && database
                .mission_query_by_account(account.ei().to_string())
                .await
                .ok_or_else(|| anyhow!("Query player mission failed"))?
                .into_iter()
                .filter(|s| !s.notified())
                .count()
                >= 3
        {
            return Ok(false);
        };

        let Some(account_map) = database.account_query_users(account.ei().to_string()).await else {
            log::error!("Account map is empty, skip {}", account.ei());
            return Ok(false);
        };

        let info = request(client, account.ei()).await?;

        if account.contract_trace() {
            Self::inject_contracts(account.ei(), database, &info).await;
        }
        Self::check_username(account, database, &account_map, bot, &info.backup).await?;

        let Some(missions) = get_missions(info) else {
            return Err(anyhow!("Player {} missions field is missing", account.ei()).into());
            //bot.send_message(user.user().into(), "Query mission failure");
        };

        //log::trace!("{}({}) missions {missions:?}", account.name(), account.ei());

        let mut pending = Vec::new();

        for mission in missions {
            if mission.is_landed() {
                continue;
            }
            if database
                .mission_single_query(mission.id().to_string())
                .await
                .flatten()
                .is_some()
            {
                continue;
            }
            database
                .mission_add(
                    mission.id().to_string(),
                    mission.name().to_string(),
                    mission.duration_type(),
                    account.ei().to_string(),
                    mission.land(),
                )
                .await;
            pending.push(format!(
                "{} \\[{}\\] \\(_{}_\\), launch time: {}, land time: {}",
                replace_all(mission.name()),
                SpaceShip::duration_type_to_str(mission.duration_type()),
                replace_all(mission.id()),
                replace_all(&timestamp_to_string(mission.launched())),
                replace_all(&timestamp_to_string(mission.land()))
            ));
        }

        if pending.is_empty() {
            return Ok(true);
        }

        helper.refresh_cache(false).await;

        for user in account_map.chat_ids() {
            bot.send_message(
                user,
                format!(
                    "*{}* Found new spaceship:\n{}",
                    replace_all(account.name()),
                    pending.join("\n")
                ),
            )
            .await
            .map_err(|e| anyhow::Error::from(e))?;
        }

        Ok(true)
    }

    async fn query(
        database: DatabaseHelper,
        bot: BotType,
        helper: MonitorHelper,
    ) -> anyhow::Result<()> {
        let current_time = kstool::time::get_current_second();
        LAST_QUERY.store(current_time, std::sync::atomic::Ordering::Relaxed);

        let client = build_reqwest_client();

        for account in database
            .account_query(None)
            .await
            .ok_or_else(|| anyhow!("Query database accounts failure"))?
        {
            //log::debug!("Start query user {}", player.ei());
            if account.disabled()
                || (!account.force_fetch(current_time as i64)
                    && current_time as i64 - account.last_fetch() < (*FETCH_PERIOD.get().unwrap()))
            {
                //log::trace!("Skip user {}", account.ei());
                continue;
            }
            let ret = Self::handle_each_account(
                &client,
                current_time as i64,
                &account,
                &database,
                &bot,
                &helper,
            )
            .await
            .inspect_err(|e| log::error!("Remote query user got error: {e:?}"));

            let fetched = *ret.as_ref().unwrap_or(&true);

            let is_err = ret.is_err();
            let is_user_error = ret.is_err_and(|x| x.is_user_error());

            if fetched {
                database
                    .account_update(account.ei().to_string(), is_user_error)
                    .await;
            }
            if is_err {
                for user in database
                    .account_query_users(account.ei().to_string())
                    .await
                    .ok_or_else(|| anyhow!("Unable query database account map"))?
                    .chat_ids()
                {
                    bot.send_message(
                        user,
                        if is_user_error {
                            format!("Remote query got error, disable `{}`", account.ei())
                        } else {
                            format!("Remote query got error, please check `{}`", account.ei())
                        },
                    )
                    .await
                    .inspect_err(|e| log::error!("Send message to user {} error: {e:?}", user.0))
                    .ok();
                }
            }
            //log::debug!("Query {} finished", player.ei());
        }
        Ok(())
    }

    fn split_mission(
        cache: &mut BTreeMap<i64, HashSet<SpaceShip>>,
        current_time: i64,
    ) -> Vec<SpaceShip> {
        if cache
            .first_key_value()
            .is_none_or(|(key, _)| key > &current_time)
        {
            return vec![];
        }

        if cache.last_key_value().is_some_and(|(key, _)| {
            //log::debug!("{key} {current_time} {}", timestamp_to_string(*key));
            key <= &current_time
        }) {
            //log::debug!("All cache value returned");
            return convert_set(std::mem::take(cache).into_values().collect_vec());
        }

        let mut board = None;
        for key in cache.keys() {
            if key <= &current_time {
                continue;
            }
            board.replace(*key);
            //log::debug!("Select {key} {}", timestamp_to_string(*key));
            break;
        }
        let Some(board) = board else {
            log::warn!("Board not found but cache is not empty, return default. Board: {current_time}, BtreeMap: {cache:?}");
            return vec![];
        };
        let tmp = cache.split_off(&board);
        convert_set(std::mem::replace(cache, tmp).into_values().collect_vec())
    }

    async fn notify(
        missions: &[SpaceShip],
        database: &DatabaseHelper,
        bot: &BotType,
    ) -> anyhow::Result<()> {
        if missions.is_empty() {
            return Ok(());
        }
        //log::debug!("Notify missions: {missions:?}");

        let mut pending = HashMap::new();
        for mission in missions {
            pending
                .entry(mission.belong())
                .or_insert_with(Vec::new)
                .push(mission);
        }

        let mut msg_map = HashMap::new();

        for (ei, missions) in pending {
            let Some(account) = database
                .account_query_ei(ei.to_string())
                .await
                .tap_none(|| log::error!("Query database players {ei} error"))
                .flatten()
            else {
                continue;
            };

            let Some(account_map) = database
                .account_query_users(ei.to_string())
                .await
                .tap_none(|| log::error!("Query database map {ei} error"))
            else {
                continue;
            };
            for spaceship in &missions {
                database.mission_updated(spaceship.id().to_string()).await;
            }
            for user in account_map.chat_ids() {
                msg_map.entry(user).or_insert_with(Vec::new).push(format!(
                    "*{}*:\n{}",
                    replace_all(account.name()),
                    missions
                        .iter()
                        .map(|s| format!("__{}__ returned\\!", replace_all(s.name())))
                        .join("\n"),
                ));
            }
        }

        for (player, msg) in msg_map {
            bot.send_message(player, msg.join("\n\n")).await?;
        }
        Ok(())
    }

    pub async fn join(self) -> anyhow::Result<()> {
        self.handle.await?
    }
}
