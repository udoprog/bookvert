use std::collections::BTreeSet;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Calculated application state.
#[derive(Default)]
pub(crate) struct State {
    pub(crate) name: Option<String>,
    pub(crate) names: BTreeSet<String>,
    pub(crate) catalogs: Vec<Catalog>,
}

impl State {
    /// Count the number of catalogs which have a picked book.
    #[inline]
    pub(crate) fn picked(&self) -> usize {
        self.catalogs.iter().filter(|c| c.picked.is_some()).count()
    }
}

/// The state for a single catalog.
pub(crate) struct Catalog {
    /// The catalog number.
    pub(crate) number: u32,
    /// The books in the catalog.
    pub(crate) books: Vec<Rc<Book>>,
    /// The picked book.
    pub(crate) picked: Option<usize>,
}

impl Catalog {
    /// Returns the selected book, if any.
    #[inline]
    pub(crate) fn selected(&self) -> Option<&Book> {
        Some(self.books.get(self.picked?)?.as_ref())
    }
}

pub(crate) struct Page {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) metadata: Metadata,
}

pub(crate) struct Book {
    pub(crate) dir: PathBuf,
    pub(crate) name: String,
    pub(crate) pages: Vec<Page>,
    pub(crate) numbers: BTreeSet<u32>,
}

impl Book {
    /// Returns a key for sorting books by name and directory.
    #[inline]
    pub(crate) fn key(&self) -> (&str, &Path) {
        (&self.name, &self.dir)
    }

    /// Returns the total size of all pages in bytes.
    #[inline]
    pub(crate) fn bytes(&self) -> u64 {
        self.pages.iter().map(|page| page.metadata.len()).sum()
    }
}
