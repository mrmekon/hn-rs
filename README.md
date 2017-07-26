# hn-rs - Rust wrapper for the Hacker News (YCombinator) API

[![OSX/Linux Build Status](https://travis-ci.org/mrmekon/hn-rs.svg?branch=master)](https://travis-ci.org/mrmekon/hn-rs)
[![Crates.io Version](https://img.shields.io/crates/v/hn.svg)](https://crates.io/crates/hn)

hn-rs is a simple binding around the firebase APIs for fetching the news feed from Hacker News.  It spawns a thread that regularly updates the top 60 items on Hacker News.

The main class, `HackerNews`, exposes this list in the most recently sorted order as a standard Rust iterator.  The iterator returns copies of the items so the application can keep ownership if it wishes.

Currently it only exposes methods to request the title and URL of news items.

News items can be marked as 'hidden' so they are not returned in future passes through the iterator.

See the `examples/` dir for usage.

## Documentation

[API documentation](https://mrmekon.github.io/hn-rs/hn/)
