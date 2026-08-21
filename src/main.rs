use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::thread;
use std::sync::{RwLock, Arc};

use eframe::egui;
use egui::{Ui, Color32, Pos2, Rect};

static MAX_HIST: usize = 500;
static UPDATE_TIME: Duration = Duration::from_millis(200);

fn main() -> eframe::Result {
	let options = eframe::NativeOptions {
		viewport: egui::ViewportBuilder::default()
			.with_icon(std::sync::Arc::new(
				eframe::icon_data::from_png_bytes(include_bytes!("../icon.png"))
					.expect("Couldn't Load Icon")
			))
			.with_inner_size([500.0, 1000.0])
			.with_drag_and_drop(true),
		..Default::default()
	};
	eframe::run_native(
		"MonitR",
		options,
		Box::new(|_| {
			Ok(Box::new(MyApp::new()))
		}),
	)
}

fn map<T, U>(val: T, a_min: T, a_max: T, b_min: U, b_max: U) -> U
where
	T: num_traits::AsPrimitive<f32>,
	U: num_traits::AsPrimitive<f32>,
	f32: num_traits::AsPrimitive<U>
{
	use num_traits::AsPrimitive;
	(((val.as_() - a_min.as_()) / (a_max.as_() - a_min.as_())) * (b_max.as_() - b_min.as_()) + b_min.as_()).as_()
}

#[derive(Clone, Debug)]
struct RingBuffer<T, const N: usize> {
	hist: [T; N],
	idx: usize,
}

impl<T: Default + Copy, const N: usize> Default for RingBuffer<T, N> {
	fn default() -> Self {
		Self {
			hist: [T::default(); N],
			idx: 0,
		}
	}
}

impl<T, const N: usize> RingBuffer<T, N> {
	fn top(&self) -> &T {
		&self.hist[self.idx]
	}

	fn iter(&self) -> impl Iterator<Item = &T> {
		self.hist.iter().rev().cycle().skip(N - self.idx - 1).take(N)
	}

	// Might need to restrict T to be Copy
	fn add(&mut self, item: T) {
		self.idx += 1;
		self.idx %= MAX_HIST;
		self.hist[self.idx] = item;
	}
}

#[derive(Clone, Debug, Default)]
struct History<T: Copy> {
	hist: RingBuffer<T, MAX_HIST>,
	name: String,
	min: T,
	max: T,
}

impl<T: Copy> History<T> {
	fn iter(&self) -> impl Iterator<Item = &T> {
		self.hist.iter()
	}

	fn add(&mut self, item: T) {
		self.hist.add(item)
	}
}

// For any type that can be converted to f32 and printed, we can draw it
impl<T: ToString + num_traits::AsPrimitive<f32>> History<T> {
	// TODO: Make this return a response
	fn draw(&self, ui: &mut Ui, stroke: egui::Stroke) {
		ui.label(self.name.clone() + " " + &self.hist.top().to_string());
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

#[derive(Clone, Debug, Default)]
struct Stats {
	used_mem: History<usize>,
	cpu_usage: History<f32>,
	cpu_temp: History<f32>,
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
	run_cmd("free", &["-w"])
		.and_then(|parts| parts[8].parse().ok())
	// if let Some(parts) = run_cmd("free", &["-w"]) {
	// 	let max = parts[8].parse();
	// 	return max.ok();
	// }

	// None
}

fn get_memory() -> Option<usize> {
	run_cmd("free", &["-w"])
		.and_then(|parts| parts[9].parse().ok())
	// if let Some(parts) = run_cmd("free", &["-w"]) {
	// 	let used = parts[9].parse();
	// 	return used.ok();
	// }

	// None
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

fn get_cpu_usage(cpu_state: &mut CpuCounts) -> Option<f32> {
	// Run `head` on /proc/stat
	let parts = run_cmd("head", &["--lines", "1", "/proc/stat"])?;

	// Parse out the different stats overall
	let user: usize = parts[1].parse().expect("Not a number?");
	let nice: usize = parts[2].parse().expect("Not a number?");
	let system: usize = parts[3].parse().expect("Not a number?");
	let idle: usize = parts[4].parse().expect("Not a number?");

	let curr_count = CpuCounts { user, nice, system, idle };

	// Get the difference from the previous state
	let diff = curr_count - *cpu_state;

	// Update the current state
	*cpu_state = curr_count;

	// Calculate the percentage of time idle
	let total = diff.total();
	let idle_p = (diff.idle as f32) / (total as f32);
	Some((1.0 - idle_p) * 100.0)
}

fn get_cpu_temp() -> Option<f32> {
	// /sys/class/thermal/thermal_zone*
	// We want /type to be x86_pkg_temp
	// And then we get the temp from /temp (in millidegrees Celsius)
	None
}

fn updater(stats: Arc<RwLock<Stats>>) {
	let mut cpu_state = CpuCounts::default();

	loop {
		let next_update = Instant::now() + UPDATE_TIME;

		if let Ok(mut stats) = stats.write() {
			if let Some(mem) = get_memory() {
				stats.used_mem.add(mem);
			}

			if let Some(cpu) = get_cpu_usage(&mut cpu_state) {
				stats.cpu_usage.add(cpu);
			}

			if let Some(temp) = get_cpu_temp() {
				stats.cpu_temp.add(temp);
			}
		}

		thread::sleep(next_update - Instant::now());
	}
}

fn cpu_updater(stats: Arc<RwLock<Stats>>) {
	let stdout = Command::new("mpstat")
		.arg("1")
		.stdout(Stdio::piped())
		.spawn().expect("Couldn't create thread")
		.stdout.expect("Couldn't get stdout");

	let reader = BufReader::new(stdout);

	for line in reader.lines() {
		match line {
			Ok(line) => {
				let idle = split(&line).last()
					.and_then(|last| last.parse::<f32>().ok());

				if let Some(idle) = idle && let Ok(mut stats) = stats.write() {
					stats.cpu_usage.add(100.0 - idle);
				}
			}
			Err(err) => {
				eprintln!("read error: {}", err);
			}
		}
	}
}

#[derive(Debug)]
struct MyApp {
	stats: Arc<RwLock<Stats>>,
}

impl MyApp {
	/// Starts the thread for the monitoring and sets up all the histories
	///
	/// Not labelled as default because it spawns processes and does stuff
	/// which doesn't really feel like what you would expect from a default function
	fn new() -> Self {
		let stats = Arc::new(RwLock::new(Stats {
			used_mem: Default::default(),
			cpu_usage: Default::default(),
			cpu_temp: Default::default(),
		}));

		if let Ok(mut stats) = stats.write() {
			stats.used_mem.name = "Used Memory".to_string();
			stats.cpu_usage.name = "CPU Usage".to_string();
			stats.cpu_temp.name = "CPU Temperature".to_string();

			if let Some(max) = get_max_memory() {
				stats.used_mem.max = max;
			}

			stats.cpu_usage.max = 100.0;
			stats.cpu_temp.max = 100.0; // Pretty safe max
		}

		let thread_stats = Arc::clone(&stats);
		thread::spawn(move || {
			updater(thread_stats);
		});

		Self { stats }
	}
}

impl eframe::App for MyApp {
	// This is called every time the screen updates
	fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
		if let Ok(stats) = self.stats.read() {
			stats.used_mem.draw(ui, egui::Stroke::new(1.0, Color32::WHITE));
			stats.cpu_usage.draw(ui, egui::Stroke::new(1.0, Color32::GREEN));
		}

		// Make sure it draws again
		ui.request_repaint_after(UPDATE_TIME / 2);
		// ui.request_repaint();
	}
}