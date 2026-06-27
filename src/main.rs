use std::io::{BufRead, BufReader};
use std::ops::{Sub, Add, Mul, Div};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::thread;
use std::sync::{mpsc, RwLock};

use eframe::egui;
use egui::{Ui, Color32, Pos2, Rect};

static MAX_HIST: usize = 500;
static UPDATE_TIME: Duration = Duration::from_millis(200);
static SCREEN_UPDATE: Duration = Duration::from_millis(10);

fn main() -> eframe::Result {
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
impl<T: Copy + Add<Output = T> + Sub<Output = T> + Mul<Output = T> + Div<Output = T>> Arithmetic for T {}

fn map<T, U>(val: T, a_min: T, a_max: T, b_min: U, b_max: U) -> U
where
	T: num_traits::AsPrimitive<f32>,
	U: num_traits::AsPrimitive<f32>,
	f32: num_traits::AsPrimitive<U>
{
	use num_traits::AsPrimitive;
	(((val.as_() - a_min.as_()) / (a_max.as_() - a_min.as_())) * (b_max.as_() - b_min.as_()) + b_min.as_()).as_()
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Stats {
	memory: Option<usize>, // Used and available
	cpu_usage: Option<f32>,
}

fn split(src: &str) -> impl Iterator<Item = &str> {
	src.split(&[' ', '\t', '\n', '\r']).filter(|s| !s.is_empty())
}

fn run_cmd(cmd: &str, args: &[&str]) -> Option<Vec<String>> {
	let proc = Command::new(cmd)
		.args(args)
		.stdout(Stdio::piped())
		.output().expect("Couldn't Create Thread")
		.stdout;

	String::from_utf8(proc).map(|output| split(&output).map(|s| s.to_string()).collect()).ok()
}

fn get_max_memory() -> Option<usize> {
	if let Some(parts) = run_cmd("free", &["-w"]) {
		let max = parts[8].parse();
		return max.ok();
	}

	None
}

fn get_memory() -> Option<usize> {
	if let Some(parts) = run_cmd("free", &["-w"]) {
		let used = parts[9].parse();
		return used.ok();
	}

	None
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
struct CpuCounts {
	user: usize,
	nice: usize,
	system: usize,
	idle: usize,
}

impl std::ops::Sub for CpuCounts {
	type Output = Self;

	fn sub(self, rhs: Self) -> Self::Output {
		Self {
			user: self.user - rhs.user,
			nice: self.nice - rhs.nice,
			system: self.system - rhs.system,
			idle: self.idle - rhs.idle,
		}
	}
}

impl CpuCounts {
	fn total(&self) -> usize {
		self.user + self.nice + self.system + self.idle
	}
}

static CPU_STATE: RwLock<CpuCounts> = RwLock::new(CpuCounts {
	user: 0,
	nice: 0,
	system: 0,
	idle: 0,
});

fn get_cpu_usage() -> Option<f32> {
	// Run `head` on /proc/stat
	let parts = run_cmd("head", &["--lines", "1", "/proc/stat"])?;

	// Read /proc/stat directly
	// let procfile = std::fs::File::open("/proc/stat").expect("Couldn't open file");
	// let reader = BufReader::new(procfile);
	// for line in reader.lines().take(1) {
	// 	if let Ok(line) = line {
	// 		println!("{}", line);
	// 	}
	// }

	// Parse out the different stats overall
	let user: usize = parts[1].parse().expect("Not a number?");
	let nice: usize = parts[2].parse().expect("Not a number?");
	let system: usize = parts[3].parse().expect("Not a number?");
	let idle: usize = parts[4].parse().expect("Not a number?");

	let curr_count = CpuCounts { user, nice, system, idle };

	// Get the difference from the previous state
	let counts = CPU_STATE.read().ok()?;
	let diff = curr_count - *counts;
	drop(counts);

	// Update the current state
	let mut state = CPU_STATE.write().ok()?;
	*state = curr_count;

	// Calculate the percentage of time idle
	let total = diff.total();
	let idle_p = (diff.idle as f32) / (total as f32);
	Some((1.0 - idle_p) * 100.0)
}

fn updater(tx: mpsc::Sender<Stats>) {
	loop {
		let _ = tx.send(Stats {
			memory: get_memory(),
			cpu_usage: get_cpu_usage(),
		});

		thread::sleep(UPDATE_TIME);
	}
}

#[derive(Clone, Debug)]
struct History<T> {
	hist: [T; MAX_HIST],
	name: String,
	min: T,
	max: T,
	idx: usize,
}

impl<T: Default + Copy> Default for History<T> {
	fn default() -> Self {
		Self {
			hist: [T::default(); MAX_HIST],
			name: Default::default(),
			min: Default::default(),
			max: Default::default(),
			idx: 0,
		}
	}
}

impl<T> History<T> {
	fn top(&self) -> &T {
		&self.hist[self.idx]
	}
}

impl<T: Copy> History<T> {
	fn iter(&self) -> impl Iterator<Item = &T> {
		self.hist.iter().rev().cycle().skip(MAX_HIST - self.idx - 1).take(MAX_HIST)
	}
}

impl<T: Copy> History<T> {
	/// Adds an item without updating the minimum and maximum bounds
	fn add_unbounded(&mut self, item: T) {
		self.idx += 1;
		self.idx %= MAX_HIST;
		self.hist[self.idx] = item;
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

// For any type that can be converted to f32 and printed, we can draw it
impl<T: ToString + num_traits::AsPrimitive<f32>> History<T> {
	// TODO: Make this return a response
	fn draw(&self, ui: &mut Ui, stroke: egui::Stroke) {
		ui.label(self.name.clone() + &self.top().to_string());
		let mut size = ui.cursor();
		size.set_height(200.0);
		let painter = ui.painter_at(size);
		ui.advance_cursor_after_rect(size);

		painter.rect_stroke(size, 0, stroke, egui::StrokeKind::Inside);

		let Rect{ min: Pos2{x: ax, y: ay}, max: Pos2{x: bx, y: by} } = size;

		let grid_stroke = egui::Stroke::new(1.0, Color32::from_white_alpha(50));

		let hover_idx = ui.pointer_hover_pos().map(|pos| map(pos.x, ui.available_width(), 0.0, 0, MAX_HIST));

		// Gridlines
		for i in 1..10 {
			let y = map(i, 0, 10, ay, by);
			painter.line_segment([
				Pos2 { x: ax, y },
				Pos2 { x: bx, y }
			], grid_stroke);
		}

		// Actual history
		painter.line(self.iter().enumerate().map(|(idx, v)| {
			let scaled_x: f32 = map(idx, 0, MAX_HIST, bx, ax);
			let scaled_y: f32 = map(*v, self.min, self.max, by, ay);

			// Hover info
			if Some(idx) == hover_idx {
				ui.place(
					size
						.with_min_x(scaled_x)
						.with_max_x(scaled_x)
						.with_max_y(size.min.y),
					egui::Label::new(v.to_string())
						.extend()
				);

				painter.line_segment([
					Pos2 { x: scaled_x, y: ay },
					Pos2 { x: scaled_x, y: by }
				], grid_stroke);
			}

			Pos2::new(scaled_x, scaled_y)
		}).collect(), stroke);
	}
}

#[derive(Debug)]
struct MyApp {
	rx: mpsc::Receiver<Stats>,
	// next_update: Instant,
	used_mem: History<usize>,
	cpu_usage: History<f32>,
}

impl MyApp {
	/// Starts the thread for the monitoring and sets up all the histories
	///
	/// Not labelled as default because it spawns processes and does stuff
	/// which doesn't really feel like what you would expect from a default function
	fn new() -> Self {
		let (tx, rx) = mpsc::channel();
		thread::spawn(move || {
			updater(tx);
		});

		let mut obj = Self {
			rx,
			// next_update: Instant::now(),
			used_mem: Default::default(),
			cpu_usage: Default::default(),
		};

		obj.used_mem.name = "Used Memory".to_string();
		obj.cpu_usage.name = "CPU Usage".to_string();

		if let Some(max) = get_max_memory() {
			obj.used_mem.max = max;
		}

		obj.cpu_usage.max = 100.0;

		obj
	}
}

impl eframe::App for MyApp {
	// This is called every time the screen updates
	fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
		if let Ok(resp) = self.rx.try_recv() {
			if let Some(mem) = resp.memory {
				self.used_mem.add(mem);
			}

			if let Some(cpu) = resp.cpu_usage {
				self.cpu_usage.add_unbounded(cpu);
			}
		}

		// let now = Instant::now();
		// if now > self.next_update {
		// 	self.next_update = now + UPDATE_TIME;
		// 	if let Some(mem) = get_memory() {
		// 		self.used_mem.add(mem);
		// 	}

		// 	if let Some(cpu) = get_cpu_usage() {
		// 		self.cpu_usage.add_unbounded(cpu);
		// 	}
		// }

		self.used_mem.draw(ui, egui::Stroke::new(1.0, Color32::WHITE));
		self.cpu_usage.draw(ui, egui::Stroke::new(1.0, Color32::GREEN));

		// Make sure it draws again
		ui.request_repaint_after(SCREEN_UPDATE);
		// ui.request_repaint();
	}
}