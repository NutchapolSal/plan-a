use adb_client::{ADBServer, ADBServerDevice};
use adb_device_ext::ADBServerTryConnectToDevice;
use chrono::Local;
use def::Plan;
use eframe::egui::{self, Pos2, Sense};
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, mpsc, Arc, Mutex},
    thread,
};
mod adb_device_ext;
mod def;

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

    println!("Hello, world!");

    let plan_wd = PathBuf::from(&userdata_path.join("plans/azurlane")); // TODO

    let plan: Plan = Plan::new(&plan_wd)?;
    println!("{:#?}", plan);

    let mut server = ADBServer::new(config.adb.host);
    let device = server.try_connect_to_device(&config)?;
    // let device = Arc::new(Mutex::new(device));

    eframe::run_native(
        "Plan Studio",
        Default::default(),
        Box::new(move |cc| {
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Ok(Box::new(MyApp::new(ocr, device)))
        }),
    )?;
    Ok(())
}

struct MyApp {
    ocr: OcrEngine,
    scr_tx: mpsc::Sender<()>,
    counter: usize,
    current_image: Arc<Mutex<Option<PathBuf>>>,
    screenshot_loading: Arc<AtomicBool>,
    clicked_poses: Vec<Pos2>,
    hover_pos: Option<Pos2>,
}

impl MyApp {
    fn new(ocr: OcrEngine, mut device: ADBServerDevice) -> Self {
        let current_image = Arc::new(Mutex::new(None));
        let screenshot_loading = Arc::new(AtomicBool::new(false));

        let current_image_1 = current_image.clone();
        let screenshot_loading_1 = screenshot_loading.clone();
        let (scr_tx, scr_rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            let mut counter = 0;
            loop {
                let Ok(()) = scr_rx.recv() else {
                    break;
                };
                screenshot_loading_1.store(true, std::sync::atomic::Ordering::Relaxed);
                counter += 1;
                let time = Local::now();
                let path = PathBuf::from(format!(
                    "./temp/{}/{} {}.png",
                    time.format("%Y-%m-%d"),
                    time.format("%Y-%m-%d %H-%M-%S"),
                    counter
                ));
                fs::create_dir_all(path.parent().unwrap()).unwrap();

                let image = device.framebuffer_inner().unwrap();
                image.save(&path).unwrap();
                *current_image_1.lock().unwrap() = Some(path);
                screenshot_loading_1.store(false, std::sync::atomic::Ordering::Relaxed);
            }
        });

        Self {
            ocr,
            scr_tx,
            counter: 0,
            current_image,
            screenshot_loading,
            clicked_poses: Default::default(),
            hover_pos: Default::default(),
        }
    }

    fn get_next_image_path(&mut self) -> PathBuf {
        self.counter += 1;
        let time = Local::now();
        let path = PathBuf::from(format!(
            "./temp/{}/{} {}.png",
            time.format("%Y-%m-%d"),
            time.format("%Y-%m-%d %H-%M-%S"),
            self.counter
        ));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        path
    }

    fn capture_screenshot(&mut self) -> Result<(), Box<dyn Error>> {
        self.scr_tx.send(())?;
        Ok(())
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    if ui.button("Capture Screenshot").clicked() {
                        self.capture_screenshot().unwrap();
                    }
                    if self
                        .screenshot_loading
                        .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        ui.spinner();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Clicked Pos history:");
                    for pos in self.clicked_poses.iter().rev().take(5).rev() {
                        ui.label(format!("[{}, {}]", pos.x, pos.y));
                    }
                });
                if let Some(pos) = self.hover_pos {
                    ui.label(format!("Hover Pos: [{}, {}]", pos.x, pos.y));
                } else {
                    ui.label("Hover Pos: None");
                }

                let scrollarea_corner = Pos2::default() - ui.next_widget_position();
                egui::ScrollArea::both().show_viewport(ui, |ui, rect| {
                    if let Some(path) = self.current_image.lock().unwrap().as_ref() {
                        let image = egui::Image::new(format!("file://{}", path.to_str().unwrap()))
                            .fit_to_original_size(1.0);
                        let res = ui.add(image).interact(Sense::click());
                        let hover_pos = res.hover_pos().map(|v| v + rect.min.to_vec2() + scrollarea_corner);
                        
                        if res.clicked() {
                            if let Some(hover_pos) = hover_pos {
                                self.clicked_poses.push(hover_pos);
                            }
                        }
                        if res.clicked_by(egui::PointerButton::Secondary) {
                            if let Some((last, hover_pos)) = self.clicked_poses.last().zip(hover_pos) {
                                self.clicked_poses.push((hover_pos - *last).to_pos2());
                            }
                        }
                        self.hover_pos = hover_pos;
                    }
                });
            });

            // ui.heading("My egui Application");
            // ui.horizontal(|ui| {
            //     let name_label = ui.label("Your name: ");
            //     ui.text_edit_singleline(&mut self.name)
            //         .labelled_by(name_label.id);
            // });
            // ui.add(egui::Slider::new(&mut self.age, 0..=120).text("age"));
            // if ui.button("Increment").clicked() {
            //     self.age += 1;
            // }
            // ui.label(format!("Hello '{}', age {}", self.name, self.age));

            // ui.image(egui::include_image!(
            //     "../../../crates/egui/assets/ferris.png"
            // ));
        });
    }
}
