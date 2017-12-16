// Copyright 2016 The Rustw Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use analysis;
use build;
use build::errors::{self, Diagnostic};
use config::Config;
use file_cache::Cache;
use listings::DirectoryListing;
use Mode;
use reprocess;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread::{self, sleep, Thread};
use std::time;

use hyper::header::ContentType;
use hyper::net::Fresh;
use hyper::server::request::Request;
use hyper::server::response::Response;
use hyper::status::StatusCode;
use hyper::uri::RequestUri;
use serde_json;
use span;
use url::parse_path;

/// An instance of the server. Runs a session of rustw.
pub struct Instance {
    builder: build::Builder,
    pub config: Arc<Config>,
    file_cache: Arc<Mutex<Cache>>,
    pending_push_data: Arc<Mutex<HashMap<String, Option<String>>>>,
    build_update_handler: Arc<Mutex<Option<BuildUpdateHandler>>>,
}

impl Instance {
    pub(super) fn new(config: Config, mode: Mode) -> Instance {
        let config = Arc::new(config);
        let build_update_handler = Arc::new(Mutex::new(None));

        let mut file_cache = Cache::new();
        if mode == Mode::Src {
            println!("Processing analysis data...");
            file_cache.update_analysis();
        }
        Instance {
            builder: build::Builder::from_config(config.clone(), build_update_handler.clone()),
            config: config,
            file_cache: Arc::new(Mutex::new(file_cache)),
            // FIXME(#58) a rebuild should cancel all pending tasks.
            pending_push_data: Arc::new(Mutex::new(HashMap::new())),
            build_update_handler: build_update_handler,
        }
    }
}

impl ::hyper::server::Handler for Instance {
    fn handle<'a, 'k>(&'a self, req: Request<'a, 'k>, res: Response<'a, Fresh>) {
        let uri = req.uri.clone();
        if let RequestUri::AbsolutePath(ref s) = uri {
            let mut handler = Handler {
                config: &self.config,
                builder: &self.builder,
                file_cache: &self.file_cache,
                pending_push_data: &self.pending_push_data,
                build_update_handler: &self.build_update_handler,
            };
            route(s, &mut handler, req, res);
        } else {
            // TODO log this and ignore it.
            panic!("Unexpected uri");
        }
    }
}

pub struct BuildUpdateHandler {
    thread: Thread,
    updates: Vec<String>,
    seen: usize,
    diagnostics: Vec<Diagnostic>,
    complete: bool,
}

impl BuildUpdateHandler {
    fn new(thread: Thread) -> BuildUpdateHandler {
        BuildUpdateHandler {
            thread: thread,
            updates: vec![],
            seen: 0,
            diagnostics: vec![],
            complete: false,
        }
    }

    pub fn push_updates(&mut self, updates: &[&str], done: bool) {
        for u in updates {
            self.updates.push((*u).to_owned());
        }
        if done {
            self.complete = true;
        }
        self.thread.unpark();
    }
}

// Handles a single request.
struct Handler<'a> {
    pub config: &'a Arc<Config>,
    builder: &'a build::Builder,
    file_cache: &'a Arc<Mutex<Cache>>,
    pending_push_data: &'a Arc<Mutex<HashMap<String, Option<String>>>>,
    build_update_handler: &'a Arc<Mutex<Option<BuildUpdateHandler>>>,
}

impl<'a> Handler<'a> {
    fn handle_error<'b: 'a, 'k: 'a>(
        &self,
        _req: Request<'b, 'k>,
        mut res: Response<'b, Fresh>,
        status: StatusCode,
        msg: String,
    ) {
        debug!("ERROR: {} ({})", msg, status);

        *res.status_mut() = status;
        res.send(msg.as_bytes()).unwrap();
    }

    fn handle_index<'b: 'a, 'k: 'a>(
        &mut self,
        _req: Request<'b, 'k>,
        mut res: Response<'b, Fresh>,
    ) {
        let mut path_buf = static_path();
        path_buf.push("index.html");

        let file_cache = self.file_cache.lock().unwrap();
        let msg = match file_cache.get_text(&path_buf) {
            Ok(data) => {
                res.headers_mut().set(ContentType::html());
                res.send(data.as_bytes()).unwrap();
                return;
            }
            Err(s) => s,
        };

        self.handle_error(_req, res, StatusCode::InternalServerError, msg);
    }

    fn handle_static<'b: 'a, 'k: 'a>(
        &mut self,
        req: Request<'b, 'k>,
        mut res: Response<'b, Fresh>,
        path: &[String],
    ) {
        let mut path_buf = static_path();
        for p in path {
            path_buf.push(p);
        }
        trace!("handle_static: requesting `{}`", path_buf.to_str().unwrap());

        let content_type = match path_buf.extension() {
            Some(s) if s.to_str().unwrap() == "html" => ContentType::html(),
            Some(s) if s.to_str().unwrap() == "css" => ContentType("text/css".parse().unwrap()),
            Some(s) if s.to_str().unwrap() == "json" => ContentType::json(),
            _ => ContentType("application/octet-stream".parse().unwrap()),
        };

        let file_cache = self.file_cache.lock().unwrap();
        let file_contents = file_cache.get_bytes(&path_buf);
        if let Ok(bytes) = file_contents {
            trace!(
                "handle_static: serving `{}`. {} bytes, {}",
                path_buf.to_str().unwrap(),
                bytes.len(),
                content_type
            );
            res.headers_mut().set(content_type);
            res.send(&bytes).unwrap();
            return;
        }

        trace!("404 {:?}", file_contents);
        self.handle_error(req, res, StatusCode::NotFound, "Page not found by the rust server".to_owned());
    }

    fn handle_src<'b: 'a, 'k: 'a>(
        &mut self,
        _req: Request<'b, 'k>,
        mut res: Response<'b, Fresh>,
        mut path: &[String],
    ) {
        for p in path {
            // In demo mode this might reveal the contents of the server outside
            // the source directory (really, rustw should run in a sandbox, but
            // hey, FIXME).
            if p.contains("..") || p == "/" {
                self.handle_error(
                    _req,
                    res,
                    StatusCode::InternalServerError,
                    "Bad path, found `..`".to_owned(),
                );
                return;
            }
        }

        let mut path_buf = PathBuf::new();
        if path[0].is_empty() { // TODO: fix panicking:
                                // thread '<unnamed>' panicked at 'index out of bounds: 
                                // the len is 0 but the index is 0', src/server.rs:219:12
            path_buf.push("/");
            path = &path[1..];
        }
        for p in path {
            path_buf.push(p);
        }

        // TODO should cache directory listings too
        if path_buf.is_dir() {
            match DirectoryListing::from_path(&path_buf) {
                Ok(listing) => {
                    res.headers_mut().set(ContentType::json());
                    res.send(
                        serde_json::to_string(&SourceResult::Directory(listing))
                            .unwrap()
                            .as_bytes(),
                    ).unwrap();
                }
                Err(msg) => self.handle_error(_req, res, StatusCode::InternalServerError, msg),
            }
        } else {
            let file_cache = self.file_cache.lock().unwrap();
            match file_cache.get_highlighted(&path_buf) {
                Ok(ref lines) => {
                    res.headers_mut().set(ContentType::json());
                    let result = SourceResult::Source {
                        path: path_buf
                            .components()
                            .map(|c| c.as_os_str().to_str().unwrap().to_owned())
                            .collect(),
                        lines: lines,
                    };
                    res.send(serde_json::to_string(&result).unwrap().as_bytes())
                        .unwrap();
                }
                Err(msg) => self.handle_error(_req, res, StatusCode::InternalServerError, msg),
            }
        }
    }

    fn handle_config<'b: 'a, 'k: 'a>(
        &mut self,
        _req: Request<'b, 'k>,
        mut res: Response<'b, Fresh>,
    ) {
        let text = serde_json::to_string(&**self.config).unwrap();

        res.headers_mut().set(ContentType::json());
        res.send(text.as_bytes()).unwrap();
    }

    fn handle_test<'b: 'a, 'k: 'a>(&mut self, _req: Request<'b, 'k>, mut res: Response<'b, Fresh>) {
        let build_result = build::BuildResult::test_result();
        let result = self.make_build_result(&build_result);
        let text = serde_json::to_string(&result).unwrap();

        res.headers_mut().set(ContentType::json());
        res.send(text.as_bytes()).unwrap();

        self.process_push_data(result);
    }

    fn handle_build<'b: 'a, 'k: 'a>(
        &mut self,
        _req: Request<'b, 'k>,
        mut res: Response<'b, Fresh>,
    ) {
        assert!(
            !self.config.demo_mode,
            "Build shouldn't happen in demo mode"
        );

        {
            let mut file_cache = self.file_cache.lock().unwrap();
            file_cache.reset();
        }

        let build_result = self.builder.build().unwrap();
        assert!(build_result.stdout.is_empty());
        let result = self.make_build_result(&build_result);
        let text = serde_json::to_string(&result).unwrap();

        res.headers_mut().set(ContentType::json());
        res.send(text.as_bytes()).unwrap();

        self.process_push_data(result);

        let mut build_update_handler = self.build_update_handler.lock().unwrap();
        *build_update_handler = None;
    }

    fn handle_build_updates<'b: 'a, 'k: 'a>(
        &mut self,
        _req: Request<'b, 'k>,
        mut res: Response<'b, Fresh>,
    ) {
        assert!(
            !self.config.demo_mode,
            "Build shouldn't happen in demo mode"
        );
        res.headers_mut()
            .set(ContentType("text/event-stream".parse().unwrap()));

        {
            let mut build_update_handler = self.build_update_handler.lock().unwrap();
            if build_update_handler.is_some() {
                debug!("build_update_handler already present, returning");
                res.send(b"event: close\ndata: {}\n\n").unwrap();
                return;
            }

            *build_update_handler = Some(BuildUpdateHandler::new(thread::current()));
        }
        let mut res = res.start().unwrap();

        let mut lowering_ctxt = errors::LoweringContext::new();
        loop {
            thread::park();

            let mut build_update_handler = self.build_update_handler.lock().unwrap();
            let build_update_handler = build_update_handler
                .as_mut()
                .expect("No build_update_handler");
            let msgs = &build_update_handler.updates[build_update_handler.seen..];
            build_update_handler.seen = build_update_handler.updates.len();
            for msg in msgs {
                let parsed = errors::parse_error(&msg, &mut lowering_ctxt);
                match parsed {
                    errors::ParsedError::Diagnostic(d) => {
                        let text = serde_json::to_string(&d).unwrap();
                        res.write_all(format!("event: error\ndata: {}\n\n", text).as_bytes())
                            .unwrap();
                        res.flush().unwrap();
                        build_update_handler.diagnostics.push(d);
                    }
                    errors::ParsedError::Message(s) => {
                        let text = serde_json::to_string(&s).unwrap();
                        res.write_all(format!("event: message\ndata: {}\n\n", text).as_bytes())
                            .unwrap();
                        res.flush().unwrap();
                    }
                    errors::ParsedError::Error => {}
                }
            }
            if build_update_handler.complete {
                res.write_all(b"event: close\ndata: {}\n\n").unwrap();
                res.end().unwrap();
                return;
            }
        }
    }

    fn make_build_result(&mut self, build_result: &build::BuildResult) -> BuildResult {
        let mut result = BuildResult::from_build(&build_result);
        let key = reprocess::make_key();
        result.push_data_key = Some(key.clone());
        let mut pending_push_data = self.pending_push_data.lock().unwrap();
        pending_push_data.insert(key, None);

        result
    }

    fn process_push_data(&self, mut result: BuildResult) {
        if let Some(key) = result.push_data_key {
            let mut errors: Vec<Diagnostic> = vec![];

            let mut build_update_handler = self.build_update_handler.lock().unwrap();
            if let Some(ref mut build_update_handler) = *build_update_handler {
                errors = build_update_handler.diagnostics.drain(..).collect();
            }

            errors.extend(result.errors.drain(..));

            let pending_push_data = self.pending_push_data.clone();
            let file_cache = self.file_cache.clone();
            let config = self.config.clone();
            let use_analysis = self.config.save_analysis;
            thread::spawn(move || {
                reprocess::reprocess_snippets(
                    key,
                    errors,
                    pending_push_data,
                    use_analysis,
                    file_cache,
                    config,
                )
            });
        }
    }

    fn handle_edit<'b: 'a, 'k: 'a>(
        &mut self,
        _req: Request<'b, 'k>,
        mut res: Response<'b, Fresh>,
        query: Option<String>,
    ) {
        assert!(!self.config.demo_mode, "Edit shouldn't happen in demo mode");
        assert!(self.config.unstable_features, "Edit is unstable");

        match parse_query_value(&query, "file=") {
            Some(location) => {
                // Split the 'filename' on colons for line and column numbers.
                let args = parse_location_string(&location);

                let cmd_line = &self.config.edit_command;
                if !cmd_line.is_empty() {
                    let cmd_line = cmd_line
                        .replace("$file", &args[0])
                        .replace("$line", &args[1])
                        .replace("$col", &args[2]);

                    let mut splits = cmd_line.split(' ');

                    let mut cmd = Command::new(splits.next().unwrap());
                    for arg in splits {
                        cmd.arg(arg);
                    }

                    match cmd.spawn() {
                        Ok(_) => debug!("edit, launched successfully"),
                        Err(e) => debug!("edit, launch failed: `{:?}`, command: `{}`", e, cmd_line),
                    }
                }

                res.headers_mut().set(ContentType::json());
                res.send("{}".as_bytes()).unwrap();
            }
            None => {
                self.handle_error(
                    _req,
                    res,
                    StatusCode::InternalServerError,
                    format!("Bad query string: {:?}", query),
                );
            }
        }
    }

    fn handle_search<'b: 'a, 'k: 'a>(
        &mut self,
        _req: Request<'b, 'k>,
        mut res: Response<'b, Fresh>,
        query: Option<String>,
    ) {
        match (
            parse_query_value(&query, "needle="),
            parse_query_value(&query, "id="),
        ) {
            (Some(needle), None) => {
                // Identifier search.
                let mut file_cache = self.file_cache.lock().unwrap();
                match file_cache.ident_search(&needle) {
                    Ok(data) => {
                        res.headers_mut().set(ContentType::json());
                        res.send(serde_json::to_string(&data).unwrap().as_bytes())
                            .unwrap();
                    }
                    Err(s) => {
                        self.handle_error(_req, res, StatusCode::InternalServerError, s);
                    }
                }
            }
            (None, Some(id)) => {
                // Search by id.
                let id = match u64::from_str(&id) {
                    Ok(l) => l,
                    Err(_) => {
                        self.handle_error(
                            _req,
                            res,
                            StatusCode::InternalServerError,
                            format!("Bad id: {}", id),
                        );
                        return;
                    }
                };
                let mut file_cache = self.file_cache.lock().unwrap();
                match file_cache.id_search(analysis::Id::new(id)) {
                    Ok(data) => {
                        res.headers_mut().set(ContentType::json());
                        res.send(serde_json::to_string(&data).unwrap().as_bytes())
                            .unwrap();
                    }
                    Err(s) => {
                        self.handle_error(_req, res, StatusCode::InternalServerError, s);
                    }
                }
            }
            _ => {
                self.handle_error(
                    _req,
                    res,
                    StatusCode::InternalServerError,
                    "Bad search string".to_owned(),
                );
            }
        }
    }

    fn handle_find<'b: 'a, 'k: 'a>(
        &mut self,
        _req: Request<'b, 'k>,
        mut res: Response<'b, Fresh>,
        query: Option<String>,
    ) {
        match parse_query_value(&query, "impls=") {
            Some(id) => {
                let id = match u64::from_str(&id) {
                    Ok(l) => l,
                    Err(_) => {
                        self.handle_error(
                            _req,
                            res,
                            StatusCode::InternalServerError,
                            format!("Bad id: {}", id),
                        );
                        return;
                    }
                };
                let mut file_cache = self.file_cache.lock().unwrap();
                match file_cache.find_impls(analysis::Id::new(id)) {
                    Ok(data) => {
                        res.headers_mut().set(ContentType::json());
                        res.send(serde_json::to_string(&data).unwrap().as_bytes())
                            .unwrap();
                    }
                    Err(s) => {
                        self.handle_error(_req, res, StatusCode::InternalServerError, s);
                    }
                }
            }
            _ => {
                self.handle_error(
                    _req,
                    res,
                    StatusCode::InternalServerError,
                    "Unknown argument to find".to_owned(),
                );
            }
        }
    }

    fn handle_plain_text<'b: 'a, 'k: 'a>(
        &mut self,
        _req: Request<'b, 'k>,
        mut res: Response<'b, Fresh>,
        query: Option<String>,
    ) {
        match (
            parse_query_value(&query, "file="),
            parse_query_value(&query, "line="),
        ) {
            (Some(file_name), Some(line)) => {
                let line = match usize::from_str(&line) {
                    Ok(l) => l,
                    Err(_) => {
                        self.handle_error(
                            _req,
                            res,
                            StatusCode::InternalServerError,
                            format!("Bad line number: {}", line),
                        );
                        return;
                    }
                };
                let file_cache = self.file_cache.lock().unwrap();

                // Hard-coded 2 lines of context before and after target line.
                let line_start = line.saturating_sub(3);
                let line_end = line + 2;

                match file_cache.get_lines(
                    &Path::new(&file_name),
                    span::Row::new_zero_indexed(line_start as u32),
                    span::Row::new_zero_indexed(line_end as u32),
                ) {
                    Ok(ref lines) => {
                        res.headers_mut().set(ContentType::json());
                        let result = TextResult {
                            text: lines,
                            file_name: file_name,
                            line_start: line_start + 1,
                            line_end: line_end,
                        };
                        res.send(serde_json::to_string(&result).unwrap().as_bytes())
                            .unwrap();
                    }
                    Err(msg) => {
                        self.handle_error(_req, res, StatusCode::InternalServerError, msg);
                    }
                }
            }
            _ => {
                self.handle_error(
                    _req,
                    res,
                    StatusCode::InternalServerError,
                    "Bad query string".to_owned(),
                );
            }
        }
    }

    fn handle_pull<'b: 'a, 'k: 'a>(
        &mut self,
        _req: Request<'b, 'k>,
        mut res: Response<'b, Fresh>,
        query: Option<String>,
    ) {
        match parse_query_value(&query, "key=") {
            Some(key) => {
                res.headers_mut().set(ContentType::json());

                loop {
                    {
                        let pending_push_data = self.pending_push_data.lock().unwrap();
                        match pending_push_data.get(&key) {
                            Some(&Some(ref s)) => {
                                // Data is ready, return it.
                                res.send(s.as_bytes()).unwrap();
                                return;
                            }
                            Some(&None) => {
                                // Task is in progress, wait.
                            }
                            None => {
                                // No push task, return nothing.
                                res.send("{}".as_bytes()).unwrap();
                                return;
                            }
                        }
                    }
                    sleep(time::Duration::from_millis(200));
                }
            }
            None => {
                self.handle_error(
                    _req,
                    res,
                    StatusCode::InternalServerError,
                    "Bad query string".to_owned(),
                );
            }
        }
    }
}

#[derive(Serialize, Debug)]
pub enum SourceResult<'a> {
    Source {
        path: Vec<String>,
        lines: &'a [String],
    },
    Directory(DirectoryListing),
}

#[derive(Serialize, Debug)]
pub struct TextResult<'a> {
    text: &'a str,
    file_name: String,
    line_start: usize,
    line_end: usize,
}

#[derive(Serialize, Debug)]
pub struct BuildResult {
    pub messages: Vec<String>,
    pub errors: Vec<Diagnostic>,
    pub push_data_key: Option<String>,
    // build_command: String,
}

impl BuildResult {
    fn from_build(build: &build::BuildResult) -> BuildResult {
        let (errors, messages) = errors::parse_errors(&build.stderr, &build.stdout);
        BuildResult {
            messages: messages,
            errors: errors,
            push_data_key: None,
        }
    }
}

fn static_path() -> PathBuf {
    const STATIC_DIR: &'static str = "static";

    let mut result = ::std::env::current_exe().unwrap();
    assert!(result.pop());
    result.push(STATIC_DIR);
    result
}

pub fn parse_location_string(input: &str) -> [String; 5] {
    let mut args = input.split(':').map(|s| s.to_owned());
    [
        args.next().unwrap(),
        args.next().unwrap_or(String::new()),
        args.next().unwrap_or(String::new()),
        args.next().unwrap_or(String::new()),
        args.next().unwrap_or(String::new()),
    ]
}

// key should include `=` suffix.
fn parse_query_value(query: &Option<String>, key: &str) -> Option<String> {
    match *query {
        Some(ref q) => {
            let start = match q.find(key) {
                Some(i) => i + key.len(),
                None => {
                    return None;
                }
            };
            let end = q[start..].find("&").map(|e| e + start).unwrap_or(q.len());
            let value = &q[start..end];
            Some(value.to_owned())
        }
        None => None,
    }
}

const STATIC_REQUEST: &'static str = "static";
const DATA_REQUEST: &'static str = "data";
const SOURCE_REQUEST: &'static str = "src";
const PLAIN_TEXT: &'static str = "plain_text";
const CONFIG_REQUEST: &'static str = "config";
const BUILD_REQUEST: &'static str = "build";
const EDIT_REQUEST: &'static str = "edit";
const PULL_REQUEST: &'static str = "pull";
const SEARCH_REQUEST: &'static str = "search";
const FIND_REQUEST: &'static str = "find";
const BUILD_UPDATE_REQUEST: &'static str = "build_updates";

fn route<'a, 'b: 'a, 'k: 'a>(
    uri_path: &str,
    handler: &'a mut Handler<'a>,
    req: Request<'b, 'k>,
    res: Response<'b, Fresh>,
) {
    let (path, query, _) = parse_path(uri_path).unwrap();

    trace!("route: path: {:?}, query: {:?}", path, query);
    if path.is_empty() || (path.len() == 1 && (path[0] == "index.html" || path[0] == "")) {
        handler.handle_index(req, res);
        return;
    }

    if path[0] == DATA_REQUEST {
        if path[1] == CONFIG_REQUEST {
            handler.handle_config(req, res);
            return;
        }

        if path[1] == PULL_REQUEST {
            handler.handle_pull(req, res, query);
            return;
        }

        if path[1] == SOURCE_REQUEST {
            let path = &path[2..];
            // Because a URL ending in "/." is normalised to "/", we miss out on "." as a source path.
            // We try to correct for that here.
            if path.len() == 1 && path[0] == "" {
                handler.handle_src(req, res, &[".".to_owned()]);
            } else {
                handler.handle_src(req, res, path);
            }
            return;
        }

        if path[1] == FIND_REQUEST {
            handler.handle_find(req, res, query);
            return;
        }

        if path[1] == BUILD_REQUEST {
            if handler.config.demo_mode {
                handler.handle_test(req, res);
            } else {
                handler.handle_build(req, res);
            }
            return;
        }

        if !handler.config.demo_mode {
            if path[0] == BUILD_UPDATE_REQUEST {
                handler.handle_build_updates(req, res);
                return;
            }

            if path[0] == EDIT_REQUEST {
                handler.handle_edit(req, res, query);
                return;
            }
        }

        if path[1] == SEARCH_REQUEST {
            handler.handle_search(req, res, query);
            return;
        }
    }

    if path[0] == PLAIN_TEXT {
        handler.handle_plain_text(req, res, query);
        return;
    }

    if path[0] == STATIC_REQUEST {
        handler.handle_static(req, res, &path[1..]);
        return;
    }

    // Hand off all other requests to React Router on frontend
    handler.handle_index(req, res);
    return;
}
