# I01 design

## Selection state

```ts
export type EditorSelection =
  | { kind: "none" }
  | { kind: "text"; blockId: string; range: TextRange; affinity: "forward" | "backward" }
  | { kind: "blocks"; blockIds: readonly string[]; anchorId: string; focusId: string }
  | { kind: "object"; blockId: string; objectType: ObjectBlockType }
  | { kind: "table"; blockId: string; selection: TableSelection; mode: "cell" | "range" | "row" | "column" | "all" };

export type ObjectBlockType = "image" | "code";
```

`text` only represents one contentEditable root. A text range spanning blocks is represented by `blocks` until a later cross-block editing command is introduced. This deliberately prevents an invalid DOM range from pretending to be a durable RichText range.

## State transitions

| Input | Previous | Next | Required effect |
| --- | --- | --- | --- |
| pointerdown in editable text | any | text | focus DOM, map Range, hide object toolbar |
| pointerdown on image | any | object(image) | prevent native image drag, show image toolbar |
| pointerdown in table cell | none/text | table(cell) then text or range | single click enters editing; drag/Shift produces range |
| Shift+pointer/keyboard table extension | table | table(range) | preserve anchor stable IDs |
| Escape | overlay open | unchanged selection | close highest overlay |
| Escape | object/table | none or nearest text | clear selection and focus appropriate editor target |
| ArrowLeft on object | object | text | focus end of preceding editable position |
| ArrowRight on object | object | text | focus start of following editable position |
| Enter at object left/right boundary | object boundary | text/structure change | semantic insert before/after only |

## Ownership

```text
DOM events → PointerRouter / KeyboardRouter
DOM Range  → SelectionResolver
             ↓
        InteractionStore
             ↓
 BlockNode / table / image renderer consume readonly selection
             ↓
 BlockSessionApi produces semantic DocumentCommand
```

## Block behavior contract

`BlockDefinition` remains renderer registration, but gets an optional `behavior` object:

```ts
interface BlockBehavior {
  selection: SelectionBehavior;
  keyboard?: KeyboardBehavior;
  pointer?: PointerBehavior;
  overlays?: OverlayBehavior;
  accessibility?: AccessibilityBehavior;
}
```

The behavior may determine state transitions and invoke session methods. It cannot mutate `DocumentBlock`, create an alternate table/image model, or add renderer-only persistence.

## Overlay protocol

Overlay registrations declare:

```ts
type OverlayKind = "blockMenu" | "contextMenu" | "toolbar" | "popover" | "dialog" | "toast";
interface OverlayRegistration {
  id: string;
  kind: OverlayKind;
  ownerBlockId?: string;
  closeOnEscape: boolean;
  closeOnOutsidePointer: boolean;
  priority: number;
}
```

Priority is `dialog > contextMenu > popover/menu > floating toolbar > transient selection`. Portal roots share one stack; CSS z-index values are tokens, not local magic numbers.
