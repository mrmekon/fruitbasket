//! fruitbasket - Framework for running Rust programs in a Mac 'app bundle' environment.
//!
//! fruitbasket provides two different (but related) services for helping you run your
//! Rust binaries as native AppKit/Cocoa applications on Mac OS X:
//!
//! * App lifecycle and environment API - fruitbasket provides an API to initialize the
//!   AppKit application environment (NSApplication), to pump the main application loop
//!   and dispatch Apple events in a non-blocking way, to terminate the application, to
//!   access resources in the app bundle, and various other tasks frequently needed by
//!   Mac applications.
//!
//! * Self-bundling app 'trampoline' - fruitbasket provides a 'trampoline' to
//!   automatically bundle a standalone binary as a Mac application in a `.app` bundle
//!   at runtime.  This allows access to features that require running from a bundle (
//!   such as XPC services), self-installing into the Applications folder, registering
//!   your app with the system as a document type or URL handler, and various other
//!   features that are only available to bundled apps with unique identifiers.
//!   Self-bundling and relaunching itself (the "trampoline" behavior) allows your app
//!   to get the features of app bundles, but still be launched in the standard Rust
//!   ways (such as `cargo run`).
//!
//! The primary goal of fruitbasket is to make it reasonably easy to develop native
//! Mac GUI applications with the standard Apple AppKit/Cocoa/Foundation frameworks
//! in pure Rust by pushing all of the Apple and Objective-C runtime logic into
//! dedicated libraries, isolating the logic of a Rust binary application from the
//! unsafe platform code.  As the ecosystem of Mac libraries for Rust grows, you
//! should be able to mix-and-match the libraries your application needs, pump the
//! event loop with fruitbasket, and never worry about Objective-C in your application.
//!
//! # Getting Started
//!
//! You likely want to create either a [Trampoline](struct.Trampoline.html) or a
//! [FruitApp](struct.FruitApp.html) right after your Rust application starts.
//! If uncertain, use a `Trampoline`.  You can hit very strange behavior when running
//! Cocoa apps outside of an app bundle.
#![deny(missing_docs)]

use std::error::Error;
use std::time::Duration;
use std::sync::mpsc::Sender;

#[cfg(any(not(target_os = "macos"), feature="dummy"))]
use std::sync::mpsc::Receiver;
#[cfg(any(not(target_os = "macos"), feature="dummy"))]
use std::thread;

extern crate time;
extern crate dirs;

#[cfg(all(target_os = "macos", not(feature="dummy")))]
#[macro_use]
extern crate objc;

#[cfg(feature = "logging")]
#[allow(unused_imports)]
#[macro_use]
extern crate log;

#[cfg(feature = "logging")]
extern crate log4rs;

#[cfg(not(feature = "logging"))]
#[allow(unused_macros)]
macro_rules! info {
    ($x:expr) => {println!($x)};
    ($x:expr, $($arg:tt)+) => {println!($x, $($arg)+)};
}

/// Info.plist entries that have default values, but can be overridden
///
/// These properties are always set in the app bundle's Property List, with the
/// default values provided here, but can be overridden by your application with
/// the Trampoline builder's `plist_key*()` functions.
pub const DEFAULT_PLIST: &'static [(&'static str, &'static str)] = &[
    ("CFBundleInfoDictionaryVersion","6.0"),
    ("CFBundlePackageType","APPL"),
    ("CFBundleSignature","xxxx"),
    ("LSMinimumSystemVersion","10.10.0"),
];

/// Info.plist entries that are set, and cannot be overridden
///
/// These properties are always set in the app bundle's Property List, based on
/// information provided to the Trampoline builder, and cannot be overridden
/// with the builder's `plist_key*()` functions.
pub const FORBIDDEN_PLIST: &'static [&'static str] = & [
    "CFBundleName",
    "CFBundleDisplayName",
    "CFBundleIdentifier",
    "CFBundleExecutable",
    "CFBundleIconFile",
    "CFBundleVersion",
];

/// Apple kInternetEventClass constant
#[allow(non_upper_case_globals)]
pub const kInternetEventClass: u32 = 0x4755524c;
/// Apple kAEGetURL constant
#[allow(non_upper_case_globals)]
pub const kAEGetURL: u32 = 0x4755524c;
/// Apple keyDirectObject constant
#[allow(non_upper_case_globals)]
pub const keyDirectObject: u32 = 0x2d2d2d2d;

#[cfg(all(target_os = "macos", not(feature="dummy")))]
mod osx;

#[cfg(all(target_os = "macos", not(feature="dummy")))]
pub use osx::FruitApp;

#[cfg(all(target_os = "macos", not(feature="dummy")))]
pub use osx::Trampoline;

#[cfg(all(target_os = "macos", not(feature="dummy")))]
pub use osx::FruitObjcCallback;

#[cfg(all(target_os = "macos", not(feature="dummy")))]
pub use osx::FruitCallbackKey;

#[cfg(all(target_os = "macos", not(feature="dummy")))]
pub use osx::parse_url_event;

#[cfg(any(not(target_os = "macos"), feature="dummy"))]
/// Docs in OS X build.
pub enum FruitCallbackKey {
    /// Docs in OS X build.
    Method(&'static str),
    /// Docs in OS X build.
    Object(*mut u64),
}

#[cfg(any(not(target_os = "macos"), feature="dummy"))]
/// Docs in OS X build.
pub type FruitObjcCallback = Box<Fn(*mut u64)>;

/// Main interface for controlling and interacting with the AppKit app
///
/// Dummy implementation for non-OSX platforms.  See OS X build for proper
/// documentation.
#[cfg(any(not(target_os = "macos"), feature="dummy"))]
pub struct FruitApp {
    tx: Sender<()>,
    rx: Receiver<()>,
}
#[cfg(any(not(target_os = "macos"), feature="dummy"))]
impl FruitApp {
    /// Docs in OS X build.
    pub fn new() -> FruitApp {
        use std::sync::mpsc::channel;
        let (tx,rx) = channel();
        FruitApp{ tx: tx, rx: rx}
    }
    /// Docs in OS X build.
    pub fn register_callback(&mut self, _key: FruitCallbackKey, _cb: FruitObjcCallback) {}
    /// Docs in OS X build.
    pub fn register_apple_event(&mut self, _class: u32, _id: u32) {}
    /// Docs in OS X build.
    pub fn set_activation_policy(&self, _policy: ActivationPolicy) {}
    /// Docs in OS X build.
    pub fn terminate(exit_code: i32) {
        std::process::exit(exit_code);
    }
    /// Docs in OS X build.
    pub fn stop(stopper: &FruitStopper) {
        stopper.stop();
    }
    /// Docs in OS X build.
    pub fn run(&mut self, period: RunPeriod) -> Result<(),()> {
        let start = time::now_utc().to_timespec();
        loop {
            if self.rx.try_recv().is_ok() {
                return Err(());
            }
            if period == RunPeriod::Once {
                break;
            }
            thread::sleep(Duration::from_millis(500));
            if let RunPeriod::Time(t) = period {
                let now = time::now_utc().to_timespec();
                if now >= start + time::Duration::from_std(t).unwrap() {
                    break;
                }
            }
        }
        Ok(())
    }
    /// Docs in OS X build.
    pub fn stopper(&self) -> FruitStopper {
        FruitStopper { tx: self.tx.clone() }
    }
    /// Docs in OS X build.
    pub fn bundled_resource_path(_name: &str, _extension: &str) -> Option<String> { None }
}

#[cfg(any(not(target_os = "macos"), feature="dummy"))]
/// Docs in OS X build.
pub fn parse_url_event(_event: *mut u64) -> String { "".into() }

/// API to move the executable into a Mac app bundle and relaunch (if necessary)
///
/// Dummy implementation for non-OSX platforms.  See OS X build for proper
/// documentation.
#[cfg(any(not(target_os = "macos"), feature="dummy"))]
pub struct Trampoline {}
#[cfg(any(not(target_os = "macos"), feature="dummy"))]
impl Trampoline {
    /// Docs in OS X build.
    pub fn new(_name: &str, _exe: &str, _ident: &str) -> Trampoline { Trampoline {} }
    /// Docs in OS X build.
    pub fn name(&mut self, _name: &str) -> &mut Self { self }
    /// Docs in OS X build.
    pub fn exe(&mut self, _exe: &str) -> &mut Self { self }
    /// Docs in OS X build.
    pub fn ident(&mut self, _ident: &str) -> &mut Self { self }
    /// Docs in OS X build.
    pub fn icon(&mut self, _icon: &str) -> &mut Self { self }
    /// Docs in OS X build.
    pub fn version(&mut self, _version: &str) -> &mut Self { self }
    /// Docs in OS X build.
    pub fn plist_key(&mut self, _key: &str, _value: &str) -> &mut Self { self }
    /// Docs in OS X build.
    pub fn plist_keys(&mut self, _pairs: &Vec<(&str,&str)>) -> &mut Self { self }
    /// Docs in OS X build.
    pub fn plist_raw_string(&mut self, _s: String) -> &mut Self { self }
    /// Docs in OS X build.
    pub fn resource(&mut self, _file: &str) -> &mut Self { self }
    /// Docs in OS X build.
    pub fn resources(&mut self, _files: &Vec<&str>) -> &mut Self{ self }
    /// Docs in OS X build.
    pub fn build(&mut self, dir: InstallDir) -> Result<FruitApp, FruitError> {
        self.self_bundle(dir)?;
        unreachable!()
    }
    /// Docs in OS X build.
    pub fn self_bundle(&mut self, _dir: InstallDir) -> Result<(), FruitError> {
        Err(FruitError::UnsupportedPlatform("fruitbasket disabled or not supported on this platform.".to_string()))
    }
    /// Docs in OS X build.
    pub fn is_bundled() -> bool { false }
}

/// Options for how long to run the event loop on each call
#[derive(PartialEq)]
pub enum RunPeriod {
    /// Run event loop once and return
    Once,
    /// Run event loop forever, never returning and blocking the main thread
    Forever,
    /// Run event loop at least the specified length of time
    Time(Duration),
}

/// Policies controlling how a Mac application's UI is interacted with
pub enum ActivationPolicy {
    /// Appears in the Dock and menu bar and can have an interactive UI with windows
    Regular,
    /// Does not appear in Dock or menu bar, but may create windows
    Accessory,
    /// Does not appear in Dock or menu bar, may not create windows (background-only)
    Prohibited,
}

/// Class for errors generated by fruitbasket.  Dereferences to a String.
#[derive(Debug)]
pub enum FruitError {
    /// fruitbasket doesn't run on this platform (safe to ignore)
    UnsupportedPlatform(String),
    /// Disk I/O errors: failed to write app bundle to disk
    IOError(String),
    /// Any other unclassified error
    GeneralError(String),
}

impl std::fmt::Display for FruitError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl From<std::io::Error> for FruitError {
    fn from(error: std::io::Error) -> Self {
        FruitError::IOError(error.to_string())
    }
}
impl Error for FruitError {
    fn description(&self) -> &str {
        "Hmm"
    }
    fn cause(&self) -> Option<&dyn Error> {
        None
    }
}

/// An opaque, thread-safe object that can interrupt the run loop.
///
/// An object that is safe to pass across thread boundaries (i.e. it implements
/// Send and Sync), and can be used to interrupt and stop the run loop, even
/// when running in 'Forever' mode.  It can be Cloned infinite times and used
/// from any thread.
#[derive(Clone)]
pub struct FruitStopper {
    tx: Sender<()>,
}
impl FruitStopper {
    /// Stop the run loop on the `FruitApp` instance that created this object
    ///
    /// This is equivalent to passing the object to [FruitApp::stop](FruitApp::stop).  See it
    /// for more documentation.
    pub fn stop(&self) {
        let _ = self.tx.send(());
    }
}

/// Options for where to save generated app bundle
pub enum InstallDir {
    /// Store in a system-defined temporary directory
    Temp,
    /// Store in the system-wide Application directory (all users)
    SystemApplications,
    /// Store in the user-specific Application directory (current user)
    UserApplications,
    /// Store in a custom directory, specified as a String
    Custom(String),
}

/// Options for where to save logging output generated by fruitbasket
pub enum LogDir {
    /// User's home directory
    Home,
    /// Temporary directory (as specified by OS)
    Temp,
    /// Custom location, provided as a String
    Custom(String),
}

/// Enable logging to rolling log files with Rust `log` library
///
/// Requires the 'logging' feature to be specified at compile time.
///
/// This is a helper utility for configuring the Rust `log` and `log4rs`
/// libraries to redirect the `log` macros (`info!()`, `warn!()`, `err!()`, etc)
/// to both stdout and a rotating log file on disk.
///
/// If you specify the Home directory with a log named ".fruit.log" and a
/// backup count of 3, eventually you will end up with the files `~/.fruit.log`,
/// `~/.fruit.log.1`, `~/.fruit.log.2`, and `~/.fruit.log.3`
///
/// The maximum disk space used by the log files, in megabytes, will be:
///
/// `(backup_count + 1) * max_size_mb`
///
/// # Arguments
///
/// `filename` - Filename for the log file, *without* path
///
/// `dir` - Directory to save log files in.  This is provided as an enum,
///   `LogDir`, which offers some standard logging directories, or allows
///   specification of any custom directory.
///
/// `max_size_mb` - Max size (in megabytes) of the log file before it is rolled
///   into an archive file in the same directory.
///
/// `backup_count` - Number of archived log files to keep before deleting old
///   logs.
///
/// # Returns
///
/// Full path to opened log file on disk
#[cfg(feature = "logging")]
pub fn create_logger(filename: &str,
                     dir: LogDir,
                     max_size_mb: u32,
                     backup_count: u32) -> Result<String, String> {
    use log::LevelFilter;
    use self::log4rs::append::console::ConsoleAppender;
    use self::log4rs::append::rolling_file::RollingFileAppender;
    use self::log4rs::append::rolling_file::policy::compound::CompoundPolicy;
    use self::log4rs::append::rolling_file::policy::compound::roll::fixed_window::FixedWindowRoller;
    use self::log4rs::append::rolling_file::policy::compound::trigger::size::SizeTrigger;
    use self::log4rs::encode::pattern::PatternEncoder;
    use self::log4rs::config::{Appender, Config, Logger, Root};

    let log_path = match dir {
        LogDir::Home => format!("{}/{}", dirs::home_dir().unwrap().display(), filename),
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
        .logger(Logger::builder().build("app::backend::db", LevelFilter::Info))
        .logger(Logger::builder()
                .appender("requests")
                .additive(false)
                .build("app::requests", LevelFilter::Info))
        .build(Root::builder().appender("stdout").appender("requests").build(LevelFilter::Info))
        .unwrap();
    match log4rs::init_config(config) {
        Ok(_) => Ok(log_path),
        Err(e) => Err(e.to_string()),
    }
}
/// Enable logging to rolling log files with Rust `log` library
///
/// Requires the 'logging' feature to be specified at compile time.
///
/// This is a helper utility for configuring the Rust `log` and `log4rs`
/// libraries to redirect the `log` macros (`info!()`, `warn!()`, `error!()`, etc)
/// to both stdout and a rotating log file on disk.
///
/// If you specify the Home directory with a log named ".fruit.log" and a
/// backup count of 3, eventually you will end up with the files `~/.fruit.log`,
/// `~/.fruit.log.1`, `~/.fruit.log.2`, and `~/.fruit.log.3`
///
/// The maximum disk space used by the log files, in megabytes, will be:
///
/// `(backup_count + 1) * max_size_mb`
///
/// # Arguments
///
/// `filename` - Filename for the log file, *without* path
///
/// `dir` - Directory to save log files in.  This is provided as an enum,
///   `LogDir`, which offers some standard logging directories, or allows
///   specification of any custom directory.
///
/// `max_size_mb` - Max size (in megabytes) of the log file before it is rolled
///   into an archive file in the same directory.
///
/// `backup_count` - Number of archived log files to keep before deleting old
///   logs.
///
/// # Returns
///
/// Full path to opened log file on disk
#[cfg(not(feature = "logging"))]
pub fn create_logger(_filename: &str,
                     _dir: LogDir,
                     _max_size_mb: u32,
                     _backup_count: u32) -> Result<String, FruitError> {
    Err(FruitError::GeneralError("Must recompile with 'logging' feature to use logger.".to_string()))
}
