# Setup context attachments

Accepted product decision (2026-10-18). Describing a mod can optionally go
further than a few sentences: the user may add documents and images while
describing a new mod or reviewing an existing one.

## What the user can add

- **Documents** (up to 12): Markdown, text, CSV, JSON, YAML, TOML, HOI4
  script/interface text, Lua, Python, JavaScript/TypeScript, HTML, or XML, each
  at most 1 MiB of UTF-8 text. Each document can be used as context for the
  setup assistant (on by default) and, when it is at most 256 KiB and nothing
  was redacted, copied into the project at `docs/reference/<name>` (off by
  default).
- **Images** (up to 12): PNG, JPEG, or WebP, each at most 12 MiB. One image can
  become the mod thumbnail. Any image can be saved as a visual reference at
  `docs/visual_direction/<name>.jpg` (on by default).

The section is collapsed by default on the "Describe the mod" screen and the
existing-mod Review screen. Nothing is required.

## Read boundary

Files are read only after the user picks them in the native file dialog; the
app never browses for them. Rust reads each pick without following links,
refuses non-regular files, oversized files, unsupported extensions, non-UTF-8
documents, and images whose bytes are not PNG, JPEG, or WebP, and reports each
refusal by file name. Accepted attachments live in a session store keyed by an
app identifier and content hash; nothing about them is written to the project
until an approved plan installs a copy.

## Analysis input

A document becomes one bounded excerpt (at most 12,000 characters). Each line
passes the profile redaction used for scan evidence: machine-local paths and
credential-shaped text are replaced by markers, so the excerpt is a redaction
fixed point. The excerpt reaches the setup assistant as approved evidence with
a reserved `@context/<n>-<name>` path and a `context-<n>` reference, never the
file's real location. Rust binds every `@context/` item to the session store by
reference, path, and excerpt hash; scan approval still covers only scan
evidence, and maintenance reanalysis still requires approved scan evidence. The
prompt treats these excerpts as untrusted supporting material that never
overrides detected project facts. Images are not sent to the setup assistant.

## Installed files

- The thumbnail is rendered in the interface with a cover crop to a 600x600
  PNG. Rust accepts it only for an image picked this session and only if it
  decodes as a PNG within the thumbnail bounds, then uses it in place of the
  generated placeholder. An existing mod's current `thumbnail.png` goes through
  the normal keep/replace review.
- Visual references are rendered in the interface to JPEG with the longest
  edge at most 1600 px. Rust accepts PNG or JPEG bytes up to 1 MiB each.
- Document copies come from the bytes Rust read at pick time, never from the
  interface.
- All copies together are at most 4 MiB, because generated files are recorded
  in the installation lock for repair. Copies are generated files of the
  `project.context_reference` and `project.visual_direction` components and use
  the normal staged, journaled, reversible transaction, conflict review, repair,
  and removal.

## Not covered

Images are not analysed by the setup assistant, and no provider receives image
bytes. Documents in other encodings, PDFs, and office formats are refused.
