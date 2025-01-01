mod definitions {

    pub(super) const VERSION: &str = "1.33.6";
    pub(super) const BUILD: &str = "1.33.6.0";
    pub(super) const VERSION_NUM: u32 = 67;
    pub(super) const PLATFORM_STRING: &str = "IOS";
    pub(super) const DEVICE_ID: &str = "egginc-bot";
    pub(super) const PLATFORM: i32 = super::proto::Platform::Ios as i32;
    pub(super) const API_BACKEND: &str =
        "https://ctx-dot-auxbrainhome.appspot.com/ei/bot_first_contact";
}
#[allow(clippy::enum_variant_names)]
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/ei.rs"));
}

pub mod functions {

    use std::{collections::HashMap, io::Cursor};

    use anyhow::anyhow;
    use base64::{prelude::BASE64_STANDARD, Engine};
    use prost::Message;
    use reqwest::Client;

    use super::definitions::*;
    use super::proto;
    use super::types::SpaceShipInfo;

    // Source: https://github.com/carpetsage/egg/blob/78cd2bdd7e020a3364e5575884135890cc01105c/lib/api/index.ts
    pub fn build_request(ei: String) -> String {
        let request = proto::EggIncFirstContactRequest {
            rinfo: Some(proto::BasicRequestInfo {
                ei_user_id: Some("".into()),
                client_version: Some(VERSION_NUM),
                version: Some(VERSION.into()),
                build: Some(BUILD.into()),
                platform: Some(PLATFORM_STRING.into()),
                country: None,
                language: None,
                debug: Some(false),
            }),
            ei_user_id: Some(ei),
            user_id: None,
            game_services_id: None,
            device_id: Some(DEVICE_ID.into()),
            username: None,
            client_version: Some(VERSION_NUM),
            platform: Some(PLATFORM),
        };

        let mut v = Vec::with_capacity(request.encoded_len());

        request.encode(&mut v).unwrap();

        BASE64_STANDARD.encode(&v)
    }

    pub fn decode_data<T: AsRef<[u8]>>(
        base64_encoded: T,
    ) -> anyhow::Result<proto::EggIncFirstContactResponse> {
        let raw = BASE64_STANDARD.decode(base64_encoded)?;
        proto::EggIncFirstContactResponse::decode(&mut Cursor::new(raw))
            .map_err(|e| anyhow!("Decode user data error: {e:?}"))
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
        let form = [("data", build_request(ei.to_string()))]
            .into_iter()
            .collect::<HashMap<_, _>>();
        let resp = client
            .post(API_BACKEND)
            .form(&form)
            .send()
            .await?
            .error_for_status()?;
        let data = decode_data(&resp.text().await?)?;
        Ok(data)
    }
}

pub mod types {

    #[derive(Clone, Debug)]
    pub struct SpaceShipInfo {
        name: String,
        id: String,
        duration_type: i64,
        duration: i64,
        launched: i64,
    }

    impl SpaceShipInfo {
        pub fn id(&self) -> &str {
            &self.id
        }
        pub fn name(&self) -> &str {
            &self.name
        }
        pub fn duration(&self) -> i64 {
            self.duration
        }
        pub fn duration_type(&self) -> i64 {
            self.duration_type
        }
        pub fn launched(&self) -> i64 {
            self.launched
        }
        pub fn land(&self) -> i64 {
            self.duration() + self.launched()
        }
        pub fn is_landed(&self) -> bool {
            kstool::time::get_current_second() as i64 > self.land()
        }

        pub fn ship_friendly_name(ship: super::proto::mission_info::Spaceship) -> &'static str {
            use super::proto::mission_info::Spaceship;
            #[allow(non_snake_case)]
            match ship {
                Spaceship::ChickenOne => "Chicken One",
                Spaceship::ChickenNine => "Chicken Nine",
                Spaceship::ChickenHeavy => "Chicken Heavy",
                Spaceship::Bcr => "BCR",
                Spaceship::MilleniumChicken => "Quintillion Chicken",
                Spaceship::CorellihenCorvette => "Cornish-Hen Corvette",
                Spaceship::Galeggtica => "Galeggtica",
                Spaceship::Chickfiant => "Defihent",
                Spaceship::Voyegger => "Voyegger",
                Spaceship::Henerprise => "Henerprise",
                Spaceship::Atreggies => "Atreggies Henliner",
            }
        }
    }

    impl From<super::proto::MissionInfo> for SpaceShipInfo {
        fn from(value: super::proto::MissionInfo) -> Self {
            Self {
                name: Self::ship_friendly_name(value.ship()).to_string(),
                id: value.identifier().to_string(),
                duration_type: value.duration_type() as i64,
                duration: value.duration_seconds() as i64,
                launched: value.start_time_derived() as i64,
            }
        }
    }
}

pub mod monitor {
    use std::collections::{BTreeMap, HashSet};
    use std::sync::atomic::AtomicU64;
    use std::sync::LazyLock;
    use std::{collections::HashMap, time::Duration};

    use anyhow::anyhow;
    use itertools::Itertools;
    use kstool_helper_generator::Helper;
    use reqwest::Client;
    use tap::TapOptional as _;
    use teloxide::prelude::Requester;
    use tokio::{task::JoinHandle, time::interval};

    use crate::bot::replace_all;
    use crate::types::{convert_set, timestamp_to_string, AccountMap, SpaceShip};
    use crate::CACHE_REQUEST_OFFSET;
    use crate::{
        bot::BotType, database::DatabaseHelper, types::Account, CACHE_REFRESH_PERIOD, CHECK_PERIOD,
        FETCH_PERIOD,
    };

    use super::functions::{get_missions, request};

    pub static LAST_QUERY: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));

    #[derive(Clone, Debug, Helper, PartialEq)]
    pub enum MonitorEvent {
        NewClient,
        RefreshCache,
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
                cache.entry(mission.land()).or_default().insert(mission);
            }
            log::debug!(
                "Cache refreshed: {}",
                cache
                    .values()
                    .map(|v| v
                        .iter()
                        .map(|s| format!("{} {}", s.belong(), timestamp_to_string(s.land())))
                        .join("; "))
                    .join("; ")
            );
            Ok(())
        }

        async fn run(
            self_helper: MonitorHelper,
            database: DatabaseHelper,
            mut helper: MonitorEventReceiver,
            bot: BotType,
        ) -> anyhow::Result<()> {
            let mut query_timer =
                interval(Duration::from_secs(*(CHECK_PERIOD.get().unwrap()) as u64));
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
                            },
                            MonitorEvent::RefreshCache => {
                                cache_refresh_timer.reset_immediately();
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

        async fn handle_each_account(
            client: &Client,
            account: &Account,
            database: &DatabaseHelper,
            bot: &BotType,
            helper: &MonitorHelper,
        ) -> anyhow::Result<bool> {
            if database
                .mission_query_by_account(account.ei().to_string())
                .await
                .ok_or_else(|| anyhow!("Query player mission failed"))?
                .into_iter()
                .filter(|s| !s.notified())
                .count()
                == 3
            {
                return Ok(false);
            };

            let Some(account_map) = database.account_query_users(account.ei().to_string()).await
            else {
                log::error!("Account map is empty, skip {}", account.ei());
                return Ok(false);
            };

            let info = request(client, account.ei()).await?;

            Self::check_username(account, database, &account_map, bot, &info.backup).await?;

            let Some(missions) = get_missions(info) else {
                return Err(anyhow!("Player {} missions field is missing", account.ei()));
                //bot.send_message(user.user().into(), "Query mission failure");
            };

            log::trace!("{}({}) missions {missions:?}", account.name(), account.ei());

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
                    "Found new spaceship: {} \\[{}\\] \\(_{}_\\), launch time: {}, land time: {}",
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

            helper.refresh_cache().await;

            for user in account_map.chat_ids() {
                bot.send_message(
                    user,
                    format!("*{}*:\n{}", replace_all(account.name()), pending.join("\n")),
                )
                .await?;
            }

            Ok(true)
        }

        async fn query(
            database: DatabaseHelper,
            bot: BotType,
            helper: MonitorHelper,
        ) -> anyhow::Result<()> {
            LAST_QUERY.store(
                kstool::time::get_current_second(),
                std::sync::atomic::Ordering::Relaxed,
            );

            let client = Client::builder().timeout(Duration::from_secs(10)).build()?;

            for account in database
                .account_query(None)
                .await
                .ok_or_else(|| anyhow!("Query database accounts failure"))?
            {
                //log::debug!("Start query user {}", player.ei());
                if account.disabled()
                    || kstool::time::get_current_second() as i64 - account.last_fetch()
                        < (*FETCH_PERIOD.get().unwrap())
                {
                    //log::debug!("Skip user {}", account.ei());
                    continue;
                }
                let ret = Self::handle_each_account(&client, &account, &database, &bot, &helper)
                    .await
                    .inspect_err(|e| log::error!("Remote query user got error: {e:?}"));

                if *ret.as_ref().unwrap_or(&true) {
                    database
                        .account_update(account.ei().to_string(), ret.is_err())
                        .await;
                }
                if ret.is_err() {
                    for user in database
                        .account_query_users(account.ei().to_string())
                        .await
                        .ok_or_else(|| anyhow!("Unable query database account map"))?
                        .chat_ids()
                    {
                        bot.send_message(
                            user,
                            format!("Remote query got error, disable {}", account.ei()),
                        )
                        .await
                        .inspect_err(|e| {
                            log::error!("Send message to user {} error: {e:?}", user.0)
                        })
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
            if cache.is_empty()
                || cache
                    .first_key_value()
                    .is_some_and(|(key, _)| key > &current_time)
            {
                return vec![];
            }

            if cache
                .last_key_value()
                .is_some_and(|(key, _)| key <= &current_time)
            {
                return convert_set(std::mem::take(cache).into_values().collect_vec());
            }

            let mut board = None;
            for key in cache.keys() {
                if key <= &current_time {
                    continue;
                }
                board.replace(*key);
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
            log::debug!("Notify missions: {missions:?}");

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
}
