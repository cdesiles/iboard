# DesignPanel

The project's design system, browsed from the board: the **Design** button in the HeaderBar opens it as a window over the board, and `idesign` shows the same panel as its whole window. It is laid out like Claude Design: a category rail on the left, the open category on the right.

## Anatomy

1. **Toolbar.** The system's title in `ink` (strong), then `· namespace` in `ink-muted`. On the right, secondary buttons: **Close**, and **Live catalog** (expand icon), which opens every component in a live WebKit window.
2. **Rail.** 200pt wide. A Search field on top, then the categories as 26pt rows, `radius-sm`: *Overview*, *Colors*, *Typography*, *Spacing*, *Corner radius*, *Shadows*, *Size*, *Components*, *Assets*, *Files*. The open row is filled `accent-soft` with its name in `ink`; the others are `ink-muted`, `surface-hover` under the pointer. Components and Files carry their count in `meta`, `ink-faint`, on the right.
3. **Pane.** The open category, scrolling: swatches for Colors (light and dark side by side), type specimens, spacing and radius bars, shadow cards, sizes, the asset and file lists. Overview shows the counts, the last change and the brief, `design/README.md`, as markdown.
4. **Components level.** Opening *Components* slides the rail sideways (220ms) to a second level: a back row (chevron and "Components" in `ui-strong`) over the alphabetical component list. The pane shows one component: its name and the count, the **New component…** button, its `@dsCard` summary in `ink-muted`, its preview card, then its README.

## States

| State | What shows |
| --- | --- |
| Browse | A category open in the pane; its row filled in the rail |
| Search | The rail lists the matching tokens and components, each with its category in `meta`; "No matches" in `ink-faint` when none |
| Component | The preview card, as tall as its `@dsCard height` declares: `surface-sunken` on a `line` edge, `radius-md`, the rendered PNG centred inside |
| Preview pending | "Rendering…" in `ink-muted`, centred in the card; only the component in view is rendered |
| Preview failed | The renderer's first error line, in `danger` |
| No preview | "No preview.html", or "No preview renderer on this machine", in `ink-faint` |
| New component | Under the toolbar: Name ("Toast"), Group and Intent ("Transient confirmation") fields, the checkbox "Draft the preview with the agent" when an agent is set up, and **Create component**, disabled until there is a name. The button reads **Cancel** while the form is open |
| Created | "Created …" in `ink-muted` under the toolbar; an agent's draft lands in the preview when it is done |
| No components | "No components yet." in the pane and the list |
| No design system | The toolbar says "No design system", then the load error (or "No design system in this project.") and "Add design/design-system.json, or `idesign import <package>`." |

## Tokens used

`bg`, `surface`, `surface-hover`, `surface-sunken`, `line`, `line-strong`, `ink`, `ink-muted`, `ink-faint`, `accent`, `accent-soft`, `danger`, `radius-sm`, `radius-md`, `space-2`, `space-3`, `space-4`, `control-h`, the `ui-body`, `ui-strong`, `ui-label` and `meta` styles, and every token of the system it shows.

## Keyboard and accessibility

- `tab` reaches the Search field, the rail rows, the toolbar buttons and the form fields; `space` or `↩` opens a focused row.
- Typing in Search filters at once; picking a hit opens its category (or the component) and clears the search.
- The rail slide is skipped when reduced motion is on: the second level appears in one frame.
- Rows are buttons labelled with their category or component name, so VoiceOver reads the rail as a list of choices.

The panel reads the system through the shared `design` module and renders previews in an out-of-process helper, so a broken preview never takes the board down. Emits DesignOpened, CategoryOpened, ComponentSelected, ComponentCreated, CatalogOpened.
