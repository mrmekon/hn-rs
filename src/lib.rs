extern crate hyper;
extern crate hyper_tls;
extern crate tokio_core;
extern crate futures;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

use futures::future::Future;
use futures::stream::Stream;

use std::str::FromStr;

use std::time::Duration;
use std::thread;
use std::sync::Arc;
use std::sync::RwLock;
use std::collections::BTreeMap;

use hyper::Uri;
use hyper::client::Client;
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;

use tokio_core::reactor::Core;
use tokio_core::reactor::Handle;

const HN_URL_TOP_STORIES: &'static str = "https://hacker-news.firebaseio.com/v0/topstories.json";
const HN_URL_ITEM: &'static str = "https://hacker-news.firebaseio.com/v0/item/";

#[derive(Deserialize, Debug)]
pub struct Item {
    by: String,
    descendants: u32,
    id: u32,
    kids: Option<Vec<u32>>,
    score: u32,
    time: u64,
    title: String,
    #[serde(rename(deserialize = "type"))]
    item_type: String,
    url: Option<String>,
}

#[derive(Debug)]
pub struct ItemCache {
    item: Item,
    seen: bool,
    hidden: bool,
}

#[derive(Clone,Default)]
pub struct Cache {
    x: Arc<RwLock<BTreeMap<u64, ItemCache>>>,
}
impl std::ops::Deref for Cache {
    type Target = RwLock<BTreeMap<u64, ItemCache>>;
    fn deref(&self) -> &Self::Target { &*self.x }
}

#[derive(Clone,Default)]
pub struct TopList {
    x: Arc<RwLock<Vec<u64>>>,
}
impl std::ops::Deref for TopList {
    type Target = RwLock<Vec<u64>>;
    fn deref(&self) -> &Self::Target { &*self.x }
}

#[derive(Default)]
pub struct IHackerNews {
    pub top: TopList,
    pub cache: Cache,
}

#[derive(Clone,Default)]
pub struct HackerNews {
    x: Arc<IHackerNews>,
}
impl std::ops::Deref for HackerNews {
    type Target = IHackerNews;
    fn deref(&self) -> &Self::Target { &*self.x }
}


impl HackerNews {
    pub fn new() -> HackerNews {
        let hn: HackerNews = Default::default();
        let thread_hn = hn.clone();
        let _ = thread::spawn(move || {
            HackerNews::hn_thread(thread_hn);
        });
        hn
    }
    fn hn_thread(hn: HackerNews) {
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        let https = HttpsConnector::new(4, &handle).unwrap();
        let client = Client::configure()
            .keep_alive(true)
            .connector(https)
            .build(&handle);
        HackerNews::update_top_stories(&mut core, &client, &hn.top);
        HackerNews::update_item_cache(&client, &handle, &hn.top, &hn.cache);
        loop { core.turn(None); }
    }
    fn update_top_stories(core: &mut Core,
                          client: &Client<HttpsConnector<HttpConnector>>,
                          top: &RwLock<Vec<u64>>)  {
        let uri = Uri::from_str(HN_URL_TOP_STORIES).ok().unwrap();
        let request = client.get(uri).and_then(|res| {
            println!("and then...");
            res.body().concat2()
        });
        let got = core.run(request).unwrap();
        let top_stories_str = std::str::from_utf8(&got).unwrap();
        println!("Body: {}", top_stories_str);
        {
            let mut writer = top.write().unwrap();
            let mut top_stories: Vec<u64> = serde_json::from_str(top_stories_str).unwrap();
            top_stories.truncate(10);
            *writer = top_stories;;
        }
    }
    fn update_item_cache(client: &Client<HttpsConnector<HttpConnector>>,
                         handle: &Handle,
                         top: &RwLock<Vec<u64>>,
                         cache: &Cache) {
        let stories = top.read().unwrap();
        let stories: Vec<&u64>  = stories.iter().filter(|s| {
            let reader = cache.read().unwrap();
            !(*reader).contains_key(*s)
        }).collect();
        for story in stories {
            println!("Get story: {}", story);
            let uri = format!("{}{}.json", HN_URL_ITEM, story);
            let id = story.clone();
            let uri = Uri::from_str(&uri).ok().unwrap();
            let future_cache = cache.clone();
            let req = client.get(uri).and_then(|res| {
                res.body().concat2()
            }).then(move |body| {
                println!("got item {}", id);
                let body = body.unwrap();
                let item_str = std::str::from_utf8(&body).unwrap();
                let item: Item = serde_json::from_str(item_str).unwrap();
                println!("body: {:?}", item);
                {
                    let mut writer = future_cache.write().unwrap();
                    (*writer).insert(id, ItemCache {
                        item: item,
                        seen: false,
                        hidden: false,
                    });
                }
                Ok(())});
            handle.spawn(req);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn init() {
        use super::HackerNews;
        use std::time::Duration;
        use std::thread;
        let _ = HackerNews::new();
        thread::sleep(Duration::from_millis(300));
    }
}
