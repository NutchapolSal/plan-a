use std::{net::SocketAddrV4, str::FromStr, vec::Vec};

use adb_client::{ADBDeviceExt, ADBServer, ADBServerDevice};

use std::error::Error;

use crate::def::Config;

pub(crate) trait ADBDeviceRunCommand {
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

pub(crate) trait ADBDeviceSimpleCommand {
    fn tap(&mut self, x: u32, y: u32) -> Result<(), Box<dyn Error>>;
    fn back(&mut self) -> Result<(), Box<dyn Error>>;
    fn start_app(&mut self, package: &str, activity: &str) -> Result<(), Box<dyn Error>>;
    fn stop_app(&mut self, package: &str) -> Result<(), Box<dyn Error>>;
}

impl ADBDeviceSimpleCommand for ADBServerDevice {
    fn tap(&mut self, x: u32, y: u32) -> Result<(), Box<dyn Error>> {
        self.shell_command(
            vec!["input", "tap", &x.to_string(), &y.to_string()],
            &mut Vec::new(),
        )?;
        Ok(())
    }

    fn back(&mut self) -> Result<(), Box<dyn Error>> {
        self.shell_command(vec!["input", "keyevent", "KEYCODE_BACK"], &mut Vec::new())?;
        Ok(())
    }

    fn start_app(&mut self, package: &str, activity: &str) -> Result<(), Box<dyn Error>> {
        self.shell_command(
            vec![
                "am",
                "start",
                "-c",
                "android.intent.category.LAUNCHER",
                "-a",
                "android.intent.action.MAIN",
                &format!("{}/{}", package, activity),
            ],
            &mut Vec::new(),
        )?;
        Ok(())
    }

    fn stop_app(&mut self, package: &str) -> Result<(), Box<dyn Error>> {
        self.shell_command(vec!["am", "stop-app", package], &mut Vec::new())?;
        Ok(())
    }
}

pub trait ADBServerTryConnectToDevice {
    fn try_connect_to_device(&mut self, config: &Config)
        -> Result<ADBServerDevice, Box<dyn Error>>;
}

impl ADBServerTryConnectToDevice for ADBServer {
    fn try_connect_to_device(
        &mut self,
        config: &Config,
    ) -> Result<ADBServerDevice, Box<dyn Error>> {
        let mut retry_connect_device = 0;
        loop {
            if 0 < retry_connect_device {
                let device_socket = SocketAddrV4::from_str(&config.adb.device_serial);
                match device_socket {
                    Ok(device_socket) => {
                        println!("{:?}", self.connect_device(device_socket));
                    }
                    Err(_) => {
                        println!("{:?}", self.connect_device(config.adb.host));
                    }
                }
            }
            let device_temp = self.get_device_by_name(&config.adb.device_serial);
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
}
