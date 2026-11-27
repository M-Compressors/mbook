//! What the thread of the server asks the main thread, and how it waits for
//! the answer.

use futures::channel::{mpsc, oneshot};
use schemars::JsonSchema;
use serde::Deserialize;

use super::pictures::{Encoding, Source};
use crate::features::edit_book::agent::error::AgentError;
use crate::features::edit_book::agent::{Answer, Op};

/// What `list_books` looks for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct ListBooks {
    /// Words the title, an author or the series contain, in any case.
    #[serde(default)]
    pub(crate) query: Option<String>,
    /// At most this many books; 50 when left out.
    #[serde(default)]
    pub(crate) limit: Option<usize>,
}

/// What `create_book` makes.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct CreateBook {
    /// The title; not empty.
    pub(crate) title: String,
    /// The main language, a BCP 47 tag such as `en`.
    pub(crate) language: String,
}

/// A file on this computer.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct FileArgs {
    /// The absolute path.
    pub(crate) path: std::path::PathBuf,
}

/// What `export_book` writes.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct ExportBook {
    /// The key of the book.
    pub(crate) book: String,
    /// `epub`, `fb2`, `pdf`, `cbz`, `docx`, `html`, `md` or `txt`.
    pub(crate) format: String,
    /// The absolute path of the file to write.
    pub(crate) path: std::path::PathBuf,
}

/// What `show` opens.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct Show {
    /// The key of the book.
    pub(crate) book: String,
    /// The place: `ch01.md` or `ch01.md#a-heading`; the start when left out.
    #[serde(default)]
    pub(crate) href: Option<String>,
    /// `read` or `edit`; `read` when left out.
    #[serde(default)]
    pub(crate) mode: Option<String>,
}

/// One call of a tool that the main thread answers.
#[derive(Debug)]
pub(crate) enum Request {
    ListBooks(ListBooks),
    CreateBook(CreateBook),
    ImportBook(FileArgs),
    ExportBook(ExportBook),
    DeleteBook {
        book: String,
    },
    Show(Show),
    /// A call on one book.
    Book {
        book: String,
        op: Op,
    },
    /// A picture to encode and add.
    AddPicture {
        book: String,
        source: Source,
        path: Option<String>,
        encoding: Encoding,
        cover: bool,
    },
    /// A picture to give as PNG.
    ViewPicture {
        book: String,
        path: String,
        max_size: u32,
    },
    Discard {
        book: String,
    },
}

/// Who asks: the name a client gave, which it chooses itself, and the MCP
/// session of the server, which the client cannot choose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Caller {
    /// The name the client gave in `initialize`, for the person to read.
    pub(crate) name: String,
    /// The session of the server the call came through; what the person
    /// allows is kept for it.
    pub(crate) session: u64,
}

/// A request, who asked, and where the answer goes.
#[derive(Debug)]
pub(crate) struct Call {
    pub(crate) request: Request,
    /// Who asked.
    pub(crate) client: Caller,
    pub(crate) reply: oneshot::Sender<Result<Answer, AgentError>>,
}

/// The side of the main thread the server keeps.
#[derive(Clone, Debug)]
pub(crate) struct Bridge {
    calls: mpsc::UnboundedSender<Call>,
}

impl Bridge {
    /// A bridge and the calls that cross it.
    pub(crate) fn new() -> (Self, mpsc::UnboundedReceiver<Call>) {
        let (calls, incoming) = mpsc::unbounded();
        (Self { calls }, incoming)
    }

    /// Hands a request to the main thread and waits for its answer.
    pub(crate) async fn ask(&self, request: Request, client: Caller) -> Result<Answer, AgentError> {
        let (reply, answer) = oneshot::channel();
        self.calls
            .unbounded_send(Call {
                request,
                client,
                reply,
            })
            .map_err(|_| AgentError::OpenFailed("the application is closing".into()))?;
        answer
            .await
            .unwrap_or_else(|_| Err(AgentError::OpenFailed("the application is closing".into())))
    }
}
