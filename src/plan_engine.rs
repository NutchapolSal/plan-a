use std::{
    collections::{HashMap, VecDeque},
    error::Error,
    fs,
    path::Path,
    sync::{Arc, Mutex},
    thread::sleep,
    time::Duration,
};

use adb_client::ADBServerDevice;
use errors::*;
use image::{io::Reader as ImageReader, DynamicImage, GenericImage, GenericImageView, RgbaImage};
use image_new::DynamicImage as DynamicImageNew;
use mlua::{Function, Lua, Variadic};
use ocrs::{ImageSource, OcrEngine};
use pathfinding::prelude::{bfs, dfs};
use template_matching::{find_extremes, match_template};

use crate::{
    adb_device_ext::ADBDeviceSimpleCommand,
    def::{Actions, Plan, Screen, ScreenGroup, ScreenIdent, ScreenTo, TextOperation},
    image_stuff::{downgrade_image, RgbaImageNew},
};

#[derive(Clone, Debug)]
pub enum ScreenEngineAction {
    Identify(Vec<(String, ScreenIdent)>),
    Navigate(String, ScreenTo),
    None,
}

mod errors {
    use std::error::Error;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct NoMoreStepsError;
    impl std::fmt::Display for NoMoreStepsError {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "No more steps needed")
        }
    }
    impl Error for NoMoreStepsError {}

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UnknownScreenError;
    impl std::fmt::Display for UnknownScreenError {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "Invalid screen")
        }
    }
    impl Error for UnknownScreenError {}

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PathNotFoundError;
    impl std::fmt::Display for PathNotFoundError {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "Path not found")
        }
    }
    impl Error for PathNotFoundError {}
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
struct ScreenState {
    curr: String,
    back: Vec<String>,
}

impl ScreenState {
    pub fn to(
        &self,
        screens: &HashMap<String, Screen>,
        target: &str,
    ) -> Result<Self, Box<dyn Error>> {
        let curr_screen = screens.get(&self.curr).ok_or(UnknownScreenError)?;
        let screen = screens.get(target).ok_or(UnknownScreenError)?;

        if !screen.nav.back {
            Ok(Self {
                curr: target.to_owned(),
                back: Vec::new(),
            })
        } else {
            let mut out = self.clone();
            if !(curr_screen.group == screen.group && curr_screen.group.is_some()) {
                out.back.push(self.curr.clone());
            }
            out.curr = target.to_owned();
            Ok(out)
        }
    }

    pub fn back(&self) -> Option<Self> {
        if self.back.is_empty() {
            return None;
        }
        let mut out = self.clone();
        out.curr = out.back.pop().unwrap();
        Some(out)
    }
}
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum ScreenStatePathfindingSource {
    Begin,
    To,
    Back,
    InGroupNavigation,
    GroupTo,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct ScreenStatePathfinding {
    state: ScreenState,
    via: ScreenStatePathfindingSource,
}

impl ScreenStatePathfinding {
    fn new(state: ScreenState) -> Self {
        Self {
            state,
            via: ScreenStatePathfindingSource::Begin,
        }
    }

    fn successors(
        &self,
        screens: &HashMap<String, Screen>,
        screen_groups: &HashMap<String, ScreenGroup>,
    ) -> Result<Vec<ScreenStatePathfinding>, Box<dyn Error>> {
        let curr_screen = screens.get(&self.state.curr).ok_or(UnknownScreenError)?;
        let mut succ = Vec::new();
        if curr_screen.nav.back {
            if let Some(back) = self.state.back() {
                succ.push(ScreenStatePathfinding {
                    state: back,
                    via: ScreenStatePathfindingSource::Back,
                });
            }
        }
        for k in curr_screen.nav.to.keys() {
            succ.push(ScreenStatePathfinding {
                state: self.state.to(screens, k)?,
                via: ScreenStatePathfindingSource::To,
            });
        }
        if let Some(group) = &curr_screen.group {
            let group = screen_groups.get(group).ok_or(UnknownScreenError)?;
            for k in group.screens.iter() {
                if k != &self.state.curr {
                    succ.push(ScreenStatePathfinding {
                        state: self.state.to(screens, k)?,
                        via: ScreenStatePathfindingSource::InGroupNavigation,
                    });
                }
            }
            for k in group.nav.to.keys() {
                succ.push(ScreenStatePathfinding {
                    state: self.state.to(screens, k)?,
                    via: ScreenStatePathfindingSource::GroupTo,
                });
            }
        }
        Ok(succ)
    }

    fn to_screento(
        &self,
        from_screen_name: &str,
        screens: &HashMap<String, Screen>,
        screen_groups: &HashMap<String, ScreenGroup>,
    ) -> ScreenTo {
        let from_screen = screens.get(from_screen_name).unwrap();
        match self.via {
            ScreenStatePathfindingSource::To => {
                from_screen.nav.to.get(&self.state.curr).unwrap().clone()
            }
            ScreenStatePathfindingSource::Back => ScreenTo::Actions(vec![Actions::Back]),
            ScreenStatePathfindingSource::InGroupNavigation => screen_groups
                .get(from_screen.group.as_ref().unwrap())
                .unwrap()
                .nav
                .to
                .get(&self.state.curr)
                .unwrap()
                .clone(),
            ScreenStatePathfindingSource::GroupTo => screen_groups
                .get(from_screen.group.as_ref().unwrap())
                .unwrap()
                .nav
                .to
                .get(&self.state.curr)
                .unwrap()
                .clone(),
            ScreenStatePathfindingSource::Begin => ScreenTo::Actions(Vec::new()),
        }
    }
}

impl Default for ScreenState {
    fn default() -> Self {
        Self {
            curr: "start".to_owned(),
            back: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScreenEngine {
    screens: HashMap<String, Screen>,
    screen_groups: HashMap<String, ScreenGroup>,
    state: ScreenState,
    navigate_plan: VecDeque<ScreenStatePathfinding>,
    idented: bool,
}
impl ScreenEngine {
    pub fn from_plan(plan: &Plan) -> Self {
        Self {
            screens: plan.screens.clone(),
            screen_groups: plan.screen_groups.clone(),
            state: Default::default(),
            navigate_plan: Default::default(),
            idented: plan.screens.get("start").unwrap().ident.is_none(),
        }
    }

    pub fn step(&mut self) -> Result<ScreenEngineAction, Box<dyn Error>> {
        loop {
            let front = self.navigate_plan.front();
            let Some(front) = front else {
                return Ok(ScreenEngineAction::None);
            };

            if front.state.curr == self.state.curr {
                self.step_navigate();
                continue;
            }

            if !self.idented {
                let ident = &self.screens.get(&self.state.curr).unwrap().ident;
                if ident.is_some() {
                    return Ok(ScreenEngineAction::Identify(vec![(
                        self.state.curr.to_owned(),
                        ident.clone().unwrap(),
                    )]));
                } else {
                    self.idented = true;
                }
            }

            return Ok(ScreenEngineAction::Navigate(
                front.state.curr.clone(),
                front.to_screento(&self.state.curr, &self.screens, &self.screen_groups),
            ));
        }
    }

    pub fn mark_identified(&mut self, screen_name: &str) {
        if screen_name != self.state.curr {
            todo!("popups not supported yet");
        }

        self.idented = true;
    }

    pub fn step_navigate(&mut self) {
        self.state = self.navigate_plan.pop_front().unwrap().state;
        self.idented = false;
    }

    fn pathfind(&self, target: &str) -> Result<Vec<ScreenStatePathfinding>, Box<dyn Error>> {
        let path = bfs(
            &ScreenStatePathfinding::new(self.state.clone()),
            |s| {
                println!("going through {:?} > {}", &s.state.back, s.state.curr);
                s.successors(&self.screens, &self.screen_groups)
                    .unwrap_or_default()
            },
            |s| s.state.curr == target,
        );

        match &path {
            Some(path) => println!("path: {:#?}", path),
            None => println!("no path found"),
        };

        path.ok_or(PathNotFoundError.into())
    }

    pub fn go_to(&mut self, screen_name: &str) -> Result<(), Box<dyn Error>> {
        self.state = self.state.to(&self.screens, screen_name)?;
        Ok(())
    }

    pub fn go_back(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(back) = self.state.back() {
            self.state = back;
            Ok(())
        } else {
            Err(NoMoreStepsError.into())
        }
    }

    pub fn get_state(&self) -> &str {
        &self.state.curr
    }

    pub fn set_navigate_target(&mut self, screen_name: &str) -> Result<(), Box<dyn Error>> {
        let res = self.pathfind(screen_name)?;

        self.navigate_plan = VecDeque::from(res);

        Ok(())
    }
}

pub struct PlanEngine<'a> {
    plan: &'a Plan,
    ocr: Arc<OcrEngine>,
    device: Arc<Mutex<ADBServerDevice>>,
    screen_engine: ScreenEngine,
    lua: Lua,
}

impl<'a> PlanEngine<'a> {
    pub fn new(plan: &'a Plan, device: Arc<Mutex<ADBServerDevice>>, ocr: Arc<OcrEngine>) -> Self {
        let lua = Lua::new();

        let device_table = lua.create_table().unwrap();
        let (d_1, d_2, d_3) = (device.clone(), device.clone(), device.clone());
        device_table
            .set(
                "tap",
                lua.create_function_mut(move |_, (x, y): (u32, u32)| {
                    println!("Tapping at {}, {}", x, y);
                    let mut device = d_1.lock().unwrap();
                    device.tap(x, y).unwrap();
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();
        device_table
            .set(
                "back",
                lua.create_function_mut(move |_, ()| {
                    println!("Pressing back");
                    let mut device = d_2.lock().unwrap();
                    device.back().unwrap();
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();
        lua.globals().set("device", device_table).unwrap();

        let screen_table = lua.create_table().unwrap();
        let ocr_1 = ocr.clone();
        screen_table
            .set(
                "try_idents",
                lua.create_function_mut(move |_, screen_names: Variadic<String>| {
                    // something something
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();
        screen_table
            .set(
                "ocr",
                lua.create_function(move |_, (x, y, width, height): (u32, u32, u32, u32)| {
                    let mut device = d_3.lock().unwrap();
                    let ocr = &ocr_1;
                    let screenshot = device.framebuffer_inner().unwrap();
                    Ok(run_ocr(ocr, screenshot, (x, y, width, height)).unwrap())
                })
                .unwrap(),
            )
            .unwrap();
        lua.globals().set("screen", screen_table).unwrap();

        Self {
            plan,
            ocr,
            device: device.clone(),
            screen_engine: ScreenEngine::from_plan(plan),
            lua,
        }
    }

    pub fn run_script(&mut self, path: &Path) -> Result<(), Box<dyn Error>> {
        self.lua.globals().raw_remove("run")?;
        let script = fs::read_to_string(self.plan.workdir.join(path))?;
        self.lua
            .load(&script)
            .set_name(path.to_string_lossy())
            .exec()?;
        let run_func = self.lua.globals().get::<_, Function>("run")?;
        run_func.call(())?;
        Ok(())
    }

    pub fn navigate_to(&mut self, screen_name: &str) -> Result<(), Box<dyn Error>> {
        self.screen_engine.set_navigate_target(screen_name)?;
        'engine_loop: loop {
            'engine_step: {
                let s = self.screen_engine.step()?;
                println!("stepping");
                match s {
                    ScreenEngineAction::Identify(idents) => {
                        let screenshot = self.device.lock().unwrap().framebuffer_inner()?;
                        for (name, ident) in idents {
                            if ident.ident_screen(self.plan, &self.ocr, screenshot.clone())? {
                                println!("identified screen {}", name);
                                self.screen_engine.mark_identified(&name);
                                break;
                            }
                        }
                        println!("No screen identified");
                    }
                    ScreenEngineAction::Navigate(name, to) => {
                        println!("Navigating to {}", name);
                        match to {
                            ScreenTo::Script(path) => {
                                println!("Running script {:?}", path);

                                match self.run_script(&path) {
                                    Ok(()) => (),
                                    Err(err) => {
                                        if let Some(mlua::Error::FromLuaConversionError {
                                            from: "nil",
                                            ..
                                        }) = err.downcast_ref::<mlua::Error>()
                                        {
                                            println!("⚠️ run function not found in script");
                                            break 'engine_step;
                                        }
                                        return Err(err);
                                    }
                                };
                            }
                            ScreenTo::Actions(vec) => {
                                let mut device = self.device.lock().unwrap();
                                for act in vec {
                                    match act {
                                        Actions::Tap(xpos, ypos) => {
                                            device.tap(xpos, ypos)?;
                                        }
                                        Actions::Back => {
                                            device.back()?;
                                        }
                                    }
                                }
                            }
                        }
                        self.screen_engine.step_navigate();
                    }
                    ScreenEngineAction::None => {
                        println!("No more steps needed");
                        break 'engine_loop;
                    }
                }
            }

            sleep(Duration::from_secs(10));
        }
        Ok(())
    }
}

trait WorkingScreenIdent {
    fn ident_screen(
        &self,
        plan: &Plan,
        ocr: &OcrEngine,
        screenshot: RgbaImageNew,
    ) -> Result<bool, Box<dyn Error>>;
}

impl WorkingScreenIdent for ScreenIdent {
    fn ident_screen(
        &self,
        plan: &Plan,
        ocr: &OcrEngine,
        screenshot: RgbaImageNew,
    ) -> Result<bool, Box<dyn Error>> {
        match self {
            ScreenIdent::RefMatch {
                reference: ref_image_path,
                rect,
            } => {
                let ref_image = ImageReader::open(plan.workdir.join(ref_image_path))?
                    .decode()?
                    .crop(rect.0, rect.1, rect.2, rect.3)
                    .to_luma32f();
                let screenshot = downgrade_image(screenshot);
                let screenshot = DynamicImage::from(screenshot)
                    .crop(
                        rect.0.saturating_sub(20),
                        rect.1.saturating_sub(20),
                        rect.2 + 20,
                        rect.3 + 20,
                    )
                    .to_luma32f();
                let m = match_template(
                    &screenshot,
                    &ref_image,
                    template_matching::MatchTemplateMethod::SumOfSquaredDifferences,
                );
                let extremes = find_extremes(&m);
                Ok(extremes.min_value < 250.0)
            }
            ScreenIdent::ImageMatch {
                image: image_path,
                pos,
            } => {
                let ref_image = ImageReader::open(plan.workdir.join(image_path))?
                    .decode()?
                    .to_luma32f();
                let screenshot = downgrade_image(screenshot);
                let screenshot = DynamicImage::from(screenshot)
                    .crop(
                        pos.0.saturating_sub(20),
                        pos.1.saturating_sub(20),
                        ref_image.width() + 20,
                        ref_image.height() + 20,
                    )
                    .to_luma32f();
                let m = match_template(
                    &screenshot,
                    &ref_image,
                    template_matching::MatchTemplateMethod::SumOfSquaredDifferences,
                );
                let extremes = find_extremes(&m);
                Ok(extremes.min_value < 250.0)
            }
            ScreenIdent::Ocr {
                ocr: ocr_target,
                operation,
                rect,
            } => {
                let text = run_ocr(ocr, screenshot, *rect)?;
                Ok(operation.run(&text, ocr_target))
            }
        }
    }
}

trait WorkingTextOp {
    fn run(&self, text: &str, target: &str) -> bool;
}

impl WorkingTextOp for TextOperation {
    fn run(&self, text: &str, target: &str) -> bool {
        match self {
            TextOperation::Exact => text == target,
            TextOperation::Contains => text.contains(target),
            TextOperation::StartsWith => text.starts_with(target),
            TextOperation::EndsWith => text.ends_with(target),
        }
    }
}

fn run_ocr(
    ocr: &OcrEngine,
    screenshot: RgbaImageNew,
    rect: (u32, u32, u32, u32),
) -> Result<String, Box<dyn Error>> {
    let screenshot = DynamicImageNew::from(screenshot)
        .crop(rect.0, rect.1, rect.2, rect.3)
        .to_rgb8();
    let screenshot = ImageSource::from_bytes(screenshot.as_raw(), screenshot.dimensions())?;
    let screenshot = ocr.prepare_input(screenshot)?;
    let text = ocr.get_text(&screenshot)?;
    Ok(text)
}
