use eframe::egui;
use egui::{Color32, Button, Ui, ColorImage, Rect};

fn main() -> eframe::Result {
	let options = eframe::NativeOptions {
		viewport: egui::ViewportBuilder::default()
			.with_inner_size([500.0, 500.0])
			.with_drag_and_drop(true),
		..Default::default()
	};
	eframe::run_native(
		"Process Monitor",
		options,
		Box::new(|cc| {
			Ok(Box::new(MyApp::default()))
		}),
	)
}

#[derive(Default, Debug)]
struct MyApp {}

impl eframe::App for MyApp {
	// This is called every time the screen updates
	fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {

	}
}