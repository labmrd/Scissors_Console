#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// linux: compile with RUSTFLAGS="-Crelocation-model=dynamic-no-pic -Clink-args=-no-pie" cargo build --release

mod data_collection;
mod ui;

use ui::{App, WindowLogger};

fn start_gui() {
	let app = App::new();
	let window = tether::Window::with_handler(app);
	window.title("Scissors Console");
	window.load(include_str!("../ui/index.html"));
}

fn main() {
	// If we can't initialize the logger, might as well panic
	WindowLogger::init().expect("Failed to initialize logger");
	unsafe { tether::start(start_gui) };
}
