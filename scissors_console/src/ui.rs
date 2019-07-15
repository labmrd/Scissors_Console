use std::{cell::RefCell, path::{Path, PathBuf}};

use log::{Level, Metadata, Record};

use std::sync::{Mutex, Arc};
use std::thread;
use std::time as std_time;

use crate::data_collection::{self, DataCollectionHandle};
use nativefiledialog_rs as nfd;

pub struct WindowHandle;
pub struct WindowLogger;

extern crate regex;

thread_local! {
	static WINDOW: RefCell<Option<tether::Window>> = RefCell::new(None);
}

pub struct App {
	folder_path: PathBuf,
	data_collection_handle: Option<DataCollectionHandle>,
	beeper_stop_flag: Arc<Mutex<bool>>,
}

enum UiEventVariant<'a> {
	Init,
	Start(&'a str),
	Stop,
	ClearLog,
	ChooseDir,
	Unknown(&'a str),
}

struct UiEvent<'a> {
	variant: UiEventVariant<'a>,
	window: tether::Window,
	app: &'a mut App,
}

impl WindowHandle {
	fn eval(js: String) {
		tether::dispatch(|| {
			WINDOW.with(|win| {
				let _ = win
					.try_borrow()
					.map(|win| {
						win.as_ref()
							.expect("WindowHandle called from outside of main thread")
							.eval(js)
					})
					.map_err(|e| log::error!("{}", e));
			});
		});

		// tether::dispatch(|window| window.eval(js))
	}

	pub fn append_to_chart(time: f64, force1: f64, force2: f64, pos: i32) {
		let js = format!("append_to_chart({},{},{},{})", time, force1, force2, pos);
		Self::eval(js)
	}
}

impl WindowLogger {
	const LOG_LEVEL: Level = Level::Info;

	const fn new() -> Self {
		Self {}
	}

	pub fn init() -> Result<(), log::SetLoggerError> {
		static LOGGER: WindowLogger = WindowLogger::new();
		log::set_logger(&LOGGER).map(|_| log::set_max_level(log::LevelFilter::max()))
	}
}

impl log::Log for WindowLogger {
	fn enabled(&self, metadata: &Metadata) -> bool {
		metadata.level() <= Self::LOG_LEVEL
	}

	fn log(&self, record: &Record) {
		let time = time::now();

		let level = record.level();
		let args = record.args();
		let time_fmt = time.strftime("%I:%M:%S %p").expect("Failed to get time");
		// Always log to stderr
		eprintln!("{}\t{}\t{}", level, time_fmt, args);

		// Only sometimes log to the ui
		if self.enabled(record.metadata()) {
			let js = format!(r#"append_to_log("{}\t{}\t{}\n")"#, level, time_fmt, args);
			WindowHandle::eval(js);
		}
	}

	fn flush(&self) {}
}

impl App {
	pub fn new() -> Self {
		let tmp_dir = std::env::temp_dir();
		let beeper_stop_flag = Arc::new(Mutex::new(true));
		App {
			folder_path: tmp_dir,
			data_collection_handle: None,
			beeper_stop_flag,
		}
	}

	fn update_ui<S: AsRef<Path>>(window: &tether::Window, folder: S) {
		let js = format!(r#"update_folder_path("{}")"#, folder.as_ref().display());
		log::trace!("calling: {}", &js);
		window.eval(js);
	}

	fn update_folder_path(&mut self, window: &tether::Window, folder: String) {
		if cfg!(windows)
		{
			let re = regex::Regex::new(r"\\{1}").unwrap();
			let folder_sanitized : String = re.replace_all(&folder,"/").into_owned();
			Self::update_ui(window, &folder_sanitized);
		}
		else
		{
			Self::update_ui(window, &folder);
		}
		self.folder_path = folder.into();
	}

	fn read_flag(flag: &Arc<Mutex<bool>>) -> bool
	{
		return *(flag.lock().unwrap());
	}
}

impl<'a> From<&'a str> for UiEventVariant<'a> {
	fn from(msg: &'a str) -> UiEventVariant<'a> {
		if msg.contains('\n') && msg.starts_with("start") {
			return UiEventVariant::Start(&msg[6..]);
		}

		match msg {
			"init" => UiEventVariant::Init,
			"stop" => UiEventVariant::Stop,
			"clear_log" => UiEventVariant::ClearLog,
			"choose_dir" => UiEventVariant::ChooseDir,
			msg => UiEventVariant::Unknown(msg),
		}
	}
}

impl tether::Handler for App {
	fn handle(&mut self, win: tether::Window, msg: &str) {
		UiEvent::process(msg, win, self);
	}
}

impl UiEventVariant<'_> {}

impl UiEvent<'_> {
	fn process(msg: &str, window: tether::Window, app: &mut App) {
		let uie = Self::new(msg, window, app);
		uie.process_impl();
	}

	fn process_impl(self) {
		match self.variant {
			UiEventVariant::Init => self.init(),
			UiEventVariant::Start(file) => self.start(file),
			UiEventVariant::Stop => self.stop(),
			UiEventVariant::ClearLog => self.clear_log(),
			UiEventVariant::ChooseDir => self.choose_dir(),
			UiEventVariant::Unknown(msg) => self.unknown(msg),
		};
	}

	fn new<'a>(msg: &'a str, window: tether::Window, app: &'a mut App) -> UiEvent<'a> {
		let variant = UiEventVariant::from(msg);
		UiEvent {
			window,
			variant,
			app,
		}
	}

	fn init(self) {
		self.window.eval(include_str!("../ui/Chart.min.js"));
		self.window.eval(include_str!("../ui/init.js"));
		let win_clone = self.window.clone();
		WINDOW.with(|win| win.replace(Some(win_clone)));

		App::update_ui(&self.window, &self.app.folder_path);

		log::debug!("init called");
	}

	fn start(self, file: &str) {
		log::debug!(
			"Start button pressed, file: {}, path: {}",
			file,
			self.app.folder_path.display()
		);

		self.window.eval("clear_chart()");

		let col_handle = &mut self.app.data_collection_handle;

		let mut fpath = PathBuf::clone(&self.app.folder_path);
		fpath.push(file);

		if col_handle.is_none() {
			*col_handle = data_collection::start(&mut fpath);

			
            // Create a new Arc
            let c_stop_flag = Arc::clone(&self.app.beeper_stop_flag);

            // Lower flag
            let mut flag = self.app.beeper_stop_flag.lock().unwrap();
            *flag = false;

            // Spawn a new thread for the beeper
            let _handle = thread::spawn(move ||
            {
                // Get audio device
                let device = rodio::default_output_device()
                    .expect("Unable to get default audio output device!");
                
                // Create sink, set volume, and add tone
                let mut sink = rodio::Sink::new(&device);
                sink.set_volume(0.75);
                sink.pause();
                
                // Create different tones for initial stretching, then for grasping
                let stretch_tone = rodio::source::SineWave::new(300);
                let grasp_tone = rodio::source::SineWave::new(450);
                
                // Play stretch tone 3 times
                sink.append(stretch_tone);
                thread::sleep(std_time::Duration::from_millis(500));

                for _i in 0..3
                {
                    thread::sleep(std_time::Duration::from_millis(1250));
                    sink.play();
                    thread::sleep(std_time::Duration::from_millis(1250));
                    sink.pause();
                    if App::read_flag(&c_stop_flag) {break;};
                }
                sink.stop();

                if App::read_flag(&c_stop_flag) {return ();};
                
                let mut sink = rodio::Sink::new(&device);
                sink.set_volume(0.5);
                sink.pause();
                sink.append(grasp_tone);

                thread::sleep(std_time::Duration::from_millis(3000));

                loop
                {
					// Wait for next cycle
                    thread::sleep(std_time::Duration::from_millis(1000));

					// beep to move in position
                    sink.play();
                    thread::sleep(std_time::Duration::from_millis(200));
                    sink.pause();

					// prepare for grasp
                    thread::sleep(std_time::Duration::from_millis(500));

					// grasp
                    sink.play();
                    thread::sleep(std_time::Duration::from_millis(1500));
                    sink.pause();	// release grasp


					// check for stop button
                    if App::read_flag(&c_stop_flag) {break;};
                }
            });

			// If its still none, that means there was a file open error
			if col_handle.is_none() {
				log::error!("File '{}' already exists", fpath.display());
			}
		}
	}

	fn stop(self) {
		if let Some(dch) = self.app.data_collection_handle.take() {
			dch.stop();
		}

		// Raise flag to stop beeper
		let mut flag = self.app.beeper_stop_flag.lock().unwrap();
		*flag = true;

		log::debug!("Stop button pressed");
	}

	fn clear_log(self) {
		self.window.eval("clear_log()");
		log::debug!("Clear log button pressed");
	}

	fn choose_dir(self) {
		use nfd::Response::{Cancel, Okay};

		log::debug!("Choose directory button pressed");

		let tmp_dir = std::env::temp_dir();

		let folder = match nfd::open_pick_folder(tmp_dir.to_str()) {
			Ok(Okay(resp)) => resp,
			Ok(Cancel) => {
				log::debug!("Folder dialog canceled");
				return;
			}
			_ => {
				log::warn!("Folder dialog failure");
				return;
			}
		};

		log::debug!("Directory chosen: {}", &folder);
		self.app.update_folder_path(&self.window, folder);
	}

	fn unknown(self, msg: &str) {
		log::error!("Unrecognized message: {}", msg);
	}
}
