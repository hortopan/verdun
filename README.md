# Verdun
Verdun is a simple and fast HTTP stress-test/benchmark tool written in Rust. 🦀

It can test a single URL, load multiple URLs from a file or automatically discover urls on a page and tests them.

It also supports using random arguments in the URL and HEADER values by using *%RAND(min,max)%* with *--random-arguments* flag.

![Verdon](https://github.com/hortopan/verdun/raw/main/resources/preview.gif "Verdun")

## Install

### MacOS

```bash
brew tap hortopan/verdun
brew install verdun
```

### LINUX

Linux static built binaries(aarch64, amd64) are available in the [releases](https://github.com/hortopan/verdun/releases) section.

## CLI arguments
* **-a, --domains <ALLOWED_DOMAINS>**
  Additional domains to navigate when running in <discover> mode
* **-b, --basic-auth <BASIC_AUTH>**
Basic auth username and password. Use ':' to separate username and password.
* **-c, --concurrent <CONCURRENT>**
Number of concurrent requests to execute. [default: 2]
* **-C, --disable-compression**
Disable gzip/deflate compression for requests.
* **-d, --duration <DURATION>**
Run for for a fixed amount of time. ex: 10m for 10 minutes, 60s for 1 minute, 2h for 2 hours.
* **-f, --follow-redirects**
Follow redirects
* **-h, --header <HEADER>**
Set custom HTTP headers.
* **-m, --mode <MODE>**
Mode to run. discover will automatically discover all URLs in the given HTML page. single will only run the given URL. [default: discover] [possible values: discover, single, file]
* **-M, --method <METHOD>**
[default: get] [possible values: get, post, head, options, put, delete, connect, trace,
            patch]
* **-n, --requests <REQUESTS>**
Number of requests to perform. Defaults to 1000 if mode is not discover and duration is not set.
* **--no-delayed-start**
 Start without the inital delay used to show config before executing.
* **-p, --prevent-duplicate-requests**
Prevent duplicate requests when in --mode discover. Each request will be checked against the list of already processed URLs.
* **-r, --random-arguments**
  Enable %RAND(min,max)% to be replaced with a random number between min and max within the URL and/or Header in Single and File mode.
* **-t, --timeout <TIMEOUT>**
HTTP request timeout in miliseconds. [default: 3000]
* **-T, --timeout-connect <TIMEOUT_CONNECT>**
HTTP connection timeout in miliseconds. [default: 1000]
* **-v, --verbose**
Enable verbose output (show all requests otherwise only errors.
