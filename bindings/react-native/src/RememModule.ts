import { NativeModule, requireNativeModule } from 'expo';

/**
 * Raw native module surface. Not meant to be used directly — see
 * {@link Memory} in `index.ts` for the ergonomic, typed API. Every
 * method here is keyed by an opaque `engineId` returned from
 * `openEngine`, since a single native module instance can back multiple
 * open `Memory` objects (separate projects/engines) at once.
 *
 * Arguments/returns are deliberately primitive (numbers, strings, plain
 * JSON-shaped objects/arrays) rather than the rich types in
 * `Remem.types.ts` — only those types reliably cross the Expo JS bridge.
 * `Memory` in `index.ts` is responsible for the `JSON.stringify`/typing
 * layer on top of this.
 */
// eslint-disable-next-line @typescript-eslint/no-empty-object-type -- NativeModule's generic parameter requires the structural `{}` shape to satisfy its EventsMap constraint; `object` fails to compile here.
declare class RememModule extends NativeModule<{}> {
  openEngine(project: string, dataDir: string | null): Promise<number>;
  closeEngine(engineId: number): Promise<void>;

  store(
    engineId: number,
    content: string,
    tagsJSON: string | null,
    importance: number
  ): Promise<Record<string, unknown>>;

  recall(
    engineId: number,
    query: string,
    limit: number,
    filterTagsJSON: string | null
  ): Promise<unknown[]>;

  search(
    engineId: number,
    query: string,
    limit: number,
    filterTagsJSON: string | null
  ): Promise<unknown[]>;

  update(
    engineId: number,
    id: string,
    content: string | null,
    importance: number,
    tagsJSON: string | null
  ): Promise<Record<string, unknown>>;

  forget(engineId: number, id: string, mode: string): Promise<boolean>;

  decay(engineId: number, factor: number): Promise<number>;

  queryKnowledge(
    engineId: number,
    subject: string | null,
    predicate: string | null,
    object: string | null
  ): Promise<unknown[]>;

  entityContext(engineId: number, entity: string): Promise<unknown[]>;

  startSession(engineId: number, id: string): Promise<void>;
  endSession(engineId: number, id: string): Promise<boolean>;
  getSession(engineId: number, id: string): Promise<Record<string, unknown> | null>;
  listSessions(engineId: number, limit: number): Promise<unknown[]>;

  consolidate(
    engineId: number,
    sessionId: string,
    model: string | null
  ): Promise<Record<string, unknown>>;
}

export default requireNativeModule<RememModule>('Remem');
