extern crate fruitbasket;
use fruitbasket::ActivationPolicy;
use fruitbasket::Trampoline;
use fruitbasket::InstallDir;
use fruitbasket::RunPeriod;
use std::time::Duration;

#[macro_use]
extern crate log;

fn main() {
    let _ = fruitbasket::create_logger(".fruitbasket.log", fruitbasket::LogDir::Home, 5, 2).unwrap();

    // Re-launch self in an app bundle if not already running from one.
    info!("Executable must run from App bundle.  Let's try:");
    let mut app = Trampoline::new("fruitbasket", "fruitbasket", "com.trevorbentley.fruitbasket")
        .version("2.1.3")
        .icon("fruitbasket.icns")
        .plist_key("CFBundleSpokenName","\"fruit basket\"")
        .plist_keys(&vec![
            ("LSMinimumSystemVersion", "10.12.0"),
            ("LSBackgroundOnly", "1"),
        ])
        .build(InstallDir::Temp).unwrap();

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
    app.run(RunPeriod::Time(Duration::from_secs(1)));

    // Demonstrate stopping an infinite run loop from another thread.
    let stopper = app.stopper();
    let _ = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(4));
        info!("Stopping run loop.");
        fruitbasket::FruitApp::stop(&stopper);
    });

    // Run 'forever', until the other thread interrupts.
    info!("Spawned process running!");
    app.run(RunPeriod::Forever);
    info!("Run loop stopped from other thread.");

    // Cleanly terminate
    fruitbasket::FruitApp::terminate(0);
    info!("This will not print.");
}
