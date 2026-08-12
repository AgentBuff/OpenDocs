export function selectDocumentText(): void {
  const page = document.querySelector<HTMLElement>(".block-editor__page");
  const selection = window.getSelection();
  if (!page || !selection) return;

  const textNodes: Text[] = [];
  const walker = document.createTreeWalker(page, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const parent = node.parentElement;
      if (!node.textContent || parent?.closest(".block-row__gutter, .block-row__menu, .block-table__controls, .code-block__toolbar, .code-block__gutter, .block-row__list-marker")) {
        return NodeFilter.FILTER_REJECT;
      }
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  let node: Node | null;
  while ((node = walker.nextNode())) textNodes.push(node as Text);

  selection.removeAllRanges();
  const range = document.createRange();
  if (textNodes.length > 0) {
    range.setStart(textNodes[0], 0);
    const last = textNodes[textNodes.length - 1];
    range.setEnd(last, last.data.length);
  } else {
    range.selectNodeContents(page);
  }
  selection.addRange(range);
}

export function isDocumentWideSelection(): boolean {
  const page = document.querySelector<HTMLElement>(".block-editor__page");
  const selection = window.getSelection();
  if (!page || !selection || selection.rangeCount === 0 || selection.isCollapsed) return false;
  const range = selection.getRangeAt(0);
  if (!page.contains(range.commonAncestorContainer)) return false;

  const textNodes: Text[] = [];
  const walker = document.createTreeWalker(page, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const parent = node.parentElement;
      if (!node.textContent || parent?.closest(".block-row__gutter, .block-row__menu, .block-table__controls, .code-block__toolbar, .code-block__gutter, .block-row__list-marker")) {
        return NodeFilter.FILTER_REJECT;
      }
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  let node: Node | null;
  while ((node = walker.nextNode())) textNodes.push(node as Text);
  const documentRange = document.createRange();
  if (textNodes.length === 0) {
    documentRange.selectNodeContents(page);
  } else {
    documentRange.setStart(textNodes[0], 0);
    const last = textNodes[textNodes.length - 1];
    documentRange.setEnd(last, last.data.length);
  }
  // `selectDocumentText` creates these exact boundaries. Keeping the check
  // explicit avoids browser differences in Range.compareBoundaryPoints across
  // sibling contentEditable roots.
  return range.startContainer === documentRange.startContainer
    && range.startOffset === documentRange.startOffset
    && range.endContainer === documentRange.endContainer
    && range.endOffset === documentRange.endOffset;
}
