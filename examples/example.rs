extern crate fruitbasket;
use fruitbasket::Trampoline;

#[macro_use]
extern crate log;

fn main() {
    let _ = fruitbasket::create_logger(".fruitbasket.log", fruitbasket::LogDir::Home, 5, 2).unwrap();
    info!("Executable must run from App bundle.  Let's try:");
    let _ = Trampoline::new("fruitbasket", "fruitbasket", "com.trevorbentley.fruitbasket")
        .version("2.1.3")
        .icon("fruitbasket.icns")
        .plist_key("CFBundleSpokenName","\"fruit basket\"")
        .plist_keys(&mut vec![
            ("LSMinimumSystemVersion".to_string(), "10.12.0".to_string()),
        ])
        .build();
    std::thread::sleep(std::time::Duration::from_millis(5000));
    info!("Spawned process running.");
}
