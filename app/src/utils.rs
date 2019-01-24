#![feature(try_from)]

use futures::future::Future;
use futures::sync as f_sync;
use tokio::codec::{Framed, LinesCodec};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

use std::convert::TryFrom;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

use log::{Level, Metadata, Record};

use events::Event;
use nidaqmx::{AiChannel, CiEncoderChannel};

type SocketTxHandle = f_sync::mpsc::UnboundedSender<events::Server>;

const SAMPLING_RATE: usize = 1000;
const DATA_SEND_RATE: usize = 10; // hz
const COUNT_MOD: usize = SAMPLING_RATE / DATA_SEND_RATE;

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
	S: Stream,
	F: Fn(&S::Item),
{

	type Item = S::Item;
	type Error = S::Error;

	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {

		let item = futures::try_ready!(self.inner.poll());

		if self.state % self.n == 0 {
			item.as_ref().map_or((), &self.f);
		}

		self.state += 1;

		Ok(Async::Ready(item))
	}

}

trait StreamExt: Stream {
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

impl<T: Stream> StreamExt for T {}

struct DataCollectionHandle {
	data_tx: SocketTxHandle,
	stop_handle: Option<f_sync::oneshot::Sender<()>>,
}

impl DataCollectionHandle {
	fn new(socket: SocketTxHandle) -> Self {
		DataCollectionHandle {
			data_tx: socket,
			stop_handle: None,
		}
	}

	fn start_collection<S: AsRef<Path>>(&mut self, file: S) {
		const CAPACITY: usize = 1024 * 1024;

		use std::fs::File;
		use std::io::BufWriter;

		let fpath = S::as_ref(&file);

		std::fs::create_dir_all(fpath).expect("couldn't create directory");

		let mut enc_fname = PathBuf::from(fpath);
		enc_fname.push("enc");
		enc_fname.set_extension("csv");

		let mut adc_fname = PathBuf::from(fpath);
		adc_fname.push("adc");
		adc_fname.set_extension("csv");

		let mut enc_file = BufWriter::with_capacity(
			CAPACITY,
			File::create(enc_fname).expect("couldn't create file"),
		);
		let mut adc_file = BufWriter::with_capacity(
			CAPACITY,
			File::create(adc_fname).expect("couldn't create file"),
		);

		let encoder_chan = CiEncoderChannel::new(SAMPLING_RATE);
		let ai_chan = AiChannel::new("/Dev1/PFI13", SAMPLING_RATE);

		let encoder_stream = encoder_chan
			.make_async()
			.map(move |data| writeln!(enc_file, "{}", data))
			.for_each(|res| res.map_err(|_| ()));

		let tx = self.data_tx.clone();
		let ai_stream = ai_chan
			.make_async()
			.bifurcate(COUNT_MOD, move |data| {
				let msg = events::Server::DataPoint(data.timestamp, data.data[0]);
				let _ = tx.unbounded_send(msg);
			})
			.map(move |data| writeln!(adc_file, "{}", data))
			.for_each(|res| res.map_err(|_| ()));

		self.join_streams(encoder_stream, ai_stream);
	}

	fn dispatch_event(&mut self, ev: events::Client) {
		match ev {
			events::Client::StartPressed(file) => {
				if self.stop_handle.is_none() {
					log::info!("Recieved Start Command, file: {}", file);
					self.start_collection(file);
				}
			}
			events::Client::StopPressed => {
				let _ = self.stop_collection();
				log::info!("Recieved Stop Command");
			}
		};
	}

	fn join_streams<F1, F2>(&mut self, data_stream1: F1, data_stream2: F2)
	where
		F1: Future<Item = (), Error = ()> + Send + 'static,
		F2: Future<Item = (), Error = ()> + Send + 'static,
	{
		let (sender, recv) = f_sync::oneshot::channel();

		let data_stream = data_stream1
			.join(data_stream2)
			.map(|_| ())
			.map_err(|_| ())
			.select(recv.map_err(|_| ()))
			.map(|_| ())
			.map_err(|_| ());

		tokio::spawn(data_stream);

		self.stop_handle = Some(sender);
	}

	fn stop_collection(&mut self) -> Result<(), ()> {
		let stop_handle = self.stop_handle.take().ok_or(())?;
		stop_handle.send(()).map_err(|_| ())
	}
}

struct GuiLogger {
	connection: Mutex<Option<SocketTxHandle>>,
}

impl GuiLogger {
	const fn new() -> Self {
		GuiLogger {
			connection: Mutex::new(None),
		}
	}

	fn register_socket(&self, tx: SocketTxHandle) {
		*self.connection.lock() = Some(tx);
	}
}

impl log::Log for GuiLogger {
	fn enabled(&self, metadata: &Metadata) -> bool {
		metadata.level() <= Self::LOG_LEVEL
	}

	fn log(&self, record: &Record) {
		if !self.enabled(record.metadata()) {
			return;
		}

		let mut connection_lock = self.connection.lock();
		let connection = &*connection_lock;

		if let Some(tx) = connection {
			let msg = format!("SERVER\t{}\t{}", record.level(), record.args());
			let _ = tx
				.unbounded_send(events::Server::Msg(msg))
				.map_err(|_| *connection_lock = None);
		} else {
			println!("SERVER STDOUT\t{}\t{}", record.level(), record.args());
		}
	}

	fn flush(&self) {}
}

impl GuiLogger {
	#[cfg(debug_assertions)]
	const LOG_LEVEL: Level = Level::Debug;

	#[cfg(not(debug_assertions))]
	const LOG_LEVEL: Level = Level::Warn;
}

const LOCALHOST: [u8; 4] = [127, 0, 0, 1];
const PORT: u16 = 58080;

const ADDR: ([u8; 4], u16) = (LOCALHOST, PORT);

static LOGGER: GuiLogger = GuiLogger::new();

// fn start_collection<S: AsRef<Path>>(file: S) -> DataCollectionHandle {
// 	const CAPACITY: usize = 1024 * 1024;

// 	use std::fs::File;
// 	use std::io::BufWriter;

// 	let fpath = S::as_ref(&file);

// 	std::fs::create_dir_all(fpath).expect("couldn't create directory");

// 	let mut enc_fname = PathBuf::from(fpath);
// 	enc_fname.push("enc");
// 	enc_fname.set_extension("csv");

// 	let mut adc_fname = PathBuf::from(fpath);
// 	adc_fname.push("adc");
// 	adc_fname.set_extension("csv");

// 	let mut enc_file = BufWriter::with_capacity(
// 		CAPACITY,
// 		File::create(enc_fname).expect("couldn't create file"),
// 	);
// 	let mut adc_file = BufWriter::with_capacity(
// 		CAPACITY,
// 		File::create(adc_fname).expect("couldn't create file"),
// 	);

// 	let encoder_chan = CiEncoderChannel::new(SAMPLING_RATE);
// 	let ai_chan = AiChannel::new("/Dev1/PFI13", SAMPLING_RATE);

// 	let encoder_stream = encoder_chan
// 		.make_async()
// 		.map(move |data| writeln!(enc_file, "{}", data))
// 		.for_each(|res| res.map_err(|_| ()));

// 	let ai_stream = ai_chan
// 		.make_async()
// 		.map(move |data| writeln!(adc_file, "{}", data))
// 		.for_each(|res| res.map_err(|_| ()));

// 	let stop_handle = DataCollectionHandle::new(encoder_stream, ai_stream);

// 	stop_handle
// }

// fn dispatch_event(ev: events::Client) {
// 	static STOP_HANDLE: Mutex<Option<DataCollectionHandle>> = Mutex::new(None);

// 	let mut stop_handle_lock = STOP_HANDLE.lock();
// 	let stop_handle = &mut *stop_handle_lock;

// 	match ev {
// 		events::Client::StartPressed(file) => {
// 			if stop_handle.is_none() {
// 				log::info!("Recieved Start Command, file: {}", file);
// 				*stop_handle = Some(start_collection(file));
// 			}
// 		}
// 		events::Client::StopPressed => {
// 			if let Some(stop_handle) = stop_handle.take() {
// 				let _ = stop_handle.stop_collection();
// 				log::info!("Recieved Stop Command");
// 			}
// 		}
// 	};
// }

fn process_connection(tcp_stream: TcpStream) -> Result<(), ()> {
	let framed = Framed::new(tcp_stream, LinesCodec::new());
	let (writer, reader) = framed.split();

	let (tx, rx) = futures::sync::mpsc::unbounded::<events::Server>();

	let log_tx = tx.clone();
	let data_tx = tx.clone();

	let mut collection_handle = DataCollectionHandle::new(data_tx);

	LOGGER.register_socket(log_tx);

	let channel_reader = rx
		.filter_map(|ev| ev.as_str())
		.fold(writer, |writer, event| {
			writer.send(event).map(|writer| writer).map_err(|_| ())
		})
		.map(|_| ());

	let socket_reader = reader
		.for_each(move |msg| {
			let uie = match events::Client::try_from(msg.as_str()) {
				Ok(uie) => uie,
				Err(_) => return Ok(()),
			};

			collection_handle.dispatch_event(uie);

			Ok(())
		})
		.and_then(|_| {
			log::debug!("Socket received FIN packet and closed connection");
			Ok(())
		})
		.or_else(|err| {
			log::debug!("Socket closed with error: {:?}", err);
			Err(err)
		})
		.then(|result| {
			log::debug!("Socket closed with result: {:?}", result);
			Ok(())
		});

	tokio::spawn(socket_reader);
	tokio::spawn(channel_reader);

	Ok(())
}

// fn main() -> Result<(), Box<std::error::Error>> {
// 	let _ = log::set_logger(&LOGGER).map(|_| log::set_max_level(log::LevelFilter::max()));

// 	let addr = SocketAddr::from(ADDR);
// 	let socket = TcpListener::bind(&addr)?;

// 	log::debug!("Listening on: {}", addr);

// 	let connection_daemon = socket
// 		.incoming()
// 		.map_err(|_| ())
// 		.for_each(move |socket| process_connection(socket));

// 	tokio::run(connection_daemon);

// 	Ok(())
// }

// use std::io::Write;
// use futures::{stream, Stream};
// use futures::future::Future;
// use tokio::runtime::Runtime;

// // use nidaqmx::{AiChannel, CiEncoderChannel, EncoderReading};

// // fn enc_stream(mut enc_chan: CiEncoderChannel) -> impl Stream<Item = EncoderReading, Error = ()> {

// // 	let iters = 0..;

// // 	stream::iter_ok(iters).map(move |_| {
// // 		enc_chan.acquire_sample()
// // 	})
// // }

// const SAMPLING_RATE: usize = 1000;

// fn main() {

// 	// let mut enc_file = std::io::BufWriter::with_capacity(1024 * 1024, std::fs::File::create("enc_data.csv").unwrap());
// 	// let mut adc_file = std::io::BufWriter::with_capacity(1024 * 1024, std::fs::File::create("adc_data.csv").unwrap());

// 	let encoder_chan = CiEncoderChannel::new(SAMPLING_RATE);
// 	// let ai_chan = AiChannel::new("/Dev1/PFI13", SAMPLING_RATE);
// 	let ai_chan = AiChannel::new("", SAMPLING_RATE);

// 	let encoder_stream = encoder_chan.make_async().for_each(move |val| {
// 		// let _ = writeln!(&mut enc_file, "{},{}", val.timestamp, val.pos);
// 		println!("{},{}", val.timestamp, val.pos);
// 		Ok(())
// 	});

// 	let ai_stream = ai_chan.make_async().for_each(move |val| {
// 		// let _ = writeln!(&mut adc_file, "{},{},{}", val.timestamp, val.data[0], val.data[1]);
// 		println!("{},{},{}", val.timestamp, val.data[0], val.data[1]);
// 		Ok(())
// 	});

// 	// let data_stream = encoder_stream.select(ai_stream).map(|_| ()).map_err(|_| ());
// 	// let data_stream = ai_stream;
// 	let data_stream = encoder_stream;

// 	println!("Started data collection");

// 	tokio::run(data_stream);

// 	println!("End of program.");
// }
