use std::{
    collections::HashMap,
    error::Error,
    fs,
    net::{AddrParseError, Ipv4Addr, SocketAddrV4},
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::Deserialize;

/// (x, y)
type Pos = (u32, u32);
/// (x, y, width, height)
type Rect = (u32, u32, u32, u32);

#[derive(Clone, Deserialize, Debug)]
pub struct Config {
    pub adb: AdbConfig,
    pub ocr: OcrConfig,
}

#[derive(Clone, Deserialize, Debug)]
pub struct AdbConfig {
    pub host: SocketAddrV4,
    pub device_serial: String,
}

impl TryFrom<AdbConfigDef> for AdbConfig {
    type Error = Box<dyn Error>;

    fn try_from(def: AdbConfigDef) -> Result<Self, Self::Error> {
        let host_str = def.host.as_deref().unwrap_or("127.0.0.1");
        let host = Ipv4Addr::from_str(host_str);
        let host = match host {
            Ok(host) => SocketAddrV4::new(host, 5037),
            Err(_) => SocketAddrV4::from_str(host_str)?,
        };
        Ok(Self {
            host,
            device_serial: def.device_serial,
        })
    }
}

impl Config {
    pub fn new(config_path: &Path) -> Result<Self, Box<dyn Error>> {
        let str = fs::read_to_string(config_path)?;
        let config: ConfigDef = toml::from_str(&str)?;
        Ok(Self {
            adb: AdbConfig::try_from(config.adb)?,
            ocr: config.ocr,
        })
    }
}

#[derive(Clone, Deserialize, Debug)]
pub struct ConfigDef {
    pub adb: AdbConfigDef,
    pub ocr: OcrConfig,
}

#[derive(Clone, Deserialize, Debug)]
pub struct AdbConfigDef {
    pub host: Option<String>,
    pub device_serial: String,
}

#[derive(Clone, Deserialize, Debug)]
pub struct OcrConfig {
    pub detection_model_path: PathBuf,
    pub recognition_model_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Plan {
    pub workdir: PathBuf,
    pub package: String,
    pub activity: String,
    pub screens: HashMap<String, Screen>,
    pub screen_groups: HashMap<String, ScreenGroup>,
    pub schedules: Vec<Schedule>,
    pub routine_location: HashMap<PathBuf, String>,
}

#[derive(Clone, Debug)]
pub struct Screen {
    pub ident: Vec<ScreenIdent>,
    pub nav: ScreenNavigation,
    pub routines: Vec<PathBuf>,
    pub group: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ScreenGroup {
    pub ident: Vec<ScreenIdent>,
    pub screens: Vec<String>,
    pub nav: ScreenNavigation,
}

impl Plan {
    pub fn new(plan_wd: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let str = fs::read_to_string(plan_wd.join("plan.toml"))?;
        let plan: PlanDef = toml::from_str(&str)?;
        let mut screens = HashMap::new();
        let mut screen_groups = HashMap::new();
        let mut routine_location = HashMap::new();
        for (name, screen_def) in plan.screens {
            if let Some(subscreens) = screen_def.subscreens {
                for (subname, subdef) in &subscreens {
                    for routine in &subdef.routines {
                        routine_location.insert(routine.to_owned(), subname.to_owned());
                    }
                    screens.insert(
                        subname.to_owned(),
                        Screen {
                            ident: subdef.ident.to_owned(),
                            nav: ScreenNavigation {
                                to: subdef.to.to_owned(),
                                back: screen_def.nav.back,
                            },
                            routines: subdef.routines.to_owned(),
                            group: Some(name.clone()),
                        },
                    );
                }
                screen_groups.insert(
                    name,
                    ScreenGroup {
                        ident: screen_def.ident,
                        screens: subscreens.keys().cloned().collect(),
                        nav: screen_def.nav,
                    },
                );
            } else {
                for routine in &screen_def.routines {
                    routine_location.insert(routine.to_owned(), name.clone());
                }
                screens.insert(
                    name,
                    Screen {
                        ident: screen_def.ident,
                        nav: screen_def.nav,
                        routines: screen_def.routines,
                        group: None,
                    },
                );
            }
        }
        if !screens.contains_key("end") {
            screens.insert(
                "end".to_owned(),
                Screen {
                    ident: Vec::new(),
                    nav: ScreenNavigation {
                        to: HashMap::new(),
                        back: false,
                    },
                    routines: Vec::new(),
                    group: None,
                },
            );
        }
        Ok(Self {
            workdir: plan_wd.to_owned(),
            package: plan.package,
            activity: plan.activity,
            screens,
            screen_groups,
            schedules: plan.schedules,
            routine_location,
        })
    }
}

#[derive(Clone, Deserialize, Debug)]
pub struct PlanDef {
    pub package: String,
    pub activity: String,
    pub screens: HashMap<String, ScreenDef>,
    pub schedules: Vec<Schedule>,
}

#[derive(Clone, Deserialize, Debug)]
pub struct ScreenDef {
    #[serde(deserialize_with = "deserialize_single_or_vec", default)]
    pub ident: Vec<ScreenIdent>,
    #[serde(flatten)]
    pub nav: ScreenNavigation,
    #[serde(default)]
    pub routines: Vec<PathBuf>,
    pub subscreens: Option<HashMap<String, Subscreen>>,
}

#[derive(Clone, Deserialize, Debug)]
pub struct ScreenNavigation {
    #[serde(default)]
    pub to: HashMap<String, ScreenTo>,
    #[serde(default)]
    pub back: bool,
}

#[derive(Clone, Deserialize, Debug)]
pub struct Subscreen {
    #[serde(deserialize_with = "deserialize_single_or_vec", default)]
    pub ident: Vec<ScreenIdent>,
    #[serde(default)]
    pub to: HashMap<String, ScreenTo>,
    #[serde(default)]
    pub routines: Vec<PathBuf>,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(untagged)]
pub enum ScreenIdent {
    RefMatch {
        #[serde(rename = "ref")]
        reference: PathBuf,
        rect: Rect,
    },
    ImageMatch {
        image: PathBuf,
        pos: Pos,
    },
    Ocr {
        ocr: String,
        operation: TextOperation,
        rect: Rect,
    },
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum TextOperation {
    Exact,
    Contains,
    StartsWith,
    EndsWith,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(untagged)]
pub enum ScreenTo {
    Script(PathBuf),
    Actions(Vec<Actions>),
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Actions {
    Tap(u32, u32),
    Back,
}

#[derive(Clone, Deserialize, Debug)]
pub struct Schedule {
    #[serde(flatten)]
    pub action: ScheduleActions,
    pub on_calendar: String,
    #[serde(default)]
    pub interruptible: bool,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum ScheduleActions {
    Routines(Vec<PathBuf>),
    Script(PathBuf),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SingleOrVec<T> {
    Single(T),
    Vec(Vec<T>),
}

impl<T> From<SingleOrVec<T>> for Vec<T> {
    fn from(single_or_vec: SingleOrVec<T>) -> Self {
        match single_or_vec {
            SingleOrVec::Single(single) => vec![single],
            SingleOrVec::Vec(vec) => vec,
        }
    }
}

fn deserialize_single_or_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Ok(Vec::<T>::from(SingleOrVec::deserialize(deserializer)?))
}