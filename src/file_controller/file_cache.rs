// Copyright 2016 The Rustw Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::path::Path;

use super::Highlighter;
use span;
use vfs::Vfs;

use super::highlight;

pub struct Cache {
    pub files: Vfs<VfsUserData>,
}

pub struct VfsUserData {
    highlighted_lines: Vec<String>,
}

impl VfsUserData {
    fn new() -> VfsUserData {
        VfsUserData {
            highlighted_lines: vec![],
        }
    }
}

macro_rules! vfs_err {
    ($e: expr) => {
        {
            let r: Result<_, String> = $e.map_err(|e| e.into());
            r
        }
    }
}

impl Cache {
    pub fn new() -> Cache {
        Cache {
            files: Vfs::new(),
        }
    }

    pub fn get_lines(
        &self,
        path: &Path,
        line_start: span::Row<span::ZeroIndexed>,
        line_end: span::Row<span::ZeroIndexed>,
    ) -> Result<String, String> {
        vfs_err!(self.files.load_file(path))?;
        vfs_err!(self.files.load_lines(path, line_start, line_end))
    }

    pub fn get_highlighted(
        &self,
        path: &Path,
        highlighter: &Highlighter,
    ) -> Result<Vec<String>, String> {
        vfs_err!(self.files.load_file(path))?;
        vfs_err!(
            self.files
                .ensure_user_data(path, |_| Ok(VfsUserData::new()))
        )?;
        vfs_err!(self.files.with_user_data(path, |u| {
            let (text, u) = u?;
            let text = match text {
                Some(t) => t,
                None => return Err(::vfs::Error::BadFileKind),
            };
            if u.highlighted_lines.is_empty() {
                if let Some(ext) = path.extension() {
                    if ext == "rs" {
                        let highlighted = highlight::highlight(
                            &highlighter.analysis,
                            &highlighter.project_dir,
                            path.to_str().unwrap().to_owned(),
                            text.to_owned(),
                        );

                        let mut highlighted_lines = vec![];
                        for line in highlighted.lines() {
                            highlighted_lines.push(line.replace("<br>", "\n"));
                        }
                        if text.ends_with('\n') {
                            highlighted_lines.push(String::new());
                        }
                        u.highlighted_lines = highlighted_lines;
                    }
                }

                // Don't try to highlight non-Rust files (and cope with highlighting failure).
                if u.highlighted_lines.is_empty() {
                    let mut highlighted_lines: Vec<String> = text.lines().map(|s| s.to_owned()).collect();
                    if text.ends_with('\n') {
                        highlighted_lines.push(String::new());
                    }
                    u.highlighted_lines = highlighted_lines;
                }
            }

            Ok(u.highlighted_lines.clone())
        }))
    }
}
