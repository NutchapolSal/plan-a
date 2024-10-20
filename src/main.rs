use std::{
    collections::VecDeque,
    error::Error,
    fs,
    io::{stdout, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    thread::sleep,
    time::Duration,
    vec::Vec,
};

use adb_client::{ADBServer, ADBServerDevice};
use chrono::Local;
use def::Plan;
use image::{io::Reader as ImageReader, DynamicImage, GrayImage, ImageBuffer, Luma, RgbaImage};
use imageproc::contrast::{otsu_level, threshold};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use regex::Regex;
use serde::Deserialize;
use template_matching::{find_extremes, match_template, MatchTemplateMethod};
mod def;

fn convert_luma_f32_to_u8(image: ImageBuffer<Luma<f32>, Vec<f32>>, max_value: f32) -> GrayImage {
    // Create a new image with u8 pixels
    let (width, height) = image.dimensions();

    // Map each pixel from f32 to u8, scaling and clamping as necessary
    ImageBuffer::from_fn(width, height, |x, y| {
        let Luma([pixel_value]) = image.get_pixel(x, y);
        // Scale f32 to u8 (assuming f32 values are in 0.0..1.0 range)
        let scaled_value = ((pixel_value / max_value) * 255.0) as u8;
        Luma([scaled_value])
    })
}

const SERIAL: &str = "192.168.1.25:5555";

fn main() -> Result<(), Box<dyn Error>> {
    println!("Hello, world!");

    let plan: Plan =
        toml::from_str(fs::read_to_string("./userdata/plans/azurlane/plan.toml")?.as_str())?;
    println!("{:?}", plan);

    let mut server = ADBServer::new(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 101), 5037));

    let mut device: ADBServerDevice;

    let mut retry_connect_device = 0;
    loop {
        if 0 < retry_connect_device {
            println!(
                "{:?}",
                server.connect_device(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 25), 5555))
            );
        }
        let device_temp = server.get_device_by_name(SERIAL);
        match device_temp {
            Ok(d) => {
                device = d;
                break;
            }
            Err(e) => {
                println!("{:?}", e);
                if 1 < retry_connect_device {
                    panic!("Failed to connect to device");
                }
                retry_connect_device += 1;
            }
        }
    }

    println!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    // let mut i = 0;
    let mut state = "start";
    loop {
        'inner: {
            println!("{} {}", Local::now().format("%Y-%m-%d %H:%M:%S"), state);
            let res = device.framebuffer_inner();
            let image = match res {
                Err(err) => {
                    println!("{:?}", err);
                    break 'inner;
                }
                Ok(image) => image,
            };
            image.save(format!(
                "./temp/{}.png",
                Local::now().format("%Y-%m-%d %H-%M-%S")
            ))?;

            // convert from image v0.25.2 struct to image v0.24.9 struct
            let image: RgbaImage =
                ImageBuffer::from_vec(image.width(), image.height(), image.into_raw()).unwrap();
            let image = DynamicImage::from(image).to_luma32f();

            let state_data = plan.states.get(state).unwrap();
            if let Some(next) = state_data.next.as_ref() {
                let mut matches: Vec<_> = next
                    .iter()
                    .map(|n| {
                        let next_state_data = plan.states.get(n).unwrap();
                        let path = PathBuf::from("./userdata/plans/azurlane/")
                            .join(next_state_data.ident.as_ref().unwrap());
                        let template = ImageReader::open(path)?.decode()?.to_luma8();
                        let template = DynamicImage::from(template).to_luma32f();

                        let res = match_template(
                            &image,
                            &template,
                            MatchTemplateMethod::SumOfSquaredDifferences,
                        );
                        Ok::<_, Box<dyn Error>>((n, find_extremes(&res)))
                    })
                    .filter_map(|res| match res {
                        Ok(res) => Some(res),
                        Err(err) => {
                            println!("{:?}", err);
                            None
                        }
                    })
                    .filter(|(_, ext_a)| ext_a.min_value < 1000.0)
                    .collect();

                matches.sort_by(|(_, ext_a1), (_, ext_a2)| {
                    ext_a1.min_value.partial_cmp(&ext_a2.min_value).unwrap()
                });

                if let Some((next, _)) = matches.first() {
                    state = next;
                    break 'inner;
                }
            }

            if let Some(to) = plan.states.get(state).unwrap().to.as_ref() {
                let to = to.first().unwrap();
                for act in &to.act {
                    match act {
                        def::Actions::Tap(pos) => {
                            device.tap(pos[0], pos[1])?;
                        }
                    }
                }
                state = &to.state;
                break 'inner;
            }
            panic!("state should have next or to");
        }

        if state == "end" {
            break;
        }

        // i += 1;
        sleep(Duration::from_secs(15));
    }

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

#[derive(Debug, Clone)]
struct LevelOutputParseError {
    output: String,
}

impl Error for LevelOutputParseError {}

impl std::fmt::Display for LevelOutputParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "can't parse level: {}", self.output)
    }
}

fn run_charge_separation_iteration(
    device: &mut ADBServerDevice,
    level_regex: &Regex,
) -> Result<(), Box<dyn Error>> {
    let output = device.run_command(vec!["dumpsys battery"])?;

    let caps = level_regex.captures(&output);
    let level = caps
        .and_then(|caps| caps.get(1))
        .and_then(|level| level.as_str().parse::<u8>().ok());
    let Some(level) = level else {
        return Err(Box::new(LevelOutputParseError { output }));
    };

    if level < 70 {
        device.run_command(vec!["settings put global charge_separation_switch 0"])?;
        print!(",")
    } else {
        let charge_separation_switch =
            device.run_command(vec!["settings get global charge_separation_switch"])?;
        let charge_separation_switch = charge_separation_switch.trim();
        if charge_separation_switch == "0" {
            device.run_command(vec!["settings put global charge_separation_switch 1"])?;
            println!(
                "charge_separation_switch=0 {}",
                Local::now().format("%Y-%m-%d %H:%M:%S")
            );
        } else {
            print!(".");
        }
    }
    stdout().lock().flush()?;
    Ok(())
}

trait ADBDeviceRunCommand {
    fn run_command<S>(
        &mut self,
        command: impl IntoIterator<Item = S>,
    ) -> Result<String, Box<dyn Error>>
    where
        S: ToString;
}

impl ADBDeviceRunCommand for ADBServerDevice {
    fn run_command<S>(
        &mut self,
        command: impl IntoIterator<Item = S>,
    ) -> Result<String, Box<dyn Error>>
    where
        S: ToString,
    {
        let mut output = Vec::new();
        self.shell_command(command, &mut output)?;
        Ok(String::from_utf8(output)?)
    }
}

trait ADBDeviceSimpleCommand {
    fn tap(&mut self, x: u32, y: u32) -> Result<(), Box<dyn Error>>;
}

impl ADBDeviceSimpleCommand for ADBServerDevice {
    fn tap(&mut self, x: u32, y: u32) -> Result<(), Box<dyn Error>> {
        self.shell_command(
            vec!["input", "tap", &x.to_string(), &y.to_string()],
            &mut Vec::new(),
        )?;
        Ok(())
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
