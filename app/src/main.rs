#![feature(futures_api)]

mod ui;
mod data_collection;

use ui::{App, WindowHandle, WindowLogger};

fn main() {
	let app = App::new();

	let logger_handle = WindowHandle::clone(&app.win);

	// If we can't initialize the logger, might as well panic
	WindowLogger::init(logger_handle).expect("Failed to initialize logger");

	tether::builder()
		.html(include_str!("../ui/index.html"))
		.minimum_size(800, 600)
		.handler(app)
		.start();
}
