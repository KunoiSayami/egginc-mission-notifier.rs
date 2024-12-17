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

        let mut v = Vec::new();

        v.reserve(request.encoded_len());

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
        pub fn launched(&self) -> i64 {
            self.launched
        }
        pub fn land(&self) -> i64 {
            self.duration() + self.launched()
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

        pub fn duration_friendly_name(
            duration: super::proto::mission_info::DurationType,
        ) -> &'static str {
            use super::proto::mission_info::DurationType;
            match duration {
                DurationType::Short => "Short",
                DurationType::Long => "Long",
                DurationType::Epic => "Epic",
                DurationType::Tutorial => "Tutorial",
            }
        }
    }

    impl From<super::proto::MissionInfo> for SpaceShipInfo {
        fn from(value: super::proto::MissionInfo) -> Self {
            Self {
                name: format!(
                    "{} {}",
                    Self::duration_friendly_name(value.duration_type()),
                    Self::ship_friendly_name(value.ship())
                ),
                id: value.identifier().to_string(),
                duration: value.duration_seconds() as i64,
                launched: value.start_time_derived() as i64,
            }
        }
    }

    #[derive(Clone, Debug)]
    pub struct Username(String);
    impl From<&super::proto::Backup> for Username {
        fn from(value: &super::proto::Backup) -> Self {
            Self(value.user_name().into())
        }
    }

    impl From<Username> for String {
        fn from(value: Username) -> Self {
            value.0
        }
    }
}

pub mod monitor {
    use std::sync::atomic::AtomicU64;
    use std::sync::LazyLock;
    use std::{collections::HashMap, time::Duration};

    use anyhow::anyhow;
    use itertools::Itertools;
    use kstool_helper_generator::Helper;
    use reqwest::Client;
    use tap::TapOptional;
    use teloxide::prelude::Requester;
    use tokio::{task::JoinHandle, time::interval};

    use crate::bot::TELEGRAM_ESCAPE_RE;
    use crate::types::timestamp_to_string;
    use crate::{bot::BotType, database::DatabaseHelper, types::Player, FETCH_PERIOD};

    use super::functions::{get_missions, request};
    use super::types::Username;

    pub static LAST_QUERY: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));

    #[derive(Clone, Debug, Helper, PartialEq)]
    pub enum MonitorEvent {
        NewClient,
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
                    handle: tokio::spawn(Self::run(database, r, bot)),
                },
                s,
            )
        }

        async fn run(
            database: DatabaseHelper,
            mut helper: MonitorEventReceiver,
            bot: BotType,
        ) -> anyhow::Result<()> {
            let mut query_timer = interval(Duration::from_secs(FETCH_PERIOD as u64));
            let mut notify_timer = interval(Duration::from_secs(3));
            let mut clear_timer = interval(Duration::from_secs(43200));
            let mut works = Vec::new();

            loop {
                tokio::select! {
                    Some(event) = helper.recv() => {
                        match event {
                            MonitorEvent::NewClient => {
                                query_timer.reset_immediately();
                                continue;
                            },
                            MonitorEvent::Exit => break,
                        }
                    }

                    _ = query_timer.tick() => {
                        let database = database.clone();
                        let bot =  bot.clone();
                        works.push(tokio::spawn(async move {
                            Self::query(database, bot).await
                                .inspect_err(|e| log::error!("Query function error: {e:?}"))
                                .ok();
                        }));
                    }

                    _ = notify_timer.tick() => {
                        Self::notify(&database, &bot).await
                            .inspect_err(|e| log::error!("Notify error: {e:?}"))
                            .ok();
                    }

                    _ = clear_timer.tick() => {
                        let alt = std::mem::take(&mut works);
                        works.extend(alt.into_iter().filter(|s| !s.is_finished()));
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

        async fn handle_each_user(
            client: &Client,
            user: &Player,
            database: &DatabaseHelper,
            bot: &BotType,
        ) -> anyhow::Result<()> {
            if database
                .mission_query_by_player(user.ei().to_string())
                .await
                .ok_or_else(|| anyhow!("Query player mission failed"))?
                .into_iter()
                .filter(|s| !s.notified())
                .count()
                == 3
            {
                return Ok(());
            };

            let info = request(client, user.ei()).await?;

            if user.nickname().is_none() {
                if let Some(ref backup) = info.backup {
                    database
                        .player_name_update(user.ei().to_string(), Username::from(backup).into())
                        .await;
                }
            }

            let Some(missions) = get_missions(info) else {
                return Err(anyhow!("Player {} missions field is missing", user.ei()));
                //bot.send_message(user.user().into(), "Query mission failure");
            };

            log::trace!("{}({}) missions {missions:?}", user.name(), user.ei());

            let mut pending = Vec::new();

            for mission in missions {
                let mission_record = database
                    .mission_single_query(mission.id().to_string())
                    .await
                    .flatten();
                if mission_record.is_some() {
                    continue;
                }
                database
                    .mission_add(
                        mission.id().to_string(),
                        mission.name().to_string(),
                        user.ei().to_string(),
                        mission.land(),
                    )
                    .await;
                pending.push(format!(
                    "Found new spaceship: {}(__{}__), launch time: {}, land time: {}",
                    mission.name(),
                    mission.id(),
                    timestamp_to_string(mission.launched()),
                    timestamp_to_string(mission.land())
                ));
            }

            if pending.is_empty() {
                return Ok(());
            }

            bot.send_message(
                user.chat_id(),
                TELEGRAM_ESCAPE_RE.replace_all(
                    &format!("*{}*:\n{}", user.name(), pending.join("\n")),
                    "\\$1",
                ),
            )
            .await?;

            Ok(())
        }

        async fn query(database: DatabaseHelper, bot: BotType) -> anyhow::Result<()> {
            LAST_QUERY.store(
                kstool::time::get_current_second(),
                std::sync::atomic::Ordering::Relaxed,
            );

            let client = Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap();

            for player in database
                .player_query(None)
                .await
                .ok_or_else(|| anyhow!("Query database players failure"))?
            {
                //log::debug!("Start query user {}", player.ei());
                if player.disabled()
                    || kstool::time::get_current_second() as i64 - player.last_fetch()
                        < FETCH_PERIOD
                {
                    //log::debug!("Skip user {}", player.ei());
                    continue;
                }
                let ret = Self::handle_each_user(&client, &player, &database, &bot)
                    .await
                    .inspect_err(|e| log::error!("Remote query user got error: {e:?}"));

                database
                    .player_update(player.ei().to_string(), ret.is_err())
                    .await;
                //log::debug!("Query {} finished", player.ei());
            }
            Ok(())
        }

        async fn notify(database: &DatabaseHelper, bot: &BotType) -> anyhow::Result<()> {
            let missions = database
                .mission_query()
                .await
                .ok_or_else(|| anyhow!("Query database mission error"))?;

            let mut pending = HashMap::new();
            for mission in &missions {
                pending
                    .entry(mission.belong())
                    .or_insert_with(Vec::new)
                    .push(mission);
            }

            let mut msg_map = HashMap::new();

            for (ei, missions) in pending {
                let Some(player) = database
                    .player_query_ei(ei.to_string())
                    .await
                    .tap_none(|| log::error!("Query database players {ei} error"))
                    .flatten()
                else {
                    continue;
                };
                for spaceship in &missions {
                    database.mission_updated(spaceship.id().to_string()).await;
                }
                msg_map
                    .entry(player.chat_id())
                    .or_insert_with(Vec::new)
                    .push(format!(
                        "{}:\n{}",
                        player.name(),
                        missions
                            .into_iter()
                            .map(|s| format!("{} returned!", s.name()))
                            .join("\n"),
                    ));
            }

            for (player, msg) in msg_map {
                bot.send_message(
                    player,
                    TELEGRAM_ESCAPE_RE.replace_all(&msg.join("\n\n"), "\\$1"),
                )
                .await?;
            }
            Ok(())
        }

        pub async fn join(self) -> anyhow::Result<()> {
            self.handle.await?
        }
    }
}
