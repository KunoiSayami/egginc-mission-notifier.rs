use std::{sync::Arc, time::Duration};

use itertools::Itertools as _;
use reqwest::ClientBuilder;
use teloxide::{
    prelude::Requester,
    types::{InputFile, Message},
};

use anyhow::anyhow;

use crate::{
    egg::{decode_coop_status, ei_request, query_coop_status},
    types::timestamp_fmt,
};

use super::{
    arg::NecessaryArg,
    contract::{COOP_ID_RE, ROOM_RE},
    BotType, EI_CHECKER_RE,
};

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
                            Ok(Self::CacheInsertFake {
                                ei: second1,
                                land_times: second2
                                    .split(' ')
                                    .filter_map(|x| {
                                        x.parse()
                                            .inspect_err(|e| {
                                                log::warn!(
                                                    "Parse {x:?} to number error, ignored: {e:?}"
                                                )
                                            })
                                            .ok()
                                    })
                                    .collect(),
                            })
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
