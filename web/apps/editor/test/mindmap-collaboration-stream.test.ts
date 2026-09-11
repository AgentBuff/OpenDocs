import { describe, expect, it } from "vitest";
import { parseMindmapPresence, parseMindmapRevisionNotice } from "../src/mindmap/collaboration-stream.js";

describe("mindmap collaboration stream", () => {
  it("drops duplicate, out-of-order, malformed, and foreign revision notices", () => {
    const valid = JSON.stringify({ artifactId: "map", revision: 8, changedEntities: ["mindmap.node\u001froot"], structureChanged: false });
    expect(parseMindmapRevisionNotice(valid, "map", 7)?.revision).toBe(8);
    expect(parseMindmapRevisionNotice(valid, "map", 8)).toBeNull();
    expect(parseMindmapRevisionNotice(valid, "map", 9)).toBeNull();
    expect(parseMindmapRevisionNotice(valid, "other", 7)).toBeNull();
    expect(parseMindmapRevisionNotice("{", "map", 7)).toBeNull();
  });

  it("validates presence snapshots and removes the local session", () => {
    const data = JSON.stringify({ artifactId: "map", participants: [
      { sessionId: "local", actorId: "a", displayName: "A", selectedNodeIds: [] },
      { sessionId: "remote", actorId: "b", displayName: "B", cursor: { x: 1, y: 2 } },
    ] });
    expect(parseMindmapPresence(data, "map", "local")?.map((item) => item.sessionId)).toEqual(["remote"]);
    expect(parseMindmapPresence(data, "other", "local")).toBeNull();
  });
});
