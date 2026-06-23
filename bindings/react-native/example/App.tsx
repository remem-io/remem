import { Memory } from '@remem-io/react-native';
import type { MemoryResult } from '@remem-io/react-native';
import { useEffect, useRef, useState } from 'react';
import { Button, SafeAreaView, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';

export default function App() {
  const memoryRef = useRef<Memory | null>(null);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [content, setContent] = useState('');
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<MemoryResult[]>([]);
  const [status, setStatus] = useState('');

  useEffect(() => {
    let mounted = true;
    Memory.open({ project: 'example-app' })
      .then((memory) => {
        if (!mounted) {
          // Component unmounted before open() resolved — close it
          // immediately rather than leaking the native engine handle.
          memory.close();
          return;
        }
        memoryRef.current = memory;
        setReady(true);
      })
      .catch((err) => setError(String(err)));

    return () => {
      mounted = false;
      memoryRef.current?.close();
      memoryRef.current = null;
    };
  }, []);

  const handleStore = async () => {
    if (!memoryRef.current || !content.trim()) return;
    setStatus('Storing…');
    try {
      const record = await memoryRef.current.store(content, { tags: ['example-app'] });
      setStatus(`Stored "${record.content}" (importance: ${record.importance.toFixed(1)})`);
      setContent('');
    } catch (err) {
      setStatus(`Store failed: ${String(err)}`);
    }
  };

  const handleSearch = async () => {
    if (!memoryRef.current || !query.trim()) return;
    setStatus('Searching…');
    try {
      const found = await memoryRef.current.search(query);
      setResults(found);
      setStatus(`Found ${found.length} result(s)`);
    } catch (err) {
      setStatus(`Search failed: ${String(err)}`);
    }
  };

  return (
    <SafeAreaView style={styles.container}>
      <ScrollView style={styles.container} contentContainerStyle={styles.content}>
        <Text style={styles.header}>remem</Text>

        {error && <Text style={styles.error}>Failed to open engine: {error}</Text>}
        {!ready && !error && <Text>Opening engine…</Text>}

        {ready && (
          <>
            <Group name="Store a memory">
              <TextInput
                style={styles.input}
                placeholder="Something to remember…"
                value={content}
                onChangeText={setContent}
              />
              <Button title="Store" onPress={handleStore} />
            </Group>

            <Group name="Search">
              <TextInput
                style={styles.input}
                placeholder="What are you looking for?"
                value={query}
                onChangeText={setQuery}
              />
              <Button title="Search" onPress={handleSearch} />
              {results.map((result) => (
                <View key={result.id} style={styles.resultRow}>
                  <Text>{result.content}</Text>
                  <Text style={styles.resultMeta}>
                    similarity: {result.similarity.toFixed(2)} · tags: {result.tags.join(', ')}
                  </Text>
                </View>
              ))}
            </Group>

            {status && <Text style={styles.status}>{status}</Text>}
          </>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}

function Group(props: { name: string; children: React.ReactNode }) {
  return (
    <View style={styles.group}>
      <Text style={styles.groupHeader}>{props.name}</Text>
      {props.children}
    </View>
  );
}

const styles = StyleSheet.create({
  header: { fontSize: 30, margin: 20 },
  groupHeader: { fontSize: 20, marginBottom: 12 },
  group: { margin: 20, backgroundColor: '#fff', borderRadius: 10, padding: 20 },
  container: { flex: 1, backgroundColor: '#eee' },
  content: { paddingBottom: 40 },
  input: {
    borderWidth: 1,
    borderColor: '#ccc',
    borderRadius: 6,
    padding: 10,
    marginBottom: 12,
    backgroundColor: '#fff',
  },
  resultRow: { marginTop: 12, paddingTop: 12, borderTopWidth: 1, borderTopColor: '#eee' },
  resultMeta: { fontSize: 12, color: '#666', marginTop: 2 },
  status: { marginHorizontal: 20, color: '#444' },
  error: { margin: 20, color: '#b00020' },
});
