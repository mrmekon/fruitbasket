extern crate fruitbasket;
use fruitbasket::ActivationPolicy;
use fruitbasket::Trampoline;
use fruitbasket::FruitApp;
use fruitbasket::InstallDir;
use fruitbasket::RunPeriod;
use fruitbasket::FruitError;
use std::time::Duration;
use std::path::PathBuf;

#[macro_use]
extern crate log;

fn main() {
    let _ = fruitbasket::create_logger(".fruitbasket.log", fruitbasket::LogDir::Home, 5, 2).unwrap();

    // Find the icon file from the Cargo project dir
    let icon = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples").join("icon.png");

    // Re-launch self in an app bundle if not already running from one.
    info!("Executable must run from App bundle.  Let's try:");
    let mut app = match Trampoline::new("fruitbasket", "fruitbasket", "com.trevorbentley.fruitbasket")
        .version("2.1.3")
        .icon("fruitbasket.icns")
        .plist_key("CFBundleSpokenName","\"fruit basket\"")
        .plist_keys(&vec![
            ("LSMinimumSystemVersion", "10.12.0"),
            ("LSBackgroundOnly", "1"),
        ])
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

    // Give it a bit of time for the launching process to quit, to prove that
    // the bundled process is not a dependent child of the un-bundled process.
    info!("Spawned process started.  Sleeping for a bit...");
    let _ = app.run(RunPeriod::Time(Duration::from_secs(1)));

    // Demonstrate stopping an infinite run loop from another thread.
    let stopper = app.stopper();
    let _ = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(4));
        info!("Stopping run loop.");
        fruitbasket::FruitApp::stop(&stopper);
    });

    // Run 'forever', until the other thread interrupts.
    info!("Spawned process running!");
    let _ = app.run(RunPeriod::Forever);
    info!("Run loop stopped from other thread.");

    // Find the icon we stored in the bundle
    let icon = fruitbasket::FruitApp::bundled_resource_path("icon", "png");
    info!("Bundled icon: {}", icon.unwrap_or("MISSING!".to_string()));

    // Cleanly terminate
    fruitbasket::FruitApp::terminate(0);
    info!("This will not print.");
}
