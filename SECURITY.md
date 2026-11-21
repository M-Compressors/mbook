# Security Policy

## Supported versions

mbook is pre-release: nothing has been published yet. Security fixes land on
`main`. Once releases exist, fixes will ship in a new patch release of the
latest minor version.

## What counts as a vulnerability

An mbook file is untrusted input. `mbook` forbids `unsafe`, so memory
corruption there should be impossible; a report that shows otherwise is
especially welcome. Please report privately anything that lets a file cause:

- a panic, abort, or infinite loop in `BookReader`, `Book::read` or any other
  API that accepts a file;
- memory or CPU use beyond the `Limits` the caller configured, such as a
  table or a resource whose declared size escapes them;
- a path accepted by the reader or the writer that would leave the book's
  folder when resources are unpacked (`..`, a leading `/`, a backslash);
- content that silently differs from what was written, without an error.

A flaw in the format itself, such as a rule of
[RFC 0001](rfcs/src/0001-mbook-format.md) that cannot be implemented safely,
is in scope as well.

Not vulnerabilities:

- Bugs in an AVIF decoder. `mbook` returns picture bytes undecoded; the
  decoder belongs to the reading application.
- A book that the writer refuses although it is valid, or accepts although a
  rule forbids it, when the result is harmless. Open a regular issue.

## Reporting

Use GitHub's private vulnerability reporting on the repository. Include the
file (or the bytes in hex), the API and options used, and the observed
behaviour.
