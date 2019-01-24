mod ui;

fn main() -> ! {
	let win_handle = ui::create();

	loop {
		std::thread::sleep(std::time::Duration::from_secs(10));
	}
}
