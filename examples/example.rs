extern crate hn;

use hn::HackerNews;
use std::thread;
use std::time::Duration;

fn main() {
    let hn = HackerNews::new();
    let ui_hn = hn.clone();
    let ui_thread = thread::spawn(move || {
        let hn = ui_hn;
        loop {
            {
                let reader = hn.cache.read().unwrap();
                println!("items: {}", (*reader).len());
                for item in reader.iter() {
                    println!("item: {:?}", item);
                }
            }
            thread::sleep(Duration::from_millis(1000));
        }
    });

    let _ = ui_thread.join();

    loop {thread::sleep(Duration::from_millis(100));}
}
