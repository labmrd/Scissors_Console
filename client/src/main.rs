#![recursion_limit = "2048"]
#![feature(try_from)]

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use std::{
	fmt,
	io,
	convert::TryFrom,
	sync::{Arc, Mutex, MutexGuard}
};

use stdweb::traits::*;

#[macro_use]
extern crate stdweb;

#[macro_use]
extern crate derive_more;

use stdweb::web;
use stdweb::web::event::ClickEvent;

use log::{Level, LevelFilter, Metadata, Record, SetLoggerError};

use events::UiEvent;

type AppResult<T> = ::std::result::Result<T, AppError>;
type IoResult<T> = std::io::Result<T>;

#[derive(Default)]
struct LogBuf(Arc<Mutex<String>>);

impl LogBuf {
	pub fn lock(&self) -> MutexGuard<String> {
		self.0.lock().unwrap()
	}

	pub fn clear_log(&self) {
		let mut buf = self.lock();
		buf.clear();

		SimpleLogger::set_status_log(&buf);
	}
}

impl Clone for LogBuf {
	fn clone(&self) -> Self {
		LogBuf {
			0: Arc::clone(&self.0),
		}
	}
}

struct SimpleLogger {
	buf: LogBuf,
}

fn get_timestamp() -> String {
	let ts = js! { return new Date().toLocaleTimeString(); };
	ts.into_string().unwrap()
}

impl log::Log for SimpleLogger {
	fn enabled(&self, metadata: &Metadata) -> bool {
		metadata.level() <= Self::LOG_LEVEL
	}

	fn log(&self, record: &Record) {
		if self.enabled(record.metadata()) {
			let mut buf = &mut *self.buf.lock();
			let _ = writeln!(
				&mut buf as &mut fmt::Write,
				"{}\t{}\t{}",
				record.level().to_string(),
				get_timestamp(),
				record.args()
			);

			Self::set_status_log(&buf);

			if cfg!(debug_assertions) {
				console!(log, buf.as_str());
			}
		}
	}

	fn flush(&self) {}
}

impl SimpleLogger {
	#[cfg(debug_assertions)]
	const LOG_LEVEL: Level = Level::Trace;

	#[cfg(not(debug_assertions))]
	const LOG_LEVEL: Level = Level::Warn;

	fn new() -> Self {
		Self {
			buf: LogBuf::default(),
		}
	}

	fn get_handle(&self) -> LogBuf {
		LogBuf::clone(&self.buf)
	}

	pub fn init() -> Result<LogBuf, SetLoggerError> {
		let logger = Box::new(SimpleLogger::new());

		let handle = logger.get_handle();

		log::set_boxed_logger(logger)
			.map(|()| log::set_max_level(LevelFilter::max()))
			.map(move |_| handle)
	}

	fn set_status_log(log: &str) {
		js! {
			let status_log = document.getElementById("statusLog");
			status_log.value = @{log};
			status_log.scrollTop = status_log.scrollHeight;
		}
	}
}

#[derive(Debug, Display)]
enum AppError {
	#[display(fmt = "Could not find element id: {}", "_0")]
	DomElementNotFound(String),
}

impl std::error::Error for AppError {}

struct ElectronHandle(stdweb::Value);

impl ElectronHandle {
	fn new() -> Self {
		let ehandle = js! {
			const { dialog } = require("electron").remote;
			return dialog;
		};
		Self { 0: ehandle }
	}

	fn show_file_dialog(&self) -> Option<String> {
		let dir = js! {
			let tmpdir = @{&self.0}.showOpenDialog({properties: ["openDirectory", "createDirectory"]});
			try {
				tmpdir = tmpdir[0]
			} catch {
				tmpdir = null;
			}
			return tmpdir;
		}
		.into_string();

		dir
	}
}

#[derive(Constructor)]
struct AppButtonHandle<'a> {
	dom: &'a web::Document,
	elem_id: &'a str,
}

impl AppButtonHandle<'_> {
	fn get_dom_elem(&self) -> Result<web::Element, AppError> {
		self.dom
			.get_element_by_id(self.elem_id)
			.ok_or_else(|| AppError::DomElementNotFound(self.elem_id.to_string()))
	}

	fn register_click_callback<F>(&self, mut callback: F) -> AppResult<()>
	where
		F: FnMut() + 'static,
	{
		let dom_elem = self.get_dom_elem()?;
		let id = self.elem_id.to_string();

		dom_elem.add_event_listener(move |_: ClickEvent| {
			log::trace!("Button id: {} pressed", &id);
			callback();
		});

		Ok(())
	}
}

fn set_folder_path_text(text: &str) {
	js! {
		document.getElementById("inputFolderPath").value = @{text};
	}
}

fn get_datafile_path() -> Option<String> {
	let data_path = js! {
		let path = document.getElementById("inputFolderPath").value;
		let file = document.getElementById("inputFilename").value;
		return path + "/" + file + ".csv";
	}
	.into_string();

	if let Some(ref path) = &data_path {
		log::trace!("Data path sent to NI DAQ system: {}", path);
	}

	data_path
}

#[derive(Clone)]
struct NidaqServerConnection {
	socket_handle: stdweb::Value,
}

impl NidaqServerConnection {
	pub fn connect<F>(ip: &str, port: &str, callback: F) -> Self
	where
		F: FnMut() + 'static,
	{
		let socket_handle = js! {
			const net = require("net");
			var cb = @{callback};

			var socket = net.connect({host:@{ip}, port: @{port}},  () => {
				cb();
			});
			socket.setEncoding("utf8");

			return socket;
		};

		Self { socket_handle }
	}

	pub fn register_data_callback<F>(&self, mut callback: F)
	where
		F: FnMut(String) + 'static,
	{
		let intermediate_cb = move |data: stdweb::Value| {
			let data = match data.into_string() {
				Some(data) => data,
				_ => {
					log::error!("Error in TCP read callback");
					return;
				}
			};
			callback(data);
		};

		js! {
			@{&self.socket_handle}.on("data", (data) => {
				@{intermediate_cb}(data);
			})
		};
	}
}

impl io::Write for NidaqServerConnection {
	fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
		js! {
			@{&self.socket_handle}.write(Buffer.from(@{buf}));
		}

		Ok(buf.len())
	}

	fn flush(&mut self) -> IoResult<()> {
		Ok(())
	}
}

fn main() -> Result<(), Box<std::error::Error>> {
	stdweb::initialize();

	let dom = stdweb::web::document();
	let log_handle = SimpleLogger::init().unwrap();
	let ehandle = ElectronHandle::new();

	let net_client = NidaqServerConnection::connect("localhost", "8080", || {
		log::info!("Connected to NI DAQ System");
	});

	let stop_btn = AppButtonHandle::new(&dom, "btnStop");
	let start_btn = AppButtonHandle::new(&dom, "btnStart");
	let clear_log_btn = AppButtonHandle::new(&dom, "btnClearLog");
	let choose_dir_btn = AppButtonHandle::new(&dom, "btnChooseDir");

	let mut net_handle = net_client.clone();
	start_btn.register_click_callback(move || {
		let data_path = match get_datafile_path() {
			Some(path) => path,
			_ => {
				log::error!("Could not get datafile path");
				return;
			}
		};

		UiEvent::StartPressed(data_path).send(&mut net_handle);
	})?;

	let mut net_handle = net_client.clone();
	stop_btn.register_click_callback(move || {
		UiEvent::StopPressed.send(&mut net_handle);
	})?;

	net_client.register_data_callback(|data: String| {

		let uie = match UiEvent::try_from(data.as_str()) {
			Ok(uie) => uie,
			Err(_) => {
				log::error!("Could not deserialize UiEvent");
				return;
			}
		};
		
		log::trace!("Deserialized: {:?}", &uie);
	});

	clear_log_btn.register_click_callback(move || {
		log_handle.clear_log();
	})?;

	choose_dir_btn.register_click_callback(move || {
		let dir = match ehandle.show_file_dialog() {
			Some(dir) => dir,
			None => {
				log::warn!("No directory selected");
				return;
			}
		};

		set_folder_path_text(&dir);

		log::trace!("Chosen directory: {}", dir);
	})?;

	stdweb::event_loop();

	Ok(())
}
