use std::collections::HashMap;

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Plan {
    pub activity: String,
    pub states: HashMap<String, State>,
}

#[derive(Deserialize, Debug)]
pub struct State {
    pub ident: Option<String>,
    pub to: Option<Vec<StateTo>>,
    pub next: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
pub struct StateTo {
    pub state: String,
    pub act: Vec<Actions>,
}

#[derive(Deserialize, Debug)]
pub enum Actions {
    #[serde(rename = "tap")]
    Tap(Vec<u32>),
}
