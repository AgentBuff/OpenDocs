import { describe, expect, it } from "vitest";

import { parsePresenterMessage, presenterAudienceUrl, presenterChannelName } from "./presenter-session.js";

describe("presentation presenter session", () => {
  it("builds a same-origin audience URL without losing unrelated query state", () => {
    const url = new URL(presenterAudienceUrl("http://localhost:5174/?theme=dark", "deck-1", "session-1"));
    expect(url.origin).toBe("http://localhost:5174");
    expect(Object.fromEntries(url.searchParams)).toEqual({ theme: "dark", doc: "deck-1", presentationMode: "audience", presentationSession: "session-1" });
    expect(presenterChannelName("session-1")).toBe("open-office:presentation:session-1");
  });

  it("accepts only a versioned view cursor and rejects model-shaped payloads", () => {
    const message = { version: 1, type: "cursor", sessionId: "session-1", artifactId: "deck-1", revision: 9, cursor: { slideId: "slide-2", cueId: "animation-2", elapsedMs: 300, status: "paused" } };
    expect(parsePresenterMessage(message)).toEqual(message);
    expect(parsePresenterMessage({ ...message, deck: { slides: [] } })).toBeNull();
    expect(parsePresenterMessage({ ...message, revision: -1 })).toBeNull();
    expect(parsePresenterMessage({ ...message, cursor: { ...message.cursor, elapsedMs: 1.5 } })).toBeNull();
    expect(parsePresenterMessage({ ...message, cursor: { ...message.cursor, deck: {} } })).toBeNull();
    expect(parsePresenterMessage({ ...message, version: 2 })).toBeNull();
  });
});
