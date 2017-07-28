use std::thread;
use std::time::Duration;
use std::path::Path;
use std::io::Write;

#[macro_use]
extern crate objc;
use objc::runtime::Object;
use objc::runtime::Class;
use std::cell::Cell;

#[cfg(feature = "logging")]
#[macro_use]
extern crate log;
#[cfg(feature = "logging")]
extern crate log4rs;

#[cfg(not(feature = "logging"))]
macro_rules! info {
    ($x:expr) => {println!($x)};
    ($x:expr, $($arg:tt)+) => {println!($x, $($arg)+)};
}

#[allow(non_upper_case_globals)]
const nil: *mut Object = 0 as *mut Object;

#[link(name = "Foundation", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
#[link(name = "ApplicationServices", kind = "framework")]
#[link(name = "AppKit", kind = "framework")]
extern {}

pub const DEFAULT_PLIST: &'static [(&'static str, &'static str)] = &[
    ("CFBundleInfoDictionaryVersion","6.0"),
    ("CFBundlePackageType","APPL"),
    ("CFBundleSignature","xxxx"),
    ("LSMinimumSystemVersion","10.10.0"),
];
pub const FORBIDDEN_PLIST: &'static [&'static str] = & [
    "CFBundleName",
    "CFBundleDisplayName",
    "CFBundleIdentifier",
    "CFBundleExecutable",
    "CFBundleIconFile",
    "CFBundleVersion",
];

pub struct NSApp {
    app: *mut objc::runtime::Object,
    pool: Cell<*mut Object>,
    run_count: Cell<u64>,
    run_mode: *mut Object,
    run_date: *mut Object,
}

#[derive(Default)]
pub struct Trampoline {
    name: String,
    exe: String,
    ident: String,
    icon: String,
    version: String,
    keys: Vec<(String,String)>,
}

use std::error::Error;
pub struct FruitError {
    err: String,
}
impl FruitError {
    fn new(err: &str) -> FruitError {
        FruitError { err: err.to_string() }
    }
}
impl std::ops::Deref for FruitError {
    type Target = String;
    fn deref(&self) -> &Self::Target { &self.err }
}

impl From<std::io::Error> for FruitError {
    fn from(error: std::io::Error) -> Self {
        FruitError::new(error.description())
    }
}

impl Trampoline {
    pub fn new(name: &str, exe: &str, ident: &str) -> Trampoline {
        Trampoline {
            name: name.to_string(),
            exe: exe.to_string(),
            ident: ident.to_string(),
            version: "1.0.0".to_string(),
            ..
            Default::default()
        }
    }
    pub fn name(&mut self, name: &str) -> &mut Self {
        self.name = name.to_string();
        self
    }
    pub fn exe(&mut self, exe: &str) -> &mut Self {
        self.exe = exe.to_string();
        self
    }
    pub fn ident(&mut self, ident: &str) -> &mut Self {
        self.ident = ident.to_string();
        self
    }
    pub fn icon(&mut self, icon: &str) -> &mut Self {
        self.icon = icon.to_string();
        self
    }
    pub fn version(&mut self, version: &str) -> &mut Self {
        self.version = version.to_string();
        self
    }
    pub fn plist_key(&mut self, key: &str, value: &str) -> &mut Self {
        self.keys.push((key.to_string(), value.to_string()));
        self
    }
    pub fn plist_keys(&mut self, mut pairs: &mut Vec<(String,String)>) -> &mut Self {
        self.keys.append(&mut pairs);
        self
    }
    pub fn build(&mut self) -> Result<NSApp, FruitError> {
        self.self_bundle()?; // terminates this process if not bundled
        info!("Process is bundled.  Continuing.");
        Ok(NSApp::new())
    }
    fn is_bundled() -> bool {
        unsafe {
            let cls = Class::get("NSBundle").unwrap();
            let bundle: *mut Object = msg_send![cls, mainBundle];
            let ident: *mut Object = msg_send![bundle, bundleIdentifier];
            ident != nil
        }
    }
    fn self_bundle(&self) -> Result<(), FruitError> {
        unsafe {
            if Self::is_bundled() {
                return Ok(());
            }
            info!("Process not bundled.  Self-bundling and relaunching.");

            let temp_dir = std::env::temp_dir();
            let bundle_dir = Path::new(&temp_dir).join(&format!("{}.app", self.name));
            let contents_dir = Path::new(&bundle_dir).join("Contents");
            let macos_dir = contents_dir.clone().join("MacOS");
            let resources_dir = contents_dir.clone().join("Resources");
            let plist = contents_dir.clone().join("Info.plist");
            let src_exe = std::env::current_exe().unwrap();
            let dst_exe = macos_dir.clone().join(&self.exe);

            std::fs::create_dir_all(&macos_dir).unwrap();
            std::fs::create_dir_all(&resources_dir).unwrap();
            info!("Copy {:?} to {:?}", src_exe, dst_exe);
            std::fs::copy(src_exe, dst_exe).unwrap();

            // Write Info.plist
            let mut f = std::fs::File::create(&plist).unwrap();

            // Mandatory fields
            write!(&mut f, "{{\n")?;
            write!(&mut f, "  CFBundleName = {};\n", self.name)?;
            write!(&mut f, "  CFBundleDisplayName = {};\n", self.name)?;
            write!(&mut f, "  CFBundleIdentifier = \"{}\";\n", self.ident)?;
            write!(&mut f, "  CFBundleExecutable = {};\n", self.exe)?;
            write!(&mut f, "  CFBundleIconFile = \"{}\";\n", self.icon)?;
            write!(&mut f, "  CFBundleVersion = \"{}\";\n", self.version)?;

            // User-supplied fields
            for &(ref key, ref val) in &self.keys {
                if !FORBIDDEN_PLIST.contains(&key.as_str()) {
                    write!(&mut f, "  {} = {};\n", key, val)?;
                }
            }

            // Default fields (if user didn't override)
            let keys: Vec<&str> = self.keys.iter().map(|x| {x.0.as_ref()}).collect();
            for &(ref key, ref val) in DEFAULT_PLIST {
                if !keys.contains(key) {
                    write!(&mut f, "  {} = {};\n", key, val)?;
                }
            }
            write!(&mut f, "}}\n")?;

            // Launch newly created bundle
            let cls = Class::get("NSWorkspace").unwrap();
            let wspace: *mut Object = msg_send![cls, sharedWorkspace];
            let cls = Class::get("NSString").unwrap();
            let app = bundle_dir.to_str().unwrap();
            info!("Launching: {}", app);
            let s: *mut Object = msg_send![cls, alloc];
            let s: *mut Object = msg_send![s,
                                           initWithBytes:app.as_ptr()
                                           length:app.len()
                                           encoding: 4]; // UTF8_ENCODING
            msg_send![wspace, launchApplication: s];

            // Note: launchedApplication doesn't return until the child process
            // calls [NSApplication sharedApplication].
            info!("Parent process exited.");
            std::process::exit(0);
        }
    }
}

pub enum LogDir {
    Home,
    Temp,
    Custom(String),
}

impl NSApp {
    pub fn new() -> NSApp {
        unsafe {
            let cls = Class::get("NSApplication").unwrap();
            let app: *mut Object = msg_send![cls, sharedApplication];
            let cls = Class::get("NSAutoreleasePool").unwrap();
            let pool: *mut Object = msg_send![cls, alloc];
            let pool: *mut Object = msg_send![pool, init];
            let cls = Class::get("NSString").unwrap();
            let rust_runmode = "kCFRunLoopDefaultMode";
            let run_mode: *mut Object = msg_send![cls, alloc];
            let run_mode: *mut Object = msg_send![run_mode,
                                                  initWithBytes:rust_runmode.as_ptr()
                                                  length:rust_runmode.len()
                                                  encoding: 4]; // UTF8_ENCODING
            let date_cls = Class::get("NSDate").unwrap();
            NSApp {
                app: app,
                pool: Cell::new(pool),
                run_count: Cell::new(0),
                run_mode: run_mode,
                run_date: msg_send![date_cls, distantPast],
            }
        }
    }
    pub fn run(&mut self, block: bool) {
        loop {
            unsafe {
                let run_count = self.run_count.get();
                // Create a new release pool every once in a while, draining the old one
                if run_count % 100 == 0 {
                    let old_pool = self.pool.get();
                    if run_count != 0 {
                        let _ = msg_send![old_pool, drain];
                    }
                    let cls = Class::get("NSAutoreleasePool").unwrap();
                    let pool: *mut Object = msg_send![cls, alloc];
                    let pool: *mut Object = msg_send![pool, init];
                    self.pool.set(pool);
                }
                let mode = self.run_mode;
                let event: *mut Object = msg_send![self.app, nextEventMatchingMask: -1
                                                  untilDate: self.run_date inMode:mode dequeue: 1];
                let _ = msg_send![self.app, sendEvent: event];
                let _ = msg_send![self.app, updateWindows];
                self.run_count.set(run_count + 1);
            }
            if !block { break; }
            thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(feature = "logging")]
pub fn create_logger(filename: &str,
                     dir: LogDir,
                     max_size_mb: u32,
                     backup_count: u32) -> Result<String, String> {
    use log::LogLevelFilter;
    use self::log4rs::append::console::ConsoleAppender;
    use self::log4rs::append::rolling_file::RollingFileAppender;
    use self::log4rs::append::rolling_file::policy::compound::CompoundPolicy;
    use self::log4rs::append::rolling_file::policy::compound::roll::fixed_window::FixedWindowRoller;
    use self::log4rs::append::rolling_file::policy::compound::trigger::size::SizeTrigger;
    use self::log4rs::encode::pattern::PatternEncoder;
    use self::log4rs::config::{Appender, Config, Logger, Root};

    let log_path = match dir {
        LogDir::Home => format!("{}/{}", std::env::home_dir().unwrap().display(), filename),
        LogDir::Temp => format!("{}/{}", std::env::temp_dir().display(), filename),
        LogDir::Custom(s) => format!("{}/{}", s, filename),
    };
    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{m}{n}")))
        .build();
    let trigger = Box::new(SizeTrigger::new(1024*1024*max_size_mb as u64));
    let roller = Box::new(FixedWindowRoller::builder()
                          .build(&format!("{}.{{}}", log_path), backup_count).unwrap());
    let policy = Box::new(CompoundPolicy::new(trigger, roller));
    let rolling = RollingFileAppender::builder()
        .build(&log_path, policy)
        .unwrap();

    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .appender(Appender::builder().build("requests", Box::new(rolling)))
        .logger(Logger::builder().build("app::backend::db", LogLevelFilter::Info))
        .logger(Logger::builder()
                .appender("requests")
                .additive(false)
                .build("app::requests", LogLevelFilter::Info))
        .build(Root::builder().appender("stdout").appender("requests").build(LogLevelFilter::Info))
        .unwrap();
    let _ = log4rs::init_config(config).unwrap();
    Ok(log_path)
}
#[cfg(not(feature = "logging"))]
pub fn create_logger(_filename: &str,
                     _dir: LogDir,
                     _max_size_mb: u32,
                     _backup_count: u32) -> Result<String, String> {
    Err("Must recompile with 'logging' feature to use logger.".to_string())
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
