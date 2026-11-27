//! What touches files: choosing and encoding pictures, decoding the cover,
//! and the library's row after a save. Encoding and decoding run on the pool
//! of `compute`; reading and writing files on the background executor.

use std::borrow::Cow;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{AppContext as _, Context, RenderImage, Window};
use rust_i18n::t;

use super::EditorPage;
use super::commands::{ChapterOp, Origin};
use super::encode::{Job, Placement, Target, encode_file, words};
use super::markup::{chapter_start, picture_markdown};
use super::picture_prompt::ask_picture;
use super::prompts::{PathRequest, ask_path};
use super::session::{Control, line_and_column, propose_chapter_path, propose_picture_path};
use crate::entities::catalogue::{BookRow, Catalogue, CatalogueError, Store};
use crate::entities::picture::{PictureError, decode_avif, picture_file_extensions};
use crate::features::edit_book::command::Command;
use crate::features::import_book::refresh_thumbnail;
use crate::shared::compute;
use crate::shared::file_picker;
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
        let control = control_of(target);
        self.chooser = Some(cx.spawn_in(window, async move |this, cx| {
            let title = match target {
                Target::Cover => t!("editor.files.choose_cover"),
                Target::Picture | Target::Chapter => t!("editor.files.add_pictures"),
            };
            let filter = t!("editor.files.picture_filter");
            let extensions = picture_file_extensions();
            let multiple = target != Target::Cover;
            let paths = match file_picker::choose(&title, &filter, &extensions, multiple).await {
                Ok(Some(paths)) => paths,
                Ok(None) => return,
                Err(error) => {
                    this.update_in(cx, |this, _, cx| {
                        this.session.set_error(control, error.to_string());
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

    /// Offers the path and output settings for each selected file, one at a time.
    pub(super) fn encode_files(
        &mut self,
        files: Vec<PathBuf>,
        target: Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.session.clear_error(&control_of(target));
        self.encoder
            .queue
            .extend(files.into_iter().map(|file| (file, target)));
        self.prompt_next_picture(window, cx);
        cx.notify();
    }

    /// Starts the configured file on the background executor.
    fn encode_picture(&mut self, job: Job, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.session.encoding += 1;
        let file = job.file.clone();
        let options = job.options;
        let done = job.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(compute::spawn(move || {
                    encode_file(&file, options).map_err(|error| words(&name_of(&file), &error))
                }))
                .await;
            this.update_in(cx, |this, window, cx| {
                this.encoded(&done, result, window, cx);
            })
            .ok();
        });
        self.encoder.running = Some((job, task));
        cx.notify();
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
            Ok(avif) => {
                self.encoder.waiting_for_book = true;
                let path = job.path.clone();
                self.encoder.placing = job.placement.as_ref().map(|placement| {
                    let markdown =
                        picture_markdown(&placement.chapter, &path, &placement.description);
                    (placement.chapter.clone(), markdown)
                });
                let (command, origin) = match job.target {
                    Target::Picture | Target::Chapter => {
                        (Command::AddPicture { path, avif }, Origin::AddPicture)
                    }
                    Target::Cover => (Command::SetCover { path, avif }, Origin::SetCover),
                };
                self.send(command, origin, window, cx);
            }
            Err(message) => {
                self.session.set_error(control_of(job.target), message);
                self.prompt_next_picture(window, cx);
            }
        }
        self.continue_save(window, cx);
        cx.notify();
    }

    /// Asks for the output settings of the next selected picture.
    fn prompt_next_picture(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.encoder.prompting || self.encoder.running.is_some() || self.encoder.waiting_for_book
        {
            return;
        }
        let Some((file, target)) = self.encoder.queue.pop_front() else {
            return;
        };
        self.encoder.prompting = true;
        let name = name_of(&file);
        let proposal = propose_picture_path(&name, self.session.pictures());
        // A picture for a chapter goes into the one that is open now.
        let chapter = self.loaded.clone().filter(|_| target == Target::Chapter);
        let title = match (target, &chapter) {
            (Target::Cover, _) => t!("editor.files.replace_cover"),
            (_, Some(_)) => t!("editor.files.add_picture_to_chapter"),
            (_, None) => t!("editor.files.add_picture"),
        };
        ask_picture(
            title,
            name,
            proposal,
            chapter.is_some(),
            window,
            cx,
            move |this, settings, window, cx| {
                this.encoder.prompting = false;
                let placement = chapter.clone().map(|chapter| Placement {
                    chapter,
                    description: settings.description,
                });
                this.encode_picture(
                    Job {
                        file: file.clone(),
                        target,
                        path: settings.path,
                        options: settings.options,
                        placement,
                    },
                    window,
                    cx,
                );
            },
            |this, window, cx| {
                this.encoder.prompting = false;
                let editor = cx.entity();
                window.defer(cx, move |window, cx| {
                    editor.update(cx, |this, cx| this.prompt_next_picture(window, cx));
                });
            },
        );
    }

    /// The book answered about a picture that was encoded: the next file is
    /// asked about. A picture that was added for a chapter is put where the
    /// caret of Visual is, when that chapter is still the one being edited.
    pub(super) fn picture_applied(
        &mut self,
        added: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.encoder.waiting_for_book = false;
        if let Some((chapter, markdown)) = self.encoder.placing.take().filter(|_| added) {
            if self.visual_is_editing(&chapter, cx) {
                self.insert_markdown(markdown, window, cx);
            } else {
                announce(place_later(), window, cx);
            }
        }
        self.prompt_next_picture(window, cx);
    }

    /// Asks for the title of a new chapter and adds it after the last one.
    /// The title becomes its first heading; the file gets the next free name.
    pub(super) fn add_chapter(window: &mut Window, cx: &mut Context<'_, Self>) {
        ask_path(
            &PathRequest {
                title: t!("editor.files.new_chapter").into(),
                label: new_chapter_label().into(),
                initial: String::new(),
                ok: t!("editor.files.new_chapter_ok").into(),
                required: None,
            },
            window,
            cx,
            |this, title, window, cx| {
                this.flush_text(window, cx);
                let path = propose_chapter_path(this.session.chapters());
                this.send(
                    Command::AddChapter {
                        path: path.clone(),
                        text: chapter_start(&title),
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
                title: t!("editor.files.rename").into(),
                label: rename_label().into(),
                initial: from.clone(),
                ok: t!("editor.files.rename_ok").into(),
                required: Some(t!("editor.files.rename_required").into()),
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
                    .background_spawn(compute::spawn(move || decode_cover(&bytes)))
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
                .background_spawn(async move { record_save(&roots, &catalogue, key).await })
                .await;
            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(()) => announce(t!("editor.files.saved"), window, cx),
                    Err(error) => {
                        let message =
                            t!("editor.files.library_not_updated", error = error).into_owned();
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

/// The label of the field that holds the title of a new chapter.
pub(super) fn new_chapter_label() -> Cow<'static, str> {
    t!("editor.files.new_chapter_label")
}
/// The label of the field that holds the new file name of a chapter.
pub(super) fn rename_label() -> Cow<'static, str> {
    t!("editor.files.rename_label")
}
/// What is said when a picture arrives after its chapter was left.
pub(super) fn place_later() -> Cow<'static, str> {
    t!("editor.files.place_later")
}

/// The control a picture of this kind belongs to.
pub(super) fn control_of(target: Target) -> Control {
    match target {
        Target::Picture => Control::Pictures,
        Target::Cover => Control::Cover,
        Target::Chapter => Control::ChapterPicture,
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
pub(crate) enum FollowUpError {
    /// The saved file could not be opened.
    #[error("{}", t!("editor.files.saved_unreadable"))]
    Read(#[source] mbook::Error),
    /// The size of the saved file could not be read.
    #[error("{}", t!("editor.files.saved_unmeasured"))]
    Measure(#[source] std::io::Error),
    /// The catalogue refused the row.
    #[error("{}", t!("editor.files.catalogue_not_updated"))]
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
pub(crate) async fn record_save(
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
    refresh_thumbnail(roots, key, cover).await;
    Ok(())
}
