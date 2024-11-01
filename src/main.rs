use std::{
    collections::VecDeque,
    error::Error,
    fs,
    io::{stdout, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    sync::{Arc, Mutex, Weak},
    thread::sleep,
    time::Duration,
    vec::{self, Vec},
};

use adb_client::{ADBServer, ADBServerDevice};
use adb_device_ext::{ADBDeviceSimpleCommand, ADBServerTryConnectToDevice};
use chrono::Local;
use debug_gui::DebugData;
use def::{Config, Plan, Schedule};
use image::{io::Reader as ImageReader, DynamicImage, GrayImage, ImageBuffer, Luma, RgbaImage};
use image_stuff::{convert_luma_f32_to_u8, downgrade_image};
use imageproc::contrast::{otsu_level, threshold};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use mlua::{ExternalError, Function, Lua, Value::Nil};
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use plan_engine::ScreenEngine;
use regex::Regex;
use rten::Model;
use serde::Deserialize;
use template_matching::{find_extremes, match_template, MatchTemplateMethod};
mod adb_device_ext;
mod debug_gui;
mod def;
mod image_stuff;
mod plan_engine;

fn main() -> Result<(), Box<dyn Error>> {
    let userdata_path = Path::new("./userdata"); // TODO
    let config = def::Config::new(&userdata_path.join("config.toml"))?;

    let debug_gui = debug_gui::run()?;

    let detection_model = Model::load_file(userdata_path.join(&config.ocr.detection_model_path))?;
    let recognition_model =
        Model::load_file(userdata_path.join(&config.ocr.recognition_model_path))?;

    let ocr = OcrEngine::new(OcrEngineParams {
        detection_model: Some(detection_model),
        recognition_model: Some(recognition_model),
        ..Default::default()
    })?;
    let ocr = Arc::new(ocr);

    println!("Hello, world!");

    let plan_wd = PathBuf::from(&userdata_path.join("plans/azurlane")); // TODO

    let (plan, plan_warnings) = Plan::new(&plan_wd)?;
    for warning in plan_warnings {
        eprintln!("{}", warning);
    }

    let mut server = ADBServer::new(config.adb.host);
    let device = server.try_connect_to_device(&config)?;
    let device = Arc::new(Mutex::new(device));

    run_plan(device, ocr, &plan, Arc::downgrade(&debug_gui))?;

    return Ok(());

    println!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));

    // let mdns = ServiceDaemon::new().expect("should be able to create mdns service daemon");
    // let recv = mdns
    //     .browse("_adb-tls-connect._tcp.local.")
    //     .expect("mdns should be able to browse");

    // // Receive the browse events in sync or async. Here is
    // // an example of using a thread. Users can call `receiver.recv_async().await`
    // // if running in async environment.
    // std::thread::spawn(move || {
    //     while let Ok(event) = recv.recv() {
    //         match event {
    //             ServiceEvent::ServiceResolved(info) => {
    //                 println!("Resolved a new service: {}", info.get_fullname());
    //                 println!("{:?}", info)
    //             }

    //             other_event => {
    //                 println!("Received other event: {:?}", &other_event);
    //             }
    //         }
    //     }
    // });

    // // Gracefully shutdown the daemon.
    // std::thread::sleep(std::time::Duration::from_secs(10));
    // mdns.shutdown().unwrap();

    Ok(())
}

fn run_plan(
    device: Arc<Mutex<ADBServerDevice>>,
    ocr: Arc<OcrEngine>,
    plan: &Plan,
    debug_gui: Weak<Mutex<DebugData>>,
) -> Result<(), Box<dyn Error>> {
    {
        let mut dev = device.lock().unwrap();
        dev.stop_app(&plan.package)?;
        dev.start_app(&plan.package, &plan.activity)?;
    }

    let mut plan_engine = plan_engine::PlanEngine::new(plan, device, ocr, debug_gui);

    // println!("{:?}", engine.get_state());

    // lua.scope(|scope| {
    //     globals.set(
    //         "set_state",
    //         scope.create_function_mut(|_, args: String| {
    //             engine.set_state(args).unwrap();
    //             Ok(())
    //         })?,
    //     )?;
    //     lua.load(
    //         r#"
    //             set_state("title-1")
    //         "#,
    //     )
    //     .exec()
    // })?;

    // println!("{:?}", engine.get_state());
    for schedule in &plan.schedules {
        match &schedule.action {
            def::ScheduleActions::Routines(vec) => {
                for routine in vec {
                    let nav_target = plan.routine_location.get(routine).unwrap();
                    plan_engine.navigate_to(nav_target)?;
                    plan_engine.run_script(routine)?;
                }
            }
            def::ScheduleActions::Script(path) => {
                println!("Running script {:?}", path);
            }
        }
    }
    plan_engine.navigate_to("end")?;
    Ok(())
}
