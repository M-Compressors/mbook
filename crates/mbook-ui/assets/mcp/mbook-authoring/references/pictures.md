# Pictures

A book stores pictures as AVIF. `add_picture` encodes what you give it.

## Adding one

`add_picture` takes `source` in one of three forms:

| Form | Example | Use it for |
| --- | --- | --- |
| A file | `{"file": "/Users/me/photos/pond.jpg"}` | A picture on this computer: AVIF, JPEG, PNG, GIF, WebP or BMP. The path is absolute. The person is asked first. |
| Base64 | `{"base64": "iVBORw0…", "media_type": "image/png"}` | Bytes you already hold. |
| SVG | `{"svg": "<svg xmlns=…>…</svg>"}` | A drawing you write yourself. It is drawn at its own size and then encoded, so give it a `width` and `height` or a `viewBox`. |

- `path` is where it goes, ending in `.avif`. Without it, the picture gets
  `img/` and the name of the file, or the next free number.
- `quality` is 1 to 100; without it, the quality the person chose in the
  editor.
- `max_width` and `max_height` make a large picture smaller, keeping its
  proportions. A picture is never made larger.
- `cover: true` makes it the cover of the book.
- The answer gives the path, the size in pixels and the bytes it takes.

A picture is not in any chapter until a chapter refers to it:

```markdown
![The pond in winter](img/pond.avif "Fig. 3. Patriarch's Ponds")
```

The path in the link is relative to the chapter.

## Knowing what a picture shows

- `get_book` lists every picture with its size, its alternative text and the
  lines that show it.
- `view_picture` gives the picture as PNG, no larger than `max_size` pixels
  on either side, when you can look at pictures.

## Removing one

`remove_picture` removes it after the person agrees. The lines that showed
it are then problems until you change them. A picture that no chapter uses
has an empty `used_by` in `get_book`.

## Limits

A picture of more than 100,000,000 pixels is refused before it is decoded.
