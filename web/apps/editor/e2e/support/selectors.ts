import type { Page } from "@playwright/test";

/**
 * Semantic selectors shared by browser specs. Keep raw CSS selectors confined
 * here so product tests describe observable editor behavior.
 */
export function activeEditor(page: Page) {
  return editableBlocks(page).first();
}

export function editableBlocks(page: Page) {
  return page.locator('[contenteditable="true"]');
}

export function blockRows(page: Page) {
  return page.locator("[data-block-id]");
}

export function blockMenuTrigger(page: Page, label: "插入块" | "打开块菜单") {
  return page.getByRole("button", { name: label });
}

export function blockMenu(page: Page) {
  return page.getByRole("menu", { name: "块菜单" });
}
