#![feature(try_from)]

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
use nidaqmx::{AiChannel, CiEncoderChannel, EncoderReading};

type SocketTxHandle = f_sync::mpsc::UnboundedSender<events::Server>;
type StopCollectionHandle = f_sync::oneshot::Sender<()>;

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

// fn start_collection<S: AsRef<Path>>(file: S) -> StopCollectionHandle {
// 	const SAMPLING_FREQ: f64 = 1e5;

// 	let mut open_opts = std::fs::OpenOptions::new();
// 	open_opts.write(true).create_new(true);

// 	let fpath = S::as_ref(&file);

// 	std::fs::create_dir_all(fpath).unwrap();

// 	let mut enc_fname = PathBuf::from(fpath);
// 	enc_fname.push("enc");
// 	enc_fname.set_extension("csv");

// 	let mut adc_fname = PathBuf::from(fpath);
// 	adc_fname.push("adc");
// 	adc_fname.set_extension("csv");

// 	let (prod, con) = futures::sync::oneshot::channel::<()>();

// 	let mut enc_file =
// 		std::io::BufWriter::with_capacity(1024 * 1024, open_opts.open(enc_fname).unwrap());
// 	let mut adc_file =
// 		std::io::BufWriter::with_capacity(1024 * 1024, open_opts.open(adc_fname).unwrap());

// 	let encoder_chan = CiEncoderChannel::new(SAMPLING_FREQ).make_async();
// 	// let ai_chan = AiChannel::new(SAMPLING_FREQ, "/Dev1/PFI13").make_async();

// 	let encoder_stream = encoder_chan.for_each(move |val| {
// 		let _ = writeln!(&mut enc_file, "{},{}", val.timestamp, val.pos);
// 		Ok(())
// 	});

// 	let ai_stream = ai_chan.for_each(move |val| {
// 		let _ = writeln!(
// 			&mut adc_file,
// 			"{},{},{}",
// 			val.timestamp, val.data[0], val.data[1]
// 		);
// 		Ok(())
// 	});

// 	let data_stream = encoder_stream.select(ai_stream).map(|_| ()).map_err(|_| ());
// 	let data_stream = data_stream
// 		.select(con.map(|_| ()).map_err(|_| ()))
// 		.map(|_| ())
// 		.map_err(|_| ());

// 	tokio::spawn(data_stream);

// 	prod
// }

// fn dispatch_event(ev: events::Client) {
// 	static STOP_HANDLE: Mutex<Option<StopCollectionHandle>> = Mutex::new(None);

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
// 				let _ = stop_handle.send(());
// 				log::info!("Recieved Stop Command");
// 			}
// 		}
// 	};
// }

// fn process_connection(tcp_stream: TcpStream) -> Result<(), ()> {
// 	let framed = Framed::new(tcp_stream, LinesCodec::new());
// 	let (writer, reader) = framed.split();

// 	let (tx, rx) = futures::sync::mpsc::unbounded::<events::Server>();

// 	let log_tx = tx.clone();

// 	LOGGER.register_socket(log_tx);

// 	let channel_reader = rx
// 		.filter_map(|ev| ev.as_str())
// 		.fold(writer, |writer, event| {
// 			writer.send(event).map(|writer| writer).map_err(|_| ())
// 		})
// 		.map(|_| ());

// 	let socket_reader = reader
// 		.for_each(move |msg| {
// 			let uie = match events::Client::try_from(msg.as_str()) {
// 				Ok(uie) => uie,
// 				Err(_) => return Ok(()),
// 			};

// 			dispatch_event(uie);

// 			Ok(())
// 		})
// 		.and_then(|_| {
// 			log::debug!("Socket received FIN packet and closed connection");
// 			Ok(())
// 		})
// 		.or_else(|err| {
// 			log::debug!("Socket closed with error: {:?}", err);
// 			Err(err)
// 		})
// 		.then(|result| {
// 			log::debug!("Socket closed with result: {:?}", result);
// 			Ok(())
// 		});

// 	tokio::spawn(socket_reader);
// 	tokio::spawn(channel_reader);

// 	Ok(())
// }

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

use std::io::Write;
use futures::{stream, Stream};
use futures::future::Future;
use tokio::runtime::Runtime;

// use nidaqmx::{AiChannel, CiEncoderChannel, EncoderReading};

// fn enc_stream(mut enc_chan: CiEncoderChannel) -> impl Stream<Item = EncoderReading, Error = ()> {

// 	let iters = 0..;

// 	stream::iter_ok(iters).map(move |_| {
// 		enc_chan.acquire_sample()
// 	})
// }

type SamplingRate = typenum::U1000;

fn main() {

	// let mut enc_file = std::io::BufWriter::with_capacity(1024 * 1024, std::fs::File::create("enc_data.csv").unwrap());
	// let mut adc_file = std::io::BufWriter::with_capacity(1024 * 1024, std::fs::File::create("adc_data.csv").unwrap());

	// let encoder_chan = CiEncoderChannel::new(SAMPLING_FREQ).make_async();
	let ai_chan = AiChannel::<SamplingRate>::new("/Dev1/PFI13");

	// let encoder_stream = encoder_chan.for_each(move |val| {
	// 	// let _ = writeln!(&mut enc_file, "{},{}", val.timestamp, val.pos);
	// 	println!("{},{}", val.timestamp, val.pos);
	// 	Ok(())
	// });

	let ai_stream = AiChannel::<SamplingRate>::make_async(ai_chan).for_each(move |val| {
		// let _ = writeln!(&mut adc_file, "{},{},{}", val.timestamp, val.data[0], val.data[1]);
		println!("{},{},{}", val.timestamp, val.data[0], val.data[1]);
		Ok(())
	});

	println!("Started data collection");

	tokio::run(ai_stream);

	println!("End of program.");
}
