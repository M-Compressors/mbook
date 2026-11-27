//! How often each chapter of a session changed, and the chapter the editor
//! holds text of that it has not sent (mbook-ui docs, "Agents (MCP)", "An
//! agent and a person in one chapter").

use std::collections::HashMap;

use mbook::Book;

/// How often each chapter changed, as an agent and as the editor see it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Revisions {
    /// Counts every change of the session.
    counter: u64,
    /// The change that last touched each chapter, by anyone.
    any: HashMap<String, u64>,
    /// The change that last touched each chapter by an agent.
    by_agent: HashMap<String, u64>,
}

impl Revisions {
    /// The revision `read_chapter` gives.
    #[cfg(not(target_os = "android"))]
    pub(crate) fn of(&self, path: &str) -> String {
        self.any.get(path).copied().unwrap_or_default().to_string()
    }

    /// The last change of a chapter by an agent; 0 for none.
    pub(crate) fn by_agent(&self, path: &str) -> u64 {
        self.by_agent.get(path).copied().unwrap_or_default()
    }

    /// Records a change of a chapter.
    pub(crate) fn touch(&mut self, path: &str, by_agent: bool) {
        self.counter += 1;
        self.any.insert(path.to_owned(), self.counter);
        if by_agent {
            self.by_agent.insert(path.to_owned(), self.counter);
        }
    }

    /// Records that every chapter may have changed: after the book was read
    /// again from its file.
    pub(crate) fn touch_all(&mut self, book: &Book) {
        for chapter in book.chapters() {
            self.touch(chapter.path(), true);
        }
    }
}

/// The chapter the editor holds text of that it has not sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Held {
    pub(crate) path: String,
    /// An agent changed the chapter after the editor loaded it.
    pub(crate) conflict: bool,
}
