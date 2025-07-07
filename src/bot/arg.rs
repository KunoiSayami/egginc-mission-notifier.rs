use teloxide::types::ChatId;

use crate::{
    database::DatabaseHelper,
    egg::monitor::{ContractSubscriberHelper, MonitorHelper},
};

#[derive(Clone, Debug)]
pub(super) struct NecessaryArg {
    database: DatabaseHelper,
    admin: Vec<ChatId>,
    monitor: MonitorHelper,
    username: String,
    subscriber: ContractSubscriberHelper,
}

impl NecessaryArg {
    pub(super) fn new(
        database: DatabaseHelper,
        admin: Vec<ChatId>,
        monitor: MonitorHelper,
        username: String,
        subscriber: ContractSubscriberHelper,
    ) -> Self {
        Self {
            database,
            admin,
            monitor,
            username,
            subscriber,
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

    pub(super) fn subscriber(&self) -> &ContractSubscriberHelper {
        &self.subscriber
    }
}
