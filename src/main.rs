use std::{error::Error, process::Command};
use mdns_sd::{ServiceDaemon, ServiceEvent};
fn main() -> Result<(), Box<dyn Error>> {
    println!("Hello, world!");

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
    Ok(())
}
