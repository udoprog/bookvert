use std::collections::BTreeSet;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// The state of a bookvert session.
#[derive(Default)]
pub struct State {
    pub name: Option<String>,
    pub names: BTreeSet<String>,
    pub catalogs: Vec<Catalog>,
}

impl State {
    /// Count the number of catalogs which have a picked book.
    #[inline]
    pub(crate) fn picked(&self) -> usize {
        self.catalogs.iter().filter(|c| c.picked.is_some()).count()
    }
}

/// The state for a single catalog.
pub struct Catalog {
    /// The catalog number.
    pub number: u32,
    /// The books in the catalog.
    pub books: Vec<Rc<Book>>,
    /// The picked book.
    pub picked: Option<usize>,
}

impl Catalog {
    /// Returns the selected book, if any.
    #[inline]
    pub fn selected(&self) -> Option<&Book> {
        Some(self.books.get(self.picked?)?.as_ref())
    }
}

pub struct Page {
    pub path: PathBuf,
    pub name: String,
    pub metadata: Metadata,
}

pub struct Book {
    pub dir: PathBuf,
    pub name: String,
    pub pages: Vec<Page>,
    pub numbers: BTreeSet<u32>,
}

impl Book {
    /// Returns a key for sorting books by name and directory.
    #[inline]
    pub fn key(&self) -> (&str, &Path) {
        (&self.name, &self.dir)
    }

    /// Returns the total size of all pages in bytes.
    #[inline]
    pub fn bytes(&self) -> u64 {
        self.pages.iter().map(|page| page.metadata.len()).sum()
    }
}
