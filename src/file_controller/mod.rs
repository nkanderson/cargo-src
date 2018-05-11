// Copyright 2016 The Rustw Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use cargo_metadata;
use std::collections::HashMap;
// use std::env;
use std::path::{Path, PathBuf};
use std::str;

use analysis::Id;
use span;

// use highlight::Highlighter;
use highlight;

// FIXME maximum size and evication policy
// FIXME keep timestamps and check on every read. Then don't empty on build.

// mod highlight;
mod file_cache;
mod results;
use file_controller::results::{
    SearchResult,
    DefResult,
    FileResult,
    LineResult,
    FindResult,
    SymbolResult
};

// pub struct Highlighter {
//     analysis: AnalysisHost,
//     project_dir: PathBuf
// }

// impl Highlighter {
//     pub fn new() -> Highlighter {
//         Highlighter {
//             analysis: AnalysisHost::new(Target::Debug),
//             project_dir: env::current_dir().unwrap()
//         }
//     }
// }

pub struct FileController {
    highlighter: highlight::Highlighter,
    // highlighter: &'a Highlighter,
    cache: file_cache::Cache
}

type Span = span::Span<span::ZeroIndexed>;

impl FileController {
    pub fn new() -> FileController {
        FileController {
            highlighter: highlight::Highlighter::new(),
            cache: file_cache::Cache::new()
        }
    }

    pub fn get_lines(
        &self,
        path: &Path,
        line_start: span::Row<span::ZeroIndexed>,
        line_end: span::Row<span::ZeroIndexed>,
    ) -> Result<String, String> {
        self.cache.get_lines(path, line_start, line_end)
    }

    pub fn get_highlighted(&self, path: &Path) -> Result<Vec<String>, String> {
        self.cache.get_highlighted(path, &self.highlighter)
    }

    pub fn get_highlighted_line(
        &self,
        file_name: &Path,
        line: span::Row<span::ZeroIndexed>,
    ) -> Result<String, String> {
        let lines = self.get_highlighted(file_name)?;
        Ok(lines[line.0 as usize].clone())
    }

    pub fn update_analysis(&self) {
        println!("Processing analysis...");
        self.highlighter.analysis
            .reload_with_blacklist(&self.highlighter.project_dir, &self.highlighter.project_dir, &::blacklist::CRATE_BLACKLIST)
            .unwrap();

        // FIXME Possibly extreme, could invalidate by crate or by file. Also, only
        // need to invalidate Rust files.
        self.cache.files.clear();

        println!("done");
    }

    // FIXME we should cache this information rather than compute every time.
    pub fn get_symbol_roots(&self) -> Result<Vec<SymbolResult>, String> {
        let all_crates = self
            .highlighter.analysis
            .def_roots()
            .unwrap_or_else(|_| vec![])
            .into_iter()
            .filter_map(|(id, name)| {
                let span = self.highlighter.analysis.get_def(id).ok()?.span;
                Some(SymbolResult {
                    id: id.to_string(),
                    name,
                    file_name: self.make_file_path(&span).display().to_string(),
                    line_start: span.range.row_start.one_indexed().0,
                })
            });

        // FIXME Unclear ot sure if we should include dep crates or not here.
        // Need to test on workspace crates. Might be nice to have deps in any
        // case, in which case we should return the primary crate(s) first.
        let metadata = match cargo_metadata::metadata_deps(None, false) {
            Ok(metadata) => metadata,
            Err(_) => return Err("Could not access cargo metadata".to_owned()),
        };

        let names: Vec<String> = metadata
            .packages
            .into_iter()
            .map(|p| p.name)
            .collect();

        Ok(all_crates.filter(|sr| names.contains(&sr.name)).collect())
    }

    // FIXME we should indicate whether the symbol has children or not
    pub fn get_symbol_children(&self, id: Id) -> Result<Vec<SymbolResult>, String> {
        self.highlighter.analysis
            .for_each_child_def(id, |id, def| {
                let span = &def.span;
                SymbolResult {
                    id: id.to_string(),
                    name: def.name.clone(),
                    file_name: self.make_file_path(&span).display().to_string(),
                    line_start: span.range.row_start.one_indexed().0,
                }
            })
            .map_err(|e| e.to_string())
    }

    pub fn id_search(&self, id: Id) -> Result<SearchResult, String> {
        self.ids_search(vec![id])
    }

    pub fn ident_search(&self, needle: &str) -> Result<SearchResult, String> {
        // First see if the needle corresponds to any definitions, if it does, get a list of the
        // ids, otherwise, return an empty search result.
        let ids = match self.highlighter.analysis.search_for_id(needle) {
            Ok(ids) => ids.to_owned(),
            Err(_) => {
                return Ok(SearchResult {
                    defs: vec![],
                });
            }
        };

        self.ids_search(ids)
    }

    pub fn find_impls(&self, id: Id) -> Result<FindResult, String> {
        let impls = self.highlighter.analysis
            .find_impls(id)
            .map_err(|_| "No impls found".to_owned())?;
        Ok(FindResult {
            results: self.make_search_results(impls)?,
        })
    }

    fn ids_search(&self, ids: Vec<Id>) -> Result<SearchResult, String> {
        let mut defs = Vec::new();

        for id in ids {
            // If all_refs.len() > 0, the first entry will be the def.
            let all_refs = self.highlighter.analysis.find_all_refs_by_id(id);
            let mut all_refs = match all_refs {
                Err(_) => return Err("Error finding references".to_owned()),
                Ok(ref all_refs) if all_refs.is_empty() => continue,
                Ok(all_refs) => all_refs.into_iter(),
            };

            let def_span = all_refs.next().unwrap();
            let def_path = self.make_file_path(&def_span);
            let line = self.make_line_result(&def_path, &def_span)?;

            defs.push(DefResult {
                file: def_path.display().to_string(),
                line,
                refs: self.make_search_results(all_refs.collect())?,
            });
        }

        // We then save each bucket of defs/refs as a vec, and put it together to return.
        return Ok(SearchResult {
            defs,
        });

    }

    fn make_file_path(&self, span: &Span) -> PathBuf {
        let file_path = Path::new(&span.file);
        file_path
            .strip_prefix(&self.highlighter.project_dir)
            .unwrap_or(file_path)
            .to_owned()
    }

    fn make_line_result(&self, file_path: &Path, span: &Span) -> Result<LineResult, String> {
        let text = match self.get_highlighted_line(file_path, span.range.row_start) {
            Ok(t) => t,
            Err(_) => return Err(format!("Error finding text for {:?}", span)),
        };
        Ok(LineResult::new(span, text))
    }

    // Sorts a set of search results into buckets by file.
    fn make_search_results(&self, raw: Vec<Span>) -> Result<Vec<FileResult>, String> {
        let mut file_buckets = HashMap::new();

        for span in &raw {
            let file_path = self.make_file_path(span);
            let line = match self.make_line_result(&file_path, span) {
                Ok(l) => l,
                Err(_) => continue,
            };
            file_buckets
                .entry(file_path.display().to_string())
                .or_insert_with(|| vec![])
                .push(line);
        }

        let mut result = vec![];
        for (file_path, mut lines) in file_buckets.into_iter() {
            lines.sort();
            let per_file = FileResult {
                file_name: file_path,
                lines: lines,
            };
            result.push(per_file);
        }
        result.sort();
        Ok(result)
    }
}
