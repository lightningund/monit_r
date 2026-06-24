use std::ops::{Sub, Add, Mul, Div};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::thread;
use std::sync::{mpsc};

use eframe::egui;
use egui::{Ui, Color32, Pos2, Vec2, Rect};

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
impl<T> Arithmetic for T where T: Copy + Add<Output = T> + Sub<Output = T> + Mul<Output = T> + Div<Output = T> {}

fn map<T, U>(val: T, a_min: T, a_max: T, b_min: U, b_max: U) -> U
where
	T: Copy + num_traits::AsPrimitive<U>,
	U: Arithmetic + 'static
{
	((val.as_() - a_min.as_()) / (a_max.as_() - a_min.as_())) * (b_max - b_min) + b_min
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Stats {
	memory: (usize, usize), // Used and available
}

fn get_max_memory() -> Option<usize> {
	let proc = Command::new("free")
		.arg("-w")
		.stdout(Stdio::piped())
		.output().expect("Couldn't Create Thread")
		.stdout;

	if let Ok(output) = String::from_utf8(proc) {
		let parts = output.split(&[' ', '\n']).filter(|s| !s.is_empty()).collect::<Vec<_>>();
		let max = parts[8].parse();
		return max.ok();
	}

	None
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

impl<T: Arithmetic + num_traits::AsPrimitive<f32> + Ord> History<T> {
	// TODO: Make this return a response
	fn draw(&self, ui: &mut Ui, stroke: egui::Stroke) {
		let size = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(500.0, 200.0)).translate(ui.cursor().min.to_vec2());
		let painter = ui.painter_at(size);
		ui.advance_cursor_after_rect(painter.clip_rect());

		painter.rect_stroke(painter.clip_rect(), 0, stroke, egui::StrokeKind::Inside);

		let Rect{ min: Pos2{x: ax, y: ay}, max: Pos2{x: bx, y: by} } = painter.clip_rect();

		painter.line(self.hist.iter().enumerate().map(|(idx, v)| {
			let scaled_x: f32 = map(idx, 0, self.hist.len(), ax, bx);
			let scaled_y: f32 = map(*v, self.min, self.max, ay, by);
			Pos2::new(scaled_x, scaled_y)
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

		if let Some(max) = get_max_memory() {
			obj.used_mem.max = max;
			obj.avail_mem.max = max;
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