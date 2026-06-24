use std::ops::{Sub, Add, Mul, Div};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::thread;
use std::sync::{mpsc};

use eframe::egui;
use egui::{Ui, Color32};

static MAX_HIST: usize = 100;
static UPDATE_TIME: Duration = Duration::from_millis(200);

fn main() -> eframe::Result {
	println!("Hello World!");

	let options = eframe::NativeOptions {
		viewport: egui::ViewportBuilder::default()
			.with_inner_size([500.0, 1000.0])
			.with_drag_and_drop(true),
		..Default::default()
	};
	eframe::run_native(
		"Process Monitor",
		options,
		Box::new(|_| {
			Ok(Box::new(MyApp::new()))
		}),
	)
}

trait Arithmetic: Copy + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> + Div<Output = Self> {}

fn map<T, U>(val: T, a_min: T, a_max: T, b_min: U, b_max: U) -> U
where
	T: Arithmetic + 'static + num_traits::AsPrimitive<U>,
	U: Arithmetic + 'static
{
	((val - a_min) / (a_max - a_min)).as_() * (b_max - b_min) + b_min
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Stats {
	memory: (usize, usize), // Used and available
}

fn get_memory() -> Option<(usize, usize)> {
	let proc = Command::new("free")
		.arg("-w")
		.stdout(Stdio::piped())
		.output().expect("Couldn't Create Thread")
		.stdout;

	if let Ok(output) = String::from_utf8(proc) {
		let parts = output.split(&[' ', '\n']).filter(|s| !s.is_empty()).collect::<Vec<_>>();
		let used = parts[9].parse();
		let avail = parts[14].parse();
		if let (Ok(used), Ok(avail)) = (used, avail) {
			return Some((used, avail));
		}
	}

	None
}

fn updater(tx: mpsc::Sender<Stats>) {
	let mut next_update = Instant::now();

	loop {
		let now = Instant::now();
		if now > next_update {
			next_update = now + UPDATE_TIME;

			if let Some((used, avail)) = get_memory() {
				let _ = tx.send(Stats {
					memory: (used, avail)
				});
			}
		}
	}
}

#[derive(Clone, Debug)]
struct History<T> {
	hist: [T; MAX_HIST],
	min: T,
	max: T,
	idx: usize,
}

impl<T: Default + Copy> Default for History<T> {
	fn default() -> Self {
		Self {
			hist: [T::default(); MAX_HIST],
			min: T::default(),
			max: T::default(),
			idx: 0,
		}
	}
}

impl<T> History<T> {
	fn top(&self) -> &T {
		&self.hist[self.idx]
	}
}

impl<T: Copy + Ord> History<T> {
	fn add(&mut self, item: T) {
		self.idx += 1;
		self.idx %= MAX_HIST;
		self.hist[self.idx] = item;
		if item > self.max { self.max = item; }
		if item < self.min { self.min = item; }
	}
}

impl<T: num_traits::AsPrimitive<f32> + Ord> History<T> {
	// TODO: Make this return a response
	fn draw(&self, ui: &mut Ui, stroke: egui::Stroke) {
		let size = egui::Rect::from_min_size(egui::Pos2::new(0.0, 0.0), egui::Vec2::new(500.0, 200.0)).translate(ui.cursor().min.to_vec2());
		let painter = ui.painter_at(size);
		ui.advance_cursor_after_rect(painter.clip_rect());

		// println!("{:?}", painter.clip_rect());

		painter.rect_stroke(painter.clip_rect(), 0, stroke, egui::StrokeKind::Inside);

		let height_scale = 100.0 / self.max.as_();
		let width_scale = 500.0 / self.hist.len() as f32;

		let start = painter.clip_rect().min.to_vec2();

		painter.line(self.hist.iter().enumerate().map(|(idx, v)| {
			let scaled_x: f32 = idx as f32 * width_scale;
			let scaled_y: f32 = (*v).as_() * height_scale;
			egui::Pos2::new(scaled_x, scaled_y) + start
		}).collect(), stroke);
	}
}

#[derive(Debug)]
struct MyApp {
	rx: mpsc::Receiver<Stats>,
	used_mem: History<usize>,
	avail_mem: History<usize>,
}

impl MyApp {
	fn new() -> Self {
		let (tx, rx) = mpsc::channel();
		thread::spawn(move || {
			updater(tx);
		});

		let mut obj = Self {
			rx,
			used_mem: Default::default(),
			avail_mem: Default::default(),
		};

		let proc = Command::new("free")
			.arg("-w")
			.stdout(Stdio::piped())
			.output().expect("Couldn't Create Thread")
			.stdout;

		if let Ok(output) = String::from_utf8(proc) {
			let parts = output.split(&[' ', '\n']).filter(|s| !s.is_empty()).collect::<Vec<_>>();
			let max = parts[8].parse();
			if let Ok(max) = max {
				obj.used_mem.max = max;
				obj.avail_mem.max = max;
			}
		}

		obj
	}
}

impl eframe::App for MyApp {
	// This is called every time the screen updates
	fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
		if let Ok(resp) = self.rx.try_recv() {
			self.used_mem.add(resp.memory.0);
			self.avail_mem.add(resp.memory.1);
		}

		ui.label(format!("{}", self.used_mem.top()));
		ui.label(format!("{}", self.avail_mem.top()));

		self.used_mem.draw(ui, egui::Stroke::new(1.0, Color32::WHITE));
		self.avail_mem.draw(ui, egui::Stroke::new(1.0, Color32::RED));

		ui.request_repaint_after(UPDATE_TIME);
	}
}