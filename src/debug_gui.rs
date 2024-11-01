use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use chrono::Local;
use eframe::egui;
use image::{DynamicImage, ImageBuffer};
use image_new::DynamicImage as DynamicImageNew;
use winit::platform::windows::EventLoopBuilderExtWindows;

pub fn run() -> Result<Arc<Mutex<DebugData>>, Box<dyn std::error::Error>> {
    let mut data = DebugData::default();
    data.items
        .push(DebugItem::Text("Hello, world!".to_string()));
    let data = Arc::new(Mutex::new(data));
    let data2 = data.clone();
    thread::spawn(move || {
        let event_loop_builder: Option<eframe::EventLoopBuilderHook> =
            Some(Box::new(|event_loop_builder| {
                event_loop_builder.with_any_thread(true);
            }));
        eframe::run_native(
            "Plan-A",
            eframe::NativeOptions {
                event_loop_builder,
                ..Default::default()
            },
            Box::new(move |cc| {
                egui_extras::install_image_loaders(&cc.egui_ctx);
                Ok(Box::new(MyApp { datas: data2 }))
            }),
        )
        .unwrap();
    });

    Ok(data)
}

#[derive(Debug, Default)]
struct MyApp {
    datas: Arc<Mutex<DebugData>>,
}

#[derive(Debug, Default)]
pub struct DebugData {
    items: Vec<DebugItem>,
    trigger: bool,
    counter: usize,
}

impl DebugData {
    pub fn push_text(&mut self, text: &str) {
        self.items.push(DebugItem::Text(text.to_string()));
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

    pub fn push_image(&mut self, image: DynamicImage) {
        let path = self.get_next_image_path();
        image.to_rgba8().save(&path).unwrap();
        self.items.push(DebugItem::Image(path));
    }
    pub fn push_image_new(&mut self, image: DynamicImageNew) {
        let path = self.get_next_image_path();
        image.save(&path).unwrap();
        self.items.push(DebugItem::Image(path));
    }
}

#[derive(Debug)]
pub enum DebugItem {
    Text(String),
    Image(PathBuf),
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(1000));
        egui::CentralPanel::default().show(ctx, |ui| {
            if ui.button("Trigger").clicked() {
                self.datas.lock().unwrap().trigger = true;
            }

            if ui.button("Clear").clicked() {
                self.datas.lock().unwrap().items.clear();
            }

            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for item in self.datas.lock().unwrap().items.iter() {
                        match item {
                            DebugItem::Text(text) => {
                                ui.label(text);
                            }
                            DebugItem::Image(path) => {
                                let img_widget =
                                    egui::Image::new(format!("file://{}", path.to_string_lossy()))
                                        .fit_to_original_size(1.0);
                                ui.add(img_widget);
                            }
                        }
                    }
                });
        });
    }
}
