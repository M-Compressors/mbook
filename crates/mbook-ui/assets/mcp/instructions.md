This server is the mbook application: a library of books in the mbook format (Markdown chapters, AVIF pictures, Mermaid diagrams, KaTeX formulas). Its tools read and change those books; every answer is text, so the screen is never needed.

Work in this order: `list_books` or `create_book`; `get_book` for the structure (no text, no pictures); `read_chapter` for text; `edit_chapter`, `write_chapter`, `add_chapter`, `add_picture`, `put_figure`, `set_meta`, `set_contents`, `set_landmarks` to change it; `check_book`; `save_book`. Nothing reaches the file before `save_book`.

Rules: chapters are GitHub Flavored Markdown without raw HTML. Write diagrams as a `mermaid` fence that starts with `%% alt: …` and formulas as `$…$` or `$$…$$`; they become resources of the book and are drawn first. Every picture, diagram and formula needs an alternative text in words. Paths in links are relative to the chapter. On `user_typing`, wait and retry; on `conflict`, read again. Removals and files of the computer (import, export, a picture from a file) are confirmed by the person in a dialog; `declined` means no.

Before writing, call `get_guide` with `overview`, `dialect`, `figures`, `pictures`, `structure` or `errors`, or read the skill `mbook-authoring`.
