use std::collections::HashMap;

use log::error;

use super::{
    DBResult,
    context::Database,
    event::{DatabaseEvent, DatabaseEventReceiver, DatabaseHelper},
};

pub struct DatabaseHandle {
    handle: tokio::task::JoinHandle<DBResult<()>>,
}

impl DatabaseHandle {
    pub async fn connect(file: &str) -> anyhow::Result<(Self, DatabaseHelper)> {
        let mut database = Database::connect(file).await?;
        database.init().await?;
        let (sender, receiver) = DatabaseHelper::new(16);
        Ok((
            Self {
                handle: tokio::spawn(Self::run(database, receiver)),
            },
            sender,
        ))
    }

    async fn handle_event(database: &mut Database, event: DatabaseEvent) -> DBResult<()> {
        match event {
            DatabaseEvent::AccountAdd {
                ei,
                user,
                __private_sender,
            } => {
                __private_sender
                    .send(database.insert_account(&ei, user).await?)
                    .ok();
            }
            DatabaseEvent::AccountQuery {
                id,
                __private_sender,
            } => {
                match id {
                    Some(id) => __private_sender.send(database.query_ei(id).await?),
                    None => __private_sender.send(database.query_all_account().await?),
                }
                .ok();
            }
            DatabaseEvent::UserRemoveAccount { user, ei } => {
                database.delete_account(user, &ei).await?;
            }
            DatabaseEvent::AccountUpdate { ei, disabled } => {
                database
                    .set_account_status(&ei, kstool::time::get_current_second() as i64, disabled)
                    .await?;
            }
            DatabaseEvent::MissionAdd {
                belong,
                name,
                duration_type,
                id,
                land,
            } => {
                database
                    .insert_spaceship(id, name, duration_type, belong, land)
                    .await?;
            }
            DatabaseEvent::MissionQuery {
                deadline,
                __private_sender,
            } => {
                let r = database.query_spaceship_by_time(deadline as i64).await?;
                __private_sender.send(r).ok();
            }
            DatabaseEvent::AccountQueryEI {
                ei,
                __private_sender,
            } => {
                let r = database.query_account(&ei).await?;
                __private_sender.send(r).ok();
            }
            DatabaseEvent::Terminate => {
                unreachable!()
            }
            DatabaseEvent::MissionUpdated { id } => {
                database.mark_spaceship(&id).await?;
            }
            DatabaseEvent::AccountNameUpdate { ei, name } => {
                database.set_account_nickname(&ei, &name).await?;
            }
            DatabaseEvent::MissionSingleQuery {
                identifier,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_spaceship_by_id(&identifier).await?)
                    .ok();
            }
            DatabaseEvent::MissionQueryByUser {
                id,
                query_recent,
                __private_sender,
            } => {
                let current = kstool::time::get_current_second() as i64;
                let mut map = HashMap::new();
                for account in database.query_ei(id).await? {
                    let missions = database.query_spaceship_by_ei(account.ei()).await?;
                    map.insert(
                        account,
                        if query_recent {
                            missions
                                .into_iter()
                                .filter(|s| {
                                    let diff = s.land() - current;
                                    diff > 0 && diff <= 3600 && !s.notified()
                                })
                                .collect()
                        } else {
                            missions
                        },
                    );
                }
                __private_sender.send(map).ok();
            }
            DatabaseEvent::MissionQueryByAccount {
                ei,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_spaceship_by_ei(&ei).await?)
                    .ok();
            }
            DatabaseEvent::AccountTimestampReset { ei } => {
                database.account_timestamp_reset(&ei).await?;
            }
            DatabaseEvent::AccountMissionReset { ei, limit } => {
                database.account_mission_reset(&ei, limit).await?;
            }
            DatabaseEvent::AccountStatusReset { ei, disabled } => {
                database.account_status_reset(&ei, disabled).await?;
            }
            DatabaseEvent::UserQueryAll(sender) => {
                sender.send(database.query_all_user().await?).ok();
            }
            DatabaseEvent::AccountQueryUsers {
                ei,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_account_map(&ei).await?)
                    .ok();
            }
            DatabaseEvent::ContractCacheInsert {
                id,
                room,
                cache,
                cleared,
                cache_checker,
                timestamp,
                __private_sender,
            } => {
                let current =
                    timestamp.unwrap_or_else(|| kstool::time::get_current_second() as i64);
                if let Some(original_cache) = database.query_contract_cache(&id, &room).await? {
                    __private_sender.send(true).ok();
                    if let Some((args, checker)) = cache_checker {
                        if !checker((original_cache.body(), original_cache.timestamp()), args) {
                            //log::warn!("Trying update outdated cache, skip");
                            return Ok(());
                        }
                    }
                    database
                        .update_contract_cache(&id, &room, &cache, current, cleared)
                        .await?;
                } else {
                    database
                        .insert_contract_cache(&id, &room, &cache, current, cleared)
                        .await?;
                    __private_sender.send(false).ok();
                }
            }
            DatabaseEvent::ContractSpecInsert(contract_spec, sender) => {
                if database
                    .query_contract_spec(contract_spec.id())
                    .await?
                    .is_some()
                {
                    sender.send(false).ok();
                    return Ok(());
                }

                let id = contract_spec.id().to_string();
                let body = minicbor_serde::to_vec(contract_spec.get_inner()).unwrap();
                database
                    .insert_contract_spec(
                        &id,
                        contract_spec.max_coop_size(),
                        contract_spec.token_time(),
                        &body,
                    )
                    .await?;
                sender.send(true).ok();
            }
            DatabaseEvent::ContractQuerySingle {
                id,
                ei,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_single_contract(&id, &ei).await?)
                    .ok();
            }
            DatabaseEvent::AccountInsertContract {
                id,
                room,
                ei,
                finished,
                __private_sender,
            } => {
                if let Some(contract) = database.query_single_contract(&id, &ei).await? {
                    let changed = contract.finished() != finished || !contract.room().eq(&room);
                    if changed {
                        database
                            .set_contract(&id, &ei, contract.room(), finished)
                            .await?;
                    }
                    __private_sender.send(changed).ok();
                } else {
                    database
                        .insert_user_contract(&id, &room, &ei, finished)
                        .await?;
                    __private_sender.send(true).ok();
                }
            }
            DatabaseEvent::ContractUpdate {
                id,
                room,
                ei,
                finished,
            } => {
                database.set_contract(&id, &ei, &room, finished).await?;
            }
            DatabaseEvent::AccountContractUpdate { ei, enabled } => {
                database.set_account_contract_trace(&ei, enabled).await?;
            }
            DatabaseEvent::AccountQueryContract {
                ei,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_contract(&ei).await?)
                    .ok();
            }
            DatabaseEvent::ContractQuerySpec {
                id,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_contract_spec(&id).await?)
                    .ok();
            }
            DatabaseEvent::ContractCacheQuery {
                id,
                room,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_contract_cache(&id, &room).await?)
                    .ok();
            }
            DatabaseEvent::ContractCacheTimestampQuery {
                id,
                room,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_contract_cache_timestamp(&id, &room).await?)
                    .ok();
            }
            DatabaseEvent::ContractStartTimeUpdate {
                id,
                room,
                start_time,
            } => {
                database
                    .set_contract_start_time(&id, &room, start_time)
                    .await?;
            }
            DatabaseEvent::ContractCacheUpdateTimestamp { id, room } => {
                database
                    .update_contract_cache_timestamp(&id, &room, 0)
                    .await?;
            }
            DatabaseEvent::SubscribeFetch(timestamp, sender) => {
                let ret = database
                    .query_subscribe(
                        timestamp.unwrap_or_else(|| kstool::time::get_current_second() as i64),
                    )
                    .await?;
                sender.send(ret).ok();
            }
            DatabaseEvent::SubscribeDel(contract, room, user) => {
                database
                    .modify_subscribe(&contract, &room, user, true)
                    .await?;
            }
            DatabaseEvent::SubscribeNew(contract, room, user) => {
                database
                    .modify_subscribe(&contract, &room, user, false)
                    .await?;
            }
            DatabaseEvent::SubscribeSingleFetch(contract, room, sender) => {
                let ret = database.query_subscribe_single(&contract, &room).await?;
                sender.send(ret).ok();
            }
            DatabaseEvent::SubscribeTimestampUpdate(contract, room, est) => {
                database.update_subscribe_est(&contract, &room, est).await?;
            }
            DatabaseEvent::SubscribeNotified(contract, room) => {
                database.update_subscribe_notified(&contract, &room).await?;
            }
        }
        Ok(())
    }

    async fn run(mut database: Database, mut receiver: DatabaseEventReceiver) -> DBResult<()> {
        while let Some(event) = receiver.recv().await {
            if let DatabaseEvent::Terminate = event {
                break;
            }
            Self::handle_event(&mut database, event)
                .await
                .inspect_err(|e| error!("Sqlite error: {e:?}"))
                .ok();
        }
        database.close().await?;
        Ok(())
    }

    pub async fn wait(self) -> anyhow::Result<()> {
        Ok(self.handle.await??)
    }
}
