//! hn-rs: Bindings for Hacker News (YCombinator) news feed API
//!
//! hn-rs is a simple binding around the firebase APIs for fetching the news
//! feed from Hacker News.  It spawns a thread that regularly updates the top
//! 60 items on Hacker News.
//!
//! The main class, `HackerNews`, exposes this list in the most recently sorted
//! order as a standard Rust iterator.  The iterator returns copies of the items
//! so the application can keep ownership if it wishes.
//!
//! Currently it only exposes methods to request the title and URL of news items.
//!
//! News items can be marked as 'hidden' so they are not returned in future
//! passes through the iterator.
//!
//! See the `examples/` dir for usage.
//!
extern crate time;
extern crate hyper;
extern crate hyper_tls;
extern crate tokio_core;
extern crate futures;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

use futures::future::Future;
use futures::future::Either;
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
const HN_URL_DISCUSSION: &'static str = "https://news.ycombinator.com/item?id=";

/// Stores the metadata about a single news item
#[derive(Deserialize, Debug, Clone)]
pub struct Item {
    by: String,
    descendants: Option<u64>,
    id: u64,
    kids: Option<Vec<u64>>,
    score: Option<u32>,
    time: u64,
    title: Option<String>,
    text: Option<String>,
    #[serde(rename(deserialize = "type"))]
    item_type: String,
    url: Option<String>,

    //
    // INTERNAL.  Not deserialized from HN JSON
    //
    #[serde(default)]
    seen: bool,
    #[serde(default)]
    hidden: bool,
}

impl Item {
    /// Return the title of the news item
    pub fn title(&self) -> String {
        self.title.clone().unwrap_or("".to_string())
    }
    /// Return the URL of the news item.
    ///
    /// This is the link to the external (non-HN) website if it has one, or a
    /// link to the HN comment section for stories without external links.
    pub fn url(&self) -> String {
        match self.url {
            Some(ref url) => url.clone(),
            None => format!("{}{}", HN_URL_DISCUSSION, self.id),
        }
    }
}

#[doc(hidden)]
#[derive(Clone,Default)]
pub struct Cache {
    x: Arc<RwLock<BTreeMap<u64, Item>>>,
}
impl std::ops::Deref for Cache {
    type Target = RwLock<BTreeMap<u64, Item>>;
    fn deref(&self) -> &Self::Target { &*self.x }
}

#[doc(hidden)]
#[derive(Clone,Default)]
pub struct TopList {
    x: Arc<RwLock<Vec<u64>>>,
}
impl std::ops::Deref for TopList {
    type Target = RwLock<Vec<u64>>;
    fn deref(&self) -> &Self::Target { &*self.x }
}

#[doc(hidden)]
#[derive(Default)]
pub struct IHackerNews {
    pub top: TopList,
    pub cache: Cache,
}

/// Main interface to the Hacker News API
///
/// # Examples
///
/// ```rust
/// extern crate hn;
/// use std::time::Duration;
/// use std::thread;
///
/// fn main() {
///   let hn = hn::HackerNews::new();
///   while hn.into_iter().count() == 0 {
///       thread::sleep(Duration::from_millis(1000));
///   }
///   for item in hn.into_iter() {
///       println!("item: {}", item.title());
///   }
/// }
/// ```
///
#[derive(Clone,Default)]
pub struct HackerNews {
    x: Arc<IHackerNews>,
}
impl std::ops::Deref for HackerNews {
    type Target = IHackerNews;
    fn deref(&self) -> &Self::Target { &*self.x }
}
impl<'a> IntoIterator for &'a HackerNews {
    type Item = Item;
    type IntoIter = HackerNewsIterator<'a>;
    fn into_iter(self) -> Self::IntoIter {
        HackerNewsIterator {
            hn: self,
            idx: 0,
        }
    }
}

/// Iterator for iterating over HN news stories
///
/// Items are returned in the same order that they were prioritized on HN at
/// the time of the last update.
pub struct HackerNewsIterator<'a> {
    hn: &'a HackerNews,
    idx: usize,
}
impl<'a> Iterator for HackerNewsIterator<'a> {
    type Item = Item;
    fn next(&mut self) -> Option<Item> {
        let reader = self.hn.top.read().unwrap();
        while self.idx < reader.len() {
            let item: Option<&u64> = (*reader).get(self.idx);
            if let Some(item) = item {
                if let Some(item) = self.hn.cache.write().unwrap().get_mut(item) {
                    self.idx += 1;
                    if !item.hidden {
                        item.seen = true;
                        return Some((*item).clone());
                    }
                }
            }
        }
        self.idx = 0;
        None
    }
}

impl HackerNews {
    /// Return a newly allocated HN wrapper, and spawn background thread
    pub fn new() -> HackerNews {
        let hn: HackerNews = Default::default();
        let thread_hn = hn.clone();
        let _ = thread::spawn(move || {
            HackerNews::hn_thread(thread_hn);
        });
        hn
    }
    /// Return number of items currently in the 'top list'
    pub fn len(&self) -> usize {
        self.top.read().unwrap().len()
    }
    /// Hide an item so it isn't returned in future iterator passes
    pub fn hide(&self, item: &Item) {
        let id = item.id;
        if let Some(item) = self.cache.write().unwrap().get_mut(&id) {
            item.hidden = true;
        }
    }
    fn hn_thread(hn: HackerNews) {
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        let https = HttpsConnector::new(4, &handle).unwrap();
        let client = Client::configure()
            .keep_alive(true)
            .connector(https)
            .build(&handle);
        let mut last_update_time = 0;
        loop {
            let now = time::now_utc().to_timespec().sec as i64;
            if now > last_update_time + 10 {
                if HackerNews::update_top_stories(&mut core, &client, &hn.top).is_ok() {
                    HackerNews::update_item_cache(&client, &handle, &hn.top, &hn.cache);
                }
                last_update_time = now;
            }
            core.turn(Some(Duration::from_millis(100)));
        }
    }
    fn update_top_stories(core: &mut Core,
                          client: &Client<HttpsConnector<HttpConnector>>,
                          top: &RwLock<Vec<u64>>) -> Result<(), hyper::error::Error> {
        let handle = core.handle();
        let uri = Uri::from_str(HN_URL_TOP_STORIES).ok().unwrap();
        let request = client.get(uri).and_then(|res| {
            res.body().concat2()
        });

        let timeout = tokio_core::reactor::Timeout::new(Duration::from_millis(5000), &handle).unwrap();
        let timed_request = request.select2(timeout).then(|res| match res {
            Ok(Either::A((data, _timeout))) => Ok(data),
            Ok(Either::B((_timeout_error, _get))) => {
                Err(hyper::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Timed out requesting top list",
                )))
            }
            Err(Either::A((error, _timeout))) => Err(error),
            Err(Either::B((timeout_error, _get))) => Err(From::from(timeout_error)),
        });

        let got = core.run(timed_request)?;
        let top_stories_str = std::str::from_utf8(&got).unwrap();
        {
            let mut writer = top.write().unwrap();
            let top_stories: Result<Vec<u64>,_> = serde_json::from_str(top_stories_str);
            if let Ok(mut top_stories) = top_stories {
                top_stories.truncate(60);
                *writer = top_stories;
            }
        }
        Ok(())
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
        let mut req_count = 0;
        for story in stories {
            if req_count >= 60 {
                // Max 60 per batch
                break;
            }
            let uri = format!("{}{}.json", HN_URL_ITEM, story);
            let id = story.clone();
            let uri = Uri::from_str(&uri).ok().unwrap();
            let future_cache = cache.clone();
            let req = client.get(uri).and_then(|res| {
                res.body().concat2()
            }).then(move |body| {
                if body.is_err() {
                    return Err(());
                }
                let body = body.unwrap();
                let item_str = std::str::from_utf8(&body).unwrap();
                let item: Result<Item,_> = serde_json::from_str(item_str);
                if let Ok(item) = item {
                    let mut writer = future_cache.write().unwrap();
                    (*writer).insert(id, item);
                }
                Ok(())
            });

            let timeout = tokio_core::reactor::Timeout::new(Duration::from_millis(5000), &handle).unwrap();
            let timed_request = req.select2(timeout).then(|_| { Ok(()) });
            handle.spawn(timed_request);
            req_count += 1;
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
