# Appearance and accessibility

The colours, type and motion of the interface, and what the application
promises to someone who uses it without sight, without a pointer or without
fine motor control. Where the first version gives less than this page asks
is listed under [Status and verification](status.md#what-is-implemented).

## Visual direction

The covers are the only saturated colour of the library; the reader shows
nothing but the page. One accent marks what is selected and how far a book
has been read.

| Token | Light | Dark | Used for |
| --- | --- | --- | --- |
| Canvas | `#F4F5F7` | `#15161A` | The background of the library |
| Surface | `#FFFFFF` | `#1E2026` | Sidebars, sheets, menus |
| Ink | `#16181D` | `#ECEDEF` | Text |
| Ink, muted | `#5F6673` | `#9AA0AB` | Authors, captions |
| Line | `#E3E5EA` | `#2C2F37` | Separators, which carry no meaning |
| Outline | `#7C8390` | `#747B89` | The border of a field, the track of a progress line, the edge of a control |
| Iris (accent) | `#4650D8` | `#8C93FF` | Selection, progress, the primary button |

| Page tone | Background | Text | Muted | Link | Selection background | Search background |
| --- | --- | --- | --- | --- | --- | --- |
| Paper | `#F7F4ED` | `#2B2926` | `#69645C` | `#533C87` | `#DED5F1` | `#FFEA9E` |
| Neutral | `#FAFAF8` | `#262626` | `#626262` | `#4D458E` | `#DED9F2` | `#FFEA9E` |
| Sepia | `#F1E6CF` | `#3B3026` | `#6B5B49` | `#56336F` | `#D9C8E2` | `#FFEA9E` |
| Night | `#1E2024` | `#D8D7D3` | `#A6A8AD` | `#BCA9EA` | `#44375D` | `#403214` |
| High contrast | `#FFFFFF` | `#000000` | `#505050` | `#000080` | `#FFFF00` | `#FFD880` |

- **Type.** The book uses its chosen typeface. Bundled Literata and Noto
  Sans include true regular, medium, bold and the corresponding italics.
  System uses the platform UI family. Older installed family choices are
  retained. Paragraphs align at the start with no indent. Semantic emphasis
  comes from the Markdown source.
- **Measure.** A single centered column uses 60 advances of the family’s
  zero glyph at the current weight and size. Compact windows use the
  available width. The default line height is 1.6.

- **Icon.** An open book on the navy of these design documents:
  `crates/mbook-ui/assets/icon.svg`, drawn once and rendered to the PNG the
  bundle and the Windows executable take.
- **Motion** answers an action: a screen is pushed, a sheet rises, the
  controls of the reader fade and page turns slide and fade. The system
  setting that reduces motion turns the transitions off.

## Appearance

- The local catalogue stores reading preferences. At the first reader
  opening, it chooses 20 logical px below 600 px or 22 px otherwise and
  saves that value. Resizing and changing window orientation keep the value.
  Catalogues with a previously saved size keep it, including legacy sizes
  below 16 px. A saved Dusk choice uses Night.
- Startup registers the local faces before opening a window. If a
  requested family is missing, the first available bundled face or the
  platform system family is used. The stored family name is retained.
- Both bundled families ship as native TrueType instances in 400, 500 and
  700, upright and italic. GPUI Fast's font matcher selects discrete faces.
  Literata uses the upstream 12-point optical cut, and Noto Sans uses its
  standard width. The renderer makes no per-size optical axis adjustment.
- The page tone is scoped to the reading surface. The interface projects
  the matching light or dark chrome. Reading tones use opaque colors.
- The renderer paints selection and search backgrounds under the existing
  glyph colors. Ordinary page text meets 7:1 in both states. Selected links
  retain their underline and link color, which achieves at least 4.5:1
  against the selection and search backgrounds.

## Accessibility

The application conforms to WCAG 2.2 at level AA, read for a native
application. GPUI reports its elements to the platform through AccessKit; an
element is reported when it has an identifier and a role.

**What assistive technology is told**

- Every control must have a role and a name. A control that shows only
  an icon gets its name as a label, and shows the same words as a tooltip.
- The name of a book on the shelf is its title, its authors and its state:
  the percentage read, `new`, or the step of its import. The progress line
  under a cover is not a separate element.
- A state that colour shows must also be in the name or in text beside
  it: selected, current chapter, unsaved, failed, has problems.
- The text of a chapter is reported with its structure, which the Markdown
  view of GPUI Kit provides: headings with their level, lists, tables,
  links, code.
- A picture of a book is reported as an image named by its alternative
  text. A picture with an empty alternative text is not reported.
- The language of a book, from its metadata, is given to the elements that
  show its text, so that a screen reader speaks it in the right voice.
- A change the user did not cause in the focused element must be
  announced: an import that finished or failed, problems found by a save, an
  update that is ready. A change that follows from an action is announced by
  the element that changed. GPUI has no live regions: an announcement is a
  notification with the alert role, and text that changes in place has the
  status role. An announcement stays until it is dismissed: by its close
  button, which a pointer reveals, or by a tap or a sideways swipe on a
  touch screen. Notifications are as wide as the window allows, up to
  382 px, and start below the system bars of a phone.
- An identifier of an element must stay the same while the element
  means the same thing: the identifier of a book is its key, of a chapter
  its path. Assistive technology then does not hear a list as replaced when
  it is redrawn.

**Keyboard and focus**

- Every action must be reachable from the keyboard alone, and no
  element may hold the focus without a way to leave it by key.
- The order of focus follows the order of reading: sidebar, then content,
  then panels.
- The focused element must show an outline of at least 2 px in the
  accent colour, with a contrast of 3:1 against what is beside it. The
  outline appears for focus that came from the keyboard.
- A dialog, a sheet and a popover keep the focus inside while they are
  open, close with Escape, and return the focus to the control that opened
  them.
- In the Markdown editor Tab indents. Escape followed by Tab leaves the
  editor, and the editor says so in its description.
- Opening a screen moves the focus to its heading or its first control;
  going back returns it to the control that led there.
- Keys are bound through the key map of GPUI, so that they are listed in the
  menus and can be changed in one place.

| Key | Action |
| --- | --- |
| Tab, Shift+Tab | Next and previous control |
| Arrows | Move inside a grid, a list or a menu |
| Return, Space | Activate; in the reader, Space turns a page |
| Escape | Close a panel; leave the editor; go back |
| The platform's Open, Save, Find, Settings | The same in the application |
| Page Up, Page Down, Home, End | Move in a chapter |
| The platform's bracket keys | Previous and next chapter |

**Seeing**

- Text must have a contrast of at least 4.5:1 against its background,
  and the border or glyph that identifies a control at least 3:1. The
  tokens of [Visual direction](#visual-direction) meet this in both
  appearances and in every page tone; a test computes it.
- The colour `Line` is for decoration only. Whatever must be seen to use a
  control uses `Outline`.
- The size of the interface is one setting that scales every length of the
  interface. The size class is computed from the width of the window
  divided by that scale, so that a scaled window reflows instead of
  clipping.
- The text of a book is never justified and never set narrower than the
  reader chose. Its size, line height and typeface are settings; no text is
  drawn as a picture.
- Nothing flashes.

**Pointer and touch**

- A control is at least 24 px in both directions for a pointer and 44 px on
  a touch device.
- Whatever a drag does must also be possible without one: a chapter has
  "Move up" and "Move down".
- Whatever a hover or a long press reveals must also have a visible
  control or a menu entry.

**Time and motion**

- The controls of the reader hide only while nothing inside them has the
  focus and the pointer is not over them, and never when the setting keeps
  them shown. They return on any key. The fade and the page turn animation
  are skipped when reduced motion is active.
- No message disappears by itself while it still asks for an action.
- When the system, or the setting, asks for reduced motion, every
  transition is replaced by an immediate change. GPUI Kit reads the system
  setting on macOS, Windows and Linux.

**Words**

- An error names what went wrong and what can be done, in text, at the
  control it belongs to.
- A field of a form has a visible label, which is also its name.
