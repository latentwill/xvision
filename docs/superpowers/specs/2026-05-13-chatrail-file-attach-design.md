# ChatRail File Attach — Design

**Date:** 2026-05-13
**Surface:** ChatRail only
**Status:** Draft for user review

## Goal

Add a narrow file-attachment feature to the persistent ChatRail so the user can attach one text-like file from the browser and have its contents available to the LLM for that chat turn.

This is a ChatRail-only feature. It is not a general dashboard upload system, not a reusable workspace artifact system, and not a persistent attachment-history project.

## Scope

This design covers:

- the ChatRail composer UI for selecting and clearing one file
- the dashboard backend route changes needed to accept the upload
- turn-scoped injection of file contents into the LLM request
- user-visible validation and failure behavior
- tests for the backend route, UI behavior, and attachment limits

This design does not cover:

- setup wizard attachments
- multi-file upload
- image, PDF, or binary ingestion
- persistent attachment records
- cross-session file reuse
- arbitrary server-path attachment

## Product Rules

1. The feature exists only in ChatRail.
2. v1 accepts browser upload only; the user does not point ChatRail at server filesystem paths.
3. v1 accepts one file per turn.
4. v1 accepts text-like files only.
5. The file is available to the model for that request only.
6. The backend must not silently persist raw uploaded file contents into chat-session history.
7. Failures must be explicit in the rail UI before the model call begins.

## User Experience

### Composer behavior

The ChatRail composer gains a lightweight attach affordance next to the existing text input and send button.

The intended flow:

1. User picks one file from the browser.
2. The composer shows a small attachment chip with filename and a remove action.
3. User sends the message.
4. The request streams as normal.
5. After the request completes or fails, the selected file is cleared from the composer.

The attachment chip is composer state, not session state. Navigating away, starting fresh, or reloading should not restore the selected file.

### Transcript behavior

The transcript should make it clear that a file was attached on the turn, but it should not print the full raw file body into the visible message transcript by default.

The user-side turn may show a compact line such as:

- `Attached: strategy_notes.md`

This is an audit hint, not a durable content store.

### Empty and invalid states

The rail should reject the file before send when:

- no file is selected but the user expects one
- the file extension or media type is disallowed
- the file is too large
- the file cannot be decoded as UTF-8 text

The error should appear inline in the composer area and should not start the SSE request.

## Accepted File Classes

v1 accepts text-like inputs only:

- `text/plain`
- `text/markdown`
- `text/csv`
- `application/json`
- common source and config files whose content is plain text, such as:
  - `.rs`
  - `.ts`
  - `.tsx`
  - `.js`
  - `.jsx`
  - `.py`
  - `.toml`
  - `.yaml`
  - `.yml`
  - `.md`
  - `.txt`
  - `.csv`
  - `.json`

v1 rejects:

- images
- PDFs
- office documents
- archives
- audio/video
- any binary payload

The validation rule should be conservative: if the backend cannot confidently treat the file as text, reject it.

## Recommended Approach

The recommended implementation is to extend the existing `POST /api/chat-rail/chat` route so it accepts `multipart/form-data` instead of JSON-only.

The multipart request contains:

- `session_id`
- `message`
- optional `provider`
- optional `model`
- optional single file part, `attachment`

This is preferred over a separate upload-token flow because:

- the feature is explicitly turn-scoped
- ChatRail already has one send path
- v1 does not need reusable upload lifecycle management

## Backend Design

### Route shape

`POST /api/chat-rail/chat` remains the single ChatRail send endpoint and continues to return SSE.

The route should accept:

- `multipart/form-data` for requests with an attachment
- `application/json` for requests without one, if keeping backward compatibility materially reduces migration risk

If supporting both shapes complicates the handler too much, the frontend may move fully to multipart for every ChatRail send. That is acceptable because ChatRail is the only consumer of this route.

### Request model

The route should normalize requests into an internal shape similar to:

- `session_id: String`
- `message: String`
- `provider: Option<String>`
- `model: Option<String>`
- `attachment: Option<TurnAttachment>`

Where `TurnAttachment` contains:

- `filename: String`
- `media_type: Option<String>`
- `text: String`
- `byte_len: usize`

No database table is introduced for attachments in v1.

### Attachment ingestion

On receipt of a multipart request, the backend should:

1. parse the form fields
2. reject missing required fields
3. read at most one file part named `attachment`
4. enforce the byte limit before full buffering if practical
5. decode the file as UTF-8 text
6. reject disallowed or undecodable files
7. build a bounded attachment preamble for the LLM turn

The backend should not write the uploaded file to a durable temp store unless the framework requires a transient temp file under the hood. The design intent is in-memory, per-request handling only.

### LLM request construction

The current `WizardLoop` / ChatRail path builds the LLM request from persisted chat history plus the new user message. For attachment turns, the backend should keep that history model and extend only the current turn.

The uploaded file should be injected as structured turn context, not as a fake prior chat message from the user.

Recommended prompt shape for the current turn:

```text
User message:
<message text>

Attached file for this turn:
Filename: <filename>
Media type: <media-type or unknown>
Contents:
--- BEGIN FILE (<best-effort language hint>) ---
<bounded file text>
--- END FILE ---
```

This keeps the feature simple and explicit for the model while avoiding any need to alter the persisted history schema.

### Persistence behavior

The existing session history should continue to persist:

- the user’s text message
- the assistant response
- existing tool-call and tool-result blocks

The raw attachment body should not be persisted in `chat_messages.content_blocks_json`.

Two acceptable v1 options for the user message record:

1. Persist only the user’s typed text and let the visible UI render a local attachment chip for the in-flight turn.
2. Persist the user’s typed text plus a tiny metadata note such as filename.

Recommendation: option 2, but metadata only. Example persisted hint:

- `Attached file: strategy_notes.md`

The stored hint should not include raw file contents.

## Limits

v1 should set explicit limits so the feature remains safe and predictable.

Recommended limits:

- one file per turn
- maximum raw upload size: 256 KiB
- maximum injected text budget after decode: 200 KiB or equivalent character cap
- maximum filename length: 255 bytes

If the file exceeds the limit, reject it rather than truncating silently.

If later we want truncation, that should be a deliberate follow-up with explicit UI copy such as “first 200 KiB attached.”

## Frontend Design

### ChatRail component changes

`frontend/web/src/components/shell/ChatRail.tsx` gains:

- hidden file input
- attach button wired to the file input
- local state for selected file
- remove/clear action
- inline validation error state

The current `send()` path should pass the selected file into the API helper. The composer should disable duplicate sends while streaming exactly as it does today.

### API helper changes

`frontend/web/src/api/chat_rail.ts` should add multipart support in `streamChat()`.

When a file is present, the helper should construct `FormData` instead of JSON and omit any manual `content-type` header so the browser sets the multipart boundary.

When no file is present, either:

- always use `FormData` for consistency, or
- keep JSON for attachment-free turns

Recommendation: always use `FormData` from ChatRail once this feature lands. It simplifies the client path and keeps the route contract uniform from the browser’s perspective.

### Client-side validation

The client should do early validation for:

- file-count > 1
- obviously disallowed extension
- size over limit

But backend validation remains authoritative.

## Error Handling

Failures should be separated into pre-stream validation errors and streamed model/tool errors.

### Pre-stream validation errors

Examples:

- unsupported file type
- file too large
- malformed multipart body
- missing `session_id`
- invalid UTF-8

These should return normal HTTP error responses and should render as a composer-level error in ChatRail without creating a placeholder assistant bubble.

### In-stream errors

Once the request has passed validation and SSE begins, downstream model/tool failures should continue to use the existing streamed error event behavior.

## Security and Privacy

This feature should be treated as local operator tooling, but it still needs narrow boundaries.

Required constraints:

- no arbitrary server-path reads
- no persistent raw attachment storage in session history
- no automatic cross-turn reuse of prior uploads
- explicit file-type and size gating

The model will still see the uploaded text for that turn, which is the core feature. The product should not imply any stronger confidentiality guarantee than the existing ChatRail model already provides.

## Testing

### Backend tests

Add route tests for:

- multipart request with one valid text attachment succeeds
- multipart request with no attachment still succeeds
- unsupported file type returns `4xx`
- oversized file returns `4xx`
- invalid UTF-8 returns `4xx`
- session history does not persist raw uploaded file contents

### Frontend tests

Add component tests for:

- selecting a file shows an attachment chip
- removing a file clears the chip
- sending with a file calls the API helper with multipart payload
- oversized or disallowed file shows inline error and blocks send
- file selection clears after successful send

### Manual verification

1. Open ChatRail on any supported route.
2. Attach a small `.md` or `.json` file.
3. Ask the model to summarize or inspect the file.
4. Confirm the assistant can reference file contents.
5. Reload or re-open the rail and confirm the raw file is not recoverable from chat history.
6. Try a PDF or image and confirm the rail rejects it before send.

## Out of Scope

- setup wizard support
- drag-and-drop polish
- multiple attachments
- attachment previews beyond filename
- durable artifact library
- semantic parsing of CSV/JSON into special tool inputs
- OCR or document extraction

## Risks

1. If the route tries to support both JSON and multipart in an ad hoc way, the handler can become brittle.
2. If the file-size limit is too high, ChatRail will become a slow upload tunnel instead of a scoped helper.
3. If the UI persists attachment hints poorly, users may infer that the full file is still attached on later turns when it is not.
4. If raw attachment content leaks into persisted history, the feature will accidentally become a data-retention change.

## Result

When this design is implemented:

- ChatRail can accept one uploaded text-like file per turn
- the model can use that file during the current request
- the feature stays local, narrow, and explicit
- and xvision avoids drifting into a broader document-management system before it is needed
