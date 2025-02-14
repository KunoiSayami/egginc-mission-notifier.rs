use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use admin::handle_admin_command;
use anyhow::anyhow;
use base64::Engine;
use chrono::TimeDelta;
use itertools::Itertools;
use reqwest::ClientBuilder;
use teloxide::{
    adaptors::DefaultParseMode,
    dispatching::{HandlerExt as _, UpdateFilterExt as _},
    dptree,
    payloads::{EditMessageTextSetters, SendMessageSetters},
    prelude::{Dispatcher, Requester as _, RequesterExt as _},
    types::{
        CallbackQuery, ChatId, InlineKeyboardButton, InlineKeyboardMarkup, LinkPreviewOptions,
        Message, MessageId, ParseMode, Update,
    },
    utils::command::BotCommands,
    Bot,
};

use crate::{
    config::Config,
    database::DatabaseHelper,
    egg::{
        decode_and_calc_score, encode_to_byte,
        monitor::{MonitorHelper, LAST_QUERY},
        query_coop_status,
    },
    types::{fmt_time_delta_short, return_tf_emoji, timestamp_to_string, SpaceShip, BASE64},
    CHECK_PERIOD, FETCH_PERIOD,
};

pub type BotType = DefaultParseMode<Bot>;

static TELEGRAM_ESCAPE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"([_*\[\]\(\)~>#\+\-=|\{}\.!])").unwrap());
pub static EI_CHECKER_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^EI\d{16}$").unwrap());
pub static COOP_ID_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[\w]+(-[\w\d]+)*$").unwrap());
pub static ROOM_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[\w\d][\-\w\d]*$").unwrap());
static SPACE_RE: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"[ \t]+").unwrap());
static CONTRACT_WEBSITE_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"^https://eicoop-carpet.netlify.app/([\w\d][\-\w\d]*)/([\w\d][\-\w\d]*)(\?d)?$",
    )
    .unwrap()
});

pub fn replace_all(s: &str) -> std::borrow::Cow<'_, str> {
    TELEGRAM_ESCAPE_RE.replace_all(s, "\\$1")
}

fn link_preview_options(enable: bool) -> LinkPreviewOptions {
    LinkPreviewOptions {
        is_disabled: !enable,
        prefer_large_media: false,
        prefer_small_media: false,
        url: None,
        show_above_text: false,
    }
}

mod admin {
    use teloxide::{prelude::Requester, types::InputFile};

    use crate::{
        egg::{decode_coop_status, ei_request},
        types::timestamp_fmt,
    };

    use super::*;

    //#[derive(Clone, Copy, Debug)]
    pub(super) enum AdminCommand<'a> {
        Query {
            ei: Option<&'a str>,
        },
        ResetNotify {
            ei: &'a str,
            limit: i32,
        },
        UserToggle {
            ei: &'a str,
            enabled: bool,
        },
        ContractCacheReset {
            id: &'a str,
            room: &'a str,
        },
        CacheReset {
            invalidate: bool,
        },
        CacheInsertFake {
            ei: &'a str,
            land_times: Vec<i64>,
        },
        ContractSave {
            id: &'a str,
            room: &'a str,
            ei: Option<&'a str>,
        },
        UserStatusSave {
            ei: &'a str,
        },
        ListUsers,
    }

    impl<'a> AdminCommand<'a> {
        fn new_save(id: &'a str, room: &'a str, ei: Option<&'a str>) -> Option<Self> {
            if !COOP_ID_RE.is_match(id)
                || !ROOM_RE.is_match(room)
                || ei.is_some_and(|ei| !EI_CHECKER_RE.is_match(ei))
            {
                None
            } else {
                Some(Self::ContractSave { id, room, ei })
            }
        }
    }

    impl<'a> TryFrom<&'a str> for AdminCommand<'a> {
        type Error = &'static str;

        fn try_from(value: &'a str) -> Result<Self, Self::Error> {
            if let Some((first, second)) = value.split_once(' ') {
                match first {
                    "query" => Ok(Self::Query { ei: Some(second) }),
                    "reset-contract" => {
                        if let Some((second1, second2)) = second.split_once(' ') {
                            Ok(Self::ContractCacheReset {
                                id: second1,
                                room: second2,
                            })
                        } else {
                            Err("Room id missing")
                        }
                    }
                    "reset" => {
                        if let Some((second1, second2)) = second.split_once(' ') {
                            if second1 == "cache" {
                                Ok(Self::CacheReset {
                                    invalidate: second2.eq("true"),
                                })
                            } else if EI_CHECKER_RE.is_match(second1) {
                                Ok(Self::ResetNotify {
                                    ei: second1,
                                    limit: second2.parse().map_err(|_| "Parse number error")?,
                                })
                            } else {
                                Err("Wrong EI format")
                            }
                        } else if second == "cache" {
                            Ok(Self::CacheReset { invalidate: false })
                        } else {
                            Err("Invalid format")
                        }
                    }
                    "cache-insert" => {
                        if let Some((second1, second2)) = second.split_once(' ') {
                            if EI_CHECKER_RE.is_match(second1) {
                                Ok(Self::CacheInsertFake { ei: second1, land_times: second2.split(' ').filter_map(|x| {
                                    x.parse()
                                        .inspect_err(|e| {
                                            log::warn!("Parse {x:?} to number error, ignored: {e:?}")
                                        })
                                        .ok()
                                }).collect() })
                            } else {
                                Err("Wrong EI format")
                            }
                        } else {
                            if !EI_CHECKER_RE.is_match(second) {
                                return Err("Wrong EI format");
                            }
                            Ok(Self::CacheInsertFake {
                                ei: second,
                                land_times: vec![30, 60, 90],
                            })
                        }
                    }
                    "contract-save" => {
                        let (second1, second2) =
                            second.split_once(' ').ok_or("Wrong command format")?;
                        if let Some((second2, second3)) = second2.split_once(' ') {
                            Self::new_save(second1, second2, Some(second3.trim()))
                        } else {
                            Self::new_save(second1, second2, None)
                        }
                        .ok_or("Fail in command argument check")
                    }
                    "bot-contract-save" => {
                        if EI_CHECKER_RE.is_match(second) {
                            Ok(Self::UserStatusSave { ei: second })
                        } else {
                            Err("Invalid EI")
                        }
                    }
                    "enable" | "disable" => Ok(Self::UserToggle {
                        ei: second,
                        enabled: first.eq("enable"),
                    }),
                    _ => Err("Invalid command"),
                }
            } else {
                match value {
                    "query" => Ok(Self::Query { ei: None }),
                    "list" => Ok(Self::ListUsers),
                    /* "test" => Ok(Self::Test), */
                    _ => Err("Invalid command"),
                }
            }
        }
    }

    async fn handle_admin_sub_command<'a>(
        bot: &BotType,
        arg: &Arc<NecessaryArg>,
        msg: &Message,
        command: AdminCommand<'a>,
    ) -> anyhow::Result<()> {
        let build_client = || {
            ClientBuilder::new()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap()
        };

        match command {
            AdminCommand::Query { ei } => {
                if let Some(ei) = ei {
                    arg.database().account_timestamp_reset(ei.to_string()).await;
                }
                arg.monitor().new_client().await;
                bot.send_message(msg.chat.id, "Request sent").await
            }
            /* AdminCommand::Test => {
                bot.send_message(msg.chat.id, "_te*st_\n*te_st*\n__test__")
                    .await
            } */
            AdminCommand::ResetNotify { ei, limit } => {
                arg.database()
                    .account_mission_reset(ei.to_string(), limit as usize)
                    .await;
                bot.send_message(msg.chat.id, "Mission reset").await
            }
            AdminCommand::UserToggle { ei, enabled } => {
                arg.database()
                    .account_status_reset(ei.to_string(), !enabled)
                    .await;
                bot.send_message(
                    msg.chat.id,
                    format!("User {ei} {}", if enabled { "enabled" } else { "disabled" }),
                )
                .await
            }
            AdminCommand::ListUsers => {
                let users = arg
                    .database()
                    .user_query_all()
                    .await
                    .ok_or_else(|| anyhow!("Query all user return none"))?;
                bot.send_message(
                    msg.chat.id,
                    users
                        .into_iter()
                        .map(|user| format!("{} {}", user.id(), user.account_to_str()))
                        .join("\n"),
                )
                .await
            }
            AdminCommand::CacheReset { invalidate } => {
                arg.monitor().refresh_cache(invalidate).await;
                bot.send_message(msg.chat.id, "Cache reset\\!").await
            }
            AdminCommand::CacheInsertFake { ei, land_times } => {
                arg.monitor().insert_cache(ei.to_string(), land_times).await;
                bot.send_message(msg.chat.id, "New cache inserted").await
            }
            AdminCommand::ContractCacheReset { id, room } => {
                arg.database()
                    .contract_cache_update_timestamp(id.into(), room.into())
                    .await;
                bot.send_message(msg.chat.id, "Timestamp updated").await
            }
            AdminCommand::ContractSave { id, room, ei } => {
                let ret = if ei.is_none() {
                    arg.database()
                        .contract_cache_query(id.into(), room.into())
                        .await
                        .flatten()
                        .and_then(|x| decode_coop_status(&x.extract(), false).ok())
                } else {
                    let client = ClientBuilder::new()
                        .timeout(Duration::from_secs(10))
                        .build()
                        .unwrap();
                    let raw = query_coop_status(&client, id, room, ei.map(|x| x.to_string()))
                        .await
                        .inspect_err(|e| log::error!("Query remote error: {e:?}"))
                        .ok();
                    raw
                };
                match ret {
                    Some(resp) => {
                        let s = format!("{resp:#?}");
                        //bot.send_message(chat_id, text);

                        bot.send_document(
                            msg.chat.id,
                            InputFile::memory(s).file_name(format!(
                                "{}-{id}-{room}-{}.txt",
                                ei.unwrap_or("None"),
                                timestamp_fmt(
                                    kstool::time::get_current_second() as i64,
                                    "%Y%m%d-%H%M%S"
                                )
                            )),
                        )
                        .await
                    }
                    None => {
                        bot.send_message(
                            msg.chat.id,
                            "Contract not found, try add EI to fetch online",
                        )
                        .await
                    }
                }
            }
            AdminCommand::UserStatusSave { ei } => {
                let client = build_client();
                match ei_request(&client, ei).await {
                    Ok(resp) => {
                        let s = format!("{resp:#?}");

                        bot.send_document(
                            msg.chat.id,
                            InputFile::memory(s).file_name(format!(
                                "{ei}-{}.txt",
                                timestamp_fmt(
                                    kstool::time::get_current_second() as i64,
                                    "%Y%m%d-%H%M%S"
                                )
                            )),
                        )
                        .await
                    }
                    Err(e) => {
                        log::error!("[User Request] Remote query error: {e:?}");
                        bot.send_message(
                            msg.chat.id,
                            format!("Got {} error, check console", e.err_type()),
                        )
                        .await
                    }
                }
            }
        }?;
        Ok(())
    }

    pub(crate) async fn handle_admin_command(
        bot: BotType,
        arg: Arc<NecessaryArg>,
        msg: Message,
        line: String,
    ) -> anyhow::Result<()> {
        if !arg.check_admin(msg.chat.id) {
            return Ok(());
        }

        match AdminCommand::try_from(line.as_str()) {
            Ok(cmd) => {
                if let Err(e) = handle_admin_sub_command(&bot, &arg, &msg, cmd).await {
                    bot.send_message(
                        msg.chat.id,
                        format!("Handle admin sub command error: {e:?}"),
                    )
                    .await?;
                    return Err(e);
                };
            }
            Err(e) => {
                bot.send_message(msg.chat.id, e).await?;
            }
        };

        Ok(())
    }
}

#[derive(Clone)]
enum ContractCommand {
    List {
        ei: String,
    },
    Calc {
        ei: String,
        id: String,
        detail: bool,
    },
    CalcRoom {
        id: String,
        room: String,
        detail: bool,
    },
    Control {
        enable: bool,
        ei: String,
    },
}

impl ContractCommand {
    fn parse(input: std::borrow::Cow<'_, str>) -> Option<Self> {
        let (first, second) = input.split_once(' ')?;

        if let Some((second, third)) = second.split_once(' ') {
            let (third, forth) = third.split_once(' ').unwrap_or((third, ""));
            match first {
                "calc" if EI_CHECKER_RE.is_match(second) && ROOM_RE.is_match(third) => {
                    Some(Self::Calc {
                        ei: second.into(),
                        id: third.into(),
                        detail: forth.eq("detail"),
                    })
                }
                "room" if COOP_ID_RE.is_match(second) && ROOM_RE.is_match(third) => {
                    Some(Self::CalcRoom {
                        id: second.into(),
                        room: third.into(),
                        detail: forth.eq("detail"),
                    })
                }
                _ => None,
            }
        } else {
            match first {
                "list" if EI_CHECKER_RE.is_match(second) => Some(Self::List { ei: second.into() }),
                "enable" | "disable" => {
                    if EI_CHECKER_RE.is_match(second) {
                        Some(Self::Control {
                            enable: first.eq("enable"),
                            ei: second.into(),
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
    }

    fn new_room(contract_id: &str, room: &str, detail: bool) -> Self {
        Self::CalcRoom {
            id: contract_id.into(),
            room: room.into(),
            detail,
        }
    }

    fn keyboard(&self, detail: bool) -> InlineKeyboardMarkup {
        let detail = if detail { " detail" } else { "" };
        InlineKeyboardMarkup::new(match &self {
            ContractCommand::Calc { ei, id, .. } => [[
                InlineKeyboardButton::callback(
                    "Refresh",
                    format!("contract calc {ei} {id}{detail}"),
                ),
                InlineKeyboardButton::callback(
                    "Refresh inline",
                    format!("contract-i calc {ei} {id}{detail}"),
                ),
            ]],
            ContractCommand::CalcRoom { id, room, .. } => [[
                InlineKeyboardButton::callback(
                    "Refresh",
                    format!("contract room {id} {room}{detail}"),
                ),
                InlineKeyboardButton::callback(
                    "Refresh inline",
                    format!("contract-i room {id} {room}{detail}"),
                ),
            ]],
            _ => unreachable!(),
        })
    }
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "snake_case")]
enum Command {
    Add { ei: String },
    Delete { ei: String },
    List { detail: String },
    Contract { cmd: String },
    Missions { user: String },
    Recent { user: String },
    Admin { line: String },
    Start { args: String },
    Help,
    Ping,
}

impl Command {
    fn decode_inner(s: String) -> Option<Self> {
        let content = BASE64
            .decode(s.as_bytes())
            .ok()
            .and_then(|x| String::from_utf8(x).ok())?;
        let (first, second) = content.split_once(' ')?;
        match first {
            "contract" => Some(Self::Contract { cmd: second.into() }),
            _ => None,
        }
    }

    fn decode(s: String) -> Self {
        Self::decode_inner(s.clone()).unwrap_or_else(|| Self::Start { args: s })
    }
}

#[derive(Clone, Debug)]
pub struct NecessaryArg {
    database: DatabaseHelper,
    admin: Vec<ChatId>,
    monitor: MonitorHelper,
    username: String,
}

impl NecessaryArg {
    pub fn new(
        database: DatabaseHelper,
        admin: Vec<ChatId>,
        monitor: MonitorHelper,
        username: String,
    ) -> Self {
        Self {
            database,
            admin,
            monitor,
            username,
        }
    }

    pub fn database(&self) -> &DatabaseHelper {
        &self.database
    }

    /* pub fn admin(&self) -> &[ChatId] {
        &self.admin
    } */

    pub fn check_admin(&self, id: ChatId) -> bool {
        self.admin.iter().any(|x| &id == x)
    }

    pub fn monitor(&self) -> &MonitorHelper {
        &self.monitor
    }

    pub fn username(&self) -> &str {
        &self.username
    }
}

pub fn bot(config: &Config) -> anyhow::Result<BotType> {
    let bot = Bot::new(config.telegram().api_key());
    Ok(match config.telegram().api_server() {
        Some(url) => bot.set_api_url(url.parse()?),
        None => bot,
    }
    .parse_mode(ParseMode::MarkdownV2))
}

pub async fn bot_run(
    bot: BotType,
    config: Config,
    database: DatabaseHelper,
    monitor: MonitorHelper,
) -> anyhow::Result<()> {
    let arg = Arc::new(NecessaryArg::new(
        database,
        config.admin().iter().map(|u| ChatId(*u)).collect(),
        monitor,
        config.telegram().username().to_string(),
    ));

    let handle_command_message = Update::filter_message().branch(
        dptree::entry()
            .filter(|msg: Message| msg.chat.is_private())
            .filter_command::<Command>()
            .endpoint(
                |msg: Message, bot: BotType, arg: Arc<NecessaryArg>, cmd: Command| {
                    let cmd = if let Command::Start { args } = cmd {
                        Command::decode(args)
                    } else {
                        cmd
                    };
                    async move {
                        match cmd {
                            Command::Ping => handle_ping(bot, msg, arg).await,
                            Command::Add { ei } => handle_add_command(bot, arg, msg, ei).await,
                            Command::Delete { ei } => {
                                handle_delete_command(bot, arg, msg, ei).await
                            }
                            Command::List { detail } => {
                                handle_list_command(bot, arg, msg, detail.eq("ei")).await
                            }
                            Command::Missions { user } => {
                                handle_missions_command(bot, arg, msg, user, false).await
                            }
                            Command::Recent { user } => {
                                handle_missions_command(bot, arg, msg, user, true).await
                            }
                            Command::Admin { line } => {
                                handle_admin_command(bot, arg, msg, line).await
                            }
                            Command::Contract { cmd } => {
                                route_contract_command(bot, arg, msg.chat.id, msg.id, cmd, false)
                                    .await
                            }
                            Command::Help => handle_help(bot, msg).await,
                            Command::Start { args: _ } => {
                                bot.send_message(
                                    msg.chat.id,
                                    "Welcome, use /help to show more information\\.",
                                )
                                .await?;
                                Ok(())
                            }
                        }
                    }
                },
            ),
    );

    let handle_message = Update::filter_message()
        .filter(|msg: Message| msg.chat.is_private())
        .endpoint(
            |msg: Message, bot: BotType, arg: Arc<NecessaryArg>| async move {
                let Some(text) = msg.text() else {
                    return Ok(());
                };

                if let Some(group) = CONTRACT_WEBSITE_RE.captures(text) {
                    let event = ContractCommand::new_room(
                        group.get(1).unwrap().as_str(),
                        group.get(2).unwrap().as_str(),
                        group.get(3).is_some(),
                    );
                    return handle_calc_score(bot, arg, msg.chat.id, msg.id, &event, false).await;
                }

                let groups = text.split_whitespace().collect_vec();

                if groups.len() >= 2 {
                    let first = groups[0];
                    let second = groups[1];
                    let detail = groups.get(2).map(|x| x.eq(&"d")).unwrap_or_default();

                    if COOP_ID_RE.is_match(first) && ROOM_RE.is_match(second) {
                        let event = ContractCommand::new_room(first, second, detail);
                        return handle_calc_score(bot, arg, msg.chat.id, msg.id, &event, false)
                            .await;
                    }
                }

                Ok(())
            },
        );

    let handle_callback_query = Update::filter_callback_query()
        .filter(|q: CallbackQuery| q.data.is_some())
        .endpoint(
            |q: CallbackQuery, bot: BotType, arg: Arc<NecessaryArg>| async move {
                handle_callback_query(bot, q, arg).await
            },
        );

    let dispatcher = Dispatcher::builder(
        bot,
        dptree::entry()
            .branch(handle_command_message)
            .branch(handle_message)
            .branch(handle_callback_query),
    )
    .dependencies(dptree::deps![arg])
    .default_handler(|_| async {});

    #[cfg(not(debug_assertions))]
    dispatcher.enable_ctrlc_handler().build().dispatch().await;

    #[cfg(debug_assertions)]
    tokio::select! {
        _ = async move {
            dispatcher.build().dispatch().await
        } => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    Ok(())
}

pub async fn handle_add_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    msg: Message,
    ei: String,
) -> anyhow::Result<()> {
    if !arg.check_admin(msg.chat.id)
        && arg
            .database()
            .account_query(Some(msg.chat.id.0))
            .await
            .ok_or_else(|| anyhow!("Query player for user not response"))?
            .len()
            >= 4
    {
        bot.send_message(msg.chat.id, "You can't add more player")
            .await?;
        return Ok(());
    }

    if !EI_CHECKER_RE.is_match(&ei) {
        bot.send_message(msg.chat.id, "Skip invalid user").await?;
        return Ok(());
    }

    let result = arg
        .database()
        .account_add(ei.clone(), msg.chat.id.0)
        .await
        .unwrap_or(false);

    bot.send_message(
        msg.chat.id,
        if result {
            format!("Player {ei} added")
        } else {
            "Can't add player, please contact administrator".into()
        },
    )
    .await?;

    if result {
        arg.monitor().new_client().await;
    }

    Ok(())
}

async fn handle_delete_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    msg: Message,
    ei: String,
) -> anyhow::Result<()> {
    let Some(_account) = arg.database().account_query_ei(ei.clone()).await.flatten() else {
        bot.send_message(msg.chat.id, "User not found").await?;
        return Ok(());
    };
    let Some(account_map) = arg.database().account_query_users(ei.clone()).await else {
        bot.send_message(msg.chat.id, "User not found").await?;
        return Ok(());
    };

    if !account_map.users().iter().any(|x| x == &msg.chat.id.0) || !arg.check_admin(msg.chat.id) {
        bot.send_message(msg.chat.id, "Permission denied").await?;
        return Ok(());
    }

    arg.database().user_remove_account(msg.chat.id.0, ei).await;

    bot.send_message(msg.chat.id, "User deleted").await?;

    Ok(())
}

async fn handle_ping(bot: BotType, msg: Message, arg: Arc<NecessaryArg>) -> anyhow::Result<()> {
    bot.send_message(
        msg.chat.id,
        format!(
            "Chat id: `{id}`\nLast system query: `{last_query}`\nCheck period: {check_period}s\nFetch period: {fetch_period}s\nIs admin: {is_admin}\nVersion: `{version}`",
            id = msg.chat.id.0,
            last_query = replace_all(&timestamp_to_string(
                LAST_QUERY.load(std::sync::atomic::Ordering::Relaxed) as i64
            )),
            check_period = CHECK_PERIOD.get().unwrap(),
            fetch_period = FETCH_PERIOD.get().unwrap(),
            is_admin = arg.check_admin(msg.chat.id),
            version = replace_all(env!("CARGO_PKG_VERSION"))
        ),
    )
    .await?;
    Ok(())
}

async fn handle_list_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    msg: Message,
    show_ei: bool,
) -> anyhow::Result<()> {
    let Some(ret) =
        (if arg.check_admin(msg.chat.id) && msg.text().is_some_and(|text| text.contains("all")) {
            arg.database().account_query(None).await
        } else {
            arg.database().account_query(Some(msg.chat.id.0)).await
        })
    else {
        log::warn!("Query result is None");
        return Ok(());
    };

    if ret.is_empty() {
        bot.send_message(msg.chat.id, "Nothing found").await?;
        return Ok(());
    }

    bot.send_message(
        msg.chat.id,
        ret.into_iter()
            .map(|s| s.line(arg.username(), show_ei))
            .join("\n"),
    )
    .await?;

    Ok(())
}

// Credit: Asuna
fn iter_spaceships(
    spaceships: Vec<SpaceShip>,
    recent: bool,
) -> Box<dyn Iterator<Item = SpaceShip>> {
    if recent {
        Box::new(spaceships.into_iter().rev())
    } else {
        Box::new(spaceships.into_iter())
    }
}

async fn handle_missions_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    msg: Message,
    user: String,
    recent: bool,
) -> anyhow::Result<()> {
    let Some(ret) = arg
        .database()
        .mission_query_by_user(
            if arg.check_admin(msg.chat.id) && !user.is_empty() {
                user.parse()?
            } else {
                msg.chat.id.0
            },
            recent,
        )
        .await
    else {
        log::warn!("Query mission result is None");
        return Ok(());
    };

    if ret.is_empty() {
        bot.send_message(msg.chat.id, "Nothing found").await?;
        return Ok(());
    }

    let text = ret
        .into_iter()
        .filter(|(_, spaceships)| !spaceships.is_empty())
        .map(|(player, spaceships)| {
            format!(
                "*{}*:\n{}",
                replace_all(player.name()),
                iter_spaceships(spaceships, recent)
                    .map(|s| {
                        let delta = s.calc_time(&msg.date);
                        let delta = if delta.is_empty() {
                            delta
                        } else {
                            format!(" {} left", delta)
                        };
                        format!(
                            "{} \\({}\\) {} {}{delta}",
                            replace_all(s.name()),
                            s.duration_type(),
                            replace_all(&timestamp_to_string(s.land())),
                            return_tf_emoji(s.notified())
                        )
                    })
                    .join("\n")
            )
        })
        .join("\n\n");

    if text.is_empty() {
        bot.send_message(
            msg.chat.id,
            if recent {
                "Recent land mission is empty, try use \\/missions command to check all missions\\."
            } else {
                "Missions is empty, try again later\\."
            },
        )
        .await?;
        return Ok(());
    }

    bot.send_message(msg.chat.id, text).await?;

    Ok(())
}

async fn route_contract_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    chat_id: ChatId,
    message_id: MessageId,
    cmd: String,
    inline: bool,
) -> anyhow::Result<()> {
    let filtered = SPACE_RE.replace_all(&cmd, " ");
    let Some(cmd) = ContractCommand::parse(filtered) else {
        bot.send_message(chat_id, "Invalid contract command\\.")
            .await?;
        return Ok(());
    };
    match cmd {
        ContractCommand::List { ei } => handle_list_contracts(bot, arg, chat_id, ei).await,
        ContractCommand::Calc { .. } | ContractCommand::CalcRoom { .. } => {
            handle_calc_score(bot, arg, chat_id, message_id, &cmd, inline).await
        }
        ContractCommand::Control { enable, ei } => {
            handle_enable_contract_tracker(bot, chat_id, arg, ei, enable).await
        }
    }
}

async fn handle_list_contracts(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    chat_id: ChatId,
    ei: String,
) -> anyhow::Result<()> {
    if !arg.check_admin(chat_id)
        && !arg
            .database()
            .account_query(Some(chat_id.0))
            .await
            .ok_or_else(|| anyhow!("Query user error"))?
            .iter()
            .any(|x| x.ei().eq(&ei))
    {
        bot.send_message(chat_id, "Permission denied").await?;
        return Ok(());
    }

    let contracts = arg
        .database()
        .account_query_contract(ei)
        .await
        .ok_or_else(|| anyhow!("Query user contract error"))?;

    let res = contracts
        .into_iter()
        .map(|contract| {
            format!(
                "`{}` `{}` {} [{}](t.me/{}?start={})[📋](t.me/{}?start={})",
                replace_all(contract.id()),
                replace_all(contract.room()),
                replace_all(&{
                    if let Some(start_time) = contract.start_time() {
                        timestamp_to_string(start_time as i64)
                    } else {
                        "Unknown".into()
                    }
                }),
                return_tf_emoji(contract.finished()),
                arg.username(),
                BASE64.encode(
                    format!("contract room {} {}", contract.id(), contract.room()).as_bytes()
                ),
                arg.username(),
                BASE64.encode(
                    format!("contract room {} {} detail", contract.id(), contract.room())
                        .as_bytes()
                )
            )
        })
        .join("\n");

    if res.is_empty() {
        bot.send_message(chat_id, "Contract not found").await?;
    } else {
        bot.send_message(chat_id, res).await?;
    }

    Ok(())
}

async fn handle_calc_score(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    chat_id: ChatId,
    message_id: MessageId,
    event: &ContractCommand,
    inline: bool,
) -> anyhow::Result<()> {
    let detail = match event {
        ContractCommand::CalcRoom { detail, .. } => detail,
        ContractCommand::Calc { ei, detail, .. } => {
            if !arg
                .database()
                .account_query(Some(chat_id.0))
                .await
                .ok_or_else(|| anyhow!("Query user error"))?
                .iter()
                .any(|x| x.ei().eq(ei))
            {
                bot.send_message(chat_id, "Permission denied").await?;

                return Ok(());
            }
            detail
        }
        _ => unreachable!(),
    };

    match process_calc(arg, event, *detail, inline).await {
        Ok(res) => {
            if inline {
                bot.edit_message_text(chat_id, message_id, res)
                    .link_preview_options(link_preview_options(false))
                    .reply_markup(event.keyboard(*detail))
                    .await
            } else {
                bot.send_message(chat_id, res)
                    .link_preview_options(link_preview_options(false))
                    .reply_markup(event.keyboard(*detail))
                    .await
            }
        }
        Err(e) => {
            log::error!("Calc function error: {e:?}");
            bot.send_message(chat_id, "Got error in calc score, check console\\.")
                .await
        }
    }?;

    Ok(())
}

async fn process_calc(
    arg: Arc<NecessaryArg>,
    event: &ContractCommand,
    detail: bool,
    inline: bool,
) -> anyhow::Result<String> {
    let contract_id = match event {
        ContractCommand::Calc { id, .. } | ContractCommand::CalcRoom { id, .. } => id,
        _ => unreachable!(),
    };

    let Some(contract_spec) = arg
        .database()
        .contract_query_spec(contract_id.to_string())
        .await
        .ok_or_else(|| anyhow!("Query contract spec error"))?
    else {
        return Err(anyhow!("Contract spec not found"));
    };

    let current_time = kstool::time::get_current_second() as i64;

    let (timestamp, room, body) = match event {
        ContractCommand::Calc { ei, .. } => {
            let Some(user_contract) = arg
                .database()
                .contract_query_single(contract_id.to_string(), ei.to_string())
                .await
                .ok_or_else(|| anyhow!("Query user contract room error"))?
            else {
                return Err(anyhow!("User room not found"));
            };

            let Some(contract_cache) = arg
                .database()
                .contract_cache_query(contract_id.to_string(), user_contract.room().to_string())
                .await
                .ok_or_else(|| anyhow!("Query contract cache error"))?
            else {
                return Err(anyhow!("Contract cache not found"));
            };
            (
                contract_cache.timestamp(),
                contract_cache.room().to_string(),
                contract_cache.extract(),
            )
        }
        ContractCommand::CalcRoom { room, .. } => {
            match arg
                .database()
                .contract_cache_query(contract_id.to_string(), room.to_string())
                .await
                .ok_or_else(|| anyhow!("Query contract cache error"))?
            {
                Some(cache) if cache.recent() => {
                    (cache.timestamp(), cache.room().to_string(), cache.extract())
                }
                _ => {
                    let client = ClientBuilder::new()
                        .timeout(Duration::from_secs(10))
                        .build()
                        .unwrap();
                    let raw = query_coop_status(&client, contract_id, room, None).await?;

                    let bytes = encode_to_byte(&raw);
                    arg.database()
                        .contract_cache_insert(
                            contract_id.into(),
                            room.into(),
                            bytes.clone(),
                            raw.cleared_for_exit() || raw.all_members_reporting(),
                            None,
                            None,
                        )
                        .await;
                    (current_time, room.clone(), bytes)
                }
            }
        }
        _ => unreachable!(),
    };

    let score = decode_and_calc_score(contract_spec, &body, false)?;

    let sub_title = if !score.is_finished() {
        let expect = current_time + score.expect_finish_time(Some(timestamp)) as i64;

        format!(
            "Expect complete: {}\n{}",
            replace_all(&timestamp_to_string(
                current_time + score.expect_finish_time(Some(timestamp)) as i64,
            )),
            if current_time > expect {
                "⚠️*Warning: The contract will be completed beyond the estimated time\\.*\n"
            } else {
                ""
            }
        )
    } else {
        "".into()
    };

    let users = score
        .member()
        .iter()
        .map(|member| {
            member
                .print(detail, Some(timestamp), score.is_cleared(), replace_all)
                .unwrap()
        })
        .join("\n");

    let result = format!(
        "[*\\({grade}\\)*](https://eicoop-carpet.netlify.app/{contract_id}/{room}) `{contract}` \\[`{room_id}`\\] {current_status}\n\
        Target: {amount}/{target} ELR: _{elr}_ Buff: _{buff}_\n\
        Contract timestamp: _{completion_time}_ / _{remain}_ remain\n\
        {sub_title}\n{users}\n\n\
        Contract last update: {last_update}\n\
        {msg_update}\
        {footer}",
        contract = replace_all(contract_id),
        room_id = replace_all(&room),
        grade = score.grade_str(),
        current_status = score.emoji(),
        elr = replace_all(&score.total_known_elr()),
        buff = replace_all(&score.display_buff()),
        completion_time = fmt_time_delta_short(TimeDelta::seconds(score.completion_time() as i64)),
        amount = replace_all(&score.current_amount()),
        target = replace_all(&score.target_amount()),
        remain = replace_all(&fmt_time_delta_short(TimeDelta::seconds(
            score.contract_remain_time(Some(timestamp)) as i64
        ))),
        last_update = replace_all(&timestamp_to_string(timestamp)),
        msg_update = if inline {
            format!(
                "Score update timestamp: {}\n",
                replace_all(&timestamp_to_string(current_time))
            )
        } else {
            String::new()
        },
        footer = if score.is_finished() && !score.is_cleared() {
            "This score is included your offline contributions, but not included your teamwork score\\.\n"
        } else {
            "This score not included your teamwork score\\."
        }
    );

    Ok(result)
}

async fn handle_enable_contract_tracker(
    bot: BotType,
    chat_id: ChatId,
    arg: Arc<NecessaryArg>,
    ei: String,
    enable: bool,
) -> anyhow::Result<()> {
    if !arg.check_admin(chat_id)
        && !arg
            .database()
            .account_query(Some(chat_id.0))
            .await
            .is_some_and(|x| x.iter().any(|account| account.ei().eq(&ei)))
    {
        bot.send_message(chat_id, "Permission denied\\.").await?;
        return Ok(());
    }

    arg.database().account_contract_update(ei, enable).await;
    bot.send_message(
        chat_id,
        format!(
            "Set contract tracker to {}",
            if enable { "enabled" } else { "disabled" }
        ),
    )
    .await?;
    Ok(())
}

async fn handle_callback_query(
    bot: BotType,
    msg: CallbackQuery,
    arg: Arc<NecessaryArg>,
) -> anyhow::Result<()> {
    //log::trace!("Callback data: {:?}", msg.data);
    let Some((first, second)) = msg.data.as_ref().and_then(|text| text.split_once(' ')) else {
        bot.answer_callback_query(msg.id).await?;
        return Ok(());
    };

    match first {
        "contract" | "contract-i" => {
            if let Some(msg) = second
                .contains(' ')
                .then_some(msg.message.as_ref())
                .flatten()
            {
                route_contract_command(
                    bot.clone(),
                    arg,
                    msg.chat().id,
                    msg.id(),
                    second.to_string(),
                    first.eq("contract-i"),
                )
                .await?;
                /* if let Ok(result) =
                    process_calc(arg, &ContractCommand::parse(second.to_string()).unwrap()).await
                {
                    bot.edit_message_text(msg.chat().id, msg.id(), result)
                        .await?;
                }; */
                if !first.ends_with("-i") {
                    bot.edit_message_reply_markup(msg.chat().id, msg.id())
                        .await?;
                }
            }
        }
        _ => {}
    }
    bot.answer_callback_query(msg.id).await?;
    Ok(())
}

async fn handle_help(bot: BotType, msg: Message) -> anyhow::Result<()> {
    bot.send_message(msg.chat.id, "Usage:\n\
    /add `\\<EI\\>` Add your account to this bot\\.\n\
    /list `\\[ei\\]` List all EI belong your telegram account\\.\n\
    /missions Display recent 6 rocket missions\\.\n\
    /recent Display recent 1 hour land missions\\.\n\
    /remove `\\<EI\\>` Remove your account from this bot\\.\n\n\
    Contract rated:\n\
    `/contract list` List your recent contracts, only available when contract tracker enabled\\.\n\
    `/contract calc \\<EI\\> \\<contract\\-id\\>` Calculate user's contract score\\.\n\
    `/contract room \\<contract\\-id\\> \\<room\\-id\\> \\[detail\\]` Calculate contract score by specify room ID\\.\n\
    `/contract enable\\|disable <EI>` Enable / Disable contract tracker \\(After add to bot\\)\\.\n\n\
    Note:\n\
    `\\[\\.\\.\\.\\]` means optional string\\.
    ").await?;
    Ok(())
}
