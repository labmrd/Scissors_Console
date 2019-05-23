// compile with RUSTFLAGS="-Crelocation-model=dynamic-no-pic -Clink-args=-no-pie" cargo build --release

mod data_collection;
mod ui;

use ui::{App, WindowLogger};

fn main() {
	let app = App::new();

	// If we can't initialize the logger, might as well panic
	WindowLogger::init().expect("Failed to initialize logger");

	tether::builder()
		.html(include_str!("../ui/index.html"))
		.minimum_size(800, 800)
		.handler(app)
		.start();
}
