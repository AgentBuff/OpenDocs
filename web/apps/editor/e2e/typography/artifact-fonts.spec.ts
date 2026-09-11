import { expect, test, type Page } from '@playwright/test';
import { deleteFixture } from '../support/fixtures.js';
import { createTextNode } from '../../src/presentation/commands.js';
import { plainPresentationRichText } from '@open-office/schema';

async function choose(page: Page, name = 'Lora') {
  await page.getByRole('combobox', { name: '字体', exact: true }).first().click();
  await page.getByRole('textbox', { name: '搜索字体' }).fill(name);
  await page.getByRole('option', { name, exact: true }).click();
}
async function loaded(page: Page, family = 'Lora') {
  await expect.poll(() => page.evaluate(family => Array.from(document.fonts).some(face => face.family.replaceAll('"','') === family && face.status === 'loaded'), family)).toBe(true);
}

test('presentation table fonts survive content editing and reload', async ({ page, request }) => {
  const response = await request.post('http://127.0.0.1:8788/api/artifacts', { data: { kind: 'presentation', title: 'Table font check' } });
  expect(response.ok()).toBeTruthy();
  const { id } = await response.json();
  try {
    await page.goto(`/?doc=${id}`);
    await page.getByRole('button', { name: '创建首张幻灯片', exact: true }).click();
    await expect(page.locator('.presentation-studio__stage')).toBeVisible();
    const snapshot = await (await request.get(`http://127.0.0.1:8788/api/artifacts/${id}/snapshot`)).json();
    const slide = snapshot.artifact.payload.data.slides[0];
    const node = { ...createTextNode('font-table', '0001'), kind: { type: 'table', data: { rows: 1, columns: 2, cells: ['Font cell', 'Other cell'].map((text, column) => ({ row: 0, column, rowSpan: 1, columnSpan: 1, content: plainPresentationRichText(text), style: { fill: { type: 'none' }, horizontalAlign: 'left', verticalAlign: 'middle' } })) } } };
    const transactionId = crypto.randomUUID();
    const inserted = await request.post(`http://127.0.0.1:8788/api/artifacts/${id}/transactions`, {
      headers: { 'If-Match': `"${snapshot.artifact.revision}"`, 'x-transaction-id': transactionId },
      data: { protocolVersion: 1, transactionId, intentId: crypto.randomUUID(), artifactId: id, actorId: 'font-e2e', baseRevision: snapshot.artifact.revision, origin: 'local', commands: [{ commandId: crypto.randomUUID(), typeId: 'presentation.insertNode', payload: { type: 'insertNode', slideId: slide.id, node, index: 0 } }] },
    });
    expect(inserted.ok(), await inserted.text()).toBeTruthy();
    await page.reload();
    await page.getByRole('gridcell').first().click();
    await choose(page);
    await expect(page.getByRole('gridcell').first().locator('span')).toHaveCSS('font-family', 'Lora, serif');
    await page.getByRole('button', { name: '打开表格属性', exact: true }).click();
    await page.getByRole('textbox', { name: '内容', exact: true }).fill('Edited font cell');
    await page.getByRole('button', { name: '应用内容', exact: true }).click();
    await expect(page.getByRole('gridcell').first()).toHaveText('Edited font cell');
    await page.reload();
    await expect(page.getByRole('gridcell').first().locator('span')).toHaveCSS('font-family', 'Lora, serif');
    await expect(page.getByRole('gridcell').nth(1)).toHaveText('Other cell');
    await expect(page.locator('.presentation-studio__thumbnail-table span')).toHaveCSS('font-family', 'Lora, serif');
    await page.getByRole('button', { name: '播放演示', exact: true }).click();
    await expect(page.locator('.presentation-playback__table span')).toHaveCSS('font-family', 'Lora, serif');
    await loaded(page);
  } finally { await deleteFixture(request, { artifactId: id, revision: 1 }); }
});

test('failed font downloads keep the saved font and support a keyboard retry', async ({ page, request }) => {
  const response = await request.post('http://127.0.0.1:8788/api/artifacts', { data: { kind: 'whiteboard', title: 'Font retry check' } });
  expect(response.ok()).toBeTruthy();
  const { id } = await response.json();
  try {
    await page.goto(`/?doc=${id}`);
    await page.getByRole('button', { name: '添加文字', exact: true }).click();
    await expect(page.getByRole('textbox', { name: '白板文字' })).toBeVisible();
    await page.route('**/fonts/lora/**', route => route.abort());
    await choose(page);
    await expect(page.getByRole('status')).toContainText('无法加载 Lora');
    await expect(page.locator('.wb-element')).toHaveCSS('font-family', '"Noto Sans SC", sans-serif');
    await page.unroute('**/fonts/lora/**');
    const search = page.getByRole('textbox', { name: '搜索字体' });
    await search.focus();
    await search.press('ArrowDown');
    await expect(page.getByRole('option', { name: 'Lora', exact: true })).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(page.locator('.wb-element')).toHaveCSS('font-family', 'Lora, serif');
    await loaded(page);
  } finally { await deleteFixture(request, { artifactId: id, revision: 1 }); }
});
for (const kind of ['mindmap', 'presentation', 'whiteboard'] as const) {
  test(`${kind} uses the common font picker and restores the real font on reload`, async ({ page, request }) => {
    const response = await request.post('http://127.0.0.1:8788/api/artifacts', { data: { kind, title: `Font check ${kind}` } });
    expect(response.ok()).toBeTruthy();
    const { id } = await response.json();
    try {
      await page.goto(`/?doc=${id}`);
      if (kind === 'mindmap') {
        await page.getByRole('button', { name: '中心主题', exact: true }).click();
        const input = page.getByRole('textbox', { name: '主题文字' });
        await input.fill('Font comparison'); await input.press('Enter');
        await choose(page);
        await expect(page.locator('.mindmap-node > span').first()).toHaveCSS('font-family', 'Lora, serif');
        await page.locator('.mindmap-node').first().dblclick();
        await page.getByRole('textbox', { name: '主题文字', exact: true }).fill('这是用于检查真实字体换行与节点高度的长中文主题 '.repeat(6));
        await page.getByRole('textbox', { name: '主题文字', exact: true }).press('Enter');
        await choose(page, '思源宋体');
        await expect(page.locator('.mindmap-node__text').first()).toHaveCSS('font-family', '"Noto Serif SC", serif');
        await expect.poll(() => page.locator('.mindmap-node').first().evaluate(node => {
          const text = node.querySelector('.mindmap-node__text')!;
          const outer = node.getBoundingClientRect(), inner = text.getBoundingClientRect();
          return inner.top >= outer.top && inner.bottom <= outer.bottom && inner.right <= outer.right;
        })).toBe(true);
        await choose(page);

      } else if (kind === 'whiteboard') {
        await page.getByRole('button', { name: '添加文字', exact: true }).click();
        await expect(page.getByRole('textbox', { name: '白板文字' })).toBeVisible();
        await page.getByRole('textbox', { name: '白板文字' }).fill('白板文字不会被字体切换覆盖');
        await choose(page);
        await expect(page.locator('.wb-element')).toHaveCSS('font-family', 'Lora, serif');
      } else {
        await page.getByRole('button', { name: '创建首张幻灯片', exact: true }).click();
        await page.getByRole('button', { name: '文本框', exact: true }).first().click();
        await page.locator('.presentation-studio__node').first().click();
        await expect(page.locator('.presentation-studio__node.is-selected')).toBeVisible();
        await choose(page);
        await expect(page.locator('.presentation-studio__stage .presentation-studio__text-content span').first()).toHaveCSS('font-family', 'Lora, serif');
        await page.locator('.presentation-studio__node').first().dblclick();
        const editor = page.getByRole('textbox', { name: '编辑文本对象' });
        await editor.fill('Font comparison 中文');
        await editor.press('Tab');
        await expect(page.locator('.presentation-studio__stage .presentation-studio__text-content')).toHaveText('Font comparison 中文');
        await expect(page.locator('.presentation-studio__stage .presentation-studio__text-content span').first()).toHaveCSS('font-family', 'Lora, serif');
        const metrics = await page.locator('.presentation-studio__stage .presentation-studio__text-content').evaluate(el => ({ size: parseFloat(getComputedStyle(el).fontSize), width: el.getBoundingClientRect().width }));
        expect(metrics.size).toBeCloseTo(12 * 12700 * metrics.width / 4800000, 2);
        expect(metrics.size).toBeGreaterThan(5);

      }
      await loaded(page);
      await expect.poll(async () => {
        const response = await request.get(`http://127.0.0.1:8788/api/artifacts/${id}/snapshot`);
        return JSON.stringify(await response.json());
      }).toContain('Lora, serif');
      await page.reload();
      if (kind === 'mindmap') await expect(page.locator('.mindmap-node > span').first()).toHaveCSS('font-family', 'Lora, serif');
      if (kind === 'whiteboard') {
        await expect(page.locator('.wb-element')).toHaveCSS('font-family', 'Lora, serif');
        await expect(page.locator('.wb-element')).toHaveText('白板文字不会被字体切换覆盖');
      }
      if (kind === 'presentation') await expect(page.locator('.presentation-studio__stage .presentation-studio__text-content span').first()).toHaveCSS('font-family', 'Lora, serif');
      await loaded(page);
      await page.screenshot({ path: `test-results/font-${kind}.png` });
      if (kind === 'presentation') {
        await expect(page.locator('.presentation-studio__stage .presentation-studio__text-content')).toHaveText('Font comparison 中文');
        await expect(page.locator('.presentation-studio__thumbnail-text span')).toHaveCSS('font-family', 'Lora, serif');
        await page.getByRole('button', { name: '播放演示', exact: true }).click();
        await expect(page.locator('.presentation-playback__text span')).toHaveCSS('font-family', 'Lora, serif');
        expect(await page.locator('.presentation-playback__text').evaluate(el => parseFloat(getComputedStyle(el).fontSize))).toBeGreaterThan(5);
        await page.screenshot({ path: 'test-results/font-presentation-playback.png' });
      }

    } finally { await deleteFixture(request, { artifactId: id, revision: 1 }); }
  });
}
