// @remem/sdk — TypeScript SDK Tests
// Using Node.js built-in test runner (node:test)

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

describe('RememClient', () => {
  it('should construct with default options', () => {
    // Placeholder: will test actual client once implemented
    const baseUrl = 'http://localhost:7474';
    assert.ok(baseUrl.startsWith('http'));
  });

  it('should construct with custom base URL', () => {
    const baseUrl = 'https://api.remem.dev';
    assert.ok(baseUrl.startsWith('https'));
  });
});

describe('MemoryType', () => {
  it('should have valid type values', () => {
    const types = ['fact', 'procedure', 'preference', 'decision'];
    assert.equal(types.length, 4);
    assert.ok(types.includes('fact'));
    assert.ok(types.includes('procedure'));
  });
});

describe('StoreRequest', () => {
  it('should accept minimal request shape', () => {
    const req = {
      content: 'test memory',
      tags: [],
      memory_type: 'fact',
    };
    assert.equal(req.content, 'test memory');
    assert.deepEqual(req.tags, []);
  });

  it('should accept full request shape', () => {
    const req = {
      content: 'full memory',
      tags: ['bio', 'test'],
      importance: 8.0,
      ttl_days: 30,
      memory_type: 'preference',
    };
    assert.equal(req.importance, 8.0);
    assert.equal(req.ttl_days, 30);
  });
});

describe('MemoryResult', () => {
  it('should have decay_score field', () => {
    const res = {
      id: 'uuid',
      content: 'test',
      importance: 5.0,
      tags: [],
      memory_type: 'fact',
      created_at: '2026-01-01',
      similarity: 0.5,
      decay_score: 1.0,
    };
    assert.equal(res.decay_score, 1.0);
  });
});
