use teloxide::types::ChatId;

use crate::{database::DatabaseHelper, egg::monitor::MonitorHelper};

#[derive(Clone, Debug)]
pub(super) struct NecessaryArg {
    database: DatabaseHelper,
    admin: Vec<ChatId>,
    monitor: MonitorHelper,
    username: String,
}

impl NecessaryArg {
    pub(super) fn new(
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
