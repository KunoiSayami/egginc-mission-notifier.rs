use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;
use std::sync::atomic::AtomicU64;
use std::{collections::HashMap, time::Duration};

use anyhow::anyhow;
use itertools::Itertools;
use kstool_helper_generator::Helper;
use reqwest::Client;
use teloxide::prelude::Requester;
use teloxide::types::ChatId;
use tokio::{task::JoinHandle, time::interval};

use crate::CACHE_REQUEST_OFFSET;
use crate::bot::replace_all;
use crate::database::types::{ContractSpec, SubscribeInfo, convert_set};
use crate::egg::coop::{CoopResult, calc_score};
use crate::egg::{encode_to_byte, query_coop_status};

use crate::functions::build_reqwest_client;
use crate::types::QueryError;
use crate::{CACHE_REFRESH_PERIOD, bot::BotType, database::DatabaseHelper};

pub static LAST_QUERY: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));

#[derive(Clone, Debug, Helper)]
pub enum ContractSubscriberEvent {
    NewContract,
    RefreshCache { invalidate: bool },
    InsertCache { user: i64, notify_times: Vec<i64> },
    Exit,
}

pub struct ContractSubscriber {
    handle: JoinHandle<anyhow::Result<()>>,
}

impl ContractSubscriber {
    pub fn create(database: DatabaseHelper, bot: BotType) -> (Self, ContractSubscriberHelper) {
        let (s, r) = ContractSubscriberHelper::new(4);
        (
            Self {
                handle: tokio::spawn(Self::run(s.clone(), database, r, bot)),
            },
            s,
        )
    }

    async fn refresh_cache(
        cache: &mut BTreeMap<i64, HashSet<SubscribeInfo>>,
        database: &DatabaseHelper,
    ) -> anyhow::Result<()> {
        let subscribes = database
            .subscribe_fetch(Some(
                (kstool::time::get_current_second() + CACHE_REQUEST_OFFSET * 2) as i64,
            ))
            .await
            .ok_or_else(|| anyhow!("Query database mission error"))?;

        for mission in subscribes {
            cache
                .entry(mission.est())
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
        self_helper: ContractSubscriberHelper,
        database: DatabaseHelper,
        mut helper: ContractSubscriberEventReceiver,
        bot: BotType,
    ) -> anyhow::Result<()> {
        let mut query_timer = interval(Duration::from_secs(600));
        let mut notify_timer = interval(Duration::from_secs(15));
        let mut clear_timer = interval(Duration::from_secs(43200));
        let mut cache_refresh_timer = interval(Duration::from_secs(CACHE_REFRESH_PERIOD * 2));
        clear_timer.reset();

        let mut cache = BTreeMap::new();

        let mut works = Vec::new();

        loop {
            tokio::select! {
                Some(event) = helper.recv() => {
                    match event {
                        ContractSubscriberEvent::NewContract => {
                            query_timer.reset_immediately();
                            cache_refresh_timer.reset_immediately();
                            continue;
                        }
                        ContractSubscriberEvent::RefreshCache { invalidate } => {
                            if invalidate {
                                cache.clear();
                            }
                            cache_refresh_timer.reset_immediately();
                            continue;
                        }
                        ContractSubscriberEvent::InsertCache { user, notify_times } => {
                            Self::insert_fake_contracts(&mut cache, user, &notify_times);
                            continue;
                        }
                        ContractSubscriberEvent::Exit => break,
                    }
                }

                _ = query_timer.tick() => {
                    let database = database.clone();
                    let bot = bot.clone();
                    let helper = self_helper.clone();
                    works.push(tokio::spawn(async move {
                        Self::query(database, bot, helper).await
                            .inspect_err(|e| log::error!("Subscribe query function error: {e:?}"))
                            .ok();
                    }));
                }

                _ = notify_timer.tick() => {
                    let current_time = kstool::time::get_current_second() as i64;
                    Self::notify(&Self::split_subscribe(&mut cache, current_time), &database, &bot).await
                        .inspect_err(|e| log::error!("Subscribe notify error: {e:?}"))
                        .ok();
                }

                _ = cache_refresh_timer.tick() => {
                    Self::refresh_cache(&mut cache, &database).await
                        .inspect_err(|e| log::error!("Subscribe refresh cache error: {e:?}"))
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

    // For debug purpose only
    fn insert_fake_contracts(
        cache: &mut BTreeMap<i64, HashSet<SubscribeInfo>>,
        user: i64,
        notify_times: &[i64],
    ) {
        let current = kstool::time::get_current_second() as i64;
        for notify_time in notify_times {
            let notify_time = current + notify_time;
            cache
                .entry(notify_time)
                .or_default()
                .insert(SubscribeInfo::random(vec![user], notify_time));
        }
        todo!()
    }

    fn calc_contract_fetch_interval(current_time: i64, subscribe: &SubscribeInfo) -> i64 {
        let diff = subscribe.est() - current_time;
        if diff > 3600 * 24 {
            return 3600 * 4;
        } else if diff > 3600 * 6 {
            return 3600 * 2;
        } else if diff > 3600 {
            return 3600;
        } else {
            return 1200;
        }
    }

    async fn handle_each_contract(
        client: &Client,
        spec: ContractSpec,
        subscribe: &SubscribeInfo,
        database: &DatabaseHelper,
        bot: &BotType,
        helper: &ContractSubscriberHelper,
    ) -> Result<bool, QueryError> {
        let current_time = kstool::time::get_current_second() as i64;

        if subscribe.users().is_empty() {
            log::error!(
                "Contract subscribes is empty skip {}/{}",
                subscribe.id(),
                subscribe.room()
            );
            return Ok(false);
        }

        let info = query_coop_status(client, subscribe.id(), subscribe.room(), None).await?;

        let bytes = encode_to_byte(&info);

        database
            .contract_cache_insert(
                subscribe.id().into(),
                subscribe.room().into(),
                bytes,
                info.cleared_for_exit(),
                None,
                None,
            )
            .await;

        let score = calc_score(spec, info)?;

        let est = match score {
            CoopResult::Normal(score) => {
                let est = score.expect_finish_time(Some(current_time)).floor() as i64;
                if (subscribe.est() - est).abs() > 30 {
                    helper.refresh_cache(false).await;

                    for user in subscribe.users() {
                        bot.send_message(
                            ChatId(*user),
                            format!(
                                "{}/{} update end time: {est}",
                                subscribe.id(),
                                subscribe.room()
                            ),
                        )
                        .await
                        .inspect_err(|e| log::error!("Send message to user {user} error: {e:?}"))
                        .ok();
                    }
                    Some(est)
                } else {
                    None
                }
            }
            CoopResult::OutOfTime(f) => {
                let est = f.floor() as i64;
                Some(est)
            }
        };

        if let Some(est) = est {
            database
                .subscribe_timestamp_update(subscribe.id().into(), subscribe.room().into(), est)
                .await;
        }

        Ok(true)
    }

    async fn query(
        database: DatabaseHelper,
        bot: BotType,
        helper: ContractSubscriberHelper,
    ) -> anyhow::Result<()> {
        let current_time = kstool::time::get_current_second();
        LAST_QUERY.store(current_time, std::sync::atomic::Ordering::Relaxed);

        let client = build_reqwest_client();

        for subscribed in database
            .subscribe_fetch(None)
            .await
            .ok_or_else(|| anyhow!("Query database subscribe failure"))?
        {
            let opt = database
                .contract_cache_timestamp_query(subscribed.id().into(), subscribed.room().into())
                .await
                .ok_or_else(|| anyhow!("Query contract cache failure"))?;

            let Some(spec) = database
                .contract_query_spec(subscribed.id().into())
                .await
                .ok_or_else(|| anyhow!("Query contract spec failure"))?
            else {
                log::warn!("Contract {} spec is empty, skip fetch", subscribed.id());
                continue;
            };

            let next_check = Self::calc_contract_fetch_interval(current_time as i64, &subscribed);

            if current_time as i64 - opt.unwrap_or(0) < next_check {
                // skip
                continue;
            }

            let is_err =
                Self::handle_each_contract(&client, spec, &subscribed, &database, &bot, &helper)
                    .await
                    .inspect_err(|e| log::error!("Remote query user got error: {e:?}"))
                    .is_err();

            if is_err {
                for user in subscribed.users() {
                    bot.send_message(
                        ChatId(*user),
                        format!("Query {}/{} Error", subscribed.id(), subscribed.room()),
                    )
                    .await
                    .inspect_err(|e| log::error!("Send message to user {user} error: {e:?}"))
                    .ok();
                }
            }
            //log::debug!("Query {} finished", player.ei());
        }
        Ok(())
    }

    fn split_subscribe(
        cache: &mut BTreeMap<i64, HashSet<SubscribeInfo>>,
        current_time: i64,
    ) -> Vec<SubscribeInfo> {
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
            log::warn!(
                "Board not found but cache is not empty, return default. Board: {current_time}, BtreeMap: {cache:?}"
            );
            return vec![];
        };
        let tmp = cache.split_off(&board);
        convert_set(std::mem::replace(cache, tmp).into_values().collect_vec())
    }

    async fn notify(
        contracts: &[SubscribeInfo],
        database: &DatabaseHelper,
        bot: &BotType,
    ) -> anyhow::Result<()> {
        if contracts.is_empty() {
            return Ok(());
        }
        //log::debug!("Notify missions: {missions:?}");
        let mut remap = HashMap::new();

        let mut pending = HashMap::new();
        for contract in contracts {
            for user in contract.users() {
                pending
                    .entry(*user)
                    .or_insert_with(HashSet::new)
                    .insert(contract);
            }
            remap
                .entry(contract.id())
                .or_insert_with(Vec::new)
                .push(contract.room());
        }

        let mut msg_map = HashMap::new();

        for (user, contracts) in pending {
            msg_map.insert(
                ChatId(user),
                contracts
                    .into_iter()
                    .map(|info| {
                        format!(
                            "{}/{} is finished",
                            replace_all(info.id()),
                            replace_all(info.room())
                        )
                    })
                    .join("\n"),
            );
        }

        for (player, msg) in msg_map {
            bot.send_message(player, format!("Contract subscribe:\n{msg}"))
                .await?;
        }
        for (contract, rooms) in remap {
            for room in rooms {
                database
                    .subscribe_notified(contract.into(), room.into())
                    .await;
            }
        }
        Ok(())
    }

    pub async fn join(self) -> anyhow::Result<()> {
        self.handle.await?
    }
}
