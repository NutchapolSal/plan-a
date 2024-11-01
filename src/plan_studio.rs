use adb_client::{ADBServer, ADBServerDevice};
use adb_device_ext::ADBServerTryConnectToDevice;
use chrono::Local;
use def::{deser_idents, Plan, ScreenIdent};
use eframe::egui::{self, Pos2, Sense};
use image_new::ImageReader;
use itertools::Itertools;
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use plan_engine::WorkingScreenIdent;
use rten::Model;
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, mpsc, Arc, Mutex, Weak},
    thread,
    time::Duration,
};
mod adb_device_ext;
mod debug_gui;
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

    println!("Hello, world!");

    let plan_wd = PathBuf::from(&userdata_path.join("plans/azurlane")); // TODO

    let (plan, plan_warnings) = Plan::new(&plan_wd)?;
    for warning in plan_warnings {
        eprintln!("{}", warning);
    }

    let mut server = ADBServer::new(config.adb.host);
    let device = server.try_connect_to_device(&config)?;
    // let device = Arc::new(Mutex::new(device));

    eframe::run_native(
        "Plan Studio",
        Default::default(),
        Box::new(move |cc| {
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Ok(Box::new(MyApp::new(ocr, device, plan, userdata_path)))
        }),
    )?;
    Ok(())
}

enum IdentRun {
    Parsed(Vec<ScreenIdent>),
    Manual(String),
}

struct MyApp {
    ocr: Arc<OcrEngine>,
    plan: Arc<Mutex<Plan>>,
    scr_tx: mpsc::Sender<()>,
    pln_tx: mpsc::Sender<()>,
    idn_tx: mpsc::Sender<IdentRun>,
    selected_screen: String,
    manual_ident: String,
    counter: usize,
    ident_result: Arc<Mutex<Result<Vec<bool>, String>>>,
    current_image: Arc<Mutex<Option<PathBuf>>>,
    screenshot_loading: Arc<AtomicBool>,
    clicked_poses: Vec<Pos2>,
    hover_pos: Option<Pos2>,
}

impl MyApp {
    fn new(ocr: OcrEngine, mut device: ADBServerDevice, plan: Plan, userdata_path: &Path) -> Self {
        let current_image = Arc::new(Mutex::new(None));
        let screenshot_loading = Arc::new(AtomicBool::new(false));
        let plan = Arc::new(Mutex::new(plan));
        let ocr = Arc::new(ocr);
        let ident_result: Arc<Mutex<Result<Vec<bool>, String>>> =
            Arc::new(Mutex::new(Ok(Vec::new())));

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

        let plan_1 = plan.clone();
        let udpath_1 = userdata_path.to_path_buf();
        let idrs_1 = ident_result.clone();
        let (pln_tx, pln_rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            let userdata_path = udpath_1;
            loop {
                let Ok(()) = pln_rx.recv() else {
                    break;
                };

                let plan_wd = PathBuf::from(&userdata_path.join("plans/azurlane")); // TODO
                let (plan, plan_warnings) = match Plan::new(&plan_wd) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("Plan Parse Error: {:?}", e);
                        continue;
                    }
                };
                for warning in plan_warnings {
                    eprintln!("{}", warning);
                }

                *plan_1.lock().unwrap() = plan;
            }
        });

        let (idn_tx, idn_rx) = mpsc::channel::<IdentRun>();
        let ocr_1 = ocr.clone();
        let plan_2 = plan.clone();
        let current_image_2 = current_image.clone();
        thread::spawn(move || {
            while let Ok(ident_run) = idn_rx.recv() {
                let idents = match ident_run {
                    IdentRun::Parsed(idents) => Ok(idents),
                    IdentRun::Manual(text) => deser_idents(&text),
                };
                let idents = match idents {
                    Ok(v) => v,
                    Err(e) => {
                        *idrs_1.lock().unwrap() = Err(format!("{:?}", e));
                        continue;
                    }
                };

                let plan = plan_2.lock().unwrap();
                let image =
                    match ImageReader::open(current_image_2.lock().unwrap().as_ref().unwrap())
                        .map_err(|e| e.into())
                        .and_then(|v| v.decode())
                    {
                        Ok(v) => v,
                        Err(e) => {
                            *idrs_1.lock().unwrap() = Err(format!("{:?}", e));
                            continue;
                        }
                    };
                let reses = idents
                    .iter()
                    .map(|id| id.ident_screen(&plan, &ocr_1, image.to_rgba8(), Weak::new()))
                    .collect::<Vec<_>>();
                if let Some(e) = reses.iter().find(|v| v.is_err()) {
                    *idrs_1.lock().unwrap() = Err(format!("{:?}", e.as_ref().unwrap_err()));
                    continue;
                } else {
                    *idrs_1.lock().unwrap() =
                        Ok(reses.iter().map(|v| *v.as_ref().unwrap()).collect());
                }
            }
        });

        Self {
            ocr,
            scr_tx,
            plan,
            pln_tx,
            idn_tx,
            ident_result,
            counter: 0,
            current_image,
            screenshot_loading,
            clicked_poses: Default::default(),
            hover_pos: Default::default(),
            selected_screen: Default::default(),
            manual_ident: Default::default(),
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

    fn reload_plan(&mut self) -> Result<(), Box<dyn Error>> {
        self.pln_tx.send(())?;
        Ok(())
    }

    fn run_idents(&mut self, idr: IdentRun) -> Result<(), Box<dyn Error>> {
        self.idn_tx.send(idr)?;
        Ok(())
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(1000));
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.columns(2, |columns| {
                    let [ref mut ui_a, ref mut ui_b] = columns else {
                        return;
                    };

                    ui_a.horizontal(|ui| {
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
                    ui_a.horizontal(|ui| {
                        ui.label("Clicked Pos history:");
                        for pos in self.clicked_poses.iter().rev().take(5).rev() {
                            ui.label(format!("[{}, {}]", pos.x, pos.y));
                        }
                    });
                    if let Some(pos) = self.hover_pos {
                        ui_a.label(format!("Hover Pos: [{}, {}]", pos.x, pos.y));
                    } else {
                        ui_a.label("Hover Pos: None");
                    }

                    ui_b.horizontal(|ui| {
                        if ui.button("reload plan").clicked() {
                            self.reload_plan().unwrap();
                        }
                        egui::ComboBox::from_label("Select screenident")
                            .selected_text(&self.selected_screen)
                            .show_ui(ui, |ui| {
                                let plan = self.plan.lock().unwrap();
                                plan.screens.iter().sorted_by_key(|v| v.0).for_each(
                                    |(name, scr)| {
                                        if scr.ident.is_empty() {
                                            return;
                                        }
                                        ui.selectable_value(
                                            &mut self.selected_screen,
                                            name.to_owned(),
                                            name,
                                        );
                                    },
                                );
                            });
                        if ui.button("run idents").clicked() {
                            let ids = self
                                .plan
                                .lock()
                                .unwrap()
                                .screens
                                .get(&self.selected_screen)
                                .unwrap()
                                .ident
                                .clone();
                            self.run_idents(IdentRun::Parsed(ids)).unwrap();
                        }
                    });

                    ui_b.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.manual_ident);
                        if ui.button("run manual").clicked() {
                            self.run_idents(IdentRun::Manual(self.manual_ident.clone()))
                                .unwrap();
                        }
                    });
                    ui_b.horizontal(|ui| {
                        match self.ident_result.lock().unwrap().as_ref() {
                            Ok(res) => {
                                ui.label("Result:");
                                for v in res.iter() {
                                    ui.label(if *v { "✅" } else { "❌" });
                                }
                            }
                            Err(e) => {
                                ui.label(format!("Error: {:?}", e));
                            }
                        };
                    });
                });

                let scrollarea_corner = Pos2::default() - ui.next_widget_position();
                egui::ScrollArea::both().show_viewport(ui, |ui, rect| {
                    if let Some(path) = self.current_image.lock().unwrap().as_ref() {
                        let image = egui::Image::new(format!("file://{}", path.to_str().unwrap()))
                            .fit_to_original_size(1.0);
                        let res = ui.add(image).interact(Sense::click());
                        let hover_pos = res
                            .hover_pos()
                            .map(|v| v + rect.min.to_vec2() + scrollarea_corner);

                        if res.clicked() {
                            if let Some(hover_pos) = hover_pos {
                                self.clicked_poses.push(hover_pos);
                            }
                        }
                        if res.clicked_by(egui::PointerButton::Secondary) {
                            if let Some((last, hover_pos)) =
                                self.clicked_poses.last().zip(hover_pos)
                            {
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
