use std::{
    error::Error,
    io::{stdout, Write},
    net::{Ipv4Addr, SocketAddrV4},
    thread::sleep,
    time::Duration,
    vec::Vec,
};

use adb_client::{ADBDeviceExt, ADBServer, ADBServerDevice};
use chrono::Local;
use regex::Regex;

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
        i += 1;
        sleep(Duration::from_secs(60));
    }
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
