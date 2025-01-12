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
