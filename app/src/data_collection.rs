use futures::{
	channel::oneshot,
	executor::LocalPool,
	future::{self, Future, FutureExt},
	stream::{self, Stream, StreamExt},
};

use nidaqmx::{AiChannel, CiEncoderChannel};

use std::{
	fs::{self, File},
	io::{BufWriter, Write},
	path::PathBuf,
	thread,
};

const SAMPLING_RATE: usize = 1000;
const DATA_SEND_RATE: usize = 10; // hz
const UPDATE_UI_SAMP_COUNT: usize = SAMPLING_RATE / DATA_SEND_RATE;

pub fn start(mut fpath: PathBuf) -> DataCollectionHandle {
	fs::create_dir_all(&fpath).expect("Failed to create directory");

	fpath.push("gibberish/");

	let mut adc_file = open_buffered_file(&mut fpath, "adc");
	let mut enc_file = open_buffered_file(&mut fpath, "enc");

	log::info!("Created files");

	let encoder_chan = CiEncoderChannel::new(SAMPLING_RATE);
	let ai_chan = AiChannel::new("/Dev1/PFI13", SAMPLING_RATE);

	let encoder_stream = encoder_chan
		.make_async()
		.map(move |data| writeln!(enc_file, "{}", data).expect("Failed to write data"))
		.for_each(|_| future::ready(()));

	let ai_stream = ai_chan
		.make_async()
		.map(move |data| writeln!(adc_file, "{}", data).expect("Failed to write data"))
		.for_each(|_| future::ready(()));

	let data_stream = encoder_stream.join(ai_stream).map(|_| ());

	// let data_stream = futures::stream::iter(1..)
	// 	.map(|idx| {
	// 		println!("did the thing: {}", idx);
	// 		thread::sleep(std::time::Duration::from_secs(1));
	// 	})
	// 	.for_each(|_| future::ready(()));

	DataCollectionHandle::start(data_stream)
}

pub struct DataCollectionHandle {
	inner: future::AbortHandle,
	thread_handle: thread::JoinHandle<bool>,
}

impl DataCollectionHandle {
	fn start<F>(fut: F) -> Self
	where
		F: Future<Output = ()> + Send + 'static,
	{
		let (fut, stop_handle) = future::abortable(fut);

		let thrd = thread::Builder::new().name("Data Collection Driver".to_string());

		let thread_handle = thrd
			.spawn(move || LocalPool::new().run_until(fut).is_err())
			.expect("Failed to spawn data collection thread");

		log::info!("Started data collection");
		Self { inner: stop_handle, thread_handle }
	}

	pub fn stop(self) {
		log::debug!("Sent abort signal");
		self.inner.abort();

		let thread_status = self.thread_handle.join();

		match thread_status {
			Ok(success_flag) if success_flag == true => log::info!("Data collection stopped"),
			_ => log::error!("Unknown error has occured when trying to stop data collection thread")
		};
	}
}

fn open_buffered_file(fpath: &mut PathBuf, name: &str) -> BufWriter<File> {
	const BUF_CAPACITY: usize = 1024 * 1024; // 1 Mb

	fpath.set_file_name(name);
	fpath.set_extension("csv");

	log::debug!("File created: {}", fpath.display());

	BufWriter::with_capacity(
		BUF_CAPACITY,
		File::create(fpath).expect("Failed to create file"),
	)
}
