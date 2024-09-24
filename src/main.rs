use std::{error::Error, process::Command};

use image::{io::Reader as ImageReader, GrayImage, ImageBuffer, Luma};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use template_matching::{find_extremes, match_template, MatchTemplateMethod};

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

    let image_a = ImageReader::open("./assets/weekly missions claimall.png")?
        .decode()?
        .to_luma32f();
    println!("a");
    let image_b = ImageReader::open("./assets/weekly missions claim.png")?
        .decode()?
        .to_luma32f();
    println!("b");
    let template = ImageReader::open("./assets/claimall button.png")?
        .decode()?
        .to_luma32f();
    println!("c");
    let out_a = match_template(
        &image_a,
        &template,
        MatchTemplateMethod::SumOfSquaredDifferences,
    );
    println!("d");
    let out_b = match_template(
        &image_b,
        &template,
        MatchTemplateMethod::SumOfSquaredDifferences,
    );
    println!("e");
    // out_a.save("./temp/out a.png")?;
    println!("{:?}", find_extremes(&out_a));
    println!("f");
    // out_b.save("./temp/out b.png")?;
    println!("{:?}", find_extremes(&out_b));
    Ok(())
}
