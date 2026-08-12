export function focusBlock(id: string): void {
  document.querySelector<HTMLElement>(`[data-block-id="${id}"] .block-row__content`)?.focus();
}
