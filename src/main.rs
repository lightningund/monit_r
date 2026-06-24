use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::thread;
use std::sync::{mpsc};

use eframe::egui;
use egui::{Ui};

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

fn mem_updater(tx: mpsc::Sender<(usize, usize)>) {
	let mut next_update = Instant::now();

	loop {
		let now = Instant::now();
		if now > next_update {
			next_update = now + UPDATE_TIME;

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
					let _ = tx.send((used, avail));
				}
			}
		}
	}
}

#[derive(Clone, Debug)]
struct History<T> {
	hist: [T; MAX_HIST],
	idx: usize,
}

impl<T: Default + Copy> Default for History<T> {
	fn default() -> Self {
		Self {
			hist: [T::default(); MAX_HIST],
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
	fn add(&mut self, item: T) {
		self.idx += 1;
		self.idx %= MAX_HIST;
		self.hist[self.idx] = item;
	}
}

#[derive(Debug)]
struct MyApp {
	work_thread: thread::JoinHandle<()>,
	rx: mpsc::Receiver<(usize, usize)>,
	memory: History<(usize, usize)>, // Stores the used and available as separate numbers
}

impl MyApp {
	fn new() -> Self {
		let (tx, rx) = mpsc::channel();
		let work_thread = thread::spawn(move || {
			mem_updater(tx);
		});
		Self {
			work_thread,
			rx,
			memory: Default::default(),
		}
	}
}

impl eframe::App for MyApp {
	// This is called every time the screen updates
	fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
		// let now = Instant::now();

		// if now > self.next_update {
		// 	self.next_update = now + UPDATE_TIME;

		// 	println!("{:?}", self.next_update);

		// 	let proc = Command::new("free")
		// 		.arg("-w")
		// 		.stdout(Stdio::piped())
		// 		.output().expect("Couldn't Create Thread")
		// 		.stdout;

		// 	if let Ok(output) = String::from_utf8(proc) {
		// 		println!("Parsed ok!");
		// 		let parts = output.split(&[' ', '\n']).filter(|s| !s.is_empty()).collect::<Vec<_>>();
		// 		let used = parts[9].parse::<usize>();
		// 		let avail = parts[14].parse::<usize>();
		// 		if let (Ok(used), Ok(avail)) = (used, avail) {
		// 			self.memory.add((used, avail));
		// 		}
		// 	}

		// 	ui.ctx().request_repaint_after(UPDATE_TIME);
		// }

		if let Ok(resp) = self.rx.try_recv() {
			println!("Received: {:?}", resp);
			self.memory.add(resp);
		}

		ui.label(format!("{:?}", self.memory.top()));

		ui.request_repaint_after(UPDATE_TIME);

		// if let Some(stdout) = &mut self.mem_monitor {
		// 	let mut output: String = "".to_string();
		// 	let bytes = stdout.read_to_string(&mut output).expect("Couldn't read");
		// 	println!("Read to string");
		// 	println!("Bytes: {}, Response: {}", bytes, output);

			// let mut lines = reader
			// 	.lines()
			// 	.filter_map(|line| line.ok())
			// 	.peekable();

			// if lines.peek().is_some() {
			// }

			// lines.for_each(|line| {
			// 	println!("{}", line);
			// 	ui.label(line);
			// });

			// println!("After lines printed");
		// }
	}
}