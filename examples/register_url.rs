/// Example that launches as Mac App with custom URL scheme handler
///
/// In one terminal, build and run:
///
/// $ cargo build --features=logging --examples
/// $ ./target/debug/examples/register_url && tail -f ~/.fruitbasket_register_url.log
///
/// In a second terminal, open custom URL:
///
/// $ open fruitbasket://test
///
/// Log output will show that the example has received and printed the custom URL.
///
extern crate fruitbasket;
use fruitbasket::ActivationPolicy;
use fruitbasket::Trampoline;
use fruitbasket::FruitApp;
use fruitbasket::InstallDir;
use fruitbasket::RunPeriod;
use fruitbasket::FruitError;
use fruitbasket::FruitCallbackKey;
use std::time::Duration;
use std::path::PathBuf;

#[macro_use]
extern crate log;

fn main() {
    let _ = fruitbasket::create_logger(".fruitbasket_register_url.log", fruitbasket::LogDir::Home, 5, 2).unwrap();

    // Find the icon file from the Cargo project dir
    let icon = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples").join("icon.png");

    // Re-launch self in an app bundle if not already running from one.
    info!("Executable must run from App bundle.  Let's try:");
    let mut app = match Trampoline::new("fruitbasket_register_url", "fruitbasket", "com.trevorbentley.fruitbasket_register_url")
        .version("2.1.3")
        .icon("fruitbasket.icns")
        .plist_key("CFBundleSpokenName","\"fruit basket\"")
        .plist_keys(&vec![
            ("LSMinimumSystemVersion", "10.12.0"),
            ("LSBackgroundOnly", "1"),
        ])
        // Register "fruitbasket://" and "fbasket://" URL schemes in Info.plist
        .plist_raw_string("
CFBundleURLTypes = ( {
  CFBundleTypeRole = \"Viewer\";
  CFBundleURLName = \"Fruitbasket Example URL\";
  CFBundleURLSchemes = (\"fruitbasket\", \"fbasket\");
} );\n".into())
        .resource(icon.to_str().unwrap())
        .build(InstallDir::Temp) {
            Err(FruitError::UnsupportedPlatform(_)) => {
                info!("This is not a Mac.  App bundling is not supported.");
                info!("It is still safe to use FruitApp::new(), though the dummy app will do nothing.");
                FruitApp::new()
            },
            Err(FruitError::IOError(e)) => {
                info!("IO error! {}", e);
                std::process::exit(1);
            },
            Err(FruitError::GeneralError(e)) => {
                info!("General error! {}", e);
                std::process::exit(1);
            },
            Ok(app) => app,
        };

    // App is guaranteed to be running in a bundle now!

    // Make it a regular app in the dock.
    // Note: Because 'LSBackgroundOnly' is set to true in the Info.plist, the
    // app will launch backgrounded and will not take focus.  If we only did
    // that, the app would stay in 'Prohibited' mode and would not create a dock
    // icon.  By overriding the activation policy now, it will stay background
    // but create the Dock and menu bar entries.  This basically implements a
    // "pop-under" behavior.
    app.set_activation_policy(ActivationPolicy::Regular);

    // Register a callback for when the ObjC application finishes launching
    let stopper = app.stopper();
    app.register_callback(FruitCallbackKey::Method("applicationWillFinishLaunching:"),
                          Box::new(move |_event| {
                              info!("applicationDidFinishLaunching.");
                              stopper.stop();
                          }));

    // Run until callback is called
    info!("Spawned process started.  Run until applicationDidFinishLaunching.");
    let _ = app.run(RunPeriod::Forever);

    info!("Application launched.  Registering URL callbacks.");
    // Register a callback to get receive custom URL schemes from any Mac program
    app.register_apple_event(fruitbasket::kInternetEventClass, fruitbasket::kAEGetURL);
    let stopper = app.stopper();
    app.register_callback(FruitCallbackKey::Method("handleEvent:withReplyEvent:"),
                          Box::new(move |event| {
                              // Event is a raw NSAppleEventDescriptor.
                              // Fruitbasket has a parser for URLs.  Call that to get the URL:
                              let url: String = fruitbasket::parse_url_event(event);
                              info!("Received URL: {}", url);
                              stopper.stop();
                          }));

    // Run 'forever', until the URL callback fires
    info!("Spawned process running!");
    let _ = app.run(RunPeriod::Forever);
    info!("Run loop stopped after URL callback.");

    // Cleanly terminate
    fruitbasket::FruitApp::terminate(0);
    info!("This will not print.");
}
