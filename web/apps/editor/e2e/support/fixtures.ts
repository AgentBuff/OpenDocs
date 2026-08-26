import { expect, type APIRequestContext, type Page } from "@playwright/test";

export interface DocumentFixture {
  artifactId: string;
  revision: number;
}

export async function createDocumentFixture(request: APIRequestContext, title = "E2E document"): Promise<DocumentFixture> {
  const response = await request.post("http://127.0.0.1:8788/api/artifacts", {
    data: { kind: "document", title },
  });
  expect(response.ok()).toBeTruthy();
  const artifact = await response.json() as { id: string; version: number };
  return { artifactId: artifact.id, revision: artifact.version };
}

export async function deleteFixture(request: APIRequestContext, fixture: DocumentFixture): Promise<void> {
  const response = await request.delete(`http://127.0.0.1:8788/api/artifacts/${fixture.artifactId}`);
  expect(response.ok()).toBeTruthy();
}

export async function readDocumentText(request: APIRequestContext, artifactId: string): Promise<string> {
  const response = await request.get(`http://127.0.0.1:8788/api/artifacts/${artifactId}/snapshot`);
  expect(response.ok()).toBeTruthy();
  const payload = await response.json() as {
    artifact: {
      payload: {
        kind: "document";
        data: { blocks?: Array<{ content?: { text?: string } | null }> };
      };
    };
  };
  if (payload.artifact.payload.kind !== "document") throw new Error("E2E fixture 不是 Document");
  return payload.artifact.payload.data.blocks?.map((block) => block.content?.text ?? "").join("\n") ?? "";
}

export async function openDocument(page: Page, artifactId: string): Promise<void> {
  await page.goto(`/?doc=${artifactId}`);
  await expect(page.locator('[contenteditable="true"]').first()).toBeVisible();
}

export async function waitForPersistedText(
  request: APIRequestContext,
  artifactId: string,
  expectedText: string,
): Promise<void> {
  await expect.poll(() => readDocumentText(request, artifactId), { timeout: 12_000 }).toContain(expectedText);
}
