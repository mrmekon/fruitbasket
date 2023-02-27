extern crate fruitbasket;

use std::io::Write;
use fruitbasket::{ActivationPolicy, FruitApp, FruitCallbackKey, FruitError, Trampoline};

#[macro_use]
extern crate log;

fn main() {
    // Setup logging.  Requires 'logging' feature to be enabled.
    let _ = fruitbasket::create_logger(".fruitbasket_terminal.log", fruitbasket::LogDir::Home, 5, 2).unwrap();

    let mut app = match Trampoline::new("fruitbasket_terminal", "fruitbasket_terminal", "com.trevorbentley.fruitbasket_terminal")
        .version(env!("CARGO_PKG_VERSION"))
        .plist_key("CFBundleSpokenName", "fruit basket terminal")
        .build(fruitbasket::InstallDir::Temp)
    {
        Err(FruitError::UnsupportedPlatform(_)) => {
            println!("This is not a Mac.  App bundling is not supported.");
            // It is still safe to use FruitApp::new(),
            // though the dummy app will do nothing.
            FruitApp::new()
        },
        Err(FruitError::IOError(e)) => {
            println!("IO error! {}", e);
            std::process::exit(1);
        },
        Err(FruitError::GeneralError(e)) => {
            println!("General error! {}", e);
            std::process::exit(1);
        },
        Ok(app) => app,
    };

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
    let _ = app.run(fruitbasket::RunPeriod::Forever);

    // Print a prompt and read a line of input:
    // > This should come before the prompt.
    // > Please enter your name: Harry Potter
    // > Hello, Harry Potter!
    print!("Please enter your name: ");
    eprintln!("This should come before the prompt.");
    std::io::stdout().flush().unwrap();
    let mut name = String::new();
    std::io::stdin().read_line(&mut name)
        .expect("Failed to read line");
    if name.ends_with('\n') {
        name.pop();
    }
    println!("Hello, {}!", name);

    // Cleanly terminate
    fruitbasket::FruitApp::terminate(0);
    println!("This will not print.");
}
