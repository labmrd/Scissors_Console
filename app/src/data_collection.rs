use futures::{
	channel::oneshot,
	executor::LocalPool,
	future::{self, Future, FutureExt},
	stream::{self, Stream, StreamExt},
};

// use nidaqmx::{AiChannel, CiEncoderChannel};

use std::{
	fs::{self, File},
	io::{BufWriter, Write},
	path::PathBuf,
	thread,
};

const SAMPLING_RATE: usize = 1000;
const DATA_SEND_RATE: usize = 10; // hz
const UPDATE_UI_SAMP_COUNT: usize = SAMPLING_RATE / DATA_SEND_RATE;

pub fn start(fpath: &str) -> DataCollectionHandle {
	// fs::create_dir_all(fpath).expect("Failed to create directory");

	// let mut adc_file = open_buffered_file(fpath, "adc");
	// let mut enc_file = open_buffered_file(fpath, "enc");

	// let encoder_chan = CiEncoderChannel::new(SAMPLING_RATE);
	// let ai_chan = AiChannel::new("/Dev1/PFI13", SAMPLING_RATE);

	// let encoder_stream = encoder_chan
	// 	.make_async()
	// 	.map(move |data| writeln!(enc_file, "{}", data).expect("Failed to write data"))
	// 	.for_each(|_| future::ready(()));

	// let ai_stream = ai_chan
	// 	.make_async()
	// 	.map(move |data| writeln!(adc_file, "{}", data).expect("Failed to write data"))
	// 	.for_each(|_| future::ready(()));

	// let data_stream = encoder_stream.join(ai_stream).map(|_| ());

	let data_stream = futures::stream::iter(1..)
		.map(|idx| {
			println!("did the thing: {}", idx);
			thread::sleep(std::time::Duration::from_secs(1));
		})
		.for_each(|_| future::ready(()));

	DataCollectionHandle::start(data_stream)
}

pub struct DataCollectionHandle {
	inner: future::AbortHandle,
}

impl DataCollectionHandle {
	fn start<F>(fut: F) -> Self
	where
		F: Future<Output = ()> + Send + 'static,
	{
		let (fut, stop_handle) = future::abortable(fut);

		thread::spawn(move || {
			if let Err(_) = LocalPool::new().run_until(fut) {
				log::info!("Data collection stopped"); // future was aborted (good)
			} else {
				log::error!("Data collection stopped prematurely");
				panic!();
			}
		});

		log::info!("Started data collection");
		Self { inner: stop_handle }
	}

	pub fn stop(self) {
		log::debug!("Sent abort signal");
		self.inner.abort()
	}
}

fn open_buffered_file(dir: &str, name: &str) -> BufWriter<File> {
	const BUF_CAPACITY: usize = 1024 * 1024; // 1 Mb

	let mut fpath = PathBuf::from(dir);
	fpath.push(name);
	fpath.set_extension("csv");

	BufWriter::with_capacity(
		BUF_CAPACITY,
		File::create(fpath).expect("Failed to create file"),
	)
}
