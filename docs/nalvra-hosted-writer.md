# NALVRA ONE hosted Writer prototype

Status: 2026-10-11. Task: NALVRA-01a12b98. Based on upstream
34511263a78c2fdd66eb01af5b4bc871cd87364c.

The optional hosted UI sends New, Open, Save, Save As, Close, DOCX export and
PDF/print actions to the NALVRA host. These actions do not perform native file
writes or clear the dirty state. Only a matching saved-document acknowledgement
clears it. Non-hosted Wordcraft keeps its existing file commands.

The host provides a validated RGB palette for menus, ribbons, canvas, inputs,
text and accents. Dark/light changes and personal accent colours follow the
workspace; document formatting remains independent. The hosted German catalogue
translates the primary menus, with English fallback for advanced features.

The same-origin bounded bridge opens complete DOCX bytes, converts registered
input formats to DOCX without replacing the active session, exposes snapshots
and generates PDFs. NALVRA owns document persistence, access checks, unsaved
change prompts and Filer/Spaces selection. Reopening from document JSON is
unsupported because it does not retain the complete media package.

Validation: two focused hosted command/catalogue tests passed. Formatting,
Clippy, asset attribution, dependency layering and the WASM build passed.
The complete cargo xtask ci run still fails the two baseline layout tests for
hidden/deleted text; both failures were reproduced on unchanged upstream.
This fork is an isolated integration prototype, without a production release.

The private NALVRA browser corpus, PDFs and runtime output are kept outside this
fork. Its real Microsoft-connected SharePoint acceptance is still outstanding.
No customer files, credentials or compiled WASM assets are committed here.
