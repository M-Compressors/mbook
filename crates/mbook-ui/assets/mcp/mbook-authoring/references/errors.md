# Errors

A tool that fails answers with `isError: true` and an object with `code`,
`message` and, for some codes, more fields. Nothing was changed by a call
that failed.

| `code` | Means | Do |
| --- | --- | --- |
| `no_such_book` | No book of the library has this key | Call `list_books` |
| `no_such_path` | No chapter, picture or figure has this path | Call `get_book` |
| `invalid_path` | The path breaks a rule of paths | See `dialect` |
| `duplicate_path` | Something already has this path | Choose another, or leave `path` out |
| `invalid_text` | A character a chapter may not hold, at `line` and `column` | Remove it |
| `invalid_meta` | A value of `field` breaks a rule | See `structure` |
| `invalid_landmark` | A kind is not of the right form, or there is more than one `start` | See `structure` |
| `old_not_found` | The `old` of edit `index` is not in the text | Read the chapter again; diagrams and formulas are references there |
| `old_ambiguous` | The `old` of edit `index` is there `count` times | Give more of the text around it, or set `replace_all` |
| `conflict` | The chapter changed after the revision you gave | Read it again and redo the change |
| `user_typing` | The person is typing in this chapter | Wait a few seconds and try again |
| `invalid_picture` | The picture cannot be read, or is too large | Give another file |
| `invalid_figure` | A diagram or formula cannot be drawn: `kind`, `line`, `message` | Fix the source; `render_figure` tries one |
| `declined` | The person declined in the dialog | Do not ask again unless they say so |
| `limit` | A limit of the application was exceeded | Make the input smaller |
| `open_failed` | The book could not be opened | Tell the person |
| `save_failed`, `io` | A file could not be written or read | Tell the person, with `message` |

## Problems

`check_book`, `save_book` and every change of a chapter also answer with
`problems`: what makes the book not yet writable. They are not errors of the
call. Each has a `message`, and `chapter`, `line` and `column` when it is in
a chapter. A book is saved only when it has none.

## Revisions

`read_chapter` gives a `revision`. `write_chapter` needs it, and refuses
with `conflict` when the chapter changed since: someone else, or another
call of yours, changed it. `edit_chapter` needs none: it checks that each
`old` is in the text as it is now.
