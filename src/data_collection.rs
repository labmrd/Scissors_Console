use futures::{
	executor::LocalPool,
	future::{self, Future, FutureExt},
	stream::{Stream, StreamExt},
	task::Waker,
	Poll,
};

use nidaqmx::{AiChannel, CiEncoderChannel};

use std::{
	fs::{self, File, OpenOptions},
	io::{BufWriter, Write},
	marker::Unpin,
	path::PathBuf,
	pin::Pin,
	thread,
};

use crate::ui;

const SAMPLING_RATE: usize = 1000;
const DATA_SEND_RATE: usize = 10; // hz
const UPDATE_UI_SAMP_COUNT: usize = SAMPLING_RATE / DATA_SEND_RATE;

pub fn start(fpath: &mut PathBuf) -> Option<DataCollectionHandle> {
	fs::create_dir_all(&fpath).expect("Failed to create directory");

	fpath.push("gibberish/");

	let mut adc_file = open_buffered_file(fpath, "adc")?;
	let mut enc_file = open_buffered_file(fpath, "enc")?;

	log::info!("Created files");

	let encoder_chan = CiEncoderChannel::new(SAMPLING_RATE);
	let ai_chan = AiChannel::new("/Dev1/PFI13", SAMPLING_RATE);

	let encoder_stream = encoder_chan
		.make_async()
		.map(move |data| writeln!(enc_file, "{}", data).expect("Failed to write data"))
		.for_each(|_| future::ready(()));

	let ai_stream = ai_chan
		.make_async()
		.bifurcate(UPDATE_UI_SAMP_COUNT, move |data| {
			ui::WindowHandle::append_to_chart(data.timestamp, data.data[0], data.data[1]);
		})
		.map(move |data| writeln!(adc_file, "{}", data).expect("Failed to write data"))
		.for_each(|_| future::ready(()));

	let data_stream = encoder_stream.join(ai_stream).map(|_| ());

	Some(DataCollectionHandle::start(data_stream))
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
		Self {
			inner: stop_handle,
			thread_handle,
		}
	}

	pub fn stop(self) {
		log::debug!("Sent abort signal");
		self.inner.abort();

		let thread_status = self.thread_handle.join();

		match thread_status {
			Ok(success_flag) if success_flag == true => log::info!("Data collection stopped"),
			_ => {
				log::error!("Unknown error has occured when trying to stop data collection thread")
			}
		};
	}
}

fn open_buffered_file(fpath: &mut PathBuf, name: &str) -> Option<BufWriter<File>> {
	const BUF_CAPACITY: usize = 1024 * 1024; // 1 Mb

	let tm = time::now();

	fpath.set_file_name(format!("{}_{}", name, tm.rfc3339()));
	fpath.set_extension("csv");

	log::debug!("File created: {}", fpath.display());

	let mut file_opts = OpenOptions::new();

	let file = file_opts.write(true).create_new(true).open(fpath).ok()?;
	let mut file = BufWriter::with_capacity(BUF_CAPACITY, file);

	let _ = write!(&mut file, "%{}", tm.rfc822());

	Some(file)
}

struct Bifurcate<S, F>
where
	S: Stream,
	F: Fn(&S::Item),
{
	inner: S,
	state: usize,
	n: usize,
	f: F,
}

impl<S, F> Stream for Bifurcate<S, F>
where
	S: Stream + Unpin,
	F: Fn(&S::Item) + Unpin,
{
	type Item = S::Item;

	fn poll_next(mut self: Pin<&mut Self>, wkr: &Waker) -> Poll<Option<Self::Item>> {
		let mut self_ref = self.as_mut();

		let pinned = Pin::new(&mut self_ref.inner);

		let poll = Stream::poll_next(pinned, wkr);

		let item = futures::ready!(poll);

		if self_ref.state % self_ref.n == 0 {
			item.as_ref().map_or((), &self_ref.f)
		}

		self_ref.state += 1;

		Poll::Ready(item)
	}
}

trait StreamBifurcate: Stream {
	fn bifurcate<F>(self, n: usize, f: F) -> Bifurcate<Self, F>
	where
		Self: Sized,
		F: Fn(&Self::Item),
	{
		Bifurcate {
			inner: self,
			state: 0,
			n,
			f,
		}
	}
}

impl<T: Stream> StreamBifurcate for T {}
