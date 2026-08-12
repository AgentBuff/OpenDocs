import { describe, expect, it } from "vitest";

import {
  EventFeedConsumer,
  OpenOfficeSdk,
  buildTransaction,
  conflictDetails,
  isVersionConflict,
} from "../src/index.js";
import { ArtifactApiClient, ArtifactApiError, type EventRecord } from "@open-office/schema/api";

function page(events: EventRecord[], revision = 3, nextCursor?: string) {
  return { artifactId: "a-1", revision, events, ...(nextCursor ? { nextCursor } : {}) };
}

describe("agent-facing SDK adapter", () => {
  it("builds semantic remote transactions with stable caller ids", () => {
    const transaction = buildTransaction({
      artifactId: "a-1",
      actorId: "agent-indexer",
      baseRevision: 7,
      transactionId: "tx-7",
      intentId: "intent-7",
      commands: [{ typeId: "document.replaceBlockText", payload: { blockId: "b-1", content: { text: "hi", runs: [] } }, commandId: "cmd-7" }],
    });
    expect(transaction).toMatchObject({
      protocolVersion: 1,
      transactionId: "tx-7",
      intentId: "intent-7",
      origin: "remote",
      commands: [{ commandId: "cmd-7", typeId: "document.replaceBlockText" }],
    });
    expect(() => buildTransaction({ artifactId: "a-1", actorId: "agent", baseRevision: 0, commands: [] })).toThrow("至少");
  });

  it("reads bounded context and carries citation refs through one typed facade", async () => {
    const calls: string[] = [];
    const sdk = new OpenOfficeSdk({
      baseUrl: "http://api.test",
      fetcher: async (input) => {
        calls.push(String(input));
        const url = String(input);
        const projection = url.includes("/outline") ? "outline" : "block";
        return new Response(JSON.stringify({
          protocolVersion: 1,
          contractVersion: 1,
          artifactId: "a-1",
          revision: 4,
          projection,
          data: projection === "outline" ? { items: [] } : {
            items: [{ blockId: "b-1", kind: "paragraph", parentId: null, order: 0, children: [], refs: [{
              artifactId: "a-1", blockId: "b-1", revision: 4, textRange: { start: 0, end: 2 }, headingPath: ["Intro"], sourceUrl: "https://example.test",
            }] }],
          },
          truncated: false,
        }), { status: 200 });
      },
    });
    const context = await sdk.context("a-1", { limit: 10, maxBytes: 4096 });
    expect(context.citations[0]?.sourceUrl).toBe("https://example.test");
    expect(calls).toHaveLength(2);
    expect(calls.every((call) => call.includes("limit=10"))).toBe(true);
  });

  it("deduplicates at-least-once events, preserves opaque cursors and surfaces revision gaps", () => {
    const api = new ArtifactApiClient({ fetcher: async () => new Response("{}", { status: 500 }) });
    const consumer = new EventFeedConsumer(api, "a-1", { revision: 4 });
    const first = consumer.accept(page([
      { eventId: "e-1", artifactId: "a-1", transactionId: "tx-1", revision: 5, typeId: "document.changed", payload: {} },
      { eventId: "e-2", artifactId: "a-1", transactionId: "tx-2", revision: 7, typeId: "document.changed", payload: {} },
    ], 7, "opaque:cursor"));
    expect(first.events.map((event) => event.eventId)).toEqual(["e-1", "e-2"]);
    expect(first.revisionGap).toEqual({ expectedRevision: 6, actualRevision: 7 });
    expect(consumer.nextCursor).toBe("opaque:cursor");
    const replay = consumer.accept(page([{ eventId: "e-2", artifactId: "a-1", transactionId: "tx-2", revision: 7, typeId: "document.changed", payload: {} }], 7));
    expect(replay.events).toEqual([]);
  });

  it("classifies machine-readable revision conflicts for re-read/rebase flows", () => {
    const error = new ArtifactApiError(409, "冲突", {
      error: "冲突",
      code: "version_conflict",
      requestId: "req-1",
      details: { artifactId: "a-1", requestedRevision: 4, currentRevision: 6 },
    });
    expect(isVersionConflict(error)).toBe(true);
    expect(conflictDetails(error)).toEqual({ artifactId: "a-1", requestedRevision: 4, currentRevision: 6 });
  });
});
