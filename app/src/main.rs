#![feature(futures_api)]

use futures::executor::LocalPool;

mod ui;

const SAMPLING_RATE: usize = 1000;
const DATA_SEND_RATE: usize = 10; // hz
const COUNT_MOD: usize = SAMPLING_RATE / DATA_SEND_RATE;

fn main() -> ! {
	let win_handle = ui::create();



	loop {
		std::thread::sleep(std::time::Duration::from_secs(10));
	}
}
