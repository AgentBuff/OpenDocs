# I00 command and API contract

I00 introduces test-only helpers; it does not add a Document semantic command or a production route.

## Permitted public API use

```text
POST /api/artifacts
GET  /api/artifacts/{id}/snapshot
PUT  /api/artifacts/{id}/snapshot
POST /api/artifacts/{id}/transactions
GET  /api/artifacts/{id}/events
```

The fixture creator must set explicit `transactionId` and revision headers for every write. A successful browser assertion must verify the revision increment and, where appropriate, a `document.*` event.

## Test helper contract

```ts
interface DocumentFixture {
  artifactId: string;
  revision: number;
  snapshot: ArtifactSnapshot;
}

createDocumentFixture(kind: FixtureKind): Promise<DocumentFixture>;
readArtifactSnapshot(id: string): Promise<ArtifactSnapshot>;
waitForRevision(id: string, revision: number): Promise<ArtifactSnapshot>;
```

Helpers are test-only and must live outside production editor imports. They may not bypass server validation.

## Explicit non-changes

- No `/api/test/**` route.
- No production-only `data-testid` API contract. Existing semantic `data-block-id` remains valid; new test locators must prefer role/name and semantic attributes over CSS implementation classes.
- No weakening of CORS, auth seams, body limits or revision checks to make tests easier.
