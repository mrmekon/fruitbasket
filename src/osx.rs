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
use std;
use std::thread;
use std::time::Duration;
use std::path::Path;
use std::path::PathBuf;
use std::io::Write;
use std::cell::Cell;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;

use super::FruitError;
use super::ActivationPolicy;
use super::RunPeriod;
use super::InstallDir;
use super::FruitStopper;
use super::DEFAULT_PLIST;
use super::FORBIDDEN_PLIST;

extern crate time;

extern crate objc;
use objc::runtime::Object;
use objc::runtime::Class;

#[allow(non_upper_case_globals)]
const nil: *mut Object = 0 as *mut Object;

#[link(name = "Foundation", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
#[link(name = "ApplicationServices", kind = "framework")]
#[link(name = "AppKit", kind = "framework")]
extern {}

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
    app: *mut Object,
    pool: Cell<*mut Object>,
    run_count: Cell<u64>,
    run_mode: *mut Object,
    tx: Sender<()>,
    rx: Receiver<()>,
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
    resources: Vec<String>,
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
    /// Add file to Resources directory of app bundle
    ///
    /// Specify full path to a file to copy into the Resources directory of the
    /// generated app bundle.  Resources can be any sort of file, and are copied
    /// around with the app when it is moved.  The app can easily access any
    /// file in its resources at runtime, even when running in sandboxed
    /// environments.
    ///
    /// The most common bundled resources are icons.
    ///
    /// # Arguments
    ///
    /// `file` - Full path to file to include
    pub fn resource(&mut self, file: &str) -> &mut Self {
        self.resources.push(file.to_string());
        self
    }

    /// Add multiple files to Resources directory of app bundle
    ///
    /// See documentation of `resource()`.  This function does the same, but
    /// allows specifying more than one resource at a time.
    pub fn resources(&mut self, files: &Vec<&str>) -> &mut Self{
        for file in files {
            self.resources.push(file.to_string());
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
                InstallDir::Custom(dir) => std::fs::canonicalize(PathBuf::from(dir))?,
            };
            info!("Install dir: {:?}", install_dir);
            let bundle_dir = Path::new(&install_dir).join(&format!("{}.app", self.name));
            info!("Bundle dir: {:?}", bundle_dir);
            let contents_dir = Path::new(&bundle_dir).join("Contents");
            let macos_dir = contents_dir.clone().join("MacOS");
            let resources_dir = contents_dir.clone().join("Resources");
            let plist = contents_dir.clone().join("Info.plist");
            let src_exe = std::env::current_exe()?;
            info!("Current exe: {:?}", src_exe);
            let dst_exe = macos_dir.clone().join(&self.exe);

            let _ = std::fs::remove_dir_all(&bundle_dir); // ignore errors
            std::fs::create_dir_all(&macos_dir)?;
            std::fs::create_dir_all(&resources_dir)?;
            info!("Copy {:?} to {:?}", src_exe, dst_exe);
            std::fs::copy(src_exe, dst_exe)?;

            for file in &self.resources {
                let file = Path::new(file);
                if let Some(filename) = file.file_name() {
                    let dst = resources_dir.clone().join(filename);
                    info!("Copy {:?} to {:?}", file, dst);
                    std::fs::copy(file, dst)?;
                }
            }

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
            let _:() = msg_send![wspace, launchApplication: s];

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
        let (tx,rx) = channel::<()>();
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
            FruitApp {
                app: app,
                pool: Cell::new(pool),
                run_count: Cell::new(0),
                run_mode: run_mode,
                tx: tx,
                rx: rx,
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
            let _:() = msg_send![self.app, setActivationPolicy: policy_int];
        }
    }

    /// Cleanly terminate the application
    ///
    /// Terminates a running application and its event loop, and terminates the
    /// process.  This function does not return, so perform any required cleanup
    /// of your Rust application before calling it.
    ///
    /// You should call this at the end of your program instead of simply exiting
    /// from `main()` to ensure that OS X knows your application has quit cleanly
    /// and can immediately inform any subsystems that are monitoring it.
    ///
    /// This can be called from any thread.
    ///
    /// # Arguments
    ///
    /// `exit_code` - Application exit code. '0' is success.
    pub fn terminate(exit_code: i32) {
        unsafe {
            let cls = objc::runtime::Class::get("NSApplication").unwrap();
            let app: *mut objc::runtime::Object = msg_send![cls, sharedApplication];
            let _:() = msg_send![app, terminate: exit_code];
        }
    }

    /// Stop the running app run loop
    ///
    /// If the run loop is running (`run()`), this stops it after the next event
    /// finishes processing.  It does not quit or terminate anything, and the
    /// run loop can be continued later.  This can be used from callbacks to
    /// interrupt a run loop running in 'Forever' mode and return control back
    /// to Rust's main thread.
    ///
    /// This can be called from any thread.
    ///
    /// # Arguments
    ///
    /// `stopper` - A thread-safe `FruitStopper` object returned by `stopper()`
    pub fn stop(stopper: &FruitStopper) {
        stopper.stop();
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
    ///
    /// # Returns
    ///
    /// Ok on natural end, Err if stopped by a Stopper.
    pub fn run(&mut self, period: RunPeriod) -> Result<(),()>{
        let start = time::now_utc().to_timespec();
        loop {
            if self.rx.try_recv().is_ok() {
                return Err(());
            }
            unsafe {
                let run_count = self.run_count.get();
                if run_count == 0 {
                    let _:() = msg_send![self.app, finishLaunching];
                }
                // Create a new release pool every once in a while, draining the old one
                if run_count % 100 == 0 {
                    let old_pool = self.pool.get();
                    if run_count != 0 {
                        let _:() = msg_send![old_pool, drain];
                    }
                    let cls = Class::get("NSAutoreleasePool").unwrap();
                    let pool: *mut Object = msg_send![cls, alloc];
                    let pool: *mut Object = msg_send![pool, init];
                    self.pool.set(pool);
                }
                let mode = self.run_mode;
                let event: *mut Object = msg_send![self.app,
                                                   nextEventMatchingMask: 0xffffffffffffffffu64
                                                   untilDate: nil
                                                   inMode: mode
                                                   dequeue: 1];
                let _:() = msg_send![self.app, sendEvent: event];
                let _:() = msg_send![self.app, updateWindows];
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
        return Ok(());
    }
    /// Create a thread-safe object that can interrupt the run loop
    ///
    /// Returns an object that is safe to pass across thread boundaries (i.e.
    /// it implements Send and Sync), and can be used to interrupt and stop
    /// the run loop, even when running in 'Forever' mode.
    ///
    /// This makes it convenient to implement the common strategy of blocking
    /// the main loop forever on the Apple run loop, until some other UI or
    /// processing thread interrupts it and lets the main thread handle cleanup
    /// and graceful shutdown.
    ///
    /// # Returns
    ///
    /// A newly allocated object that can be passed across thread boundaries and
    /// cloned infinite times..
    pub fn stopper(&self) -> FruitStopper {
        FruitStopper {
            tx: self.tx.clone()
        }
    }

    /// Locate a resource in the executing Mac App bundle
    ///
    /// Looks for a resource by name and extension in the bundled Resources
    /// directory.
    ///
    /// # Arguments
    ///
    /// `name` - Name of the file to find, without the extension
    ///
    /// `extension` - Extension of the file to find.  Can be an empty string for
    /// files with no extension.
    ///
    /// # Returns
    ///
    /// The full, absolute path to the resource, or None if not found.
    pub fn bundled_resource_path(name: &str, extension: &str) -> Option<String> {
        unsafe {
            let cls = Class::get("NSBundle").unwrap();
            let bundle: *mut Object = msg_send![cls, mainBundle];
            let cls = Class::get("NSString").unwrap();
            let objc_str: *mut Object = msg_send![cls, alloc];
            let objc_name: *mut Object = msg_send![objc_str,
                                                  initWithBytes:name.as_ptr()
                                                  length:name.len()
                                                  encoding: 4]; // UTF8_ENCODING
            let objc_str: *mut Object = msg_send![cls, alloc];
            let objc_ext: *mut Object = msg_send![objc_str,
                                                  initWithBytes:extension.as_ptr()
                                                  length:extension.len()
                                                  encoding: 4]; // UTF8_ENCODING
            let ini: *mut Object = msg_send![bundle,
                                             pathForResource:objc_name
                                             ofType:objc_ext];
            let _:() = msg_send![objc_name, release];
            let _:() = msg_send![objc_ext, release];
            let cstr: *const i8 = msg_send![ini, UTF8String];
            if cstr != std::ptr::null() {
                let rstr = std::ffi::CStr::from_ptr(cstr).to_string_lossy().into_owned();
                return Some(rstr);
            }
            None
        }
    }
}
