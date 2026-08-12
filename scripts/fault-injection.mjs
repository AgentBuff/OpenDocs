#!/usr/bin/env node

/**
 * Safe HTTP fault-injection smoke. It only sends malformed/stale requests and must never mutate
 * an artifact. Set OO_ARTIFACT_ID to a disposable artifact when testing the conflict path.
 */
const api = process.env.OO_API_URL ?? "http://127.0.0.1:8787";
const artifactId = process.env.OO_ARTIFACT_ID;
const json = async (url, options) => {
  const response = await fetch(url, options);
  let body = null;
  try { body = await response.json(); } catch { /* error bodies may be empty */ }
  return { response, body };
};

const health = await json(`${api}/api/health`);
if (!health.response.ok || health.body?.status !== "ok") throw new Error("health check 失败");
const malformed = await json(`${api}/api/artifacts/not-an-id/transactions`, {
  method: "POST",
  headers: { "content-type": "application/json", "if-match": '"0"', "x-transaction-id": "fault-malformed" },
  body: JSON.stringify({ commands: "not-an-array" }),
});
if (![400, 404, 409, 422].includes(malformed.response.status)) {
  throw new Error(`malformed transaction 返回异常状态 ${malformed.response.status}`);
}

let conflict = "skipped";
if (artifactId) {
  const stale = await json(`${api}/api/artifacts/${encodeURIComponent(artifactId)}/transactions`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "if-match": '"0"',
      "x-transaction-id": "fault-stale-revision",
    },
    body: JSON.stringify({
      protocolVersion: 1,
      transactionId: "fault-stale-revision",
      intentId: "fault-stale-revision",
      artifactId,
      actorId: "release-fault-check",
      baseRevision: 0,
      origin: "system",
      commands: [],
    }),
  });
  if (![400, 409, 422].includes(stale.response.status)) {
    throw new Error(`stale revision 没有返回预期冲突状态：${stale.response.status}`);
  }
  conflict = stale.response.status;
}
console.log(JSON.stringify({ api, malformed: malformed.response.status, staleRevision: conflict }, null, 2));
