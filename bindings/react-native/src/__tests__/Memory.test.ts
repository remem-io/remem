import nativeMock, { calls } from '../__mocks__/RememModule';
import { Memory } from '../index';

jest.mock('../RememModule', () => ({
  __esModule: true,
  default: jest.requireActual('../__mocks__/RememModule').default,
}));

describe('Memory', () => {
  beforeEach(() => {
    nativeMock.__reset();
  });

  it('open() calls openEngine with project and dataDir', async () => {
    await Memory.open({ project: 'my-project', dataDir: '/tmp/foo' });

    expect(calls).toEqual([{ method: 'openEngine', args: ['my-project', '/tmp/foo'] }]);
  });

  it('open() defaults project to "default" and dataDir to null', async () => {
    await Memory.open();

    expect(calls).toEqual([{ method: 'openEngine', args: ['default', null] }]);
  });

  it('close() calls closeEngine with the engine handle', async () => {
    const memory = await Memory.open();
    nativeMock.__reset();

    await memory.close();

    expect(calls).toEqual([{ method: 'closeEngine', args: [0] }]);
  });

  it('store() encodes tags as a JSON array and omits empty tags as null', async () => {
    const memory = await Memory.open();
    nativeMock.__reset();

    await memory.store('hello', { tags: ['a', 'b'] });
    await memory.store('hello again');

    expect(calls).toEqual([
      { method: 'store', args: [0, 'hello', '["a","b"]', -1] },
      { method: 'store', args: [0, 'hello again', null, -1] },
    ]);
  });

  it('store() passes importance through directly when given, -1 sentinel otherwise', async () => {
    const memory = await Memory.open();
    nativeMock.__reset();

    await memory.store('important thing', { importance: 9 });

    expect(calls).toEqual([{ method: 'store', args: [0, 'important thing', null, 9] }]);
  });

  it('update() distinguishes omitted tags (null = unchanged) from empty tags (clears)', async () => {
    const memory = await Memory.open();
    nativeMock.__reset();

    await memory.update('mem-1', { content: 'new content' });
    await memory.update('mem-2', { tags: [] });
    await memory.update('mem-3', { tags: ['x'] });

    expect(calls).toEqual([
      { method: 'update', args: [0, 'mem-1', 'new content', -1, null] },
      { method: 'update', args: [0, 'mem-2', null, -1, '[]'] },
      { method: 'update', args: [0, 'mem-3', null, -1, '["x"]'] },
    ]);
  });

  it('update() passes importance through directly when given, -1 sentinel otherwise', async () => {
    const memory = await Memory.open();
    nativeMock.__reset();

    await memory.update('mem-1', { importance: 7 });
    await memory.update('mem-2', {});

    expect(calls).toEqual([
      { method: 'update', args: [0, 'mem-1', null, 7, null] },
      { method: 'update', args: [0, 'mem-2', null, -1, null] },
    ]);
  });

  it('recall() and search() default limit and encode filterTags like store', async () => {
    const memory = await Memory.open();
    nativeMock.__reset();

    await memory.recall('query');
    await memory.search('query', { filterTags: ['x'] });

    expect(calls).toEqual([
      { method: 'recall', args: [0, 'query', 8, null] },
      { method: 'search', args: [0, 'query', 20, '["x"]'] },
    ]);
  });

  it('forget() defaults mode to "delete"', async () => {
    const memory = await Memory.open();
    nativeMock.__reset();

    await memory.forget('mem-1');
    await memory.forget('mem-2', 'archive');

    expect(calls).toEqual([
      { method: 'forget', args: [0, 'mem-1', 'delete'] },
      { method: 'forget', args: [0, 'mem-2', 'archive'] },
    ]);
  });

  it('decay() defaults factor to 0.9', async () => {
    const memory = await Memory.open();
    nativeMock.__reset();

    await memory.decay();

    expect(calls).toEqual([{ method: 'decay', args: [0, 0.9] }]);
  });

  it('queryKnowledge() defaults all fields to null', async () => {
    const memory = await Memory.open();
    nativeMock.__reset();

    await memory.queryKnowledge({ subject: 'Alice' });

    expect(calls).toEqual([{ method: 'queryKnowledge', args: [0, 'Alice', null, null] }]);
  });

  it('listSessions() defaults limit to 20', async () => {
    const memory = await Memory.open();
    nativeMock.__reset();

    await memory.listSessions();

    expect(calls).toEqual([{ method: 'listSessions', args: [0, 20] }]);
  });

  it('consolidate() defaults model to null', async () => {
    const memory = await Memory.open();
    nativeMock.__reset();

    await memory.consolidate('session-1');
    await memory.consolidate('session-2', 'claude-haiku-4-5');

    expect(calls).toEqual([
      { method: 'consolidate', args: [0, 'session-1', null] },
      { method: 'consolidate', args: [0, 'session-2', 'claude-haiku-4-5'] },
    ]);
  });
});
