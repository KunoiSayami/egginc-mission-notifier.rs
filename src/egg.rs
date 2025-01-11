mod definitions {

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
        "", "K", "M", "B", "T", "q", "Q", "s", "S", "o", "N", "d", "U", "D", "Td", "qd", "Qd",
        "sd", "Sd", "Od", "Nd", "V", "uV", "dV", "tV", "qV", "QV", "sV", "SV", "OV", "NV", "tT",
    ];

    pub(super) const DEFAULT_UNIT: &str = "A Lot";

    pub(super) const API_BACKEND: &str = determine_api();

    const fn determine_api() -> &'static str {
        match option_env!("API_BACKEND") {
            Some(s) => s,
            None => &DEFAULT_API_BACKEND,
        }
    }
}
#[allow(clippy::enum_variant_names)]
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/ei.rs"));
}

pub mod functions {

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
    pub fn build_coop_status_request(
        contract_id: &str,
        coop_id: &str,
        ei: Option<String>,
    ) -> String {
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
            .post(API_BACKEND)
            .form(&form)
            .send()
            .await?
            .error_for_status()?;
        let data = decode_data(&resp.text().await?, false)?;
        Ok(data)
    }
}

pub mod types {
    use serde::{Deserialize, Serialize};

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

    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    pub struct ContractGradeSpec {
        grade: i32,
        length: f64,
        goal1: f64,
        goal3: f64,
    }

    impl ContractGradeSpec {
        fn extract_goal(goal: &super::proto::contract::Goal) -> f64 {
            goal.target_amount()
        }

        pub fn into_kv(self) -> (super::proto::contract::PlayerGrade, Self) {
            (
                match self.grade {
                    1 => super::proto::contract::PlayerGrade::GradeC,
                    2 => super::proto::contract::PlayerGrade::GradeB,
                    3 => super::proto::contract::PlayerGrade::GradeA,
                    4 => super::proto::contract::PlayerGrade::GradeAa,
                    5 => super::proto::contract::PlayerGrade::GradeAaa,
                    _ => super::proto::contract::PlayerGrade::GradeUnset,
                },
                self,
            )
        }

        pub fn length(&self) -> f64 {
            self.length
        }

        pub fn goal1(&self) -> f64 {
            self.goal1
        }
        pub fn goal3(&self) -> f64 {
            self.goal3
        }
    }

    impl From<&super::proto::contract::GradeSpec> for ContractGradeSpec {
        fn from(value: &super::proto::contract::GradeSpec) -> Self {
            Self {
                grade: value.grade() as i32,
                length: value.length_seconds(),
                goal1: value
                    .goals
                    .first()
                    .map(Self::extract_goal)
                    .unwrap_or_default(),
                goal3: value
                    .goals
                    .last()
                    .map(Self::extract_goal)
                    .unwrap_or_default(),
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
    use crate::types::{convert_set, timestamp_to_string, AccountMap, ContractSpec, SpaceShip};
    use crate::CACHE_REQUEST_OFFSET;
    use crate::{
        bot::BotType, database::DatabaseHelper, types::Account, CACHE_REFRESH_PERIOD, CHECK_PERIOD,
        FETCH_PERIOD,
    };

    use super::functions::{get_missions, request};
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
        ) {
            let Some(contracts) = info
                .backup
                .as_ref()
                .and_then(|x| x.contracts.as_ref())
                .map(|x| &x.contracts)
            else {
                return;
            };

            for local_contract in contracts {
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
                database
                    .account_insert_contract(
                        contract.identifier().into(),
                        local_contract.coop_identifier().into(),
                        ei.into(),
                        contract.start_time(),
                        true,
                    )
                    .await;
                log::trace!(
                    "{ei} found contract {} {}",
                    contract.identifier(),
                    local_contract.coop_identifier()
                );
            }
            let Some(contracts) = info
                .backup
                .as_ref()
                .and_then(|x| x.contracts.as_ref())
                .map(|x| &x.current_coop_statuses)
            else {
                return;
            };
            for contract in contracts {
                database
                    .contract_cache_insert(
                        contract.contract_identifier().into(),
                        contract.coop_identifier().into(),
                        super::functions::encode_to_byte(contract),
                    )
                    .await;
                database
                    .contract_update(
                        contract.contract_identifier().into(),
                        ei.into(),
                        contract.cleared_for_exit(),
                    )
                    .await;
                log::trace!(
                    "{ei} found online contract {} {} {}",
                    contract.contract_identifier(),
                    contract.coop_identifier(),
                    contract.cleared_for_exit()
                );
            }
        }

        async fn handle_each_account(
            client: &Client,
            current_time: i64,
            account: &Account,
            database: &DatabaseHelper,
            bot: &BotType,
            helper: &MonitorHelper,
        ) -> anyhow::Result<bool> {
            if !account.force_fetch(current_time)
                && database
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

            if account.contract_trace() {
                Self::inject_contracts(account.ei(), database, &info).await;
            }
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

            helper.refresh_cache(false).await;

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
            let current_time = kstool::time::get_current_second();
            LAST_QUERY.store(current_time, std::sync::atomic::Ordering::Relaxed);

            let client = Client::builder().timeout(Duration::from_secs(10)).build()?;

            for account in database
                .account_query(None)
                .await
                .ok_or_else(|| anyhow!("Query database accounts failure"))?
            {
                //log::debug!("Start query user {}", player.ei());
                if account.disabled()
                    || (!account.force_fetch(current_time as i64)
                        && current_time as i64 - account.last_fetch()
                            < (*FETCH_PERIOD.get().unwrap()))
                {
                    //log::debug!("Skip user {}", account.ei());
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
}

mod coop {
    use anyhow::anyhow;
    use chrono::TimeDelta;
    use reqwest::Client;
    use std::{collections::HashMap, sync::LazyLock};

    use crate::{
        bot::replace_all,
        egg::functions::{build_coop_status_request, parse_num_with_unit},
        types::{fmt_time_delta, ContractSpec},
    };

    use super::{
        definitions::{API_BACKEND, UNIT},
        functions::decode_data,
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
        let (completion_time, expect_remain_time, remain_time) = if !coop.all_goals_achieved() {
            let remain = goal3 - coop.total_amount();
            let (total_elr, offline_egg) = coop
                .contributors
                .iter()
                .filter(|x| x.production_params.is_some() && x.farm_info.is_some())
                .fold((0.0, 0.0), |(mut acc, mut acc2), x| {
                    let elr = x.production_params.as_ref().unwrap().elr();
                    acc += elr;
                    // offline laying
                    acc2 += x.farm_info.as_ref().unwrap().timestamp().abs() * elr;
                    (acc, acc2)
                });
            let expect_remain_time = (remain - offline_egg) / total_elr / 0.8;
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

        let big_g = match coop.grade() as proto::contract::PlayerGrade {
            proto::contract::PlayerGrade::GradeUnset => 1.0,
            proto::contract::PlayerGrade::GradeC => 1.0,
            proto::contract::PlayerGrade::GradeB => 2.0,
            proto::contract::PlayerGrade::GradeA => 3.5,
            proto::contract::PlayerGrade::GradeAa => 5.0,
            proto::contract::PlayerGrade::GradeAaa => 7.0,
        };

        let mut output = vec![];

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
            );
            output.push(format!(
                "{} elr: {} shipped: {}  {score:.2}",
                replace_all(player.user_name()),
                replace_all(&pu(s2h(production.elr() * production.farm_population()))),
                replace_all(&pu(player.production_params.as_ref().unwrap().delivered())),
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
        let user_total_delivered = production.delivered() + production.elr() * expect_remain_time;
        let ratio =
            (user_total_delivered * coop_size as f64) / goal3.min(goal1.max(total_delivered));

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
            * (1.0 + 4.0 * (remain_time / coop_total_time).powi(3))
        //* (1.0 + (big_b + big_r + big_t) / 100.0)
    }

    pub fn decode_2(spec: ContractSpec, data: &[u8]) -> anyhow::Result<String> {
        let res: proto::ContractCoopStatusResponse = decode_data(data, true)?;
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
}

pub use coop::{decode_2, query_coop_status};
