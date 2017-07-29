extern crate fruitbasket;
use fruitbasket::Trampoline;
use fruitbasket::InstallDir;

#[macro_use]
extern crate log;

fn main() {
    let _ = fruitbasket::create_logger(".fruitbasket.log", fruitbasket::LogDir::Home, 5, 2).unwrap();

    // Re-launch self in an app bundle if not already running from one.
    info!("Executable must run from App bundle.  Let's try:");
    let _ = Trampoline::new("fruitbasket", "fruitbasket", "com.trevorbentley.fruitbasket")
        .version("2.1.3")
        .icon("fruitbasket.icns")
        .plist_key("CFBundleSpokenName","\"fruit basket\"")
        .plist_keys(&vec![
            ("LSMinimumSystemVersion", "10.12.0"),
        ])
        .build(InstallDir::Temp);

    // App is guaranteed to be running in a bundle now.
    // Give it a bit of time for the launching process to quit, to prove that
    // the bundled process is not a dependent child of the un-bundled process.
    info!("Spawned process started.  Sleeping for a bit...");
    std::thread::sleep(std::time::Duration::from_millis(5000));
    info!("Spawned process running!");
}
