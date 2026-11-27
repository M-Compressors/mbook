//! The calls of an agent, end to end through a session on a book on disk.

use std::future::Future;
use std::path::{Path, PathBuf};

use futures::StreamExt;
use futures::executor::block_on;
use futures::future::join;
use serde_json::{Value, json};
use tempfile::TempDir;

use super::error::AgentError;
use super::types::{
    AddChapter, ChapterArgs, Edit, EditChapter, EntryView, Kind, LandmarkView, Lines, MetaPatch,
    MoveChapter, Patch, PathArgs, PutFigure, ReadChapter, Search, Series, SetContents,
    SetLandmarks, WriteChapter,
};
use super::{Op, render};
use crate::features::edit_book::actor::{EditHandle, session};
use crate::features::edit_book::command::Command;
use crate::features::edit_book::error::EditError;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const PICTURE: &[u8] = include_bytes!("../../../../../mbook/tests/fixtures/plain.avif");

/// A book with two chapters, a picture shown in the first and a formula.
fn sample() -> Result<(TempDir, PathBuf)> {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("book.mbook");
    let mut book = mbook::Book::new("The Example", "en")?;
    book.add_chapter(
        "ch01.md",
        "# Arrival\n\nThe train came in.\n\n## The station\n\n![The hall](img/a.avif)\n\nIt cost $5 and $10.\n\n## The square\n\nEnd.\n",
    )?;
    book.add_chapter("ch02.md", "# Departure\n\n![v](math/v.tex)\n")?;
    book.add_picture("img/a.avif", PICTURE)?;
    book.add_formula("math/v.tex", "v = s / t\n")?;
    book.save(&path)?;
    Ok((folder, path))
}

/// Runs a session on the book at `path` beside a script that owns its
/// handle.
fn run<T, F, Fut>(path: &Path, script: F) -> T
where
    F: FnOnce(EditHandle) -> Fut,
    Fut: Future<Output = T>,
{
    let (handle, actor) = session(path.to_owned());
    block_on(async { join(actor, script(handle)).await.1 })
}

/// The structured answer of a call.
async fn call(handle: &EditHandle, op: Op) -> std::result::Result<Value, AgentError> {
    handle.agent(op).await.map(|answer| answer.value)
}

fn read(chapter: &str) -> Op {
    Op::ReadChapter(ReadChapter {
        chapter: chapter.into(),
        anchor: None,
        lines: None,
    })
}

fn edit(chapter: &str, old: &str, new: &str) -> Op {
    Op::EditChapter(EditChapter {
        chapter: chapter.into(),
        edits: vec![Edit {
            old: old.into(),
            new: new.into(),
            replace_all: false,
        }],
        allow_unrendered: false,
    })
}

fn add(text: &str) -> AddChapter {
    AddChapter {
        text: text.into(),
        path: None,
        position: None,
        allow_unrendered: false,
    }
}

#[test]
fn the_structure_of_a_book_names_headings_pictures_and_their_uses() -> Result {
    let (_folder, path) = sample()?;
    let book = run(&path, |handle| async move {
        call(
            &handle,
            Op::GetBook {
                open_in_editor: true,
            },
        )
        .await
    })?;
    assert_eq!(book["meta"]["title"], "The Example");
    let chapter = &book["chapters"][0];
    assert_eq!(chapter["title"], "Arrival");
    assert_eq!(chapter["lines"], 13);
    assert_eq!(
        chapter["headings"][1],
        json!({"level": 2, "title": "The station", "anchor": "the-station"})
    );
    assert_eq!(book["pictures"][0]["alt"], "The hall");
    assert_eq!(
        book["pictures"][0]["used_by"],
        json!([{"chapter": "ch01.md", "line": 7}])
    );
    assert_eq!(
        book["figures"][0],
        json!({"kind": "formula", "path": "math/v.tex",
               "used_by": [{"chapter": "ch02.md", "line": 3}]})
    );
    // A saved book keeps its contents as a list.
    assert_eq!(book["contents"]["automatic"], false);
    assert_eq!(
        book["contents"]["entries"][1]["href"],
        "ch01.md#the-station"
    );
    assert_eq!(book["open_in_editor"], true);
    assert_eq!(book["unsaved"], false);
    assert_eq!(book["problems"], 0);
    Ok(())
}

#[test]
fn a_chapter_is_read_whole_by_section_or_by_lines() -> Result {
    let (_folder, path) = sample()?;
    let (whole, section, lines) = run(&path, |handle| async move {
        let whole = call(&handle, read("ch01.md")).await?;
        let section = call(
            &handle,
            Op::ReadChapter(ReadChapter {
                chapter: "ch01.md".into(),
                anchor: Some("the-station".into()),
                lines: None,
            }),
        )
        .await?;
        let lines = call(
            &handle,
            Op::ReadChapter(ReadChapter {
                chapter: "ch01.md".into(),
                anchor: None,
                lines: Some(Lines { from: 3, to: 3 }),
            }),
        )
        .await?;
        Ok::<_, AgentError>((whole, section, lines))
    })?;
    assert_eq!(whole["total_lines"], 13);
    assert_eq!(whole["first_line"], 1);
    assert_eq!(whole["last_line"], 13);
    assert_eq!(
        section["text"],
        "## The station\n\n![The hall](img/a.avif)\n\nIt cost $5 and $10.\n\n"
    );
    assert_eq!(section["first_line"], 5);
    assert_eq!(lines["text"], "The train came in.\n");
    assert_eq!(
        (lines["first_line"].clone(), lines["last_line"].clone()),
        (json!(3), json!(3))
    );
    Ok(())
}

#[test]
fn a_chapter_is_not_read_by_an_unknown_anchor_or_by_both_ways() -> Result {
    let (_folder, path) = sample()?;
    let (unknown, both) = run(&path, |handle| async move {
        let ask = |anchor: Option<&str>, lines| {
            Op::ReadChapter(ReadChapter {
                chapter: "ch01.md".into(),
                anchor: anchor.map(str::to_owned),
                lines,
            })
        };
        (
            call(&handle, ask(Some("nowhere"), None)).await,
            call(
                &handle,
                ask(Some("the-station"), Some(Lines { from: 1, to: 1 })),
            )
            .await,
        )
    });
    assert_eq!(unknown.map_err(|e| e.code()), Err("no_such_path"));
    assert_eq!(both.map_err(|e| e.code()), Err("invalid_arguments"));
    Ok(())
}

#[test]
fn a_search_gives_lines_and_columns_and_stops_at_its_limit() -> Result {
    let (_folder, path) = sample()?;
    let (found, limited, broken) = run(&path, |handle| async move {
        let search = |query: &str, regex, limit| {
            Op::Search(Search {
                query: query.into(),
                regex,
                limit,
            })
        };
        (
            call(&handle, search("train", false, None)).await,
            call(&handle, search("#+ ", true, Some(1))).await,
            call(&handle, search("(", true, None)).await,
        )
    });
    let found = found?;
    assert_eq!(
        found["matches"],
        json!([{"chapter": "ch01.md", "line": 3, "column": 5, "snippet": "The train came in."}])
    );
    assert_eq!(found["truncated"], false);
    let limited = limited?;
    assert_eq!(limited["matches"].as_array().map(Vec::len), Some(1));
    assert_eq!(limited["truncated"], true);
    assert_eq!(broken.map_err(|e| e.code()), Err("invalid_arguments"));
    Ok(())
}

#[test]
fn a_new_chapter_takes_its_figures_into_resources() -> Result {
    let (_folder, path) = sample()?;
    let (added, text, figure) = run(&path, |handle| async move {
        let added = call(
            &handle,
            Op::AddChapter(add(
                "# Three\n\nSpeed $v = s / t$.\n\n```mermaid\n%% alt: A to B\nflowchart LR\n  A --> B\n```\n\n$$\nx^2\n$$\n",
            )),
        )
        .await?;
        let text = call(&handle, read("ch03.md")).await?;
        let figure = call(
            &handle,
            Op::ReadFigure(PathArgs {
                path: "dia/0001.mmd".into(),
            }),
        )
        .await?;
        Ok::<_, AgentError>((added, text, figure))
    })?;
    assert_eq!(added["path"], "ch03.md");
    assert_eq!(
        added["figures"],
        json!([
            {"kind": "formula", "path": "math/v.tex", "line": 3, "new": false},
            {"kind": "diagram", "path": "dia/0001.mmd", "line": 5, "new": true},
            {"kind": "formula", "path": "math/0001.tex", "line": 11, "new": true},
        ])
    );
    let warnings = added["warnings"].as_array().cloned().unwrap_or_default();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["code"], "missing_alt");
    assert_eq!(warnings[0]["line"], 11);
    assert_eq!(added["problems"], json!([]));
    assert_eq!(
        text["text"],
        "# Three\n\nSpeed ![v = s / t](math/v.tex).\n\n![A to B](dia/0001.mmd)\n\n![x^2](math/0001.tex)\n"
    );
    assert_eq!(figure["source"], "flowchart LR\n  A --> B\n");
    assert_eq!(
        figure["used_by"],
        json!([{"chapter": "ch03.md", "line": 5}])
    );
    Ok(())
}

#[test]
fn a_figure_that_cannot_be_drawn_refuses_the_whole_change() -> Result {
    let (_folder, path) = sample()?;
    let (refused, book, stored) = run(&path, |handle| async move {
        let text = "# Three\n\n$\\frac{$\n";
        let refused = call(&handle, Op::AddChapter(add(text))).await;
        let book = call(
            &handle,
            Op::GetBook {
                open_in_editor: false,
            },
        )
        .await;
        let mut anyway = add(text);
        anyway.allow_unrendered = true;
        let stored = call(&handle, Op::AddChapter(anyway)).await;
        (refused, book, stored)
    });
    match refused {
        Err(AgentError::InvalidFigure { kind, line, .. }) => {
            assert_eq!((kind, line), (Kind::Formula, Some(3)));
        }
        other => return Err(format!("{other:?}").into()),
    }
    let book = book?;
    assert_eq!(book["chapters"].as_array().map(Vec::len), Some(2));
    assert_eq!(book["figures"].as_array().map(Vec::len), Some(1));
    assert_eq!(stored?["figures"][0]["path"], "math/0001.tex");
    Ok(())
}

#[test]
fn an_edit_moves_only_the_figures_it_writes() -> Result {
    let (_folder, path) = sample()?;
    let text = run(&path, |handle| async move {
        call(&handle, edit("ch01.md", "End.", "End at $t$.")).await?;
        call(&handle, read("ch01.md")).await
    })?;
    let text = text["text"].as_str().unwrap_or_default();
    assert!(text.contains("It cost $5 and $10."));
    assert!(text.contains("End at ![t](math/0001.tex)."));
    Ok(())
}

#[test]
fn an_edit_that_quotes_nothing_changes_nothing() -> Result {
    let (_folder, path) = sample()?;
    let (refused, text) = run(&path, |handle| async move {
        let refused = call(&handle, edit("ch01.md", "Nowhere", "x")).await;
        (refused, call(&handle, read("ch01.md")).await)
    });
    assert_eq!(refused, Err(AgentError::OldNotFound { index: 0 }));
    assert!(
        text?["text"]
            .as_str()
            .is_some_and(|t| t.starts_with("# Arrival"))
    );
    Ok(())
}

#[test]
fn a_chapter_is_written_whole_only_at_the_revision_read() -> Result {
    let (_folder, path) = sample()?;
    let (stale, fresh) = run(&path, |handle| async move {
        let before = call(&handle, read("ch02.md")).await?;
        let revision = before["revision"].as_str().unwrap_or_default().to_owned();
        handle
            .send(Command::SetChapterText {
                path: "ch02.md".into(),
                text: "# Departure\n\nLater.\n".into(),
            })
            .await
            .map_err(AgentError::from)?;
        let write = |revision: String| {
            Op::WriteChapter(WriteChapter {
                chapter: "ch02.md".into(),
                text: "# Gone\n".into(),
                revision,
                allow_unrendered: false,
            })
        };
        let stale = call(&handle, write(revision)).await;
        let now = call(&handle, read("ch02.md")).await?;
        let fresh = call(
            &handle,
            write(now["revision"].as_str().unwrap_or_default().into()),
        )
        .await;
        Ok::<_, AgentError>((stale, fresh))
    })?;
    assert_eq!(stale.map_err(|e| e.code()), Err("conflict"));
    assert_eq!(fresh?["path"], "ch02.md");
    Ok(())
}

#[test]
fn a_chapter_the_editor_holds_cannot_be_changed_by_an_agent() -> Result {
    let (_folder, path) = sample()?;
    let (typing, released) = run(&path, |handle| async move {
        handle.hold(Some(("ch01.md".into(), 0)));
        let typing = call(&handle, edit("ch01.md", "End.", "Fin.")).await;
        handle.hold(None);
        let released = call(&handle, edit("ch01.md", "End.", "Fin.")).await;
        (typing, released)
    });
    assert_eq!(typing, Err(AgentError::UserTyping));
    assert!(released.is_ok());
    Ok(())
}

#[test]
fn text_the_editor_typed_over_a_change_of_an_agent_is_refused_once() -> Result {
    let (_folder, path) = sample()?;
    let (seen, refused, taken) = run(&path, |handle| async move {
        let (_, seen) = handle.editor_text("ch02.md").await?;
        call(&handle, edit("ch02.md", "Departure", "Leaving"))
            .await
            .map_err(|_| EditError::Ended)?;
        handle.hold(Some(("ch02.md".into(), seen)));
        let mine = || Command::SetChapterText {
            path: "ch02.md".into(),
            text: "# Mine\n".into(),
        };
        let refused = handle.send(mine()).await.map(drop);
        let (_, now) = handle.editor_text("ch02.md").await?;
        handle.hold(Some(("ch02.md".into(), now)));
        let taken = handle.send(mine()).await.map(drop);
        Ok::<_, EditError>((seen, refused, taken))
    })?;
    assert_eq!(seen, 0);
    assert!(matches!(refused, Err(EditError::Conflict)));
    assert!(taken.is_ok());
    Ok(())
}

#[test]
fn a_subscriber_is_told_the_chapters_an_agent_changed() -> Result {
    let (_folder, path) = sample()?;
    let change = run(&path, |handle| async move {
        let mut changes = handle.subscribe();
        handle
            .send(Command::SetChapterText {
                path: "ch01.md".into(),
                text: "# Arrival\n".into(),
            })
            .await
            .map_err(AgentError::from)?;
        call(&handle, edit("ch02.md", "Departure", "Leaving")).await?;
        Ok::<_, AgentError>(changes.next().await)
    })?;
    let change = change.ok_or("no change was told")?;
    assert_eq!(change.chapters.len(), 1);
    assert_eq!(change.chapters[0].0, "ch02.md");
    assert!(change.chapters[0].1 > 0);
    Ok(())
}

#[test]
fn a_chapter_is_moved_renamed_and_removed() -> Result {
    let (_folder, path) = sample()?;
    let (moved, book, removed) = run(&path, |handle| async move {
        let moved = call(
            &handle,
            Op::MoveChapter(MoveChapter {
                chapter: "ch02.md".into(),
                position: Some(0),
                new_path: Some("start.md".into()),
            }),
        )
        .await?;
        let book = call(
            &handle,
            Op::GetBook {
                open_in_editor: false,
            },
        )
        .await?;
        let removed = call(
            &handle,
            Op::RemoveChapter(ChapterArgs {
                chapter: "nope.md".into(),
            }),
        )
        .await;
        Ok::<_, AgentError>((moved, book, removed))
    })?;
    assert_eq!(moved["path"], "start.md");
    assert_eq!(book["chapters"][0]["path"], "start.md");
    assert_eq!(book["chapters"][1]["path"], "ch01.md");
    assert_eq!(removed.map_err(|e| e.code()), Err("no_such_path"));
    Ok(())
}

#[test]
fn a_picture_is_named_after_its_file_and_can_be_the_cover() -> Result {
    let (_folder, path) = sample()?;
    let (named, numbered, cover, book, missing) = run(&path, |handle| async move {
        let picture = |name: Option<&str>, cover| Op::AddPicture {
            path: None,
            name: name.map(str::to_owned),
            avif: PICTURE.to_vec(),
            cover,
        };
        let named = call(&handle, picture(Some("/x/My Pond.JPG"), false)).await?;
        let numbered = call(&handle, picture(Some("/x/a.png"), false)).await?;
        let cover = call(&handle, picture(None, true)).await?;
        let book = call(
            &handle,
            Op::GetBook {
                open_in_editor: false,
            },
        )
        .await?;
        let missing = call(
            &handle,
            Op::RemovePicture(PathArgs {
                path: "img/x.avif".into(),
            }),
        )
        .await;
        Ok::<_, AgentError>((named, numbered, cover, book, missing))
    })?;
    assert_eq!(named["path"], "img/my-pond.avif");
    assert_eq!(numbered["path"], "img/0001.avif");
    assert_eq!(cover["path"], "img/0002.avif");
    assert!(named["width"].as_u64().is_some_and(|width| width > 0));
    assert_eq!(book["meta"]["cover"], "img/0002.avif");
    assert_eq!(book["pictures"][1]["used_by"], json!([]));
    assert_eq!(missing.map_err(|e| e.code()), Err("no_such_path"));
    Ok(())
}

#[test]
fn a_picture_is_given_with_its_file() -> Result {
    let (_folder, path) = sample()?;
    let answer = run(&path, |handle| async move {
        handle
            .agent(Op::Picture(PathArgs {
                path: "img/a.avif".into(),
            }))
            .await
    })?;
    assert_eq!(answer.avif.as_deref(), Some(PICTURE));
    assert_eq!(answer.value["alt"], "The hall");
    Ok(())
}

#[test]
fn a_figure_is_added_changed_and_shown() -> Result {
    let (_folder, path) = sample()?;
    let (added, changed, clash, source) = run(&path, |handle| async move {
        let put = |kind, path: Option<&str>, source: &str, preview| PutFigure {
            kind,
            source: source.into(),
            path: path.map(str::to_owned),
            preview,
            allow_unrendered: false,
        };
        let added = handle
            .agent(Op::PutFigure(put(
                Kind::Diagram,
                None,
                "flowchart TD\n  A --> B\n",
                true,
            )))
            .await?;
        let changed = call(
            &handle,
            Op::PutFigure(put(Kind::Formula, Some("math/v.tex"), "v = d / t", false)),
        )
        .await?;
        let clash = call(
            &handle,
            Op::PutFigure(put(
                Kind::Diagram,
                Some("math/v.tex"),
                "flowchart TD",
                false,
            )),
        )
        .await;
        let source = call(
            &handle,
            Op::ReadFigure(PathArgs {
                path: "math/v.tex".into(),
            }),
        )
        .await?;
        Ok::<_, AgentError>((added, changed, clash, source))
    })?;
    assert_eq!(added.value["path"], "dia/0001.mmd");
    assert_eq!(added.value["render"]["ok"], true);
    assert!(
        added
            .png
            .as_deref()
            .is_some_and(|png| png.starts_with(b"\x89PNG"))
    );
    assert_eq!(changed["path"], "math/v.tex");
    assert_eq!(clash.map_err(|e| e.code()), Err("duplicate_path"));
    assert_eq!(source["source"], "v = d / t");
    Ok(())
}

#[test]
fn a_figure_that_cannot_be_drawn_is_not_put() -> Result {
    let (_folder, path) = sample()?;
    let refused = run(&path, |handle| async move {
        call(
            &handle,
            Op::PutFigure(PutFigure {
                kind: Kind::Formula,
                source: "\\frac{".into(),
                path: None,
                preview: false,
                allow_unrendered: false,
            }),
        )
        .await
    });
    assert_eq!(refused.map_err(|e| e.code()), Err("invalid_figure"));
    Ok(())
}

#[test]
fn a_figure_is_rendered_without_a_book() -> Result {
    let drawn = render(Kind::Formula, "x^2", false)?;
    assert_eq!(drawn.value["ok"], true);
    assert!(drawn.png.is_some());
    assert_eq!(
        render(Kind::Diagram, "not a diagram", true).map_err(|e| e.code()),
        Err("invalid_figure")
    );
    Ok(())
}

#[test]
fn a_patch_of_the_metadata_changes_what_it_names_or_nothing() -> Result {
    let (_folder, path) = sample()?;
    let (meta, refused, after) = run(&path, |handle| async move {
        let meta = call(
            &handle,
            Op::SetMeta(MetaPatch {
                authors: Some(vec!["A. Writer".into(), "B. Writer".into()]),
                series: Patch::Set(Series {
                    name: "Trains".into(),
                    index: Some("2".into()),
                }),
                genres: Some(vec!["travel".into()]),
                publisher: Patch::Set("Rails".into()),
                ..MetaPatch::default()
            }),
        )
        .await?;
        let refused = call(
            &handle,
            Op::SetMeta(MetaPatch {
                publisher: Patch::Remove,
                title: Some(String::new()),
                ..MetaPatch::default()
            }),
        )
        .await;
        let after = call(
            &handle,
            Op::GetBook {
                open_in_editor: false,
            },
        )
        .await?;
        Ok::<_, AgentError>((meta, refused, after))
    })?;
    assert_eq!(meta["authors"], json!(["A. Writer", "B. Writer"]));
    assert_eq!(meta["series"], json!({"name": "Trains", "index": "2"}));
    assert_eq!(meta["genres"], json!(["travel"]));
    assert_eq!(meta["title"], "The Example");
    match refused {
        Err(AgentError::InvalidMeta { field, .. }) => assert_eq!(field, "title"),
        other => return Err(format!("{other:?}").into()),
    }
    assert_eq!(after["meta"]["publisher"], "Rails");
    Ok(())
}

#[test]
fn contents_are_kept_as_a_list_or_follow_the_headings() -> Result {
    let (_folder, path) = sample()?;
    let (list, automatic, neither) = run(&path, |handle| async move {
        let list = call(
            &handle,
            Op::SetContents(SetContents {
                automatic: false,
                entries: Some(vec![EntryView {
                    level: 1,
                    title: "Start".into(),
                    href: "ch01.md#the-station".into(),
                    fault: None,
                }]),
            }),
        )
        .await?;
        let automatic = call(
            &handle,
            Op::SetContents(SetContents {
                automatic: true,
                entries: None,
            }),
        )
        .await?;
        let neither = call(
            &handle,
            Op::SetContents(SetContents {
                automatic: false,
                entries: None,
            }),
        )
        .await;
        Ok::<_, AgentError>((list, automatic, neither))
    })?;
    assert_eq!(list["automatic"], false);
    assert_eq!(
        list["entries"],
        json!([{"level": 1, "title": "Start", "href": "ch01.md#the-station"}])
    );
    assert_eq!(automatic["automatic"], true);
    assert_eq!(automatic["entries"][0]["title"], "Arrival");
    assert_eq!(neither.map_err(|e| e.code()), Err("invalid_arguments"));
    Ok(())
}

#[test]
fn landmarks_are_set_as_a_list() -> Result {
    let (_folder, path) = sample()?;
    let (set, twice) = run(&path, |handle| async move {
        let landmark = |kind: &str| LandmarkView {
            kind: kind.into(),
            title: String::new(),
            href: "ch01.md".into(),
        };
        let set = call(
            &handle,
            Op::SetLandmarks(SetLandmarks {
                landmarks: vec![landmark("start")],
            }),
        )
        .await;
        let twice = call(
            &handle,
            Op::SetLandmarks(SetLandmarks {
                landmarks: vec![landmark("start"), landmark("start")],
            }),
        )
        .await;
        (set, twice)
    });
    assert_eq!(
        set?,
        json!({"landmarks": [{"kind": "start", "title": "", "href": "ch01.md"}]})
    );
    assert_eq!(twice.map_err(|e| e.code()), Err("invalid_landmark"));
    Ok(())
}

#[test]
fn a_book_with_problems_is_checked_and_not_saved() -> Result {
    let (_folder, path) = sample()?;
    let (checked, saved) = run(&path, |handle| async move {
        call(
            &handle,
            edit("ch02.md", "# Departure", "# Departure\n\n[x](nowhere.md)"),
        )
        .await?;
        let checked = call(&handle, Op::Check).await?;
        let saved = call(&handle, Op::Save).await?;
        Ok::<_, AgentError>((checked, saved))
    })?;
    let problem = &checked["problems"][0];
    assert_eq!(problem["chapter"], "ch02.md");
    assert_eq!(
        (problem["line"].clone(), problem["column"].clone()),
        (json!(3), json!(1))
    );
    assert_eq!(saved["saved"], false);
    assert!(
        mbook::Book::open(&path)?
            .chapter("ch02.md")
            .is_some_and(|c| c.text() == "# Departure\n\n![v](math/v.tex)\n")
    );
    Ok(())
}

#[test]
fn a_saved_book_has_the_changes_of_the_agent() -> Result {
    let (_folder, path) = sample()?;
    let saved = run(&path, |handle| async move {
        call(&handle, Op::AddChapter(add("# Three\n\n$x$\n"))).await?;
        call(&handle, Op::Save).await
    })?;
    assert_eq!(saved, json!({"saved": true, "problems": []}));
    let book = mbook::Book::open(&path)?;
    assert!(book.chapter("ch03.md").is_some());
    assert!(book.formula("math/0001.tex").is_some());
    Ok(())
}

#[test]
fn discarding_reads_the_book_from_its_file_and_tells_every_chapter() -> Result {
    let (_folder, path) = sample()?;
    let (book, told) = run(&path, |handle| async move {
        let mut changes = handle.subscribe();
        call(&handle, Op::AddChapter(add("# Three\n"))).await?;
        drop(changes.next().await);
        handle.discard().await.map_err(AgentError::from)?;
        let book = call(
            &handle,
            Op::GetBook {
                open_in_editor: false,
            },
        )
        .await?;
        Ok::<_, AgentError>((book, changes.next().await))
    })?;
    assert_eq!(book["chapters"].as_array().map(Vec::len), Some(2));
    assert_eq!(book["unsaved"], false);
    assert_eq!(told.map(|change| change.chapters.len()), Some(2));
    Ok(())
}

#[test]
fn a_call_on_a_book_that_did_not_open_fails() -> Result {
    let folder = tempfile::tempdir()?;
    let missing = folder.path().join("missing.mbook");
    let failed = run(
        &missing,
        |handle| async move { call(&handle, Op::Check).await },
    );
    assert_eq!(failed.map_err(|e| e.code()), Err("open_failed"));
    Ok(())
}

#[test]
fn a_call_on_a_path_the_book_does_not_have_fails_without_a_change() -> Result {
    let (_folder, path) = sample()?;
    let codes = run(&path, |handle| async move {
        let missing = || PathArgs {
            path: "nope.avif".into(),
        };
        let mut codes = Vec::new();
        for op in [
            read("nope.md"),
            edit("nope.md", "a", "b"),
            Op::WriteChapter(WriteChapter {
                chapter: "nope.md".into(),
                text: String::new(),
                revision: "0".into(),
                allow_unrendered: false,
            }),
            Op::MoveChapter(MoveChapter {
                chapter: "nope.md".into(),
                position: None,
                new_path: None,
            }),
            Op::ReadFigure(missing()),
            Op::Picture(missing()),
            Op::RemovePicture(missing()),
            Op::RemoveFigure(missing()),
        ] {
            codes.push(call(&handle, op).await.map_err(|error| error.code()));
        }
        let book = call(
            &handle,
            Op::GetBook {
                open_in_editor: false,
            },
        )
        .await;
        (codes, book)
    });
    let (codes, book) = codes;
    assert!(
        codes.iter().all(|code| *code == Err("no_such_path")),
        "{codes:?}"
    );
    assert_eq!(book?["unsaved"], false);
    Ok(())
}

#[test]
fn arguments_that_do_not_go_together_are_refused() -> Result {
    let (_folder, path) = sample()?;
    let codes = run(&path, |handle| async move {
        let contents = |automatic, entries: Option<Vec<EntryView>>| {
            Op::SetContents(SetContents { automatic, entries })
        };
        let entry = |level: u8, title: &str| EntryView {
            level,
            title: title.into(),
            href: "ch01.md".into(),
            fault: None,
        };
        let mut far = add("# Far\n");
        far.position = Some(9);
        let mut codes = Vec::new();
        for op in [
            contents(true, Some(vec![entry(1, "A")])),
            contents(false, Some(vec![entry(0, "A")])),
            contents(false, Some(vec![entry(1, "")])),
            Op::Search(Search {
                query: String::new(),
                regex: false,
                limit: None,
            }),
            Op::AddChapter(far),
        ] {
            codes.push(call(&handle, op).await.map_err(|error| error.code()));
        }
        codes
    });
    assert_eq!(
        codes,
        [
            Err("invalid_arguments"),
            Err("invalid_arguments"),
            Err("invalid_arguments"),
            Err("invalid_arguments"),
            Err("invalid_path"),
        ]
    );
    Ok(())
}

#[test]
fn an_empty_list_of_contents_follows_the_headings() -> Result {
    let (_folder, path) = sample()?;
    let contents = run(&path, |handle| async move {
        call(
            &handle,
            Op::SetContents(SetContents {
                automatic: false,
                entries: Some(Vec::new()),
            }),
        )
        .await
    })?;
    assert_eq!(contents["automatic"], true);
    Ok(())
}

#[test]
fn a_contents_entry_that_leads_nowhere_says_so() -> Result {
    let (_folder, path) = sample()?;
    let book = run(&path, |handle| async move {
        call(
            &handle,
            Op::SetContents(SetContents {
                automatic: false,
                entries: Some(vec![EntryView {
                    level: 1,
                    title: "Lost".into(),
                    href: "ch01.md#nowhere".into(),
                    fault: None,
                }]),
            }),
        )
        .await?;
        call(
            &handle,
            Op::GetBook {
                open_in_editor: false,
            },
        )
        .await
    })?;
    assert_eq!(
        book["contents"]["entries"][0]["fault"],
        "leads to no chapter or heading of the book"
    );
    Ok(())
}

#[test]
fn a_new_chapter_that_is_refused_leaves_none_of_its_figures() -> Result {
    let (_folder, path) = sample()?;
    let (refused, book) = run(&path, |handle| async move {
        let mut taken = add("```mermaid\n%% alt: x\nflowchart TD\n```\n");
        taken.path = Some("ch01.md".into());
        let refused = call(&handle, Op::AddChapter(taken)).await;
        (
            refused,
            call(
                &handle,
                Op::GetBook {
                    open_in_editor: false,
                },
            )
            .await,
        )
    });
    assert_eq!(refused.map_err(|error| error.code()), Err("duplicate_path"));
    assert_eq!(book?["figures"].as_array().map(Vec::len), Some(1));
    Ok(())
}

#[test]
fn a_chapter_put_at_a_position_and_its_named_lines_are_listed() -> Result {
    let (_folder, path) = sample()?;
    let book = run(&path, |handle| async move {
        let mut first = add("# Zero\n\n[fig-0]: #fig-0\nA line.\n");
        first.path = Some("ch00.md".into());
        first.position = Some(0);
        call(&handle, Op::AddChapter(first)).await?;
        call(
            &handle,
            Op::GetBook {
                open_in_editor: false,
            },
        )
        .await
    })?;
    assert_eq!(book["chapters"][0]["path"], "ch00.md");
    assert_eq!(book["chapters"][0]["anchors"], json!(["fig-0"]));
    Ok(())
}

#[test]
fn a_figure_is_removed_and_a_figure_of_another_kind_is_not_changed() -> Result {
    let (_folder, path) = sample()?;
    let (removed, kept) = run(&path, |handle| async move {
        let put = PutFigure {
            kind: Kind::Diagram,
            source: "flowchart TD\n".into(),
            path: Some("dia/x.mmd".into()),
            preview: false,
            allow_unrendered: false,
        };
        call(&handle, Op::PutFigure(put)).await?;
        let formula = PutFigure {
            kind: Kind::Formula,
            source: "x".into(),
            path: Some("dia/x.mmd".into()),
            preview: false,
            allow_unrendered: false,
        };
        let kept = call(&handle, Op::PutFigure(formula)).await;
        let removed = call(
            &handle,
            Op::RemoveFigure(PathArgs {
                path: "dia/x.mmd".into(),
            }),
        )
        .await?;
        Ok::<_, AgentError>((removed, kept))
    })?;
    assert_eq!(removed, json!({}));
    assert_eq!(kept.map_err(|error| error.code()), Err("duplicate_path"));
    Ok(())
}

#[test]
fn a_figure_that_cannot_be_drawn_is_stored_when_asked() -> Result {
    let (_folder, path) = sample()?;
    let put = run(&path, |handle| async move {
        call(
            &handle,
            Op::PutFigure(PutFigure {
                kind: Kind::Formula,
                source: "\\frac{".into(),
                path: None,
                preview: true,
                allow_unrendered: true,
            }),
        )
        .await
    })?;
    assert_eq!(put["render"]["ok"], false);
    assert_eq!(put["path"], "math/0001.tex");
    Ok(())
}

#[test]
fn a_series_and_a_publisher_are_removed_by_null() -> Result {
    let (_folder, path) = sample()?;
    let meta = run(&path, |handle| async move {
        call(
            &handle,
            Op::SetMeta(MetaPatch {
                series: Patch::Set(Series {
                    name: "S".into(),
                    index: None,
                }),
                publisher: Patch::Set("P".into()),
                ..MetaPatch::default()
            }),
        )
        .await?;
        call(
            &handle,
            Op::SetMeta(MetaPatch {
                series: Patch::Remove,
                publisher: Patch::Remove,
                ..MetaPatch::default()
            }),
        )
        .await
    })?;
    assert_eq!(meta["series"], Value::Null);
    assert_eq!(meta["publisher"], Value::Null);
    Ok(())
}

#[test]
fn a_problem_of_a_formula_is_placed_in_its_source() -> Result {
    let (_folder, path) = sample()?;
    let checked = run(&path, |handle| async move {
        call(
            &handle,
            edit(
                "ch02.md",
                "![v](math/v.tex)",
                "![v](math/v.tex)\n\n[x](math/v.tex)",
            ),
        )
        .await?;
        call(&handle, Op::Check).await
    })?;
    assert!(
        checked["problems"]
            .as_array()
            .is_some_and(|problems| !problems.is_empty())
    );
    Ok(())
}
