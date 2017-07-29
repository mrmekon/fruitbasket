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
use std::thread;
use std::time::Duration;
use std::path::Path;
use std::path::PathBuf;
use std::io::Write;
use std::error::Error;
use std::cell::Cell;

extern crate time;

#[macro_use]
extern crate objc;
use objc::runtime::Object;
use objc::runtime::Class;

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

/// Main interface for controlling and interacting with the AppKit app
///
/// `FruitApp` is an instance of an AppKit app, equivalent to (and containing)
/// the NSApplication singleton that is responsible for the app's lifecycle
/// and participation in the Mac app ecosystem.
///
/// You must initialize a single instance of FruitApp before using any Apple
/// frameworks, and after creating it you must regularly pump its event loop.
///
/// You must follow all of the standard requirements for NSApplication.  Most
/// notably: FruitApp **must** be created on your app's main thread, and **must**
/// be pumped from the same main thread.  Doing otherwise angers the beast.
///
/// An application does *not* need to be in a Mac app bundle to run, so this can
/// be created in any application with `FruitApp::new()`.  However, many Apple
/// frameworks *do* require the application to be running from a bundle, so you
/// may want to consider creating your FruitApp instance from the `Trampoline`
/// struct's builder instead.
///
pub struct FruitApp {
    app: *mut objc::runtime::Object,
    pool: Cell<*mut Object>,
    run_count: Cell<u64>,
    run_mode: *mut Object,
    run_date: *mut Object,
}

/// API to move the executable into a Mac app bundle and relaunch (if necessary)
///
/// `Trampoline` is a builder pattern for creating a `FruitApp` application
/// instance that is guaranteed to be running inside a Mac app bundle.  See the
/// module documentation for why this is often important.
///
/// If the currently running process is already in an app bundle, Trampoline
/// does nothing and is equivalent to calling `FruitApp::new()`.
///
/// The builder takes a variety of information that is required for creating a
/// Mac app (notably: app name, executable name, unique identifier), as well
/// as optional metadata to describe your app and its interactions to the OS,
/// and optional file resources to bundle with it.  It creates an app bundle,
/// either in an install path of your choosing or in a temporary directory,
/// launches the bundled app, and terminates the non-bundled binary.
///
/// Care should be taken to call this very early in your application, since any
/// work done prior to this will be discarded when the app is relaunched.  Your
/// program should also gracefully support relaunching from a different directory.
/// Take care not to perform any actions that would prevent relaunching, such as
/// claiming locks, until after the trampoline.
///
#[derive(Default)]
pub struct Trampoline {
    name: String,
    exe: String,
    ident: String,
    icon: String,
    version: String,
    keys: Vec<(String,String)>,
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

impl Trampoline {
    /// Creates a new Trampoline builder to build a Mac app bundle
    ///
    /// This creates a new Trampoline builder, which takes the information
    /// required to construct a Mac `.app` bundle.  If your application
    /// is already running in a bundle, the builder does not create a bundle
    /// and simply returns a newly constructed `FruitApp` object with the Mac
    /// application environment initialized.  If your application is not in
    /// a bundle, a new bundle is created and launched, and your program's
    /// current process is killed.
    ///
    /// # Arguments
    ///
    /// `name` - Name for your Mac application.  This is what is displayed
    ///  in the dock and the menu bar.
    ///
    /// `exe` - Name for the executable file in your application.  This is the
    /// name of the process that executes, and what appears in `ps` or Activity
    /// Monitor.
    ///
    /// `ident` - Unique application identifier for your app.  This should be
    /// in the reverse DNS format (ex: `com.company.AppName`), and must contain
    /// only alpha-numerics, `-`, and `.` characters.  It can be used to
    /// register your app as a system-wide handler for documents and URIs, and
    /// is used when code signing your app for distribution.
    ///
    /// # Returns
    ///
    /// A newly constructed Trampoline builder.
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
    /// Set name of application.  Same as provided to `new()`.
    pub fn name(&mut self, name: &str) -> &mut Self {
        self.name = name.to_string();
        self
    }
    /// Set name of executable.  Same as provided to `new()`.
    pub fn exe(&mut self, exe: &str) -> &mut Self {
        self.exe = exe.to_string();
        self
    }
    /// Set app bundle ID.  Same as provided to `new()`.
    pub fn ident(&mut self, ident: &str) -> &mut Self {
        self.ident = ident.to_string();
        self
    }
    /// Set bundle icon file.
    ///
    /// This is the name of the icon file in the Resources directory.  It should
    /// be just the file name, without a path.  OS X uses this icon file for the
    /// icon displayed in the Dock when your application is running, and the
    /// icon that appears next to it in Finder and the Application list.
    ///
    /// Icons are typically provided in a multi-icon set file in the `.icns`
    /// format.
    ///
    /// It is optional, but strongly recommended for apps that will be
    /// distributed to end users.
    pub fn icon(&mut self, icon: &str) -> &mut Self {
        self.icon = icon.to_string();
        self
    }
    /// Set the bundle version.
    ///
    /// This sets the version number in the app bundle.  It is optional, and
    /// defaults to "1.0.0" if not provided.
    pub fn version(&mut self, version: &str) -> &mut Self {
        self.version = version.to_string();
        self
    }
    /// Set an arbitrary key/value pair in the Info.plist
    ///
    /// Bundles support specifying a large variety of configuration options in
    /// their Property List files, many of which are only needed for specific
    /// use cases.  This function lets you specify any arbitrary key/value
    /// pair that your application might need.
    ///
    /// Note that some keys are provided with a default value if not specified,
    /// and a few keys are always configured by the `Trampoline` builder and
    /// cannot be overridden with this function.
    ///
    /// `Trampoline` creates Info.plist files in the "old-style" OpenStep format.
    /// Be sure to format your values appropriately for this style.  Read up on
    /// [Old-Style ASCII Property Lists](https://developer.apple.com/library/content/documentation/Cocoa/Conceptual/PropertyLists/OldStylePlists/OldStylePLists.html).  You can also verify your
    /// formatting by creating a simple `test.plist` with your key/value pairs
    /// in it, surround the entire file in braces (`{' and ' }'), and then run
    /// `plutil test.plist` to validate the formatting.
    ///
    /// See the [Apple documentation](https://developer.apple.com/library/content/documentation/General/Reference/InfoPlistKeyReference/Introduction/Introduction.html#//apple_ref/doc/uid/TP40009247)
    /// on Info.plist keys for options.
    ///
    /// # Arguments
    ///
    /// `key` - Property List key to set (ex: `CFBundleURLTypes`)
    ///
    /// `value` - Value for the key, in JSON format.  You must provide quote
    /// characters yourself for any values that require quoted strings.  Format
    /// in "old-style" OpenStep plist format.
    pub fn plist_key(&mut self, key: &str, value: &str) -> &mut Self {
        self.keys.push((key.to_string(), value.to_string()));
        self
    }
    /// Set multiple arbitrary key/value pairs in the Info.plist
    ///
    /// See documentation of `plist_key()`.  This function does the same, but
    /// allows specifying more than one key/value pair at a time.
    pub fn plist_keys(&mut self, pairs: &Vec<(&str,&str)>) -> &mut Self {
        for &(ref key, ref value) in pairs {
            self.keys.push((key.to_string(), value.to_string()));
        }
        self
    }
    /// Finishes building and launching the app bundle
    ///
    /// This builds and executes the "trampoline", meaning it is a highly
    /// destructive action.  A Mac app bundle will be created on disk if the
    /// program is not already executing from one, the new bundle will be
    /// launched as a new process, and the currently executing process will
    /// be terminated.
    ///
    /// The behavior, when used as intended, is similar to `fork()` (except
    /// the child starts over from `main()` instead of continuing from the
    /// same instruction, and the parent dies).  The parent dies immediately,
    /// the child relaunches, re-runs the `Trampoline` builder, but this time
    /// it returns an initialized `FruitApp`.
    ///
    /// **WARNING**: the parent process is terminated with `exit(0)`, which
    /// does not Drop your Rust allocations.  This should always be called as
    /// early as possible in your program, before any allocations or locking.
    ///
    /// # Arguments
    ///
    /// `dir` - Directory to create app bundle in (if one is created)
    ///
    /// # Returns
    ///
    /// * Result<_, FruitError> if not running in a bundle and a new bundle
    ///   could not be created.
    /// * Result<_, FruitError> if running in a bundle but the Mac app
    ///   environment could not be initialized.
    /// * Terminates the process if not running in a Mac app bundle and a new
    ///   bundle was successfully created.
    /// * Result<FruitApp, _> if running in a Mac bundle (either when launched
    ///   from one initially, or successfully re-launched by `Trampoline`)
    ///   containing the initialized app environment,
    pub fn build(&mut self, dir: InstallDir) -> Result<FruitApp, FruitError> {
        self.self_bundle(dir)?; // terminates this process if not bundled
        info!("Process is bundled.  Continuing.");
        Ok(FruitApp::new())
    }
    /// Returns whether the current process is running from a Mac app bundle
    pub fn is_bundled() -> bool {
        unsafe {
            let cls = Class::get("NSBundle").unwrap();
            let bundle: *mut Object = msg_send![cls, mainBundle];
            let ident: *mut Object = msg_send![bundle, bundleIdentifier];
            ident != nil
        }
    }
    fn self_bundle(&self, dir: InstallDir) -> Result<(), FruitError> {
        unsafe {
            if Self::is_bundled() {
                return Ok(());
            }
            info!("Process not bundled.  Self-bundling and relaunching.");

            let install_dir: PathBuf = match dir {
                InstallDir::Temp => std::env::temp_dir(),
                InstallDir::SystemApplications => PathBuf::from("/Applications/"),
                InstallDir::UserApplications => std::env::home_dir().unwrap().join("Applications/"),
                InstallDir::Custom(dir) => PathBuf::from(dir),
            };
            let bundle_dir = Path::new(&install_dir).join(&format!("{}.app", self.name));
            let contents_dir = Path::new(&bundle_dir).join("Contents");
            let macos_dir = contents_dir.clone().join("MacOS");
            let resources_dir = contents_dir.clone().join("Resources");
            let plist = contents_dir.clone().join("Info.plist");
            let src_exe = std::env::current_exe()?;
            let dst_exe = macos_dir.clone().join(&self.exe);

            std::fs::create_dir_all(&macos_dir)?;
            std::fs::create_dir_all(&resources_dir)?;
            info!("Copy {:?} to {:?}", src_exe, dst_exe);
            std::fs::copy(src_exe, dst_exe)?;

            // Write Info.plist
            let mut f = std::fs::File::create(&plist)?;

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

impl FruitApp {
    /// Initialize the Apple app environment
    ///
    /// Initializes the NSApplication singleton that initializes the Mac app
    /// environment and  creates a memory pool for Objective-C allocations on
    /// the main thread.
    ///
    /// # Returns
    ///
    /// A newly allocated FruitApp for managing the app
    pub fn new() -> FruitApp {
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
            FruitApp {
                app: app,
                pool: Cell::new(pool),
                run_count: Cell::new(0),
                run_mode: run_mode,
                run_date: msg_send![date_cls, distantPast],
            }
        }
    }

    /// Set the app "activation policy" controlling what UI it does/can present.
    pub fn set_activation_policy(&self, policy: ActivationPolicy) {
        let policy_int = match policy {
            ActivationPolicy::Regular => 0,
            ActivationPolicy::Accessory => 1,
            ActivationPolicy::Prohibited => 2,
        };
        unsafe {
            msg_send![self.app, setActivationPolicy: policy_int];
        }
    }

    /// Runs the main application event loop
    ///
    /// The application's event loop must be run frequently to dispatch all
    /// events generated by the Apple frameworks to their destinations and keep
    /// the UI updated.  Take care to keep this running frequently, as any
    /// delays will cause the UI to hang and cause latency on other internal
    /// operations.
    ///
    /// # Arguments
    ///
    /// `period` - How long to run the event loop before returning
    pub fn run(&mut self, period: RunPeriod) {
        let start = time::now_utc().to_timespec();
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
            if period == RunPeriod::Once {
                break;
            }
            thread::sleep(Duration::from_millis(50));
            if let RunPeriod::Time(t) = period {
                let now = time::now_utc().to_timespec();
                if now >= start + time::Duration::from_std(t).unwrap() {
                    break;
                }
            }
        }
    }
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
#[cfg(not(feature = "logging"))]
pub fn create_logger(_filename: &str,
                     _dir: LogDir,
                     _max_size_mb: u32,
                     _backup_count: u32) -> Result<String, String> {
    Err("Must recompile with 'logging' feature to use logger.".to_string())
}
