//! Each script read once and parsed once per run, shared by the Resource map and `check`; gdformat and
//! gdlint read and parse their files themselves.
//!
//! Text is read as 0.4.0 reads it (UTF-8 with replacement, universal newlines). A script that changes on
//! disk during the run keeps the text first read for those two, where 0.4.0 would read it again.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rayon::prelude::*;

use crate::parse::{self, Parsed};
use crate::py;

#[derive(Default)]
pub struct Scripts {
    texts: Mutex<HashMap<PathBuf, Arc<str>>>,
    trees: Mutex<HashMap<PathBuf, Arc<Parsed>>>,
}

impl Scripts {
    pub fn new() -> Scripts {
        Scripts::default()
    }

    /// The script's text; errors are not cached, so a failed read fails again.
    pub fn text(&self, path: &Path) -> io::Result<Arc<str>> {
        if let Some(text) = self.texts.lock().expect("an unpoisoned lock").get(path) {
            return Ok(Arc::clone(text));
        }
        let text: Arc<str> = py::read_text(path)?.into();
        let mut texts = self.texts.lock().expect("an unpoisoned lock");
        Ok(Arc::clone(texts.entry(path.to_path_buf()).or_insert(text)))
    }

    /// Several scripts' texts, read in parallel.
    pub fn texts(&self, paths: &[PathBuf]) -> Vec<io::Result<Arc<str>>> {
        paths.par_iter().map(|path| self.text(path)).collect()
    }

    /// The trees of scripts whose `texts` are already read, parsing those not yet parsed in one batch.
    pub fn trees(&self, paths: &[PathBuf], texts: &[Arc<str>]) -> Vec<Arc<Parsed>> {
        // Held across the batch, so two steps never parse the same script twice.
        let mut trees = self.trees.lock().expect("an unpoisoned lock");
        let (mut wanted, mut queued) = (Vec::new(), HashSet::new());
        for (path, text) in paths.iter().zip(texts) {
            if !trees.contains_key(path) && queued.insert(path) {
                wanted.push((path, text));
            }
        }
        let sources: Vec<&str> = wanted.iter().map(|(_, text)| text.as_ref()).collect();
        for ((path, _), parsed) in wanted.iter().zip(parse::parse_all(&sources)) {
            trees.insert((*path).clone(), Arc::new(parsed));
        }
        paths.iter().map(|path| Arc::clone(&trees[path])).collect()
    }
}
