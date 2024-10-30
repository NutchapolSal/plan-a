use std::{
    collections::VecDeque,
    error::Error,
    fs,
    io::{stdout, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    sync::{Arc, Mutex},
    thread::sleep,
    time::Duration,
    vec::{self, Vec},
};

use adb_client::{ADBServer, ADBServerDevice};
use adb_device_ext::ADBDeviceSimpleCommand;
use chrono::Local;
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
mod def;
mod image_stuff;
mod plan_engine;

fn main() -> Result<(), Box<dyn Error>> {
    let userdata_path = Path::new("./userdata"); // TODO
    let config = def::Config::new(&userdata_path.join("config.toml"))?;

    let detection_model = Model::load_file(userdata_path.join(&config.ocr.detection_model_path))?;
    let recognition_model =
        Model::load_file(userdata_path.join(&config.ocr.recognition_model_path))?;

    let ocr = OcrEngine::new(OcrEngineParams {
        detection_model: Some(detection_model),
        recognition_model: Some(recognition_model),
        ..Default::default()
    })?;
    let ocr = Arc::new(ocr);

    // let test_img = ImageReader::open("./userdata/plans/azurlane/assets/settings-header.png")?
    //     .decode()?
    //     .to_rgb8();
    // let test_img = ImageSource::from_bytes(test_img.as_raw(), test_img.dimensions())?;
    // let test_img = ocr.prepare_input(test_img)?;
    // let test_output = ocr.get_text(&test_img)?;
    // println!("{:?}", test_output);

    // return Ok(());

    println!("Hello, world!");

    let plan_wd = PathBuf::from(&userdata_path.join("plans/azurlane")); // TODO

    let plan: Plan = Plan::new(&plan_wd)?;
    println!("{:#?}", plan);

    let mut server = ADBServer::new(config.adb.host);
    let device = try_connect_to_device(&config, &mut server)?;
    let device = Arc::new(Mutex::new(device));

    run_plan(device, ocr, &plan)?;

    return Ok(());

    // globals.set(
    //     "sleep",
    //     lua.create_function(|_, seconds: f64| {
    //         sleep(Duration::from_secs_f64(seconds));
    //         Ok(())
    //     })?,
    // )?;

    // lua.load(
    //     r#"
    //     print("Hello from Lua!")
    //     sleep(1)
    // "#,
    // )
    // .exec()?;
    // println!("rust code");
    // lua.load(
    //     r#"
    //     print("More Lua!")
    //     sleep(1)
    // "#,
    // )
    // .exec()?;
    // println!("more rust code");

    // {
    //     let mut rust_val = 0;
    //     println!("{}", rust_val);
    //     lua.scope(|scope| {
    //         lua.globals().set(
    //             "sketchy",
    //             scope.create_function_mut(|_, val: i64| {
    //                 rust_val = val;
    //                 Ok(())
    //             })?,
    //         )?;

    //         lua.load(
    //             r#"
    //             sketchy(5)
    //         "#,
    //         )
    //         .exec()
    //     })?;

    //     println!("{}", rust_val);
    // }

    // return Ok(());

    println!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    // let mut i = 0;
    let mut state = "start";
    // loop {
    //     'inner: {
    //         // println!("{} {}", Local::now().format("%Y-%m-%d %H:%M:%S"), state);
    //         // let res = device.framebuffer_inner();
    //         // let image = match res {
    //         //     Err(err) => {
    //         //         println!("{:?}", err);
    //         //         break 'inner;
    //         //     }
    //         //     Ok(image) => image,
    //         // };
    //         // image.save(format!(
    //         //     "./temp/{}.png",
    //         //     Local::now().format("%Y-%m-%d %H-%M-%S")
    //         // ))?;

    //         // let image: RgbaImage = downgrade_image(image);
    //         // let image = DynamicImage::from(image).to_luma32f();

    //         // let state_data = plan.screens.get(state).unwrap();
    //         // if let Some(next) = state_data.next.as_ref() {
    //         //     let mut matches: Vec<_> = next
    //         //         .iter()
    //         //         .map(|n| {
    //         //             let next_state_data = plan.screens.get(n).unwrap();
    //         //             let path = PathBuf::from("./userdata/plans/azurlane/")
    //         //                 .join(next_state_data.ident.as_ref().unwrap());
    //         //             let template = ImageReader::open(path)?.decode()?.to_luma8();
    //         //             let template = DynamicImage::from(template).to_luma32f();

    //         //             let res = match_template(
    //         //                 &image,
    //         //                 &template,
    //         //                 MatchTemplateMethod::SumOfSquaredDifferences,
    //         //             );
    //         //             Ok::<_, Box<dyn Error>>((n, find_extremes(&res)))
    //         //         })
    //         //         .filter_map(|res| match res {
    //         //             Ok(res) => Some(res),
    //         //             Err(err) => {
    //         //                 println!("{:?}", err);
    //         //                 None
    //         //             }
    //         //         })
    //         //         .filter(|(_, ext_a)| ext_a.min_value < 1000.0)
    //         //         .collect();

    //         //     matches.sort_by(|(_, ext_a1), (_, ext_a2)| {
    //         //         ext_a1.min_value.partial_cmp(&ext_a2.min_value).unwrap()
    //         //     });

    //         //     if let Some((next, _)) = matches.first() {
    //         //         state = next;
    //         //         break 'inner;
    //         //     }
    //         // }

    //         // if let Some(to) = plan.screens.get(state).unwrap().to.as_ref() {
    //         //     let to = to.first().unwrap();
    //         //     for act in &to.act {
    //         //         match act {
    //         //             def::Actions::Tap(pos) => {
    //         //                 device.tap(pos[0], pos[1])?;
    //         //             }
    //         //         }
    //         //     }
    //         //     state = &to.state;
    //         //     break 'inner;
    //         // }
    //         // panic!("state should have next or to");
    //     }

    //     if state == "end" {
    //         break;
    //     }

    //     // i += 1;
    //     sleep(Duration::from_secs(15));
    // }

    // let mut echo_hello = Command::new("adb");
    // // echo_hello.arg("-h");
    // let hello_1 = echo_hello.output().expect("failed to execute process");
    // // let hello_2 = echo_hello.output().expect("failed to execute process");
    // println!("{:?}", hello_1);

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

    // template_matching_example(
    //     "./assets/weekly missions claimall.png",
    //     "./assets/claimall button.png",
    //     "./temp/a",
    // )?;
    // template_matching_example(
    //     "./assets/weekly missions claim.png",
    //     "./assets/claimall button.png",
    //     "./temp/b",
    // )?;
    Ok(())
}

fn run_plan(
    device: Arc<Mutex<ADBServerDevice>>,
    ocr: Arc<OcrEngine>,
    plan: &Plan,
) -> Result<(), Box<dyn Error>> {
    {
        let mut dev = device.lock().unwrap();
        dev.stop_app(&plan.package)?;
        dev.start_app(&plan.package, &plan.activity)?;
    }

    let mut plan_engine = plan_engine::PlanEngine::new(plan, device, ocr);

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
                    plan_engine.run_script(&plan.workdir.join(routine))?;
                }
            }
            def::ScheduleActions::Script(path) => {
                println!("Running script {:?}", path);
            }
        }
    }
    Ok(())
}

fn try_connect_to_device(
    config: &Config,
    server: &mut ADBServer,
) -> Result<ADBServerDevice, Box<dyn Error>> {
    let mut retry_connect_device = 0;
    loop {
        if 0 < retry_connect_device {
            let device_socket = SocketAddrV4::from_str(&config.adb.device_serial);
            match device_socket {
                Ok(device_socket) => {
                    println!("{:?}", server.connect_device(device_socket));
                }
                Err(_) => {
                    println!("{:?}", server.connect_device(config.adb.host));
                }
            }
            println!(
                "{:?}",
                server.connect_device(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 25), 5555))
            );
        }
        let device_temp = server.get_device_by_name(&config.adb.device_serial);
        match device_temp {
            Ok(d) => {
                break Ok(d);
            }
            Err(e) => {
                if 1 < retry_connect_device {
                    return Err(Box::new(e));
                }
                retry_connect_device += 1;
            }
        }
    }
}

fn template_matching_example(
    image_path: &str,
    template_path: &str,
    output_path: &str,
) -> Result<(), Box<dyn Error>> {
    let image = ImageReader::open(image_path)?.decode()?.to_luma8();
    let image = DynamicImage::from(image).to_luma32f();
    let template = ImageReader::open(template_path)?.decode()?.to_luma8();
    let template = DynamicImage::from(template).to_luma32f();
    let out = match_template(
        &image,
        &template,
        MatchTemplateMethod::SumOfSquaredDifferences,
    );
    let ext_a = find_extremes(&out);
    println!("{:?}", ext_a);
    let out = convert_luma_f32_to_u8(
        ImageBuffer::<Luma<f32>, Vec<f32>>::from_raw(out.width, out.height, out.data.to_vec())
            .unwrap(),
        ext_a.max_value,
    );
    DynamicImage::from(out)
        .to_luma8()
        .save(format!("{output_path} out.png"))?;
    Ok(())
}
