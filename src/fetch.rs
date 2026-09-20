//! HTTP for feeds: `ureq` with rustls, conditional GET (`If-None-Match`,
//! `If-Modified-Since`), a per-feed timeout, eight fetches at a time on a
//! thread pool. Milestone 1.
