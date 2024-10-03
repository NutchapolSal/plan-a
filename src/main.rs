use std::{
    collections::VecDeque,
    error::Error,
    io::{stdout, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    process::Command,
    sync::{Arc, Mutex},
    thread::sleep,
    time::Duration,
    vec::Vec,
};

use adb_client::{ADBServer, ADBServerDevice};
use chrono::Local;
use image::{io::Reader as ImageReader, DynamicImage, GrayImage, ImageBuffer, Luma};
use imageproc::contrast::{otsu_level, threshold};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use regex::Regex;
use template_matching::{find_extremes, match_template, MatchTemplateMethod};

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
    let level_regex = Regex::new(r"level: (\d+)").unwrap();

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
    let mut i = 0;
    loop {
        if i % 3 == 0 {
            match run_charge_separation_iteration(&mut device, &level_regex) {
                Ok(_) => {}
                Err(e) => {
                    println!("{:?}", e);
                }
            };
        }
        let image = device.framebuffer_inner()?;
        image.save("./temp/screen.png")?;
        i += 1;
        sleep(Duration::from_secs(60));
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

fn template_matching_example(
    image_path: &str,
    template_path: &str,
    output_path: &str,
) -> Result<(), Box<dyn Error>> {
    let image = ImageReader::open(image_path)?.decode()?.to_luma8();
    let image = threshold(&image, otsu_level(&image));
    image.save(format!("{output_path} image threshold.png"))?;
    let image = DynamicImage::from(image).to_luma32f();
    let template = ImageReader::open(template_path)?.decode()?.to_luma8();
    let template = threshold(&template, otsu_level(&template));
    template.save(format!("{output_path} template threshold.png"))?;
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
