//! What touches files: choosing and encoding pictures, decoding the cover,
//! and the library's row after a save. Everything blocking runs on the
//! background executor.

use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{AppContext as _, Context, PathPromptOptions, RenderImage, Window};

use super::EditorPage;
use super::commands::{ChapterOp, Origin};
use super::encode::{Encoded, Job, Target, encode_file, words};
use super::prompts::{PathRequest, ask_path};
use super::session::{Control, line_and_column, propose_chapter_path, propose_picture_path};
use crate::entities::catalogue::{BookRow, Catalogue, CatalogueError, Store};
use crate::entities::picture::{PictureError, decode_avif};
use crate::features::edit_book::command::Command;
use crate::features::import_book::refresh_thumbnail;
use crate::shared::model::BookKey;
use crate::shared::roots::Roots;
use crate::shared::ui::announce;

/// The widest the cover is decoded for the screen, in pixels.
const COVER_WIDTH: u32 = 360;
/// The most pixels the cover of the book may have for the screen.
const COVER_MAX_PIXELS: u64 = 100_000_000;

impl EditorPage {
    /// Asks the platform for picture files, then queues them for encoding.
    pub(super) fn choose_pictures(
        &mut self,
        target: Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: target == Target::Picture,
            prompt: Some("Add".into()),
        });
        let control = control_of(target);
        self.chooser = Some(cx.spawn_in(window, async move |this, cx| {
            let paths = match chosen.await {
                Ok(Ok(Some(paths))) => paths,
                Ok(Ok(None)) | Err(_) => return,
                Ok(Err(_)) => {
                    this.update_in(cx, |this, _, cx| {
                        this.session.set_error(
                            control,
                            "The file dialog could not be opened. Drop the file on the window instead."
                                .to_owned(),
                        );
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            this.update_in(cx, |this, window, cx| {
                this.encode_files(paths, target, window, cx);
            })
            .ok();
        }));
    }

    /// Queues picture files for encoding, one at a time.
    pub(super) fn encode_files(
        &mut self,
        files: Vec<PathBuf>,
        target: Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.session.clear_error(&control_of(target));
        for file in files {
            self.session.encoding += 1;
            self.encoder.queue.push_back(Job { file, target });
        }
        self.encode_next(window, cx);
        cx.notify();
    }

    /// Starts the next file, when none is being encoded.
    fn encode_next(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.encoder.running.is_some() {
            return;
        }
        let Some(job) = self.encoder.queue.pop_front() else {
            return;
        };
        let file = job.file.clone();
        let done = job.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    encode_file(&file).map_err(|error| words(&name_of(&file), &error))
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.encoded(&done, result, window, cx);
            })
            .ok();
        });
        self.encoder.running = Some((job, task));
    }

    /// A file was encoded, or could not be.
    fn encoded(
        &mut self,
        job: &Job,
        result: Result<Vec<u8>, String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.encoder.running = None;
        self.session.encoding = self.session.encoding.saturating_sub(1);
        match result {
            Ok(avif) => self.encoder.ready.push_back(Encoded {
                name: job.name(),
                target: job.target,
                avif,
            }),
            Err(message) => self.session.set_error(control_of(job.target), message),
        }
        self.encode_next(window, cx);
        self.prompt_next(window, cx);
        self.continue_save(window, cx);
        cx.notify();
    }

    /// Asks for the path of the next picture that is ready.
    pub(super) fn prompt_next(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.encoder.prompting {
            return;
        }
        let Some(picture) = self.encoder.ready.pop_front() else {
            return;
        };
        self.encoder.prompting = true;
        let proposal = propose_picture_path(&picture.name, self.session.pictures());
        let title = match picture.target {
            Target::Picture => "Add a picture",
            Target::Cover => "Replace the cover",
        };
        ask_path(
            &PathRequest {
                title,
                label: "Path of the picture in the book",
                initial: proposal,
                ok: "Add",
            },
            window,
            cx,
            move |this, path, window, cx| {
                this.encoder.prompting = false;
                let command = match picture.target {
                    Target::Picture => (
                        Command::AddPicture {
                            path,
                            avif: picture.avif.clone(),
                        },
                        Origin::AddPicture,
                    ),
                    Target::Cover => (
                        Command::SetCover {
                            path,
                            avif: picture.avif.clone(),
                        },
                        Origin::SetCover,
                    ),
                };
                this.send(command.0, command.1, window, cx);
                this.prompt_next(window, cx);
            },
            |this, window, cx| {
                this.encoder.prompting = false;
                this.prompt_next(window, cx);
            },
        );
    }

    /// Asks for the path of a new chapter.
    pub(super) fn add_chapter(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let initial = propose_chapter_path(self.session.chapters());
        ask_path(
            &PathRequest {
                title: "Add a chapter",
                label: "Path of the chapter in the book",
                initial,
                ok: "Add",
            },
            window,
            cx,
            |this, path, window, cx| {
                this.flush_text(window, cx);
                this.send(
                    Command::AddChapter {
                        path: path.clone(),
                        text: String::new(),
                    },
                    Origin::Chapter(ChapterOp::Add(path)),
                    window,
                    cx,
                );
            },
            |_, _, _| {},
        );
    }

    /// Asks for another path for a chapter.
    pub(super) fn change_path(from: String, window: &mut Window, cx: &mut Context<'_, Self>) {
        ask_path(
            &PathRequest {
                title: "Change the path",
                label: "New path of the chapter",
                initial: from.clone(),
                ok: "Change",
            },
            window,
            cx,
            move |this, to, window, cx| {
                if to == from {
                    return;
                }
                this.flush_text(window, cx);
                this.send(
                    Command::MoveChapterPath {
                        from: from.clone(),
                        to: to.clone(),
                    },
                    Origin::Chapter(ChapterOp::Rename {
                        from: from.clone(),
                        to,
                    }),
                    window,
                    cx,
                );
            },
            |_, _, _| {},
        );
    }

    /// Removes a chapter, after the user agreed.
    pub(super) fn remove_chapter(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.flush_text(window, cx);
        self.send(
            Command::RemoveChapter { path: path.clone() },
            Origin::Chapter(ChapterOp::Remove(path)),
            window,
            cx,
        );
    }

    /// Removes a picture, after the user agreed.
    pub(super) fn remove_picture(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.send(
            Command::RemovePicture { path },
            Origin::RemovePicture,
            window,
            cx,
        );
    }

    /// Decodes the cover for the screen when it is another picture than the
    /// one shown.
    pub(super) fn refresh_cover(&mut self, cx: &mut Context<'_, Self>) {
        let wanted = self.session.meta().and_then(|meta| meta.cover.clone());
        // A cover is always replaced under a new path, so the path says
        // whether the picture changed.
        if wanted == self.cover.path {
            return;
        }
        self.cover.generation += 1;
        let generation = self.cover.generation;
        self.cover.path.clone_from(&wanted);
        self.cover.image = None;
        self.cover.failed = false;
        let Some(path) = wanted else {
            self.cover.task = None;
            return;
        };
        let reply = self.handle.picture(&path);
        self.cover.task = Some(cx.spawn(async move |this, cx| {
            let image = match reply.await {
                Ok(bytes) => cx
                    .background_spawn(async move { decode_cover(&bytes) })
                    .await
                    .ok(),
                Err(_) => None,
            };
            this.update(cx, |this, cx| {
                if this.cover.generation == generation {
                    this.cover.failed = image.is_none();
                    this.cover.image = image;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// Asks the book for the text of the chapters the problems are in, so
    /// that the problems can say line and column.
    pub(super) fn refresh_places(&mut self, cx: &mut Context<'_, Self>) {
        let mut wanted: Vec<(String, Vec<u32>)> = Vec::new();
        for problem in self.session.problems() {
            if let (Some(path), Some(offset)) = (&problem.path, problem.offset) {
                match wanted.iter_mut().find(|(known, _)| known == path) {
                    Some((_, offsets)) => offsets.push(offset),
                    None => wanted.push((path.clone(), vec![offset])),
                }
            }
        }
        if wanted.is_empty() {
            self.places_task = None;
            return;
        }
        let replies: Vec<_> = wanted
            .iter()
            .map(|(path, _)| self.handle.chapter_text(path))
            .collect();
        self.places_task = Some(cx.spawn(async move |this, cx| {
            let mut places = Vec::new();
            for ((path, offsets), reply) in wanted.into_iter().zip(replies) {
                let Ok(text) = reply.await else { continue };
                for offset in offsets {
                    let at = usize::try_from(offset).unwrap_or(usize::MAX);
                    places.push(((path.clone(), offset), line_and_column(&text, at)));
                }
            }
            this.update(cx, |this, cx| {
                this.session.places.extend(places);
                cx.notify();
            })
            .ok();
        }));
    }

    /// The book is on disk: the library's row and the thumbnail are made
    /// again, on the background, then the page says so.
    pub(super) fn after_save(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let store = Store::global(cx);
        let roots = store.roots.clone();
        let catalogue = store.catalogue.clone();
        let key = self.key;
        let leave = self.saving.leave_after;
        self.saving.leave_after = false;
        self.chooser = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { record_save(&roots, &catalogue, key) })
                .await;
            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(()) => announce("Saved", window, cx),
                    Err(error) => {
                        let message = format!(
                            "The book was saved, but the library could not be updated: {error}."
                        );
                        this.session.set_error(Control::Save, message.clone());
                        announce(message, window, cx);
                    }
                }
                cx.emit(super::EditorEvent::Saved(key));
                if leave {
                    cx.emit(super::EditorEvent::Close);
                }
                cx.notify();
            })
            .ok();
        }));
    }
}

/// The control a picture of this kind belongs to.
fn control_of(target: Target) -> Control {
    match target {
        Target::Picture => Control::Pictures,
        Target::Cover => Control::Cover,
    }
}

/// The name of a file without its folders.
fn name_of(file: &std::path::Path) -> String {
    file.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// Decodes the AVIF bytes of a cover for the screen.
fn decode_cover(bytes: &[u8]) -> Result<Arc<RenderImage>, PictureError> {
    Ok(decode_avif(bytes, COVER_MAX_PIXELS)?
        .fit_width(COVER_WIDTH)
        .into_render_image())
}

/// Why the library could not be told about a saved book.
#[derive(Debug, thiserror::Error)]
pub(super) enum FollowUpError {
    /// The saved file could not be opened.
    #[error("the saved file could not be read")]
    Read(#[source] mbook::Error),
    /// The size of the saved file could not be read.
    #[error("the saved file could not be measured")]
    Measure(#[source] std::io::Error),
    /// The catalogue refused the row.
    #[error("the catalogue could not be updated")]
    Catalogue(#[source] CatalogueError),
}

/// Makes the row of the saved book and its thumbnail again. It blocks: run it
/// on the background.
///
/// The row keeps the day the book was added and the format it came from.
///
/// # Errors
///
/// [`FollowUpError`] when the saved file cannot be read or the catalogue
/// cannot be written. A thumbnail that cannot be made is not an error: the
/// shelf draws a placeholder.
pub(super) fn record_save(
    roots: &Roots,
    catalogue: &Catalogue,
    key: BookKey,
) -> Result<(), FollowUpError> {
    let path = roots.book(key);
    let file_len = File::open(&path)
        .and_then(|file| file.metadata())
        .map_err(FollowUpError::Measure)?
        .len();
    let mut reader = mbook::BookReader::open(&path).map_err(FollowUpError::Read)?;
    let before = catalogue.book(key).map_err(FollowUpError::Catalogue)?;
    let (added_ms, source_format) =
        before.map_or((0, None), |row| (row.added_ms, row.source_format));
    let row = BookRow::from_reader(key, &reader, file_len, added_ms, source_format);
    catalogue.put_book(&row).map_err(FollowUpError::Catalogue)?;
    let cover = reader.read_cover().map_err(FollowUpError::Read)?;
    refresh_thumbnail(roots, key, cover.as_deref());
    Ok(())
}
