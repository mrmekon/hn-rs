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
            println!("Refresh:");
            for item in hn.into_iter() {
                println!("item: {}", item.title());
            }
            if let Some(ref item) = hn.into_iter().nth(0) {
                hn.hide(item);
            }
            println!("");
            thread::sleep(Duration::from_millis(10000));
        }
    });

    let _ = ui_thread.join();

    loop {thread::sleep(Duration::from_millis(100));}
}
