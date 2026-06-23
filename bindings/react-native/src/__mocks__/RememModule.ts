/**
 * Mock native module for unit tests. Records every call (name + args) so
 * tests can assert on exactly what `Memory` would send across the JS
 * bridge — this is the cheapest way to verify the tag-encoding and
 * importance-sentinel conventions are correct without a real engine.
 */

export const calls: { method: string; args: unknown[] }[] = [];

function record(method: string, args: unknown[]): void {
  calls.push({ method, args });
}

function reset(): void {
  calls.length = 0;
}

const mock = {
  __reset: reset,

  async openEngine(...args: unknown[]) {
    record('openEngine', args);
    return 0;
  },
  async closeEngine(...args: unknown[]) {
    record('closeEngine', args);
  },
  async store(...args: unknown[]) {
    record('store', args);
    return {
      id: 'mock-id',
      content: args[1],
      importance: 5,
      tags: [],
      memoryType: 'fact',
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
      decayScore: 1,
      sourceSession: null,
      ttlDays: null,
    };
  },
  async recall(...args: unknown[]) {
    record('recall', args);
    return [];
  },
  async search(...args: unknown[]) {
    record('search', args);
    return [];
  },
  async update(...args: unknown[]) {
    record('update', args);
    return {
      id: args[1],
      content: 'updated',
      importance: 5,
      tags: [],
      memoryType: 'fact',
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
      decayScore: 1,
      sourceSession: null,
      ttlDays: null,
    };
  },
  async forget(...args: unknown[]) {
    record('forget', args);
    return true;
  },
  async decay(...args: unknown[]) {
    record('decay', args);
    return 0;
  },
  async queryKnowledge(...args: unknown[]) {
    record('queryKnowledge', args);
    return [];
  },
  async entityContext(...args: unknown[]) {
    record('entityContext', args);
    return [];
  },
  async startSession(...args: unknown[]) {
    record('startSession', args);
  },
  async endSession(...args: unknown[]) {
    record('endSession', args);
    return true;
  },
  async getSession(...args: unknown[]) {
    record('getSession', args);
    return null;
  },
  async listSessions(...args: unknown[]) {
    record('listSessions', args);
    return [];
  },
  async consolidate(...args: unknown[]) {
    record('consolidate', args);
    return {
      sessionId: args[1],
      newFacts: 0,
      updatedFacts: 0,
      contradictions: [],
      knowledgeGraphUpdates: [],
    };
  },
};

export default mock;
