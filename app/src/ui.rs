use std::{path::PathBuf, sync::Arc, thread};

use log::{Level, Metadata, Record};
use parking_lot::Mutex;

pub fn create() -> WindowHandle {
    let app = App::new();

    let logger_handle = WindowHandle::clone(&app.win);

    // If we can't initialize the logger, might as well panic
    WindowLogger::init(logger_handle).unwrap();

    let win_handle = WindowHandle::clone(&app.win);

    thread::spawn(move || {
        tether::builder()
            .html(include_str!("../ui/index.html"))
            .minimum_size(800, 600)
            .handler(app)
            .start();
    });

    win_handle
}

pub struct WindowHandle {
    inner: Arc<Mutex<Option<tether::Window>>>,
}

struct WindowLogger {
    handle: WindowHandle,
}

struct App {
    folder_path: PathBuf,
    win: WindowHandle,
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
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    fn get(&self) -> Option<tether::Window> {
        self.inner.try_lock().and_then(|lock| *lock)
    }

    fn set(&self, other: tether::Window) {
        let mut lock = self.inner.lock();
        *lock = Some(other);
    }

    fn clone(handle: &Self) -> Self {
        Self {
            inner: Arc::clone(&handle.inner),
        }
    }

    fn append_to_chart(&self, time: f64, force: f64) {
        if let Some(handle) = self.get() {
            let call = format!("append_to_chart({},{})", time, force);
            handle.eval(&call);
        }
    }
}

unsafe impl Send for WindowHandle {}
unsafe impl Sync for WindowHandle {}

impl WindowLogger {
	#[cfg(debug_assertions)]
	const LOG_LEVEL: Level = Level::Debug;

	#[cfg(not(debug_assertions))]
	const LOG_LEVEL: Level = Level::Warn;

    fn new(handle: WindowHandle) -> Self {
        Self { handle }
    }

    fn init(handle: WindowHandle) -> Result<(), log::SetLoggerError> {
        let boxed = Box::new(WindowLogger::new(handle));
        log::set_boxed_logger(boxed).map(|_| log::set_max_level(log::LevelFilter::max()))
    }
}

impl log::Log for WindowLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Self::LOG_LEVEL
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let level = record.level();
        let args = record.args();

        if let Some(handle) = self.handle.get() {
            let js = format!(r#"append_to_log("{}\t{}\n")"#, level, args);
            handle.eval(&js);
        } else {
            println!("{}\t{}", level, args);
        }
    }

    fn flush(&self) {}
}

impl App {
    fn new() -> Self {
        let tmp_dir = std::env::temp_dir();
        let win = WindowHandle::new();
        App {
            folder_path: tmp_dir,
            win,
        }
    }

    fn update_ui<S: AsRef<str>>(window: &tether::Window, folder: S) {
        let js = format!("update_folder_path({})", tether::escape(folder.as_ref()));
        window.eval(&js);
    }

    fn update_folder_path(&mut self, window: &tether::Window, folder: String) {
        Self::update_ui(window, &folder);
        self.folder_path = folder.into();
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
    fn message(&mut self, win: tether::Window, msg: &str) {
        self.win.set(win);
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

        App::update_ui(&self.window, self.app.folder_path.to_string_lossy());

        log::debug!("init called");
    }

    fn start(self, file: &str) {
        log::debug!(
            "Start button pressed, file: {}, path: {}",
            file,
            self.app.folder_path.to_string_lossy()
        );
    }

    fn stop(self) {
        self.app.win.append_to_chart(1.0, 100.0);
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
