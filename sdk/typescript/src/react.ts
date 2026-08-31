/**
 * React integration hooks for remem.
 */

import { useState, useEffect, useCallback } from "react";
import { Memory } from "./index.js";
import type { MemoryResult, MemoryConfig, StoreOptions, RecallOptions } from "./types.js";

/**
 * Hook to instantiate and manage a persistent Memory client.
 */
export function useMemory(config: MemoryConfig) {
  const [client] = useState(() => new Memory(config));
  const [isHealthy, setIsHealthy] = useState<boolean | null>(null);

  useEffect(() => {
    client
      .getHealth()
      .then((h) => setIsHealthy(h.status === "ok"))
      .catch(() => setIsHealthy(false));
  }, [client]);

  return { client, isHealthy };
}

/**
 * Hook to execute and cache guided recall queries within React components.
 */
export function useRecall(client: Memory, initialQuery?: string, options?: RecallOptions) {
  const [results, setResults] = useState<MemoryResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const recall = useCallback(
    async (query: string, customOptions?: RecallOptions) => {
      setLoading(true);
      setError(null);
      try {
        const data = await client.recall(query, { ...options, ...customOptions });
        setResults(data);
        return data;
      } catch (err) {
        const e = err instanceof Error ? err : new Error(String(err));
        setError(e);
        throw e;
      } finally {
        setLoading(false);
      }
    },
    [client, options]
  );

  useEffect(() => {
    if (initialQuery) {
      recall(initialQuery).catch(() => {});
    }
  }, [initialQuery, recall]);

  return { results, loading, error, recall };
}

/**
 * Hook to store new memories from React user interactions.
 */
export function useStore(client: Memory) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const store = useCallback(
    async (content: string, options?: StoreOptions) => {
      setSaving(true);
      setError(null);
      try {
        return await client.store(content, options);
      } catch (err) {
        const e = err instanceof Error ? err : new Error(String(err));
        setError(e);
        throw e;
      } finally {
        setSaving(false);
      }
    },
    [client]
  );

  return { store, saving, error };
}
