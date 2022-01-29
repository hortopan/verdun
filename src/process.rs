use super::*;
use select::document::Document;
use select::predicate::Name;
use std::sync::mpsc::{channel, Sender};
use tokio::sync::Semaphore;

#[derive(Debug, Clone)]
pub struct UrlItem {
    pub parent: Url,
    pub url: Url,
}

#[derive(Debug, Clone)]
pub enum Action {
    ProcessURL(UrlItem),
    Ping,
}

#[derive(Debug)]
pub struct HttpResponse {
    pub status: reqwest::StatusCode,
    pub duration: Duration,
    pub length: usize,
}

#[derive(Debug)]
pub enum HttpResult {
    Response(HttpResponse),
    Error(reqwest::Error),
}

type HttpResultsHolder = Arc<Mutex<Vec<HttpResult>>>;

#[tokio::main]
pub async fn run(config: config::Config, requested_stop: Arc<AtomicBool>) -> HttpResultsHolder {
    let allowed_domains = config.allowed_domains.clone();
    let method = config.method.clone();
    let verbose = config.verbose;
    let headers = config.headers.clone();
    let prevent_duplicate_requests = config.prevent_duplicate_requests;
    let requests = config.requests;
    let duration = config.duration;
    let timeout = config.timeout;
    let concurrent = config.concurrent;
    let basic_auth = config.basic_auth.clone();

    let ad = allowed_domains.clone();
    let http_client = reqwest::Client::builder()
        .redirect(match config.follow_redirects {
            true => reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() > 5 {
                    attempt.error("too many redirects")
                } else if is_allowed_host(&attempt.url(), &ad) {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }),
            false => reqwest::redirect::Policy::none(),
        })
        .user_agent(format!("{}/{}", APP_NAME, VERSION))
        .connect_timeout(config.timeout_connect)
        .timeout(timeout)
        .gzip(!config.disable_compression)
        .deflate(!config.disable_compression)
        .use_rustls_tls()
        .build()
        .unwrap();

    let (tx, rx) = channel();

    let etx = tx.clone();
    let mode = config.mode.clone();

    std::thread::spawn(move || {
        let config = config.clone();

        match mode {
            config::Mode::Discover => {
                etx.send(Action::ProcessURL(UrlItem {
                    parent: config.url.clone().unwrap(),
                    url: config.url.clone().unwrap(),
                }))
                .unwrap();
            }

            config::Mode::Single => {
                let r = Action::ProcessURL(UrlItem {
                    parent: config.url.clone().unwrap(),
                    url: config.url.clone().unwrap(),
                });
                loop {
                    let x = etx.send(r.clone());
                    if x.is_err() {
                        break;
                    }
                }
            }

            config::Mode::File => {
                let mut i = 0;
                loop {
                    match config.urls.as_ref().unwrap().get(i) {
                        None => i = 0,
                        Some(url) => {
                            let x = etx.send(Action::ProcessURL(UrlItem {
                                parent: url.clone(),
                                url: url.clone(),
                            }));
                            if x.is_err() {
                                break;
                            }

                            i += 1;
                        }
                    }
                }
            }
        }
    });

    let mtx = tx.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(100));
        let _x = mtx.send(Action::Ping);
    });

    let semaphore = Arc::new(Semaphore::new(concurrent as usize));

    let results: HttpResultsHolder = Arc::new(Mutex::new(Vec::new()));

    let mut total_processed = 0;
    let mut should_process_work = true;
    let mut requested_stop_at: Option<Instant> = None;
    let mut processed: HashSet<Url> = HashSet::new();
    let started = Instant::now();
    let mut last_send_progress = Instant::now();

    loop {
        if requests.is_some() && requests.unwrap() <= total_processed {
            should_process_work = false;
        }

        if !should_process_work {
            if (total_processed != 0 && semaphore.available_permits() == concurrent as usize)
                || (requested_stop_at.is_some() && requested_stop_at.unwrap().elapsed() > timeout)
            {
                break;
            } else {
                continue;
            }
        } else {
            if requested_stop.load(std::sync::atomic::Ordering::Relaxed) {
                should_process_work = false;
                requested_stop_at = Some(Instant::now());
            } else {
                if let Some(val) = duration {
                    if started.elapsed() >= val {
                        should_process_work = false;
                    }
                }
            }
        }

        let permit = semaphore.clone().acquire_owned().await;
        if permit.is_err() {
            continue;
        }

        let permit = permit.unwrap();

        let msg = rx.recv();
        if msg.is_err() {
            break;
        }

        if last_send_progress.elapsed().as_secs() == 1 {
            println!(
                "{}",
                format!("Processed {} requests", total_processed).magenta()
            );

            last_send_progress = Instant::now();
        }

        match msg.unwrap() {
            Action::ProcessURL(item) => {
                if prevent_duplicate_requests && processed.contains(&item.url) {
                    continue;
                }

                total_processed += 1;

                let tx = tx.clone();
                let http_client = http_client.clone();

                if prevent_duplicate_requests {
                    processed.insert(item.url.clone());
                }

                let headers = headers.clone();
                let basic_auth = basic_auth.clone();

                tokio::task::spawn(execute(
                    item,
                    tx.clone(),
                    http_client.clone(),
                    results.clone(),
                    permit,
                    verbose,
                    headers,
                    mode,
                    method.clone(),
                    allowed_domains.clone(),
                    basic_auth,
                ));
            }

            _ => {}
        }
    }

    results
}

pub async fn execute(
    item: UrlItem,
    tx: Sender<Action>,
    http_client: reqwest::Client,
    results: HttpResultsHolder,
    _permit: tokio::sync::OwnedSemaphorePermit,
    verbose: bool,
    headers: reqwest::header::HeaderMap,
    mode: config::Mode,
    method: reqwest::Method,
    allowed_domains: config::AllowedDomains,
    basic_auth: Option<config::BasicAuth>,
) {
    let url = item.url.clone();
    let start_time = Instant::now();

    let mut resp = http_client.request(method, url.clone());
    if let Some(basic_auth) = basic_auth {
        resp = resp.basic_auth(basic_auth.username, basic_auth.password);
    }
    let resp = resp.headers(headers).send().await;

    let duration = start_time.elapsed();

    if resp.is_err() {
        let err = resp.err().unwrap();
        error!("{url}: {}", err.to_string().red());
        results.lock().unwrap().push(HttpResult::Error(err));
        return;
    }

    let resp = resp.unwrap();
    let content_type = resp.headers().get("Content-Type");
    let content_type = match content_type {
        Some(ct) => ct.to_str().unwrap().to_string(),
        None => "".to_string(),
    };
    let status = resp.status();

    let bytes = resp.bytes().await;

    if bytes.is_err() {
        let err = bytes.err().unwrap();
        error!("{url}: {}", err.to_string().red());
        results.lock().unwrap().push(HttpResult::Error(err));
        return;
    }

    let bytes = bytes.unwrap();
    let length = bytes.len();

    if verbose {
        println!(
            "{}: {} in {:.5}s",
            url.to_string().blue(),
            status,
            duration.as_secs_f32().to_string().yellow()
        );
    }

    results
        .lock()
        .unwrap()
        .push(HttpResult::Response(HttpResponse {
            status,
            duration,
            length: length as usize,
        }));

    if mode == config::Mode::Single {
        return;
    }

    if content_type == "" || !content_type.starts_with("text/html") {
        return;
    }

    if status != 200 {
        error!("{url}: {}", status);
        return;
    }

    let text = String::from_utf8(bytes.to_vec());

    if text.is_err() {
        error!("{url}: {}", text.err().unwrap().to_string().red());
        return;
    }

    let document = Document::from(text.unwrap().as_str());

    let urls: Vec<_> = document
        .find(Name("a"))
        .filter_map(|n| match n.attr("href") {
            None => None,
            Some(href) => get_valid_url(href, &item, &allowed_domains),
        })
        .collect();

    for url in urls {
        let parent = match url.host() == item.parent.host() {
            true => item.parent.clone(),
            false => url.clone(),
        };

        let _r = tx.send(Action::ProcessURL(UrlItem { parent, url }));
    }
}

fn get_valid_url(
    input: impl ToString,
    item: &UrlItem,
    allowed_domains: &config::AllowedDomains,
) -> Option<Url> {
    let mut input = input.to_string();

    if input.starts_with("//") {
        input = format!("{}:{input}", item.parent.scheme());
    }

    if !input.starts_with("http://") && !input.starts_with("https://") {
        if input.starts_with("/") {
            input = format!(
                "{}://{}{}",
                item.parent.scheme(),
                item.parent.host_str().unwrap(),
                input
            );
        } else {
            input = format!(
                "{}://{}{}/{}",
                item.parent.scheme(),
                item.parent.host_str().unwrap(),
                item.parent.path(),
                input
            );
        }
    }

    match Url::parse(&input) {
        Ok(url) => match is_allowed_host(&url, &allowed_domains) {
            true => Some(url),
            false => None,
        },
        Err(e) => {
            error!("{} -> {}", input.red(), e.to_string().magenta());
            None
        }
    }
}

fn is_allowed_host(url: &Url, allowed_domains: &config::AllowedDomains) -> bool {
    match allowed_domains {
        config::AllowedDomains::All => true,
        config::AllowedDomains::Custom(domains) => {
            for domain in domains {
                if match domain {
                    config::DomainMatch::Exact(d) => url.host_str() == Some(d),
                    config::DomainMatch::Regex(r) => r.is_match(url.host_str().unwrap()),
                } {
                    return true;
                }
            }

            false
        }
    }
}
