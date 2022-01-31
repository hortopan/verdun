use super::*;
use clap::Parser;
use regex::Regex;

#[derive(Parser, Debug)]
#[clap(version = VERSION, about = ABOUT, author = AUTHORS)]
pub struct RawConfig {
    #[clap(help = "URL or FILE (when in file mode).")]
    pub url: String,

    #[clap(arg_enum, short, long, default_value_t = Mode::Discover, help = "Mode to run. discover will automatically discover all URLs in the given HTML page. single will only run the given URL.")]
    pub mode: Mode,

    #[clap(arg_enum, short = 'M', long, default_value_t = Method::GET)]
    pub method: Method,

    #[clap(
        short,
        long,
        default_value_t = 2,
        help = "Number of concurrent requests to execute."
    )]
    pub concurrent: u16,

    #[clap(
        short = 'T',
        long,
        default_value_t = 1000,
        help = "HTTP connection timeout in miliseconds."
    )]
    pub timeout_connect: u64,

    #[clap(
        short,
        long,
        default_value_t = 3000,
        help = "HTTP request timeout in miliseconds."
    )]
    pub timeout: u64,

    #[clap(
        short = 'C',
        long,
        help = "Disable gzip/deflate compression for requests."
    )]
    pub disable_compression: bool,

    #[clap(
        short,
        long,
        help = "Enable verbose output (show all requests otherwise only errors)."
    )]
    pub verbose: bool,
    #[clap(
        short = 'n',
        long,
        help = "Number of requests to perform. Defaults to 1000 if mode is not discover and duration is not set."
    )]
    pub requests: Option<u64>,

    #[clap(
        short = 'd',
        long,
        help = "Run for for a fixed amount of time.\nex: 10m for 10 minutes, 60s for 1 minute, 2h for 2 hours."
    )]
    pub duration: Option<String>,

    #[clap(short, long, help = "Follow redirects")]
    pub follow_redirects: bool,

    #[clap(short, long, help = "Custom HTTP headers")]
    pub header: Option<Vec<String>>,

    #[clap(
        short,
        long = "domains",
        help = "Additional allowed domains when using --mode discover. \nUse ',' to separate multiple domains. Supports wildcards: *.example.com ."
    )]
    pub allowed_domains: Option<Vec<String>>,

    #[clap(
        short,
        long,
        help = "Prevent duplicate requests when in --mode discover\nEach request will be checked against the list of already processed URLs."
    )]
    pub prevent_duplicate_requests: bool,

    #[clap(
        long,
        help = "Start without the inital delay used to show config before executing."
    )]
    pub no_delayed_start: bool,

    #[clap(
        short,
        long,
        help = "Basic auth username and password.\nUse ':' to separate username and password."
    )]
    pub basic_auth: Option<String>,

    #[clap(
        long,
        short,
        help = "Enable %RAND(min,max)% to be replaced with a random number between min and max within the URL and/or Header in Single and File mode."
    )]
    pub random_arguments: bool,
}

#[derive(clap::ArgEnum, Copy, Clone, Debug, PartialEq)]
pub enum Mode {
    Discover,
    Single,
    File,
}

#[derive(clap::ArgEnum, Copy, Clone, Debug, PartialEq)]
pub enum Method {
    GET,
    POST,
    HEAD,
    OPTIONS,
    PUT,
    DELETE,
    CONNECT,
    TRACE,
    PATCH,
}

#[derive(Debug, Clone)]
pub enum DomainMatch {
    Exact(String),
    Regex(Regex),
}

#[derive(Debug, Clone)]
pub enum AllowedDomains {
    All,
    Custom(Vec<DomainMatch>),
}

#[derive(Debug, Clone)]
pub struct BasicAuth {
    pub username: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub url: Option<Url>,
    pub concurrent: u16,
    pub timeout_connect: std::time::Duration,
    pub timeout: std::time::Duration,
    pub disable_compression: bool,
    pub verbose: bool,
    pub requests: Option<u64>,
    pub follow_redirects: bool,
    pub headers: reqwest::header::HeaderMap,
    pub mode: Mode,
    pub method: reqwest::Method,
    pub allowed_domains: AllowedDomains,
    pub prevent_duplicate_requests: bool,
    pub duration: Option<std::time::Duration>,
    pub no_delayed_start: bool,
    pub urls: Option<Vec<Url>>,
    pub basic_auth: Option<BasicAuth>,
    pub random_arguments: bool,
}

impl Config {
    pub fn new() -> Self {
        let raw_config = RawConfig::parse();

        if raw_config.timeout_connect < 50 {
            error!(
                "{}",
                "Timeout connect must be at least 50 miliseconds".red()
            );
            std::process::exit(1);
        }

        if raw_config.timeout < 50 {
            error!("{}", "Timeout must be at least 50 miliseconds".red());
            std::process::exit(1);
        }

        let mut headers = reqwest::header::HeaderMap::new();

        if let Some(header) = raw_config.header {
            for h in header {
                let mut parts = h.splitn(2, ':');
                let key = parts.next().unwrap();
                let value = parts.next().unwrap();
                headers.insert(
                    reqwest::header::HeaderName::from_bytes(key.as_bytes()).unwrap(),
                    reqwest::header::HeaderValue::from_str(value).unwrap(),
                );
            }
        }

        let requests = match raw_config.requests {
            Some(requests) => Some(requests),
            None => match raw_config.mode {
                Mode::Discover => None,
                _ => match raw_config.duration.as_ref() {
                    None => Some(1000),
                    Some(_val) => None,
                },
            },
        };

        if let Some(val) = raw_config.requests {
            if val < raw_config.concurrent as u64 {
                error!(
                    "{}",
                    "Number of requests must be greater than or equal to the number of concurrent requests".red()
                );
                std::process::exit(1);
            }
        }

        if raw_config.prevent_duplicate_requests && raw_config.mode != Mode::Discover {
            error!(
                "{}",
                "--prevent-duplicate-requests is only supported in --mode discover".red()
            );
            std::process::exit(1);
        }

        let url = match raw_config.mode {
            Mode::File => None,
            _ => Some(
                Url::parse(&raw_config.url).expect(&format!("Invalid URL: {}", raw_config.url)),
            ),
        };

        let urls = match raw_config.mode {
            Mode::File => {
                let fc = match std::fs::read_to_string(std::path::Path::new(&raw_config.url)) {
                    Ok(fc) => fc,
                    Err(e) => {
                        error!("{} : {}", e.to_string().red(), raw_config.url.magenta());
                        std::process::exit(1);
                    }
                };

                let urls = fc
                    .split("\n")
                    .filter_map(|s| match Url::parse(s) {
                        Ok(url) => Some(url),
                        Err(e) => {
                            error!("{}", e.to_string().red());
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                if urls.len() == 0 {
                    error!(
                        "{}, {}",
                        "No valid URLs found in file".red(),
                        raw_config.url.magenta()
                    );
                    std::process::exit(1);
                }

                Some(urls)
            }

            _ => None,
        };

        let allowed_domains =
            allowed_domains_from_config(raw_config.allowed_domains, raw_config.mode, &url, &urls);

        let basic_auth = match raw_config.basic_auth {
            Some(val) => {
                let val = val.split(':').collect::<Vec<_>>();

                if val.len() == 0 {
                    error!(
                        "{}",
                        "Invalid basic auth format. Should be username:password or username".red()
                    );
                    std::process::exit(1);
                }

                Some(BasicAuth {
                    username: val.get(0).unwrap().to_string(),
                    password: match val.get(1) {
                        Some(password) => Some(password.to_string()),
                        None => None,
                    },
                })
            }
            None => None,
        };

        Config {
            url,
            concurrent: raw_config.concurrent,
            timeout_connect: Duration::from_millis(raw_config.timeout_connect),
            timeout: Duration::from_millis(raw_config.timeout),
            disable_compression: raw_config.disable_compression,
            verbose: raw_config.verbose,
            requests,
            follow_redirects: raw_config.follow_redirects,
            headers,
            mode: raw_config.mode,
            method: match raw_config.method {
                Method::GET => reqwest::Method::GET,
                Method::POST => reqwest::Method::POST,
                Method::HEAD => reqwest::Method::HEAD,
                Method::OPTIONS => reqwest::Method::OPTIONS,
                Method::PUT => reqwest::Method::PUT,
                Method::DELETE => reqwest::Method::DELETE,
                Method::CONNECT => reqwest::Method::CONNECT,
                Method::TRACE => reqwest::Method::TRACE,
                Method::PATCH => reqwest::Method::PATCH,
            },
            allowed_domains,
            prevent_duplicate_requests: raw_config.prevent_duplicate_requests,
            no_delayed_start: raw_config.no_delayed_start,
            basic_auth,
            urls,
            random_arguments: raw_config.random_arguments,
            duration: match raw_config.duration {
                Some(time) => {
                    let r = Regex::new("^(\\d{1,})([s,m,h,d,M,y])$").unwrap();

                    match r.captures(&time) {
                        Some(caps) => {
                            if caps.len() != 3 {
                                error!("{}", "Invalid time format for duration".red());
                                std::process::exit(1);
                            }

                            let t = caps.get(1).unwrap().as_str().parse::<u64>();
                            if t.is_err() {
                                error!("{}", "Invalid time format for duration".red());
                                std::process::exit(1);
                            }

                            let t = t.unwrap();

                            match caps.get(2).unwrap().as_str() {
                                "s" => Some(Duration::from_secs(t)),
                                "m" => Some(Duration::from_secs(t * 60)),
                                "h" => Some(Duration::from_secs(t * 60 * 60)),
                                "d" => Some(Duration::from_secs(t * 60 * 60 * 24)),
                                "M" => Some(Duration::from_secs(t * 60 * 60 * 24 * 30)),
                                "y" => Some(Duration::from_secs(t * 60 * 60 * 24 * 365)),
                                _ => {
                                    error!("{}", "Invalid time format for duration".red());
                                    std::process::exit(1);
                                }
                            }
                        }
                        None => {
                            error!("{}", "Invalid time format for duration".red());
                            std::process::exit(1);
                        }
                    }
                }
                None => None,
            },
        }
    }
}

pub fn allowed_domains_from_config(
    allowed_domains: Option<Vec<String>>,
    mode: Mode,
    url: &Option<Url>,
    urls: &Option<Vec<Url>>,
) -> AllowedDomains {
    match allowed_domains {
        Some(domains) => {
            if domains.contains(&"*".to_string()) {
                AllowedDomains::All
            } else {
                let mut domains = domains
                    .into_iter()
                    .map(|d| {
                        if !d.contains("*") {
                            return DomainMatch::Exact(d);
                        }

                        let d = d.split('.').collect::<Vec<&str>>();
                        let d: Vec<_> = d
                            .iter()
                            .map(|s| match s {
                                &"*" => ".*?",
                                _ => s,
                            })
                            .collect();

                        let d = d.join("\\.");
                        DomainMatch::Regex(
                            Regex::new(&format!("^{d}$").replace("?\\.", "?\\.?")).unwrap(),
                        )
                    })
                    .collect::<Vec<_>>();

                if url.is_some() {
                    domains.push(DomainMatch::Exact(
                        url.as_ref().unwrap().domain().unwrap().to_string(),
                    ));
                }

                AllowedDomains::Custom(domains)
            }
        }
        None => match mode {
            Mode::File => {
                let mut domains = urls
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|url| url.host().unwrap().to_string())
                    .collect::<Vec<_>>();

                domains.dedup();

                let domains = domains
                    .into_iter()
                    .map(|d| DomainMatch::Exact(d))
                    .collect::<Vec<_>>();

                AllowedDomains::Custom(domains)
            }
            _ => AllowedDomains::Custom(vec![DomainMatch::Exact(
                url.as_ref().unwrap().host_str().unwrap().to_string(),
            )]),
        },
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn domains_from_config() {
        let domains = vec!["example.com".to_string(), "*.example.com".to_string()];
        let urls = Some(vec![url::Url::parse("https://test.com").unwrap()]);

        match super::allowed_domains_from_config(
            Some(domains.clone()),
            super::Mode::File,
            &None,
            &urls,
        ) {
            super::AllowedDomains::Custom(domains) => {
                assert_eq!(domains.len(), 2);

                match domains.get(0) {
                    Some(super::DomainMatch::Exact(d)) => assert_eq!(d, "example.com"),
                    _ => panic!("Expected exact domain"),
                }

                match domains.get(1) {
                    Some(super::DomainMatch::Regex(r)) => {
                        assert_eq!(r.as_str(), "^.*?\\.?example\\.com$")
                    }
                    _ => panic!("Expected regex domain"),
                }
            }
            _ => panic!("Invalid domain match"),
        }
    }
}
