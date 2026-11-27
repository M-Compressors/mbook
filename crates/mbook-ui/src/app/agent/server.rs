//! The MCP server: its tools, the skill, the resources and the prompts
//! (mbook-ui docs, "Agents (MCP)").
//!
//! A tool only shapes its arguments into a [`Request`] and hands it to the
//! main thread through the [`Bridge`]; what the tool does is done there.

use std::future::{Future, ready};
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::prompt::PromptContext;
use rmcp::handler::server::router::prompt::PromptRouter;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, GetPromptRequestParams,
    GetPromptResponse, GetSkillRequestParams, GetSkillResult, Implementation, ListPromptsResult,
    ListResourcesResult, ListSkillsResult, ListToolsResult, PaginatedRequestParams, PromptMessage,
    ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Role, ServerCapabilities,
    ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, prompt, prompt_router, tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) use super::bridge::Bridge;
use super::bridge::{Caller, CreateBook, ExportBook, FileArgs, ListBooks, Request, Show};
use super::pictures::{Encoding, Source};
use super::skill;
use crate::features::edit_book::agent::error::AgentError;
use crate::features::edit_book::agent::types::{
    AddChapter, BookView, ChapterArgs, ChapterChanged, ChapterText, ContentsView, Done,
    EditChapter, FigurePut, FigureSource, Kind, LandmarksView, Matches, MetaPatch, MetaView,
    MoveChapter, PathArgs, PathView, PictureAdded, PictureInfo, ProblemsView, PutFigure,
    ReadChapter, Render, Saved, Search, SetContents, SetLandmarks, WriteChapter,
};
use crate::features::edit_book::agent::{self, Answer, Op};

/// The arguments of a tool on one book: its key and the rest.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub(crate) struct OnBook<T> {
    /// The key of the book, as `list_books` gives it.
    pub(crate) book: String,
    #[serde(flatten)]
    pub(crate) args: T,
}

/// The key of a book.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub(crate) struct BookArgs {
    /// The key of the book, as `list_books` gives it.
    pub(crate) book: String,
}

/// What `add_picture` adds.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub(crate) struct AddPicture {
    /// The key of the book.
    pub(crate) book: String,
    /// `{"file": "/abs/path.png"}`, `{"base64": "…", "media_type": "image/png"}`
    /// or `{"svg": "<svg …>"}`.
    pub(crate) source: Source,
    /// Where it goes, ending in `.avif`; from the name of the file or the next
    /// free number when left out.
    #[serde(default)]
    pub(crate) path: Option<String>,
    /// 1 to 100; 80 when left out.
    #[serde(default)]
    pub(crate) quality: Option<u8>,
    /// Make a wider picture this wide, keeping its proportions.
    #[serde(default)]
    pub(crate) max_width: Option<u32>,
    /// Make a taller picture this tall, keeping its proportions.
    #[serde(default)]
    pub(crate) max_height: Option<u32>,
    /// Make it the cover of the book.
    #[serde(default)]
    pub(crate) cover: bool,
}

/// What `view_picture` shows.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub(crate) struct ViewPicture {
    /// The key of the book.
    pub(crate) book: String,
    /// The path of the picture.
    pub(crate) path: String,
    /// The largest side of the image given, in pixels; 1024 when left out, at
    /// most 2048.
    #[serde(default)]
    pub(crate) max_size: Option<u32>,
}

/// What `render_figure` draws.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub(crate) struct RenderFigure {
    /// A diagram or a formula.
    pub(crate) kind: Kind,
    /// The Mermaid source, or `KaTeX` without `$`.
    pub(crate) source: String,
    /// Set a formula as a display formula; true when left out.
    #[serde(default)]
    pub(crate) display: Option<bool>,
}

/// What `get_guide` gives.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub(crate) struct Guide {
    /// `overview`, `format`, `dialect`, `figures`, `pictures`, `structure`
    /// or `errors`.
    pub(crate) topic: String,
}

/// The text of a topic.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct GuideText {
    pub(crate) topic: String,
    pub(crate) text: String,
}

/// A book of the library.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct BookSummary {
    /// The key every other tool takes.
    pub(crate) book: String,
    pub(crate) title: String,
    pub(crate) authors: Vec<String>,
    pub(crate) language: String,
    pub(crate) series: Option<String>,
    /// When it was added, in milliseconds since the Unix epoch.
    pub(crate) added: u64,
    /// A window of the application edits it.
    pub(crate) open_in_editor: bool,
}

/// The books of the library.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct Books {
    pub(crate) books: Vec<BookSummary>,
}

/// A book that was made or imported.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct Created {
    pub(crate) book: String,
    /// What the conversion of an import could not keep.
    pub(crate) warnings: Vec<String>,
}

/// A file that was written.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct Written {
    pub(crate) path: String,
    pub(crate) bytes: u64,
}

/// What a prompt is about.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub(crate) struct PromptArgs {
    /// The key of the book; the agent asks for it when left out.
    #[serde(default)]
    pub(crate) book: Option<String>,
    /// What the chapter is about, the chapter to work on, or what to look
    /// for.
    #[serde(default)]
    pub(crate) about: Option<String>,
}

/// The name a client gave, or a word for it.
fn client(context: &RequestContext<RoleServer>) -> String {
    context
        .peer
        .peer_info()
        .map(|info| info.client_info.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "An agent".into())
}

/// The answer of a call as a result of a tool.
fn result(outcome: Result<Answer, AgentError>) -> CallToolResult {
    match outcome {
        Ok(answer) => {
            let mut result = CallToolResult::structured(answer.value);
            if let Some(png) = answer.png {
                result
                    .content
                    .push(ContentBlock::image(STANDARD.encode(png), "image/png"));
            }
            result
        }
        Err(error) => CallToolResult::structured_error(error.to_json()),
    }
}

/// The number the next session of the server gets.
static SESSIONS: AtomicU64 = AtomicU64::new(1);

/// The MCP server of the application, one per session of a client.
#[derive(Clone, Debug)]
pub(crate) struct Server {
    bridge: Bridge,
    /// The number of this session, which no client chooses.
    session: u64,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl Server {
    pub(crate) fn new(bridge: Bridge) -> Self {
        Self {
            bridge,
            session: SESSIONS.fetch_add(1, Ordering::Relaxed),
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    /// Hands a request to the main thread.
    async fn ask(&self, request: Request, context: &RequestContext<RoleServer>) -> CallToolResult {
        let caller = Caller {
            name: client(context),
            session: self.session,
        };
        result(self.bridge.ask(request, caller).await)
    }

    /// Hands a call on one book to the main thread.
    async fn on_book(
        &self,
        book: String,
        op: Op,
        context: &RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.ask(Request::Book { book, op }, context).await
    }
}

#[tool_router]
impl Server {
    #[tool(
        description = "The books of the library with their keys. `query` keeps those whose title, authors or series contain it.",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<Books>()
    )]
    async fn list_books(
        &self,
        Parameters(args): Parameters<ListBooks>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.ask(Request::ListBooks(args), &context).await
    }

    #[tool(
        description = "Makes a new book in the library, with an empty chapter `ch01.md`, and gives its key.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false),
        output_schema = schema_for_output::<Created>()
    )]
    async fn create_book(
        &self,
        Parameters(args): Parameters<CreateBook>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.ask(Request::CreateBook(args), &context).await
    }

    #[tool(
        description = "Imports a book file of this computer (EPUB, FB2, PDF, CBZ, DOCX, HTML, Markdown, text or mbook) into the library. A book already in it gives its key.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true),
        output_schema = schema_for_output::<Created>()
    )]
    async fn import_book(
        &self,
        Parameters(args): Parameters<FileArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.ask(Request::ImportBook(args), &context).await
    }

    #[tool(
        description = "Writes the saved book in another format to a file of this computer. The person is asked first when the file exists.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true),
        output_schema = schema_for_output::<Written>()
    )]
    async fn export_book(
        &self,
        Parameters(args): Parameters<ExportBook>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.ask(Request::ExportBook(args), &context).await
    }

    #[tool(
        description = "Removes a book from the library, file and all. The person is asked first.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true),
        output_schema = schema_for_output::<Done>()
    )]
    async fn delete_book(
        &self,
        Parameters(args): Parameters<BookArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.ask(Request::DeleteBook { book: args.book }, &context)
            .await
    }

    #[tool(
        description = "The structure of a book: metadata, chapters with their headings and anchors, contents, landmarks, pictures and figures with the lines that show them. No text and no pictures.",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<BookView>()
    )]
    async fn get_book(
        &self,
        Parameters(args): Parameters<BookArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(
            args.book,
            Op::GetBook {
                open_in_editor: false,
            },
            &context,
        )
        .await
    }

    #[tool(
        description = "The text of a chapter, exactly as it is stored, with its revision. `anchor` gives the section from that heading; `lines` gives those lines.",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<ChapterText>()
    )]
    async fn read_chapter(
        &self,
        Parameters(args): Parameters<OnBook<ReadChapter>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::ReadChapter(args.args), &context)
            .await
    }

    #[tool(
        description = "Finds text, or a regular expression, in every chapter, with line and column.",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<Matches>()
    )]
    async fn search_book(
        &self,
        Parameters(args): Parameters<OnBook<Search>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::Search(args.args), &context)
            .await
    }

    #[tool(
        description = "The source of a diagram or formula and the lines that show it.",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<FigureSource>()
    )]
    async fn read_figure(
        &self,
        Parameters(args): Parameters<OnBook<PathArgs>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::ReadFigure(args.args), &context)
            .await
    }

    #[tool(
        description = "A picture of the book as a PNG image no larger than `max_size`, with its size, alternative text and uses.",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<PictureInfo>()
    )]
    async fn view_picture(
        &self,
        Parameters(args): Parameters<ViewPicture>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let request = Request::ViewPicture {
            book: args.book,
            path: args.path,
            max_size: args.max_size.unwrap_or(1024),
        };
        self.ask(request, &context).await
    }

    #[tool(
        description = "One topic of the guide to writing books here: overview, format (the mbook format in short, with a link to its specification), dialect, figures, pictures, structure or errors.",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<GuideText>()
    )]
    async fn get_guide(&self, Parameters(args): Parameters<Guide>) -> CallToolResult {
        result(
            skill::guide(&args.topic)
                .map(|text| {
                    Answer::of(&GuideText {
                        topic: args.topic.clone(),
                        text: text.to_owned(),
                    })
                })
                .ok_or_else(|| {
                    AgentError::InvalidArguments(format!(
                        "no topic {:?}; the topics are {}",
                        args.topic,
                        skill::topics().collect::<Vec<_>>().join(", ")
                    ))
                }),
        )
    }

    #[tool(
        description = "Draws a diagram or formula without any book: its size, a PNG image, or the error of the renderer.",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<Render>()
    )]
    async fn render_figure(&self, Parameters(args): Parameters<RenderFigure>) -> CallToolResult {
        let display = args.display.unwrap_or(true);
        result(
            crate::shared::compute::spawn(move || agent::render(args.kind, &args.source, display))
                .await,
        )
    }

    #[tool(
        description = "Adds a chapter. Mermaid fences and $…$ in the text become resources of the book; start a diagram with `%% alt: …`.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false),
        output_schema = schema_for_output::<ChapterChanged>()
    )]
    async fn add_chapter(
        &self,
        Parameters(args): Parameters<OnBook<AddChapter>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::AddChapter(args.args), &context)
            .await
    }

    #[tool(
        description = "Replaces parts of a chapter: each `old` must be in the stored text exactly once unless `replace_all`. All edits or none. Mermaid fences and $…$ in `new` become resources.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false),
        output_schema = schema_for_output::<ChapterChanged>()
    )]
    async fn edit_chapter(
        &self,
        Parameters(args): Parameters<OnBook<EditChapter>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::EditChapter(args.args), &context)
            .await
    }

    #[tool(
        description = "Replaces the whole text of a chapter; needs the revision `read_chapter` gave.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true),
        output_schema = schema_for_output::<ChapterChanged>()
    )]
    async fn write_chapter(
        &self,
        Parameters(args): Parameters<OnBook<WriteChapter>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::WriteChapter(args.args), &context)
            .await
    }

    #[tool(
        description = "Moves a chapter to another position, gives it another path, or both. Contents and landmarks follow it; links of other chapters do not.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true),
        output_schema = schema_for_output::<PathView>()
    )]
    async fn move_chapter(
        &self,
        Parameters(args): Parameters<OnBook<MoveChapter>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::MoveChapter(args.args), &context)
            .await
    }

    #[tool(
        description = "Removes a chapter, with the contents entries and landmarks that led into it. The person is asked first.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true),
        output_schema = schema_for_output::<Done>()
    )]
    async fn remove_chapter(
        &self,
        Parameters(args): Parameters<OnBook<ChapterArgs>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::RemoveChapter(args.args), &context)
            .await
    }

    #[tool(
        description = "Adds a picture from a file, base64 or SVG, encoded as AVIF. A chapter shows it with ![words](path).",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false),
        output_schema = schema_for_output::<PictureAdded>()
    )]
    async fn add_picture(
        &self,
        Parameters(args): Parameters<AddPicture>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let request = Request::AddPicture {
            book: args.book,
            source: args.source,
            path: args.path,
            encoding: Encoding {
                quality: args.quality,
                max_width: args.max_width,
                max_height: args.max_height,
            },
            cover: args.cover,
        };
        self.ask(request, &context).await
    }

    #[tool(
        description = "Removes a picture. The person is asked first.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true),
        output_schema = schema_for_output::<Done>()
    )]
    async fn remove_picture(
        &self,
        Parameters(args): Parameters<OnBook<PathArgs>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::RemovePicture(args.args), &context)
            .await
    }

    #[tool(
        description = "Adds a diagram or formula, or replaces the source of the one at `path`; it is drawn first. `preview` adds a PNG image.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true),
        output_schema = schema_for_output::<FigurePut>()
    )]
    async fn put_figure(
        &self,
        Parameters(args): Parameters<OnBook<PutFigure>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::PutFigure(args.args), &context)
            .await
    }

    #[tool(
        description = "Removes a diagram or formula. The person is asked first.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true),
        output_schema = schema_for_output::<Done>()
    )]
    async fn remove_figure(
        &self,
        Parameters(args): Parameters<OnBook<PathArgs>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::RemoveFigure(args.args), &context)
            .await
    }

    #[tool(
        description = "Changes the metadata: a missing field stays, null or an empty list removes it.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true),
        output_schema = schema_for_output::<MetaView>()
    )]
    async fn set_meta(
        &self,
        Parameters(args): Parameters<OnBook<MetaPatch>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::SetMeta(args.args), &context)
            .await
    }

    #[tool(
        description = "Makes the contents follow the headings (`automatic: true`), or keeps `entries`, the whole list.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true),
        output_schema = schema_for_output::<ContentsView>()
    )]
    async fn set_contents(
        &self,
        Parameters(args): Parameters<OnBook<SetContents>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::SetContents(args.args), &context)
            .await
    }

    #[tool(
        description = "Replaces the landmarks: where the text starts, the glossary, the index and the like.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true),
        output_schema = schema_for_output::<LandmarksView>()
    )]
    async fn set_landmarks(
        &self,
        Parameters(args): Parameters<OnBook<SetLandmarks>>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::SetLandmarks(args.args), &context)
            .await
    }

    #[tool(
        description = "Every problem that keeps the book from being saved, with chapter, line and column.",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<ProblemsView>()
    )]
    async fn check_book(
        &self,
        Parameters(args): Parameters<BookArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::Check, &context).await
    }

    #[tool(
        description = "Writes the book to its file when it has no problems; otherwise lists them.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true),
        output_schema = schema_for_output::<Saved>()
    )]
    async fn save_book(
        &self,
        Parameters(args): Parameters<BookArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.on_book(args.book, Op::Save, &context).await
    }

    #[tool(
        description = "Reads the book again from its file; every unsaved change, the person's too, is lost. The person is asked first.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true),
        output_schema = schema_for_output::<Done>()
    )]
    async fn discard_changes(
        &self,
        Parameters(args): Parameters<BookArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.ask(Request::Discard { book: args.book }, &context)
            .await
    }

    #[tool(
        description = "Opens the book in the window of the application, at `href`, to read (`mode: read`) or to edit (`mode: edit`).",
        annotations(read_only_hint = true),
        output_schema = schema_for_output::<Done>()
    )]
    async fn show(
        &self,
        Parameters(args): Parameters<Show>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.ask(Request::Show(args), &context).await
    }
}

/// The opening of every prompt: what to read first.
fn prompt_start(args: &PromptArgs) -> String {
    let book = args.book.as_deref().map_or_else(
        || "Ask me which book, or call `list_books`.".to_owned(),
        |book| format!("The book is `{book}`."),
    );
    format!(
        "{book} If you have not read the skill `mbook-authoring`, call `get_guide` with `overview` first."
    )
}

#[prompt_router]
impl Server {
    #[prompt(
        name = "write_chapter",
        description = "Write a chapter of a book from what it should be about."
    )]
    async fn write_chapter_prompt(
        &self,
        Parameters(args): Parameters<PromptArgs>,
    ) -> Vec<PromptMessage> {
        let about = args
            .about
            .clone()
            .unwrap_or_else(|| "what I tell you next".into());
        vec![PromptMessage::new_text(
            Role::User,
            format!(
                "{} Write a new chapter about {about}. Read `get_book` to fit it to the chapters around it, add it with `add_chapter`, fix the problems it answers with, then `check_book` and `save_book`.",
                prompt_start(&args)
            ),
        )]
    }

    #[prompt(description = "Add diagrams, formulas and pictures where a chapter needs them.")]
    async fn illustrate(&self, Parameters(args): Parameters<PromptArgs>) -> Vec<PromptMessage> {
        let chapter = args
            .about
            .clone()
            .unwrap_or_else(|| "the chapter I name".into());
        vec![PromptMessage::new_text(
            Role::User,
            format!(
                "{} Illustrate {chapter}: read it, decide where a Mermaid diagram, a formula or a picture explains more than words, and add them with `edit_chapter`, `put_figure` or `add_picture`. Give each an alternative text in words. Then `check_book` and `save_book`.",
                prompt_start(&args)
            ),
        )]
    }

    #[prompt(description = "Check a book and fix what keeps it from being saved.")]
    async fn review_book(&self, Parameters(args): Parameters<PromptArgs>) -> Vec<PromptMessage> {
        vec![PromptMessage::new_text(
            Role::User,
            format!(
                "{} Call `check_book`, fix every problem with the smallest change, look for pictures and figures without an alternative text in `get_book`, then save.",
                prompt_start(&args)
            ),
        )]
    }
}

impl ServerHandler for Server {
    fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, ErrorData>> + Send + '_ {
        self.tool_router
            .call(ToolCallContext::new(self, request, context))
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        ready(Ok(ListToolsResult::with_all_items(
            self.tool_router.list_all(),
        )))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tool_router.get(name).cloned()
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<GetPromptResponse, ErrorData>> + Send + '_ {
        self.prompt_router.get_prompt(PromptContext::new(
            self,
            request.name,
            request.arguments,
            context,
        ))
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListPromptsResult, ErrorData>> + Send + '_ {
        ready(Ok(ListPromptsResult::with_all_items(
            self.prompt_router.list_all(),
        )))
    }

    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .enable_skills()
                .build(),
        )
        .with_server_info(Implementation::new("mbook", env!("CARGO_PKG_VERSION")))
        .with_instructions(skill::INSTRUCTIONS)
    }

    fn list_skills(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListSkillsResult, ErrorData>> + Send + '_ {
        ready(Ok(ListSkillsResult::with_all_items(vec![skill::skill()])))
    }

    fn get_skill(
        &self,
        request: GetSkillRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<GetSkillResult, ErrorData>> + Send + '_ {
        let skill = skill::skill();
        ready(if request.uri == skill.uri {
            Ok(GetSkillResult::new(skill))
        } else {
            Err(ErrorData::invalid_params(
                format!("no skill at {}", request.uri),
                None,
            ))
        })
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, ErrorData>> + Send + '_ {
        ready(Ok(ListResourcesResult::with_all_items(skill::resources())))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResponse, ErrorData>> + Send + '_ {
        ready(
            skill::read(&request.uri)
                .map(|contents| ReadResourceResult::new(vec![contents]).into())
                .ok_or_else(|| {
                    ErrorData::resource_not_found(format!("no resource at {}", request.uri), None)
                }),
        )
    }
}
