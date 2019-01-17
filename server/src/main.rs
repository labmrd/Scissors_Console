#![feature(try_from)]

use futures::sync as f_sync;
use futures::future::Future;
use tokio::codec::{Framed, LinesCodec};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

use std::convert::TryFrom;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

use log::{Level, Metadata, Record};

use events::Event;
use nidaqmx::{AiChannel, CiEncoderChannel, EncoderReading};

type SocketTxHandle = f_sync::mpsc::UnboundedSender<events::Server>;

struct StopCollectionHandle {
	sender: f_sync::oneshot::Sender<()>,
}

impl StopCollectionHandle {
	fn new<F1, F2>(data_stream1: F1, data_stream2: F2) -> Self
	where
		F1: Future<Item = (), Error = ()> + Send + 'static,
		F2: Future<Item = (), Error = ()> + Send + 'static
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

		StopCollectionHandle { sender }
	}

	fn stop_collection(self) {
		self.sender.send(());
	}
}

const SAMPLING_RATE: usize = 1000;

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

fn start_collection<S: AsRef<Path>>(file: S) -> StopCollectionHandle {
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

	let ai_stream = ai_chan
		.make_async()
		.map(move |data| writeln!(adc_file, "{}", data))
		.for_each(|res| res.map_err(|_| ()));

	let stop_handle = StopCollectionHandle::new(encoder_stream, ai_stream);

	stop_handle
}

fn dispatch_event(ev: events::Client) {
	static STOP_HANDLE: Mutex<Option<StopCollectionHandle>> = Mutex::new(None);

	let mut stop_handle_lock = STOP_HANDLE.lock();
	let stop_handle = &mut *stop_handle_lock;

	match ev {
		events::Client::StartPressed(file) => {
			if stop_handle.is_none() {
				log::info!("Recieved Start Command, file: {}", file);
				*stop_handle = Some(start_collection(file));
			}
		}
		events::Client::StopPressed => {
			if let Some(stop_handle) = stop_handle.take() {
				let _ = stop_handle.send(());
				log::info!("Recieved Stop Command");
			}
		}
	};
}

fn process_connection(tcp_stream: TcpStream) -> Result<(), ()> {
	let framed = Framed::new(tcp_stream, LinesCodec::new());
	let (writer, reader) = framed.split();

	let (tx, rx) = futures::sync::mpsc::unbounded::<events::Server>();

	let log_tx = tx.clone();

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

			dispatch_event(uie);

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

fn main() -> Result<(), Box<std::error::Error>> {
	let _ = log::set_logger(&LOGGER).map(|_| log::set_max_level(log::LevelFilter::max()));

	let addr = SocketAddr::from(ADDR);
	let socket = TcpListener::bind(&addr)?;

	log::debug!("Listening on: {}", addr);

	let connection_daemon = socket
		.incoming()
		.map_err(|_| ())
		.for_each(move |socket| process_connection(socket));

	tokio::run(connection_daemon);

	Ok(())
}

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
