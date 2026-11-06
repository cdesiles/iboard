# Modal

One frame for everything that stops the board until you answer it: a confirmation, a settings form, a document, or a work surface where you and an agent write together. Four sizes, one anatomy, one set of keys, so every modal in iboard reads the same way.

## When to use one

- **Use a modal** when the task has its own commit point (Delete, Save, Add to Todo) and you don't need the screen behind it while you do it.
- **Use a popover instead** (LabelPicker, menus) when the choice is small and belongs to the thing you clicked. It sits under its trigger and has no scrim.
- **Use a pane instead** (ReviewPanel, the Agent log) when you need to read the page while you use it.
- **Never** for information only: background events (a file changed, an agent finished) go to the status bar. A modal never opens by itself.
- Reader, Editor and Stats are screens, not modals.

## Sizes

| Size | Width | Height | For | Today |
| --- | --- | --- | --- | --- |
| **S** | `modal-s` 360 | Fits its content | One question, one action | ConfirmDialog |
| **M** | `modal-m` 480 | Fits its content; the body scrolls past 520 | Forms and settings | Settings (was 440), GroupsDialog |
| **L** | `modal-l` 760 | Fits its content, up to the window minus the inset | One document to read or write: a 720pt sheet plus 20pt each side | Nothing yet (a commit message, an export preview) |
| **XL** | The window minus `modal-inset` (32) on every side, at most `modal-xl` 1200 × `modal-xl-h` 900 | Fixed: always that tall, so nothing jumps while content arrives | Work surfaces: long writing, an agent's answer streaming in, two panes | NewIssueModal, RefinePanel (was "up to 1080") |

On a 14" MacBook Pro window (1512 × 945pt) XL is 1200 × 880: most of the screen, with a strip of the board still showing round it so you know where you are.

- **Placement.** S is centred. M and L sit 12% from the top of the window, so when their content grows only the bottom edge moves; the title never jumps. XL fills the inset area.
- **Small windows.** Every size clamps to the window minus the inset; under 720pt of window width the inset drops to 16. XL's two panes stack under 900pt of modal width.
- **Only XL may hold two panes**, split by the PaneDivider gutter (16pt, dot grip, resizable, remembered per modal).

## Anatomy

- **Scrim** over the whole window, header and status bar included (`scrim`).
- **Frame**: `surface`, 1px `line` (the scrim and `shadow-modal` set it apart; the edge only finishes it in dark mode), `radius-md`, `shadow-modal` (stronger than `shadow-popover`, since it floats above everything). Nothing inside the frame scrolls but the body.
- **Head**, 40pt, `line` rule below: the title in `ui-title` (a noun: "New issue", "Project groups", "Settings"), then optional context in `meta` (Inter) `ink-muted` (the folder or file it acts on: "in todo/", "named in the background"), an optional tag or Unsaved pill, then a ghost close button (×, tooltip "Close (esc)"). S has no head bar: its title is the question and Cancel is the close.
- **Body**: padding 16 (20 for S), gaps 16 between fields. It is the only part that scrolls (overlay scrollbar). XL bodies have no padding of their own: each pane pads itself.
- **Foot**, 40pt, `line` rule above. On the left, context in `meta` (Inter, `ink-muted`): the file that will be written, the config path, "Saved for this session". The keys that apply now join it only while ⌘ is held. On the right, buttons in this order: an optional ghost button (Discard, End refining), the secondary close (Cancel or Close) with a `line-soft` hairline and no fill, then **one** filled primary, rightmost. Its key shows beside it while ⌘ is held and in its tooltip.
- **The primary's colour says what it does**: `primary` (ink) saves or adds, `danger` destroys (S only), `review` sends to an agent. Never two filled buttons in one foot. Its key is `↩` in S and in the new-issue modal (where `↩` already meant add), `⌘↩` everywhere else.
- **The agent tint**: only a modal whose content is written by the agent (RefinePanel) takes the `review-soft` head. A modal where the agent only helps you (NewIssueModal) keeps a plain head; its violet is on the button that calls the agent and on what the agent wrote.

## Keys and behaviour

- **Opening**: focus goes to the first field, or to the primary in S. `tab` cycles inside the frame and never leaves it. On close, focus goes back to the control that opened it.
- **`esc`**, in order: closes a menu open inside the modal; else stops a running agent and keeps your input; else closes the modal. If the modal holds unsaved input, M and L ask first ("Discard changes to groups?", Discard / Keep editing). XL work surfaces never ask: they park your input and give it back next time (see NewIssueModal).
- **Scrim click** closes S only, where nothing can be lost. M, L and XL ignore it: a stray click must never throw away typed work.
- **Stacking**: at most two levels, and the only modal that may open over another is an S (ConfirmDialog over GroupsDialog). It brings its own scrim.
- **Motion**: the scrim fades in over 120ms, the frame fades and rises 8pt over 160ms (ease-out), and closing is a 100ms fade. With reduced motion, both appear and disappear in one frame.
- **Accessibility**: `role="dialog"` (`alertdialog` for S), `aria-modal="true"`, `aria-labelledby` the title. A region that fills in later carries `aria-busy` and one `role="status"` line, as in Thinking.

## Tokens

New in `tokens.json`: `modal-s` 360, `modal-m` 480, `modal-l` 760, `modal-xl` 1200, `modal-xl-h` 900, `modal-inset` 32 and `shadow-modal`. `bundle.css` uses them with these values as fallbacks, so previews work before `tokens.css` is regenerated.

## Migration

- Settings goes from 440 to 480 (M), the same as GroupsDialog.
- RefinePanel becomes XL: same layout, a fixed height instead of "up to 1080 wide", so the panes no longer change size between the Thinking state and the answer.
- ConfirmDialog is already S; nothing changes but the shadow.
- QuickAdd's inline composer is replaced by NewIssueModal.

The consumer provides the size, the title and context, the body, the foot's actions, a `dirty` flag and the close handler. In egui, `widgets::modal(ui, id, size, title)`, sized with `theme::modal_size(size, window)`.
