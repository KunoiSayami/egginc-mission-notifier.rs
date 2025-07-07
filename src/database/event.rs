use super::types::*;
use std::collections::HashMap;
pub(super) type CheckerArg = ((f64, f64, i64), fn((&[u8], i64), (f64, f64, i64)) -> bool);

kstool_helper_generator::oneshot_helper! {
#[derive(Debug)]
pub enum DatabaseEvent {

    #[ret(bool)]
    AccountAdd {
        ei: String,
        user: i64,
    },

    #[ret(Vec<User>)]
    UserQueryAll,

    #[ret(Vec<Account>)]
    AccountQuery {
        id: Option<i64>,
    },
    #[ret(AccountMap)]
    AccountQueryUsers {
        ei: String,
    },

    #[ret(Option<Account>)]
    AccountQueryEI {
        ei: String,
    },

    UserRemoveAccount {
        user: i64,
        ei: String,
    },

    AccountUpdate{
        ei: String,
        disabled: bool,
    },

    AccountContractUpdate{
        ei: String,
        enabled: bool,
    },

    AccountNameUpdate {
        ei: String,
        name: String,
    },

    AccountTimestampReset {
        ei: String,
    },
    AccountMissionReset {
        ei: String,
        limit: usize,
    },
    AccountStatusReset {
        ei: String,
        disabled: bool,
    },

    MissionAdd {
        id: String,
        name: String,
        duration_type: i64,
        belong: String,
        land: i64
    },

    #[ret(Vec<SpaceShip>)]
    MissionQuery{
        deadline: u64,
    },
    #[ret(HashMap<Account, Vec<SpaceShip>>)]
    MissionQueryByUser { id: i64, query_recent: bool },

    #[ret(Vec<SpaceShip>)]
    MissionQueryByAccount { ei: String },

    MissionUpdated { id: String },

    #[ret(Option<SpaceShip>)]
    MissionSingleQuery { identifier: String },

    #[ret(Option<Contract>)]
    ContractQuerySingle {
        id: String,
        ei: String
    },
    #[ret(Option<ContractSpec>)]
    ContractQuerySpec {
        id: String,
    },
    #[ret(Vec<Contract>)]
    AccountQueryContract {
        ei: String,
    },
    #[ret(bool)]
    AccountInsertContract {
        id: String,
        room: String,
        ei: String,
        finished: bool,
    },
    ContractUpdate {
        id: String,
        room: String,
        ei: String,
        finished: bool,
    },
    ContractStartTimeUpdate {
        id: String,
        room: String,
        start_time: f64,
    },
    #[ret(Option<ContractCache>)]
    ContractCacheQuery {
        id: String,
        room: String
    },
    #[ret(Option<i64>)]
    ContractCacheTimestampQuery {
        id: String,
        room: String
    },
    #[ret(bool)]
    ContractCacheInsert {
        id: String,
        room: String,
        cache: Vec<u8>,
        cleared: bool,
        timestamp: Option<i64>,
        cache_checker: Option<CheckerArg>,
    },
    ContractCacheUpdateTimestamp {
        id: String,
        room: String,
    },
    #[ret(bool)]
    ContractSpecInsert(ContractSpec),

    SubscribeNew(String, String, i64),
    #[ret(Vec<SubscribeInfo>)]
    SubscribeFetch(Option<i64>),
    SubscribeTimestampUpdate(String, String, i64),
    SubscribeNotified(String, String),
    #[ret(Option<SubscribeInfo>)]
    SubscribeSingleFetch(String, String),
    SubscribeDel(String, String, i64),

    Terminate,
}
}
